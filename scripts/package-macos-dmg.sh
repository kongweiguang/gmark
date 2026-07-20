#!/usr/bin/env bash
# @author kongweiguang
# Build an unsigned macOS application bundle and place it in an installable DMG.

set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "Usage: $0 <version> <arch: x86_64|aarch64> <output.dmg>" >&2
    exit 2
fi
VERSION="$1"
ARCH="$2"
OUTPUT="$3"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || {
    echo "version must be exact SemVer" >&2
    exit 1
}
[[ "$ARCH" == "x86_64" || "$ARCH" == "aarch64" ]] || {
    echo "unsupported macOS architecture: $ARCH" >&2
    exit 1
}
[[ "$(uname -s)" == "Darwin" ]] || {
    echo "macOS DMG packaging must run on macOS" >&2
    exit 1
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAGE="$ROOT/dist/macos-$ARCH"
APP="$STAGE/gmark.app"
VOLUME="$STAGE/volume"
APPLE_VERSION="${VERSION%%-*}"

rm -rf "$STAGE"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$VOLUME"
cp "$ROOT/target/release/gmark" "$APP/Contents/MacOS/gmark"
cp "$ROOT/resources/macos/Info.plist" "$APP/Contents/Info.plist"
cp "$ROOT/resources/macos/gmark.icns" "$APP/Contents/Resources/gmark.icns"
cp "$ROOT/README.md" "$ROOT/LICENSE" "$APP/Contents/Resources/"
chmod +x "$APP/Contents/MacOS/gmark"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $APPLE_VERSION" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $APPLE_VERSION" "$APP/Contents/Info.plist"

# Ad-hoc signing seals the bundle bytes but provides no Apple trust identity.
# Users must approve this build manually because the project has no Developer ID.
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"

cp -R "$APP" "$VOLUME/gmark.app"
ln -s /Applications "$VOLUME/Applications"
mkdir -p "$(dirname "$OUTPUT")"
rm -f "$OUTPUT"
hdiutil create -quiet -fs HFS+ -volname "gmark" -srcfolder "$VOLUME" -format UDZO "$OUTPUT"
[[ -f "$OUTPUT" ]] || { echo "DMG was not created" >&2; exit 1; }
