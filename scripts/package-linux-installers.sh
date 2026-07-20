#!/usr/bin/env bash
# @author kongweiguang
# Create a portable AppImage and a native Debian installer from one release binary.

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
STAGE="$ROOT/dist/linux-package"
APPDIR="$STAGE/gmark.AppDir"
DEBROOT="$STAGE/deb"
mkdir -p "$OUT"
rm -rf "$STAGE"

install -Dm755 "$ROOT/target/release/gmark" "$APPDIR/usr/bin/gmark"
install -Dm644 "$ROOT/resources/linux/com.kongweiguang.gmark.desktop" \
    "$APPDIR/usr/share/applications/com.kongweiguang.gmark.desktop"
install -Dm644 "$ROOT/resources/linux/icons/hicolor/256x256/apps/com.kongweiguang.gmark.png" \
    "$APPDIR/usr/share/icons/hicolor/256x256/apps/com.kongweiguang.gmark.png"
install -Dm644 "$ROOT/resources/linux/icons/hicolor/512x512/apps/com.kongweiguang.gmark.png" \
    "$APPDIR/usr/share/icons/hicolor/512x512/apps/com.kongweiguang.gmark.png"
ln -s usr/bin/gmark "$APPDIR/AppRun"
ln -s usr/share/applications/com.kongweiguang.gmark.desktop "$APPDIR/gmark.desktop"
ln -s usr/share/icons/hicolor/512x512/apps/com.kongweiguang.gmark.png "$APPDIR/com.kongweiguang.gmark.png"
for legal in README.md PRIVACY.md SECURITY.md LICENSE NOTICE; do
    install -Dm644 "$ROOT/$legal" "$APPDIR/usr/share/doc/gmark/$legal"
done

APPIMAGETOOL="$STAGE/appimagetool.AppImage"
APPIMAGETOOL_SHA256="a6d71e2b6cd66f8e8d16c37ad164658985e0cf5fcaa950c90a482890cb9d13e0"
curl --fail --location --retry 3 \
    https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage \
    --output "$APPIMAGETOOL"
echo "$APPIMAGETOOL_SHA256  $APPIMAGETOOL" | sha256sum --check --status || {
    echo "appimagetool checksum mismatch" >&2
    exit 1
}
chmod +x "$APPIMAGETOOL"
ARCH=x86_64 "$APPIMAGETOOL" --appimage-extract-and-run "$APPDIR" \
    "$OUT/gmark-v$VERSION-linux-x86_64.AppImage"
chmod +x "$OUT/gmark-v$VERSION-linux-x86_64.AppImage"

install -Dm755 "$ROOT/target/release/gmark" "$DEBROOT/usr/bin/gmark"
cp -a "$APPDIR/usr/share/." "$DEBROOT/usr/share/"
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
