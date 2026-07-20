#!/usr/bin/env bash
# @author kongweiguang
# Mount and verify the final unsigned macOS DMG without altering /Applications.

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "Usage: $0 <installer.dmg> <expected-version>" >&2
    exit 2
fi
DMG="$1"
EXPECTED="$2"
[[ -f "$DMG" ]] || { echo "DMG is missing: $DMG" >&2; exit 1; }
[[ "$(uname -s)" == "Darwin" ]] || { echo "DMG smoke must run on macOS" >&2; exit 1; }

MOUNT="$(mktemp -d)/gmark"
cleanup() {
    hdiutil detach "$MOUNT" -quiet >/dev/null 2>&1 || true
    rm -rf "$(dirname "$MOUNT")"
}
trap cleanup EXIT
mkdir -p "$MOUNT"
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$MOUNT" "$DMG"
APP="$MOUNT/gmark.app"
[[ -x "$APP/Contents/MacOS/gmark" ]] || { echo "DMG app executable is missing" >&2; exit 1; }
for relative in README.md PRIVACY.md SECURITY.md LICENSE NOTICE; do
    [[ -f "$APP/Contents/Resources/$relative" ]] || { echo "DMG is missing $relative" >&2; exit 1; }
done
codesign --verify --deep --strict "$APP"
version="$($APP/Contents/MacOS/gmark --version)"
[[ "$version" =~ (^|[[:space:]])$EXPECTED([[:space:]]|$) ]] || {
    echo "installed app version mismatch: $version" >&2
    exit 1
}
echo "macOS DMG mount/version smoke passed"
