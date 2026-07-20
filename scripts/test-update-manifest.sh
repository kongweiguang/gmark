#!/usr/bin/env bash
# @author kongweiguang
# Cross-language smoke test for the Python/OpenSSL update-manifest signer.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPORARY="$(mktemp -d)"
trap 'rm -rf "$TEMPORARY"' EXIT
mkdir -p "$TEMPORARY/dist"

if [[ -z "${PYTHON:-}" ]]; then
    if command -v python >/dev/null 2>&1; then
        PYTHON=python
    else
        PYTHON=python3
    fi
fi

for suffix in \
    windows-x86_64-setup.exe \
    macos-x86_64.dmg macos-aarch64.dmg \
    linux-x86_64.AppImage linux-x86_64.deb; do
    printf 'artifact:%s\n' "$suffix" > "$TEMPORARY/dist/gmark-v0.1.0-$suffix"
done

openssl genpkey -algorithm Ed25519 -out "$TEMPORARY/private.pem"
openssl pkey -in "$TEMPORARY/private.pem" -pubout -outform DER \
    -out "$TEMPORARY/public.der"
PUBLIC_KEY_BASE64="$(tail -c 32 "$TEMPORARY/public.der" | base64 | tr -d '\r\n')"

"$PYTHON" "$ROOT/scripts/create-update-manifest.py" \
    --version 0.1.0 \
    --release-tag v0.1.0 \
    --dist "$TEMPORARY/dist" \
    --private-key "$TEMPORARY/private.pem" \
    --public-key-base64 "$PUBLIC_KEY_BASE64" \
    --output "$TEMPORARY/update-manifest.json" \
    --rollout-percent 25

"$PYTHON" "$ROOT/scripts/verify-update-manifest.py" \
    --manifest "$TEMPORARY/update-manifest.json" \
    --public-key-base64 "$PUBLIC_KEY_BASE64" \
    --dist "$TEMPORARY/dist" \
    --version 0.1.0 \
    --release-tag v0.1.0 \
    --expected-rollout-percent 25 \
    --expect-paused false

"$PYTHON" - "$TEMPORARY/update-manifest.json" <<'PY'
import base64
import hashlib
import json
import pathlib
import sys

envelope = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert envelope["schema_version"] == 1
assert envelope["algorithm"] == "Ed25519"
assert len(base64.b64decode(envelope["signature"], validate=True)) == 64
payload = json.loads(base64.b64decode(envelope["payload"], validate=True))
assert payload["version"] == "0.1.0"
assert payload["rollout_percent"] == 25
assert set(payload["artifacts"]) == {
    "windows-x86_64", "macos-x86_64", "macos-aarch64",
    "linux-x86_64", "linux-x86_64-deb",
}
for artifact in payload["artifacts"].values():
    assert len(artifact["sha256"]) == 64
    int(artifact["sha256"], 16)
PY

WRONG_KEY="$("$PYTHON" -c 'import base64; print(base64.b64encode(bytes(32)).decode())')"
if "$PYTHON" "$ROOT/scripts/create-update-manifest.py" \
    --version 0.1.0 \
    --release-tag v0.1.0 \
    --dist "$TEMPORARY/dist" \
    --private-key "$TEMPORARY/private.pem" \
    --public-key-base64 "$WRONG_KEY" \
    --output "$TEMPORARY/invalid.json" >/dev/null 2>&1; then
    echo "manifest signer accepted a mismatched public key" >&2
    exit 1
fi

cp "$TEMPORARY/dist/gmark-v0.1.0-linux-x86_64.AppImage" \
    "$TEMPORARY/original.AppImage"
printf 'tampered\n' >> "$TEMPORARY/dist/gmark-v0.1.0-linux-x86_64.AppImage"
if "$PYTHON" "$ROOT/scripts/verify-update-manifest.py" \
    --manifest "$TEMPORARY/update-manifest.json" \
    --public-key-base64 "$PUBLIC_KEY_BASE64" \
    --dist "$TEMPORARY/dist" \
    --version 0.1.0 \
    --release-tag v0.1.0 >/dev/null 2>&1; then
    echo "manifest verifier accepted a tampered artifact" >&2
    exit 1
fi
mv "$TEMPORARY/original.AppImage" \
    "$TEMPORARY/dist/gmark-v0.1.0-linux-x86_64.AppImage"

"$PYTHON" "$ROOT/scripts/create-update-manifest.py" \
    --version 0.1.0 \
    --release-tag v0.1.0 \
    --dist "$TEMPORARY/dist" \
    --private-key "$TEMPORARY/private.pem" \
    --public-key-base64 "$PUBLIC_KEY_BASE64" \
    --output "$TEMPORARY/paused-update-manifest.json" \
    --rollout-percent 0 \
    --paused
"$PYTHON" "$ROOT/scripts/verify-update-manifest.py" \
    --manifest "$TEMPORARY/paused-update-manifest.json" \
    --public-key-base64 "$PUBLIC_KEY_BASE64" \
    --dist "$TEMPORARY/dist" \
    --version 0.1.0 \
    --release-tag v0.1.0 \
    --expected-rollout-percent 0 \
    --expect-paused true

echo "update-manifest signer smoke passed"
