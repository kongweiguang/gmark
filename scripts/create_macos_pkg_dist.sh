#!/usr/bin/env bash
# @author kongweiguang
# Build, sign, and notarize the production macOS installer package.

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
RESOURCES_DIR="$PROJECT_ROOT/resources/macos"
APP_BUNDLE="gmark.app"
PKG_DIR="$DIST_DIR/pkg"
COMPONENT_PKG="gmark-component.pkg"
OUTPUT_PKG="$DIST_DIR/gmark-${VERSION}.pkg"

required=(
    resources/macos/pkg/Distribution.xml
    resources/macos/pkg/postinstall
    resources/macos/pkg/preuninstall
)
for relative in "${required[@]}"; do
    [[ -f "$PROJECT_ROOT/$relative" ]] || {
        echo "required macOS PKG input is missing: $relative" >&2
        exit 1
    }
done
if [[ "$DRY_RUN" -eq 1 ]]; then
    grep -q '__GMARK_VERSION__' "$RESOURCES_DIR/pkg/Distribution.xml"
    echo "dry-run: macOS PKG build, installer signing, notarization, staple, and install plan is valid"
    exit 0
fi

: "${GMARK_MACOS_INSTALLER_SIGNING_IDENTITY:?required}"
: "${GMARK_MACOS_NOTARY_APPLE_ID:?required}"
: "${GMARK_MACOS_NOTARY_TEAM_ID:?required}"
: "${GMARK_MACOS_NOTARY_APP_PASSWORD:?required}"

if [[ ! -d "$DIST_DIR/$APP_BUNDLE" ]]; then
    echo "Signed app bundle not found; run create_macos_app_dist.sh first" >&2
    exit 1
fi
codesign --verify --deep --strict --verbose=2 "$DIST_DIR/$APP_BUNDLE"
xcrun stapler validate "$DIST_DIR/$APP_BUNDLE"

rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/root/Applications" "$PKG_DIR/scripts"
cp -R "$DIST_DIR/$APP_BUNDLE" "$PKG_DIR/root/Applications/"
cp "$RESOURCES_DIR/pkg/postinstall" "$RESOURCES_DIR/pkg/preuninstall" "$PKG_DIR/scripts/"
chmod +x "$PKG_DIR/scripts/"*

pkgbuild --root "$PKG_DIR/root" \
    --scripts "$PKG_DIR/scripts" \
    --identifier "com.kongweiguang.gmark" \
    --version "$APPLE_VERSION" \
    --install-location "/" \
    --sign "$GMARK_MACOS_INSTALLER_SIGNING_IDENTITY" \
    "$PKG_DIR/$COMPONENT_PKG"

cp "$RESOURCES_DIR/pkg/Distribution.xml" "$PKG_DIR/Distribution.xml"
sed -i '' "s/__GMARK_VERSION__/${APPLE_VERSION}/g" "$PKG_DIR/Distribution.xml"
productbuild --distribution "$PKG_DIR/Distribution.xml" \
    --package-path "$PKG_DIR" \
    --sign "$GMARK_MACOS_INSTALLER_SIGNING_IDENTITY" \
    "$OUTPUT_PKG"

pkgutil --check-signature "$OUTPUT_PKG"
xcrun notarytool submit "$OUTPUT_PKG" \
    --apple-id "$GMARK_MACOS_NOTARY_APPLE_ID" \
    --team-id "$GMARK_MACOS_NOTARY_TEAM_ID" \
    --password "$GMARK_MACOS_NOTARY_APP_PASSWORD" \
    --wait
xcrun stapler staple "$OUTPUT_PKG"
xcrun stapler validate "$OUTPUT_PKG"
spctl --assess --type install --verbose=2 "$OUTPUT_PKG"
rm -rf "$PKG_DIR"

echo "Signed and notarized installer: $OUTPUT_PKG"
