#!/usr/bin/env bash
# @author kongweiguang
# Build a Velopack-managed macOS bundle, compatibility archive, update package, and DMG.

set -euo pipefail

if [[ $# -ne 4 ]]; then
    echo "Usage: $0 <version> <arch: x86_64|aarch64> <output.dmg> <compat.app.tar.gz>" >&2
    exit 2
fi
VERSION="$1"
ARCH="$2"
OUTPUT="$3"
COMPAT_OUTPUT="$4"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || {
    echo "version must be exact SemVer" >&2
    exit 1
}
[[ "$ARCH" == "x86_64" || "$ARCH" == "aarch64" ]] || {
    echo "unsupported macOS architecture: $ARCH" >&2
    exit 1
}
[[ "$(uname -s)" == "Darwin" ]] || {
    echo "macOS packaging must run on macOS" >&2
    exit 1
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# A clean GitHub runner has no dist directory; create only the two requested parents before
# resolving absolute paths so packaging never depends on a previous local release.
OUTPUT_PARENT="$(dirname "$OUTPUT")"
COMPAT_PARENT="$(dirname "$COMPAT_OUTPUT")"
mkdir -p "$OUTPUT_PARENT" "$COMPAT_PARENT"
OUTPUT="$(cd "$OUTPUT_PARENT" && pwd)/$(basename "$OUTPUT")"
COMPAT_OUTPUT="$(cd "$COMPAT_PARENT" && pwd)/$(basename "$COMPAT_OUTPUT")"
OUT_DIR="$(dirname "$OUTPUT")"
STAGE="$OUT_DIR/macos-$ARCH-velopack-stage"
INPUT_APP="$STAGE/gmark.app"
VPK_OUTPUT="$STAGE/vpk"
PORTABLE_EXTRACT="$STAGE/portable"
COMPAT_ROOT="$STAGE/compat"
COMPAT_APP="$COMPAT_ROOT/gmark.app"
VOLUME="$STAGE/volume"
APPLE_VERSION="${VERSION%%-*}"
VPK="${VPK_PATH:-vpk}"
if [[ "$ARCH" == "aarch64" ]]; then
    RUNTIME="osx-arm64"
    CHANNEL="osx-arm64"
else
    RUNTIME="osx-x64"
    CHANNEL="osx-x64"
fi

# The stage path is fixed below the resolved release output so recursive cleanup cannot escape it.
rm -rf "$STAGE"
mkdir -p "$INPUT_APP/Contents/MacOS" "$INPUT_APP/Contents/Resources" "$VPK_OUTPUT" "$COMPAT_ROOT" "$VOLUME"
cp "$ROOT/target/release/gmark" "$INPUT_APP/Contents/MacOS/gmark"
cp "$ROOT/resources/macos/Info.plist" "$INPUT_APP/Contents/Info.plist"
cp "$ROOT/resources/macos/gmark.icns" "$INPUT_APP/Contents/Resources/gmark.icns"
cp "$ROOT/README.md" "$ROOT/LICENSE" "$INPUT_APP/Contents/Resources/"
chmod +x "$INPUT_APP/Contents/MacOS/gmark"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $APPLE_VERSION" "$INPUT_APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $APPLE_VERSION" "$INPUT_APP/Contents/Info.plist"

"$VPK" pack \
    --packId GMark \
    --packVersion "$VERSION" \
    --packDir "$INPUT_APP" \
    --mainExe gmark \
    --packTitle GMark \
    --packAuthors kongweiguang \
    --runtime "$RUNTIME" \
    --channel "$CHANNEL" \
    --outputDir "$VPK_OUTPUT" \
    --icon "$ROOT/resources/macos/gmark.icns" \
    --bundleId com.kongweiguang.gmark \
    --signAppIdentity - \
    --delta None \
    --noInst true \
    --yes true \
    --skip-updates true

PORTABLE="$VPK_OUTPUT/GMark-$RUNTIME-Portable.zip"
NUPKG="$VPK_OUTPUT/GMark-$VERSION-$RUNTIME-full.nupkg"
[[ -f "$PORTABLE" && -f "$NUPKG" ]] || {
    echo "Velopack did not create the macOS portable bundle and full package" >&2
    exit 1
}
mkdir -p "$PORTABLE_EXTRACT"
ditto -x -k "$PORTABLE" "$PORTABLE_EXTRACT"
FINAL_APP="$(find "$PORTABLE_EXTRACT" -maxdepth 1 -type d -name '*.app' -print -quit)"
[[ -n "$FINAL_APP" && -x "$FINAL_APP/Contents/MacOS/gmark" ]] || {
    echo "Velopack portable archive has no runnable app bundle" >&2
    exit 1
}
[[ -f "$FINAL_APP/Contents/sq.version" || -f "$FINAL_APP/Contents/Resources/sq.version" ]] || {
    echo "Velopack app manifest is missing" >&2
    exit 1
}
codesign --verify --deep --strict "$FINAL_APP"

rm -f "$COMPAT_OUTPUT"
# The compatibility archive must contain the finished Velopack bundle at one top-level gmark.app;
# using the original input path here would nest GMark.app and strand old V2 clients on migration.
ditto "$FINAL_APP" "$COMPAT_APP"
tar -czf "$COMPAT_OUTPUT" -C "$COMPAT_ROOT" gmark.app
cp "$NUPKG" "$OUT_DIR/gmark-v$VERSION-macos-$ARCH-full.nupkg"

cp -R "$FINAL_APP" "$VOLUME/gmark.app"
ln -s /Applications "$VOLUME/Applications"
rm -f "$OUTPUT"
hdiutil create -quiet -fs HFS+ -volname "Gmark" -srcfolder "$VOLUME" -format UDZO "$OUTPUT"
[[ -f "$OUTPUT" && -f "$COMPAT_OUTPUT" ]] || {
    echo "macOS release artifacts were not created" >&2
    exit 1
}
