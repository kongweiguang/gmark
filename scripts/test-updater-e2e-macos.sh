#!/usr/bin/env bash
# @author kongweiguang
# This static gate keeps the macOS driver honest on non-macOS development hosts.
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
DRIVER="$SCRIPT_DIR/updater-e2e-macos.sh"

fail() {
    echo "[test-updater-e2e-macos] error: $*" >&2
    exit 1
}

[[ -f "$DRIVER" ]] || fail "missing driver: $DRIVER"
bash -n "$DRIVER" || fail "driver has invalid shell syntax"

for required_text in \
    'System Events' \
    'unsaved-decision' \
    'trigger-update' \
    'Check for Updates' \
    'Download Update' \
    'Restart and Install' \
    'Keep Editing' \
    'Discard and Close' \
    'Save and Close' \
    'does not write updater protocol'; do
    grep -Fq -- "$required_text" "$DRIVER" || fail "driver is missing required real-UI contract: $required_text"
done

grep -Eq '^#!/usr/bin/env bash$' "$DRIVER" || fail "driver must be a bash script"
grep -Fq -- '@author kongweiguang' "$DRIVER" || fail "driver is missing the required author marker"

# These operations would let a test pass without the running app/helper/agent.
grep -Eq 'rm[[:space:]]+-rf|xattr[[:space:]]+-d|kill[[:space:]]+-9' "$DRIVER" && \
    fail "driver contains a destructive or Gatekeeper-bypass operation"
grep -Eq 'touch[[:space:]]+.*(ack|pid|result|lifetime)|(^|[[:space:];|&])>[[:space:]]*[^[:space:]]*(ack|pid|result|lifetime)' "$DRIVER" && \
    fail "driver appears to manufacture updater protocol markers"

if [[ "$(uname -s)" != "Darwin" ]]; then
    set +e
    output=$(bash "$DRIVER" \
        --phase unsaved-decision \
        --decision cancel \
        --pid 1 \
        --ui-check-root /tmp/gmark-updater-ui \
        --updates-root /tmp/gmark-updater-root \
        --current-binary /tmp/gmark.app/Contents/MacOS/gmark \
        --version 0.2.1 2>&1)
    status=$?
    set -e
    [[ "$status" -ne 0 ]] || fail "driver unexpectedly succeeded outside macOS"
    printf '%s\n' "$output" | grep -Fq -- 'requires Darwin' || \
        fail "non-macOS failure did not explain the Aqua/Darwin requirement"
fi

echo "[test-updater-e2e-macos] PASS: shell/static real-UI contract checks"
