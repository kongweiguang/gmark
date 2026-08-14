#!/usr/bin/env bash
# @author kongweiguang
# Build a Velopack-managed AppImage plus a package-manager-owned Debian package.

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "Usage: $0 <version> <output-directory>" >&2
    exit 2
fi
VERSION="$1"
OUT="$2"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || {
    echo "version must be exact SemVer" >&2
    exit 1
}
[[ "$(uname -s)" == "Linux" && "$(uname -m)" == "x86_64" ]] || {
    echo "Linux x86_64 packaging requires an x86_64 Linux host" >&2
    exit 1
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$(mkdir -p "$OUT" && cd "$OUT" && pwd)"
STAGE="$OUT/linux-velopack-stage"
INPUT="$STAGE/input"
VPK_OUTPUT="$STAGE/vpk"
DEBROOT="$STAGE/deb"
VPK="${VPK_PATH:-vpk}"

# The stage path is fixed beneath the resolved output root so cleanup cannot follow caller-controlled traversal.
rm -rf "$STAGE"
mkdir -p "$INPUT" "$VPK_OUTPUT"
install -Dm755 "$ROOT/target/release/gmark" "$INPUT/gmark"
install -Dm644 "$ROOT/README.md" "$INPUT/README.md"
install -Dm644 "$ROOT/LICENSE" "$INPUT/LICENSE"

"$VPK" pack \
    --packId GMark \
    --packVersion "$VERSION" \
    --packDir "$INPUT" \
    --mainExe gmark \
    --packTitle GMark \
    --packAuthors kongweiguang \
    --runtime linux-x64 \
    --channel linux-x64 \
    --outputDir "$VPK_OUTPUT" \
    --icon "$ROOT/resources/linux/icons/hicolor/512x512/apps/com.kongweiguang.gmark.png" \
    --delta None \
    --yes true \
    --skip-updates true

APPIMAGE="$VPK_OUTPUT/GMark-linux-x64.AppImage"
NUPKG="$VPK_OUTPUT/GMark-$VERSION-linux-x64-full.nupkg"
[[ -f "$APPIMAGE" && -f "$NUPKG" ]] || {
    echo "Velopack did not create the Linux AppImage and full package" >&2
    exit 1
}
install -Dm755 "$APPIMAGE" "$OUT/gmark-v$VERSION-linux-x86_64.AppImage"
install -Dm644 "$NUPKG" "$OUT/gmark-v$VERSION-linux-x86_64-full.nupkg"

# DEB remains package-manager-owned; embedding Velopack there would bypass dpkg's database and file ownership.
install -Dm755 "$ROOT/target/release/gmark" "$DEBROOT/usr/bin/gmark"
install -Dm644 "$ROOT/resources/linux/com.kongweiguang.gmark.desktop" \
    "$DEBROOT/usr/share/applications/com.kongweiguang.gmark.desktop"
install -Dm644 "$ROOT/resources/linux/icons/hicolor/256x256/apps/com.kongweiguang.gmark.png" \
    "$DEBROOT/usr/share/icons/hicolor/256x256/apps/com.kongweiguang.gmark.png"
install -Dm644 "$ROOT/resources/linux/icons/hicolor/512x512/apps/com.kongweiguang.gmark.png" \
    "$DEBROOT/usr/share/icons/hicolor/512x512/apps/com.kongweiguang.gmark.png"
for legal in README.md LICENSE; do
    install -Dm644 "$ROOT/$legal" "$DEBROOT/usr/share/doc/gmark/$legal"
done
mkdir -p "$DEBROOT/DEBIAN"
cat > "$DEBROOT/DEBIAN/control" <<EOF
Package: gmark
Version: $VERSION
Section: editors
Priority: optional
Architecture: amd64
Maintainer: kongweiguang
Description: Native Markdown and large text editor built with Rust and GPUI
EOF
dpkg-deb --root-owner-group --build "$DEBROOT" "$OUT/gmark-v$VERSION-linux-x86_64.deb"
