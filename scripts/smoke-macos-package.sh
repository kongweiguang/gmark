#!/usr/bin/env bash
# @author kongweiguang
# Validate final notarized macOS app/PKG assets and exercise clean-runner install lifecycle.

set -euo pipefail

usage() {
    echo "Usage: $0 --app-archive <zip> --pkg <pkg> [--dry-run]" >&2
    exit 2
}

APP_ARCHIVE=""
PKG=""
DRY_RUN=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --app-archive) APP_ARCHIVE="${2:-}"; shift 2 ;;
        --pkg) PKG="${2:-}"; shift 2 ;;
        --dry-run) DRY_RUN=1; shift ;;
        *) usage ;;
    esac
done
[[ -f "$APP_ARCHIVE" && -f "$PKG" ]] || {
    echo "macOS app archive and PKG are required" >&2
    exit 1
}
if [[ "${GMARK_RELEASE_MODE:-}" == "production" && "$DRY_RUN" -eq 1 ]]; then
    echo "macOS dry-run is forbidden in production" >&2
    exit 1
fi

TEMPORARY="$(mktemp -d)"
INSTALLED=0
cleanup() {
    if [[ "$INSTALLED" -eq 1 ]]; then
        if [[ -L /usr/local/bin/gmark && "$(readlink /usr/local/bin/gmark)" == "/Applications/gmark.app/Contents/MacOS/gmark" ]]; then
            sudo rm -f /usr/local/bin/gmark
        fi
        sudo rm -rf /Applications/gmark.app
        sudo pkgutil --forget com.kongweiguang.gmark >/dev/null 2>&1 || true
    fi
    rm -rf "$TEMPORARY"
}
trap cleanup EXIT

if [[ "$DRY_RUN" -eq 1 ]]; then
    if command -v unzip >/dev/null 2>&1; then
        unzip -q "$APP_ARCHIVE" -d "$TEMPORARY"
    elif command -v python3 >/dev/null 2>&1; then
        python3 - "$APP_ARCHIVE" "$TEMPORARY" <<'PY'
import pathlib
import sys
import zipfile

with zipfile.ZipFile(sys.argv[1]) as archive:
    archive.extractall(pathlib.Path(sys.argv[2]))
PY
    else
        echo "dry-run requires unzip or Python 3" >&2
        exit 1
    fi
else
    [[ "$(uname -s)" == "Darwin" ]] || {
        echo "production macOS smoke must run on macOS" >&2
        exit 1
    }
    ditto -x -k "$APP_ARCHIVE" "$TEMPORARY"
fi
APP="$TEMPORARY/gmark.app"
required=(
    Contents/MacOS/gmark Contents/Info.plist Contents/Resources/gmark.icns
    Contents/Resources/README.md Contents/Resources/PRIVACY.md
    Contents/Resources/SECURITY.md Contents/Resources/LICENSE
    Contents/Resources/NOTICE
)
for relative in "${required[@]}"; do
    [[ -f "$APP/$relative" ]] || {
        echo "macOS app archive is missing $relative" >&2
        exit 1
    }
done
[[ "$DRY_RUN" -eq 1 || -x "$APP/Contents/MacOS/gmark" ]] || {
    echo "macOS app binary is not executable" >&2
    exit 1
}
if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "dry-run: macOS final app/PKG layout is valid; platform trust and install were skipped"
    exit 0
fi

codesign --verify --deep --strict --verbose=2 "$APP"
xcrun stapler validate "$APP"
spctl --assess --type execute --verbose=2 "$APP"
pkgutil --check-signature "$PKG"
xcrun stapler validate "$PKG"
spctl --assess --type install --verbose=2 "$PKG"

if [[ -e /Applications/gmark.app || -e /usr/local/bin/gmark ]]; then
    echo "clean-runner smoke refuses to overwrite a pre-existing gmark installation" >&2
    exit 1
fi
INSTALLED=1
sudo installer -pkg "$PKG" -target /
[[ -x /Applications/gmark.app/Contents/MacOS/gmark ]]
codesign --verify --deep --strict --verbose=2 /Applications/gmark.app
# 第二次安装覆盖 package upgrade/reinstall transaction；runner 必须保持同一有效签名 app。
sudo installer -pkg "$PKG" -target /
codesign --verify --deep --strict --verbose=2 /Applications/gmark.app

echo "macOS clean-runner trust/install/reinstall/cleanup smoke passed"
