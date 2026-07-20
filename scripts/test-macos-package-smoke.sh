#!/usr/bin/env bash
# @author kongweiguang
# Cross-platform dry-run fixture for the macOS final-package smoke script.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPORARY="$(mktemp -d)"
trap 'rm -rf "$TEMPORARY"' EXIT
APP="$TEMPORARY/gmark.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
printf '#!/bin/sh\nexit 0\n' > "$APP/Contents/MacOS/gmark"
chmod +x "$APP/Contents/MacOS/gmark"
printf 'plist\n' > "$APP/Contents/Info.plist"
printf 'icon\n' > "$APP/Contents/Resources/gmark.icns"
for file in README.md PRIVACY.md SECURITY.md LICENSE NOTICE; do
    printf '%s\n' "$file" > "$APP/Contents/Resources/$file"
done
if command -v zip >/dev/null 2>&1; then
    (cd "$TEMPORARY" && zip -qr gmark.zip gmark.app)
elif command -v python3 >/dev/null 2>&1; then
    python3 - "$TEMPORARY" <<'PY'
import pathlib
import sys
import zipfile

root = pathlib.Path(sys.argv[1])
with zipfile.ZipFile(root / "gmark.zip", "w", zipfile.ZIP_DEFLATED) as archive:
    for path in (root / "gmark.app").rglob("*"):
        if path.is_file():
            archive.write(path, path.relative_to(root))
PY
else
    echo "test requires zip or Python 3" >&2
    exit 1
fi
printf 'unsigned development pkg fixture\n' > "$TEMPORARY/gmark.pkg"

bash "$ROOT/scripts/smoke-macos-package.sh" \
    --app-archive "$TEMPORARY/gmark.zip" --pkg "$TEMPORARY/gmark.pkg" --dry-run

if GMARK_RELEASE_MODE=production bash "$ROOT/scripts/smoke-macos-package.sh" \
    --app-archive "$TEMPORARY/gmark.zip" --pkg "$TEMPORARY/gmark.pkg" \
    --dry-run >/dev/null 2>&1; then
    echo "production macOS smoke accepted dry-run" >&2
    exit 1
fi

echo "macOS package smoke dry-run tests passed"
