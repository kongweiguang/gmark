#!/usr/bin/env bash
# @author kongweiguang
# Unsigned development fixture for Linux archive layout and traversal checks.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPORARY="$(mktemp -d)"
trap 'rm -rf "$TEMPORARY"' EXIT
PACKAGE="$TEMPORARY/package"
mkdir -p "$PACKAGE/share/applications"
mkdir -p "$PACKAGE/share/icons/hicolor/256x256/apps"
mkdir -p "$PACKAGE/share/icons/hicolor/512x512/apps"
printf '#!/bin/sh\nexit 0\n' > "$PACKAGE/gmark"
chmod +x "$PACKAGE/gmark"
for file in README.md PRIVACY.md SECURITY.md LICENSE NOTICE; do
    printf '%s\n' "$file" > "$PACKAGE/$file"
done
printf '[Desktop Entry]\n' > "$PACKAGE/share/applications/com.kongweiguang.gmark.desktop"
printf 'png\n' > "$PACKAGE/share/icons/hicolor/256x256/apps/com.kongweiguang.gmark.png"
printf 'png\n' > "$PACKAGE/share/icons/hicolor/512x512/apps/com.kongweiguang.gmark.png"
ARCHIVE="$TEMPORARY/gmark-v0.1.0-linux-x86_64.tar.gz"
tar -C "$PACKAGE" -czf "$ARCHIVE" .

bash "$ROOT/scripts/smoke-linux-package.sh" --artifact "$ARCHIVE" \
    --signature "$TEMPORARY/missing.asc" --public-key "$TEMPORARY/missing-key.asc" \
    --fingerprint 0000000000000000000000000000000000000000 --dry-run

if GMARK_RELEASE_MODE=production bash "$ROOT/scripts/smoke-linux-package.sh" \
    --artifact "$ARCHIVE" --signature "$TEMPORARY/missing.asc" \
    --public-key "$TEMPORARY/missing-key.asc" \
    --fingerprint 0000000000000000000000000000000000000000 \
    --unsigned-dev >/dev/null 2>&1; then
    echo "production Linux smoke accepted unsigned-dev" >&2
    exit 1
fi

echo "Linux package smoke dry-run tests passed"
