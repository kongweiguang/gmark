#!/usr/bin/env bash
# @author kongweiguang
# Verify a final Linux portable archive and its detached GPG signature on a clean runner.

set -euo pipefail

usage() {
    echo "Usage: $0 --artifact <tar.gz> --signature <asc> --public-key <asc> --fingerprint <hex> [--dry-run|--unsigned-dev]" >&2
    exit 2
}

ARTIFACT=""
SIGNATURE=""
PUBLIC_KEY=""
FINGERPRINT=""
MODE="production"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact) ARTIFACT="${2:-}"; shift 2 ;;
        --signature) SIGNATURE="${2:-}"; shift 2 ;;
        --public-key) PUBLIC_KEY="${2:-}"; shift 2 ;;
        --fingerprint) FINGERPRINT="${2:-}"; shift 2 ;;
        --dry-run) MODE="dry-run"; shift ;;
        --unsigned-dev) MODE="unsigned-dev"; shift ;;
        *) usage ;;
    esac
done

[[ -f "$ARTIFACT" ]] || { echo "Linux archive is missing" >&2; exit 1; }
if [[ "${GMARK_RELEASE_MODE:-}" == "production" && "$MODE" != "production" ]]; then
    echo "dry-run and unsigned-dev are forbidden in production" >&2
    exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ "$MODE" == "production" ]]; then
    [[ -f "$SIGNATURE" && -f "$PUBLIC_KEY" && -n "$FINGERPRINT" ]] || {
        echo "production Linux smoke requires signature, public key, and fingerprint" >&2
        exit 1
    }
    python "$ROOT/scripts/linux-artifact-signing.py" verify \
        --artifact "$ARTIFACT" --signature "$SIGNATURE" \
        --public-key "$PUBLIC_KEY" --expected-fingerprint "$FINGERPRINT"
else
    echo "${MODE}: detached signature verification skipped explicitly"
fi

TEMPORARY="$(mktemp -d)"
trap 'rm -rf "$TEMPORARY"' EXIT

# 拒绝绝对路径和父目录跳转，避免 smoke 自身解包恶意归档。
while IFS= read -r entry; do
    normalized="${entry#./}"
    if [[ "$normalized" == /* || "$normalized" == ".." || "$normalized" == ../* ||
          "$normalized" == */../* || "$normalized" == */.. ]]; then
        echo "unsafe archive entry: $entry" >&2
        exit 1
    fi
done < <(tar -tzf "$ARTIFACT")
tar -xzf "$ARTIFACT" -C "$TEMPORARY"

required=(
    gmark README.md PRIVACY.md SECURITY.md LICENSE NOTICE
    share/applications/com.kongweiguang.gmark.desktop
    share/icons/hicolor/256x256/apps/com.kongweiguang.gmark.png
    share/icons/hicolor/512x512/apps/com.kongweiguang.gmark.png
)
for relative in "${required[@]}"; do
    [[ -f "$TEMPORARY/$relative" ]] || {
        echo "Linux archive is missing $relative" >&2
        exit 1
    }
done
[[ -x "$TEMPORARY/gmark" ]] || { echo "gmark is not executable" >&2; exit 1; }

if [[ "$MODE" != "dry-run" ]]; then
    file "$TEMPORARY/gmark" | grep -q 'ELF'
    if ldd "$TEMPORARY/gmark" 2>&1 | grep -q 'not found'; then
        echo "Linux release binary has unresolved runtime libraries" >&2
        ldd "$TEMPORARY/gmark" >&2
        exit 1
    fi
fi

echo "Linux clean-runner archive/signature/layout smoke passed"
