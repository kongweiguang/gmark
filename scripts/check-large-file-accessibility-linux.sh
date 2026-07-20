#!/usr/bin/env bash
# @author kongweiguang

set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: $0 <gmark-binary> <fixture> <output-json>" >&2
    exit 2
fi

app="$(realpath "$1")"
fixture="$(realpath "$2")"
output="$(realpath -m "$3")"
output_dir="$(dirname "$output")"
config_root="$output_dir/config"

[[ -x "$app" ]] || { echo "gmark binary is not executable: $app" >&2; exit 2; }
[[ -f "$fixture" ]] || { echo "fixture is not a file: $fixture" >&2; exit 2; }
mkdir -p "$output_dir" "$config_root"
[[ ! -e "$output" ]] || { echo "refusing to overwrite: $output" >&2; exit 2; }

gdbus call --session \
    --dest org.a11y.Bus \
    --object-path /org/a11y/bus \
    --method org.freedesktop.DBus.Properties.Set \
    org.a11y.Status IsEnabled '<true>' >/dev/null

GMARK_UI_CHECK_CONFIG_ROOT="$config_root" \
GMARK_SOAK_READY_PATH="$output_dir/ready.json" \
GMARK_SOAK_MODE="linux-at-spi-check" \
"$app" "$fixture" \
    >"$output_dir/stdout.log" 2>"$output_dir/stderr.log" &
app_pid=$!
cleanup() {
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

python3 - "$output" "$fixture" <<'PY'
import json
import os
import sys
import time

import pyatspi

output, fixture = sys.argv[1:]


def children(node):
    for index in range(node.childCount):
        yield node.getChildAtIndex(index)


def walk(node):
    yield node
    for child in children(node):
        yield from walk(child)


def snapshot(node):
    result = []
    for item in walk(node):
        try:
            name = item.name or ""
        except Exception:
            name = ""
        try:
            role = item.getRoleName()
        except Exception:
            role = "unknown"
        try:
            interfaces = [str(interface) for interface in item.get_interfaces()]
        except Exception:
            interfaces = []
        actions = []
        try:
            interface = item.queryAction()
            actions = [interface.getName(index) for index in range(interface.nActions)]
        except Exception:
            pass
        result.append({
            "name": name,
            "role": role,
            "actions": actions,
            "interfaces": interfaces,
        })
    return result


def current_application():
    desktop = pyatspi.Registry.getDesktop(0)
    for application in children(desktop):
        nodes = snapshot(application)
        names = {node["name"] for node in nodes}
        if "Source editor" in names and os.path.basename(fixture) in names:
            return application
    return None


def find_application():
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        application = current_application()
        if application is not None:
            return application
        time.sleep(0.1)
    raise RuntimeError("gmark did not publish its initial AT-SPI editor tree within 30 seconds")


def find_named(root, name):
    for node in walk(root):
        try:
            if node.name == name:
                return node
        except Exception:
            pass
    raise RuntimeError(f"AT-SPI node missing: {name}")


def invoke(root, name):
    node = find_named(root, name)
    interface = node.queryAction()
    for index in range(interface.nActions):
        if interface.getName(index).lower() in {"click", "press", "invoke"}:
            if not interface.doAction(index):
                raise RuntimeError(f"AT-SPI action failed: {name}")
            return
    raise RuntimeError(f"AT-SPI node has no invokable action: {name}")


def wait_for_name(name):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        application = current_application()
        if application is not None:
            current = snapshot(application)
            if name in {node["name"] for node in current}:
                return application, current
        time.sleep(0.1)
    raise RuntimeError(f"AT-SPI action did not expose: {name}")


application = find_application()
before = snapshot(application)
required = {
    os.path.basename(fixture),
    "Source editor",
    "Source",
    "Document status",
    "Save",
    "Find",
    "Go to line",
}
names = {node["name"] for node in before}
missing = sorted(required - names)
if missing:
    raise RuntimeError(f"required AT-SPI nodes missing: {missing}")

invoke(application, "Find")
application, after_find = wait_for_name("Find in document")

# AT-SPI actions force one real application frame on headless/X11 runners. Read the
# refreshed tree so a completed background viewport cannot remain hidden behind the
# adapter's initial pre-index snapshot.
source = find_named(application, "Source editor")
visible_chunks = []
source_character_count = 0
for node in walk(source):
    try:
        text = node.queryText()
        count = text.characterCount
        source_character_count += count
        if sum(len(chunk) for chunk in visible_chunks) < 4096:
            visible_chunks.append(text.getText(0, min(count, 4096)))
    except Exception:
        pass
visible_text = "".join(visible_chunks)[:4096]
if "生产报告" not in visible_text:
    with open(output + ".diagnostic.json", "x", encoding="utf-8") as handle:
        json.dump(after_find, handle, ensure_ascii=False, indent=2)
        handle.write("\n")
    raise RuntimeError("AT-SPI Text interface did not expose viewport text")

invoke(application, "Go to line")
application, after_go_to_line = wait_for_name("Go to line or byte")

document = {
    "schema_version": 1,
    "platform": "linux-at-spi",
    "fixture": fixture,
    "node_count": len(before),
    "source_character_count": source_character_count,
    "source_text_prefix": visible_text[:512],
    "find_invoked": True,
    "go_to_line_invoked": True,
    "before": before,
    "after_find": after_find,
    "after_go_to_line": after_go_to_line,
}
with open(output, "x", encoding="utf-8") as handle:
    json.dump(document, handle, ensure_ascii=False, indent=2)
    handle.write("\n")
print(json.dumps({key: document[key] for key in (
    "schema_version",
    "platform",
    "fixture",
    "node_count",
    "source_character_count",
    "find_invoked",
    "go_to_line_invoked",
)}, ensure_ascii=False, indent=2))
PY
