#!/usr/bin/env bash
# @author kongweiguang
# Build an unsigned macOS application bundle and place it in an installable DMG.

set -euo pipefail

if [[ $# -ne 4 ]]; then
    echo "Usage: $0 <version> <arch: x86_64|aarch64> <output.dmg> <updater.app.tar.gz>" >&2
    exit 2
fi
VERSION="$1"
ARCH="$2"
OUTPUT="$3"
UPDATER_OUTPUT="$4"
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
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Helpers" "$APP/Contents/Resources" "$VOLUME"
cp "$ROOT/target/release/gmark" "$APP/Contents/MacOS/gmark"
cp "$ROOT/target/release/gmark-update-helper" "$APP/Contents/Helpers/gmark-update-helper"
cp "$ROOT/target/release/gmark-update-agent" "$APP/Contents/Helpers/gmark-update-agent"
cp "$ROOT/resources/macos/Info.plist" "$APP/Contents/Info.plist"
cp "$ROOT/resources/macos/gmark.icns" "$APP/Contents/Resources/gmark.icns"
cp "$ROOT/README.md" "$ROOT/LICENSE" "$APP/Contents/Resources/"
chmod +x "$APP/Contents/MacOS/gmark"
chmod +x "$APP/Contents/Helpers/gmark-update-helper"
chmod +x "$APP/Contents/Helpers/gmark-update-agent"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $APPLE_VERSION" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $APPLE_VERSION" "$APP/Contents/Info.plist"

# Ad-hoc signing seals the bundle bytes but provides no Apple trust identity.
# Users must approve this build manually because the project has no Developer ID.
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"

mkdir -p "$(dirname "$UPDATER_OUTPUT")"
rm -f "$UPDATER_OUTPUT"
tar -czf "$UPDATER_OUTPUT" -C "$STAGE" gmark.app
[[ -f "$UPDATER_OUTPUT" ]] || { echo "macOS updater archive was not created" >&2; exit 1; }

cp -R "$APP" "$VOLUME/gmark.app"
ln -s /Applications "$VOLUME/Applications"
mkdir -p "$(dirname "$OUTPUT")"
rm -f "$OUTPUT"
hdiutil create -quiet -fs HFS+ -volname "Gmark" -srcfolder "$VOLUME" -format UDZO "$OUTPUT"
[[ -f "$OUTPUT" ]] || { echo "DMG was not created" >&2; exit 1; }
