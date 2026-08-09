# @author kongweiguang

"""Parse every GitHub Actions workflow as YAML 1.2 and check its basic shape."""

from __future__ import annotations

import re
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]


class GithubLoader(yaml.SafeLoader):
    """PyYAML defaults to YAML 1.1 where the GitHub key `on` becomes boolean."""


for first_character, resolvers in list(GithubLoader.yaml_implicit_resolvers.items()):
    GithubLoader.yaml_implicit_resolvers[first_character] = [
        resolver for resolver in resolvers if resolver[0] != "tag:yaml.org,2002:bool"
    ]
GithubLoader.add_implicit_resolver(
    "tag:yaml.org,2002:bool",
    re.compile(r"^(?:true|false)$", re.IGNORECASE),
    list("tTfF"),
)


def fail(path: Path, message: str) -> None:
    raise SystemExit(f"workflow validation failed for {path.relative_to(ROOT)}: {message}")


def validate_release_contract(path: Path, jobs: dict[str, object]) -> None:
    release = jobs.get("release")
    if not isinstance(release, dict):
        fail(path, "release workflow requires a release job")
    needs = release.get("needs")
    if not isinstance(needs, list) or set(needs) != {
        "validate",
        "windows",
        "linux",
        "macos",
    }:
        fail(path, "release job must depend on validation and every platform artifact")
    condition = release.get("if")
    required_conditions = (
        "always()",
        "needs.validate.result == 'success'",
        "needs.windows.result == 'success'",
        "needs.linux.result == 'success'",
        "needs.macos.result == 'success'",
    )
    if not isinstance(condition, str) or any(
        required not in condition for required in required_conditions
    ):
        fail(path, "release job must explicitly allow skipped non-release gates")


def main() -> None:
    workflows = sorted((ROOT / ".github" / "workflows").glob("*.y*ml"))
    if not workflows:
        raise SystemExit("workflow validation failed: no workflows found")
    for path in workflows:
        try:
            document = yaml.load(path.read_text(encoding="utf-8"), Loader=GithubLoader)
        except (OSError, UnicodeError, yaml.YAMLError) as error:
            fail(path, f"invalid YAML: {error}")
        if not isinstance(document, dict):
            fail(path, "top-level document must be a mapping")
        if not isinstance(document.get("name"), str) or "on" not in document:
            fail(path, "workflow requires string name and on trigger")
        jobs = document.get("jobs")
        if not isinstance(jobs, dict) or not jobs:
            fail(path, "jobs must be a non-empty mapping")
        for job_name, job in jobs.items():
            if not isinstance(job, dict):
                fail(path, f"job {job_name} must be a mapping")
            if "uses" not in job and not isinstance(job.get("steps"), list):
                fail(path, f"job {job_name} requires steps or reusable-workflow uses")
        if path.name == "build-release.yml":
            validate_release_contract(path, jobs)
    print(f"parsed and validated {len(workflows)} GitHub Actions workflows")


if __name__ == "__main__":
    main()
