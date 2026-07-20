# @author kongweiguang

"""Fail when a RustSec exception escapes its reviewed build-time dependency topology."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def fail(message: str) -> None:
    print(f"audit exception verification failed: {message}", file=sys.stderr)
    raise SystemExit(1)


metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
    )
)
packages = {package["id"]: package for package in metadata["packages"]}
nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}


def package_id(name: str, version: str) -> str:
    matches = [
        package["id"]
        for package in packages.values()
        if package["name"] == name and package["version"] == version
    ]
    if len(matches) != 1:
        fail(f"expected exactly one {name} {version}, found {len(matches)}")
    return matches[0]


def direct_parents(child_id: str) -> set[tuple[str, str]]:
    return {
        (packages[node_id]["name"], packages[node_id]["version"])
        for node_id, node in nodes.items()
        if child_id in node["dependencies"]
    }


quick_xml_030 = package_id("quick-xml", "0.30.0")
quick_xml_039 = package_id("quick-xml", "0.39.4")
if direct_parents(quick_xml_030) != {("xcb", "1.7.0")}:
    fail("quick-xml 0.30.0 is no longer isolated to xcb 1.7.0")
if direct_parents(quick_xml_039) != {("wayland-scanner", "0.31.10")}:
    fail("quick-xml 0.39.4 is no longer isolated to wayland-scanner 0.31.10")

xcb = packages[package_id("xcb", "1.7.0")]
xcb_quick_xml = [dependency for dependency in xcb["dependencies"] if dependency["name"] == "quick-xml"]
if len(xcb_quick_xml) != 1 or xcb_quick_xml[0]["kind"] != "build":
    fail("xcb quick-xml dependency is no longer build-only")

scanner = packages[package_id("wayland-scanner", "0.31.10")]
target_kinds = {kind for target in scanner["targets"] for kind in target["kind"]}
if target_kinds != {"proc-macro"}:
    fail(f"wayland-scanner target kinds changed: {sorted(target_kinds)}")

print("RustSec exceptions remain restricted to reviewed build-time XML parsers")
