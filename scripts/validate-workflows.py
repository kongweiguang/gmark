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


def joined_run_scripts(steps: object) -> str:
    """集中扫描发布 shell，避免危险的覆盖参数藏在不同步骤而绕过门禁。"""

    if not isinstance(steps, list):
        return ""
    return "\n".join(
        step.get("run", "")
        for step in steps
        if isinstance(step, dict) and isinstance(step.get("run"), str)
    )


def validate_release_contract(
    path: Path, document: dict[str, object], jobs: dict[str, object]
) -> None:
    """锁定不可变发布与签名清单契约，防止失败重跑改变客户端已见版本。"""

    triggers = document.get("on")
    dispatch = triggers.get("workflow_dispatch") if isinstance(triggers, dict) else None
    inputs = dispatch.get("inputs") if isinstance(dispatch, dict) else None
    if not isinstance(inputs, dict):
        fail(path, "release workflow requires explicit dispatch inputs")
    if "rerun_failed_release" in inputs:
        fail(path, "same-version release reruns are forbidden; publish a new SemVer or RC")
    test_release = inputs.get("test_release")
    if not isinstance(test_release, dict) or test_release.get("default") is not False:
        fail(path, "test_release must be an explicit false-by-default new prerelease path")

    quality = jobs.get("quality")
    focused = jobs.get("updater-quality")
    quality_condition = quality.get("if") if isinstance(quality, dict) else None
    focused_condition = focused.get("if") if isinstance(focused, dict) else None
    if (
        not isinstance(quality_condition, str)
        or "!inputs.test_release" not in quality_condition
        or "rerun_failed_release" in quality_condition
    ):
        fail(path, "full quality gate must only be skipped for the explicit test prerelease path")
    if (
        not isinstance(focused_condition, str)
        or "inputs.test_release" not in focused_condition
        or "rerun_failed_release" in focused_condition
    ):
        fail(path, "focused updater gate must only run for the explicit test prerelease path")

    release = jobs.get("release")
    if not isinstance(release, dict):
        fail(path, "release workflow requires a release job")
    steps = release.get("steps")
    release_scripts = joined_run_scripts(steps)
    if "rerun_failed_release" in release_scripts or "RERUN_FAILED_RELEASE" in release_scripts:
        fail(path, "release steps must not expose a same-version rerun escape hatch")
    if "--clobber" in release_scripts:
        fail(path, "release steps must not clobber any published asset")
    for line in release_scripts.splitlines():
        normalized = line.strip()
        if re.search(r"\bgit\s+tag\b", normalized) and re.search(
            r"(?:^|\s)(?:-f|--force)(?:\s|$)", normalized
        ):
            fail(path, "release tags must never be rewritten with git tag -f/--force")
        if re.search(r"\bgit\s+push\b", normalized) and re.search(
            r"(?:^|\s)(?:-f|--force(?:-with-lease)?)(?:\s|$)", normalized
        ):
            fail(path, "release tags must never be force-pushed")
        if re.search(r"\bgit\s+fetch\b", normalized) and re.search(
            r"(?:^|\s)(?:-f|--force)(?:\s|$)", normalized
        ):
            fail(path, "release tag discovery must not force-update local refs")
    if re.search(r"\bgh\s+release\s+(?:edit|upload)\b", release_scripts):
        fail(path, "existing GitHub releases must not be edited or uploaded into")

    tag_step = None
    if isinstance(steps, list):
        tag_step = next(
            (
                step
                for step in steps
                if isinstance(step, dict) and step.get("name") == "Create release tag"
            ),
            None,
        )
    tag_script = tag_step.get("run") if isinstance(tag_step, dict) else None
    if (
        not isinstance(tag_script, str)
        or 'git fetch --tags origin' not in tag_script
        or 'if git rev-parse -q --verify "refs/tags/$RELEASE_TAG"' not in tag_script
        or "already exists" not in tag_script
        or "tags are immutable" not in tag_script
        or "exit 1" not in tag_script
        or 'git tag -a "$RELEASE_TAG"' not in tag_script
        or 'git push origin "$RELEASE_TAG"' not in tag_script
    ):
        fail(path, "release tags must fail closed when an existing stable or prerelease tag is found")

    publish_step = None
    if isinstance(steps, list):
        publish_step = next(
            (
                step
                for step in steps
                if isinstance(step, dict) and step.get("name") == "Publish GitHub Release"
            ),
            None,
        )
    publish_script = publish_step.get("run") if isinstance(publish_step, dict) else None
    if (
        not isinstance(publish_script, str)
        or 'if gh release view "$RELEASE_TAG"' not in publish_script
        or "already exists" not in publish_script
        or "published releases and assets are immutable" not in publish_script
        or "exit 1" not in publish_script
        or "gh release create" not in publish_script
    ):
        fail(path, "existing GitHub releases must fail closed without asset replacement")

    manifest_step = None
    if isinstance(steps, list):
        manifest_step = next(
            (
                step
                for step in steps
                if isinstance(step, dict)
                and step.get("name") == "Create and verify signed manifests"
            ),
            None,
        )
    manifest_env = manifest_step.get("env") if isinstance(manifest_step, dict) else None
    manifest_script = manifest_step.get("run") if isinstance(manifest_step, dict) else None
    if (
        not isinstance(manifest_env, dict)
        or manifest_env.get("RELEASE_NOTES") != "${{ inputs.release_notes }}"
        or not isinstance(manifest_script, str)
        or "--output dist/update-manifest.json" not in manifest_script
        or "--v2-output dist/update-manifest-v2.json" not in manifest_script
        or "verify-update-manifest.py --manifest dist/update-manifest.json" not in manifest_script
        or "verify-update-manifest-v2.py --manifest dist/update-manifest-v2.json" not in manifest_script
        or 'manifest_notes="$RELEASE_NOTES"' not in manifest_script
        or 'manifest_notes="$RELEASE_TITLE"' not in manifest_script
        or '--notes "$manifest_notes"' not in manifest_script
    ):
        fail(
            path,
            "stable releases must retain verified legacy and v2 manifests with release-note fallback",
        )

    if isinstance(steps, list):
        manifest_index = steps.index(manifest_step) if manifest_step in steps else -1
        tag_index = steps.index(tag_step) if tag_step in steps else -1
        publish_index = steps.index(publish_step) if publish_step in steps else -1
        if not (0 <= manifest_index < tag_index < publish_index):
            fail(path, "immutable tag creation must follow manifest verification and precede publishing")

    for platform in ("windows", "linux", "macos"):
        job = jobs.get(platform)
        condition = job.get("if") if isinstance(job, dict) else None
        required = (
            "always()",
            "needs.validate.result == 'success'",
            "needs.quality.result == 'skipped'",
            "needs.updater-quality.result == 'skipped'",
        )
        if not isinstance(condition, str) or any(part not in condition for part in required):
            fail(path, f"{platform} artifacts must run after skipped repeat-release gates")

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
            validate_release_contract(path, document, jobs)
    print(f"parsed and validated {len(workflows)} GitHub Actions workflows")


if __name__ == "__main__":
    main()
