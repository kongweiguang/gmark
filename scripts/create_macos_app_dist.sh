#!/usr/bin/env bash
# @author kongweiguang
# Build, sign, and notarize the production macOS app bundle.

set -euo pipefail

DRY_RUN=0
if [[ $# -eq 2 && "$1" == "--dry-run" ]]; then
    DRY_RUN=1
    VERSION="$2"
elif [[ $# -eq 1 ]]; then
    VERSION="$1"
else
    echo "Usage: $0 [--dry-run] <semver>" >&2
    exit 1
fi
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
    echo "version must be exact SemVer" >&2
    exit 1
fi
if [[ "${GMARK_RELEASE_MODE:-}" == "production" && "$DRY_RUN" -eq 1 ]]; then
    echo "macOS dry-run is forbidden in production" >&2
    exit 1
fi

APPLE_VERSION="${VERSION%%-*}"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$PROJECT_ROOT/dist"
APP_DIR="$DIST_DIR/gmark.app"

required=(
    resources/macos/Info.plist resources/macos/gmark.icns
    README.md PRIVACY.md SECURITY.md LICENSE NOTICE
)
for relative in "${required[@]}"; do
    [[ -f "$PROJECT_ROOT/$relative" ]] || {
        echo "required macOS app input is missing: $relative" >&2
        exit 1
    }
done
if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "dry-run: macOS app layout, codesign, notarization, staple, and Gatekeeper plan is valid"
    exit 0
fi

: "${GMARK_MACOS_APP_SIGNING_IDENTITY:?required}"
: "${GMARK_MACOS_NOTARY_APPLE_ID:?required}"
: "${GMARK_MACOS_NOTARY_TEAM_ID:?required}"
: "${GMARK_MACOS_NOTARY_APP_PASSWORD:?required}"

rm -rf "$DIST_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

cd "$PROJECT_ROOT"
cargo build --release --locked
cp target/release/gmark "$APP_DIR/Contents/MacOS/gmark"
cp resources/macos/Info.plist "$APP_DIR/Contents/Info.plist"
cp resources/macos/gmark.icns "$APP_DIR/Contents/Resources/gmark.icns"
cp README.md PRIVACY.md SECURITY.md LICENSE NOTICE "$APP_DIR/Contents/Resources/"
chmod +x "$APP_DIR/Contents/MacOS/gmark"

/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $APPLE_VERSION" \
    "$APP_DIR/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $APPLE_VERSION" \
    "$APP_DIR/Contents/Info.plist"

codesign --force --options runtime --timestamp \
    --sign "$GMARK_MACOS_APP_SIGNING_IDENTITY" "$APP_DIR"
codesign --verify --deep --strict --verbose=2 "$APP_DIR"

NOTARIZATION_ZIP="$DIST_DIR/gmark-notarization.zip"
ditto -c -k --sequesterRsrc --keepParent "$APP_DIR" "$NOTARIZATION_ZIP"
xcrun notarytool submit "$NOTARIZATION_ZIP" \
    --apple-id "$GMARK_MACOS_NOTARY_APPLE_ID" \
    --team-id "$GMARK_MACOS_NOTARY_TEAM_ID" \
    --password "$GMARK_MACOS_NOTARY_APP_PASSWORD" \
    --wait
xcrun stapler staple "$APP_DIR"
xcrun stapler validate "$APP_DIR"
spctl --assess --type execute --verbose=2 "$APP_DIR"
rm -f "$NOTARIZATION_ZIP"

echo "Signed and notarized app: $APP_DIR"
