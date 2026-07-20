# @author kongweiguang

"""Import generated pulldown-cmark spec tests into gmark's pinned JSON corpus."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


SCHEMA = 1
PULLDOWN_CMARK_VERSION = "0.13.4"
PULLDOWN_CMARK_CHECKSUM = "e9f068eba8e7071c5f9511831b44f32c740d5adf574e990f946ddb53db2f314e"
PULLDOWN_CMARK_REVISION = "38e4d08f14ec4bd9783270e9623db7681ebed968"
SUITES = {
    "commonmark": ("spec.rs", 652),
    "gfm_table": ("gfm_table.rs", 9),
    "gfm_strikethrough": ("gfm_strikethrough.rs", 3),
    "gfm_tasklist": ("gfm_tasklist.rs", 2),
}
CASE_PATTERN = re.compile(
    r"fn\s+(?P<function>[a-z0-9_]+_test_(?P<id>\d+))\(\)\s*\{\s*"
    r'let\s+original\s*=\s*r##"(?P<markdown>.*?)"##;\s*'
    r'let\s+expected\s*=\s*r##"(?P<html>.*?)"##;\s*'
    r"test_markdown_html\(original,\s*expected,\s*(?P<flags>[^)]*)\);\s*\}",
    re.DOTALL,
)


def parse_suite(path: Path, expected_count: int) -> list[dict[str, object]]:
    source = path.read_text(encoding="utf-8")
    cases = []
    for match in CASE_PATTERN.finditer(source):
        flags = [value.strip() == "true" for value in match.group("flags").split(",")]
        if len(flags) != 5:
            raise ValueError(f"{path}: expected five parser flags")
        cases.append(
            {
                "id": int(match.group("id")),
                "markdown": match.group("markdown"),
                "html": match.group("html"),
                "smart_punctuation": flags[0],
                "metadata_blocks": flags[1],
                "old_footnotes": flags[2],
                "subscript": flags[3],
                "wikilinks": flags[4],
            }
        )
    if len(cases) != expected_count:
        raise ValueError(f"{path}: expected {expected_count} cases, parsed {len(cases)}")
    return cases


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--suite-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    suites = []
    for name, (file_name, expected_count) in SUITES.items():
        suites.append(
            {
                "name": name,
                "cases": parse_suite(args.suite_dir / file_name, expected_count),
            }
        )

    corpus = {
        "schema": SCHEMA,
        "source": {
            "crate": "pulldown-cmark",
            "version": PULLDOWN_CMARK_VERSION,
            "crate_checksum": PULLDOWN_CMARK_CHECKSUM,
            "vcs_revision": PULLDOWN_CMARK_REVISION,
        },
        "suites": suites,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(corpus, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )


if __name__ == "__main__":
    main()
