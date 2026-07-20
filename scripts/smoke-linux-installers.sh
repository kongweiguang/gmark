#!/usr/bin/env bash
# @author kongweiguang
# Verify AppImage extraction and the Debian install/reinstall/uninstall lifecycle.

set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "Usage: $0 <installer.AppImage> <installer.deb> <expected-version>" >&2
    exit 2
fi
APPIMAGE="$1"
DEB="$2"
EXPECTED="$3"
[[ -f "$APPIMAGE" && -f "$DEB" ]] || { echo "Linux installers are missing" >&2; exit 1; }

TEMPORARY="$(mktemp -d)"
cleanup() {
    if dpkg-query -W -f='${Status}' gmark 2>/dev/null | grep -q 'install ok installed'; then
        sudo dpkg --purge gmark >/dev/null 2>&1 || true
    fi
    rm -rf "$TEMPORARY"
}
trap cleanup EXIT

cp "$APPIMAGE" "$TEMPORARY/gmark.AppImage"
chmod +x "$TEMPORARY/gmark.AppImage"
(cd "$TEMPORARY" && ./gmark.AppImage --appimage-extract >/dev/null)
version="$($TEMPORARY/squashfs-root/AppRun --version)"
[[ "$version" =~ (^|[[:space:]])$EXPECTED([[:space:]]|$) ]] || {
    echo "AppImage version mismatch: $version" >&2
    exit 1
}

sudo dpkg -i "$DEB"
version="$(/usr/bin/gmark --version)"
[[ "$version" =~ (^|[[:space:]])$EXPECTED([[:space:]]|$) ]] || {
    echo "Debian package version mismatch: $version" >&2
    exit 1
}
# Reinstall exercises the same path used by package-manager upgrades.
sudo dpkg -i "$DEB"
sudo dpkg --purge gmark
[[ ! -e /usr/bin/gmark ]] || { echo "Debian uninstall left /usr/bin/gmark" >&2; exit 1; }
echo "Linux AppImage and Debian install lifecycle smoke passed"
