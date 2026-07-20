# @author kongweiguang

"""Independently verify a signed update manifest and every referenced artifact."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

from release_crypto import resolve_openssl


ARTIFACT_SUFFIXES = {
    "windows-x86_64": "windows-x86_64-setup.exe",
    "macos-x86_64": "macos-x86_64.dmg",
    "macos-aarch64": "macos-aarch64.dmg",
    "linux-x86_64": "linux-x86_64.AppImage",
    "linux-x86_64-deb": "linux-x86_64.deb",
}
ENVELOPE_KEYS = {"schema_version", "algorithm", "payload", "signature"}
PAYLOAD_KEYS = {
    "schema_version",
    "version",
    "published_at",
    "paused",
    "rollout_percent",
    "release_url",
    "artifacts",
}
ED25519_SPKI_PREFIX = bytes.fromhex("302a300506032b6570032100")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--public-key-base64", required=True)
    parser.add_argument("--dist", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--release-tag", required=True)
    parser.add_argument("--expected-rollout-percent", type=int)
    parser.add_argument("--expect-paused", choices=("true", "false"))
    return parser.parse_args()


def fail(message: str) -> None:
    raise SystemExit(f"update manifest verification failed: {message}")


def decode_base64(value: object, label: str) -> bytes:
    if not isinstance(value, str):
        fail(f"{label} must be a base64 string")
    try:
        return base64.b64decode(value, validate=True)
    except ValueError as error:
        fail(f"{label} is invalid base64: {error}")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as artifact:
        for chunk in iter(lambda: artifact.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_signature(payload: bytes, signature: bytes, public_key: bytes) -> None:
    if len(public_key) != 32:
        fail(f"Ed25519 public key must be 32 bytes, got {len(public_key)}")
    if len(signature) != 64:
        fail(f"Ed25519 signature must be 64 bytes, got {len(signature)}")
    with tempfile.TemporaryDirectory(prefix="gmark-update-verify-") as temporary:
        root = Path(temporary)
        payload_path = root / "payload.json"
        signature_path = root / "signature.bin"
        public_path = root / "public.der"
        payload_path.write_bytes(payload)
        signature_path.write_bytes(signature)
        public_path.write_bytes(ED25519_SPKI_PREFIX + public_key)
        result = subprocess.run(
            [
                resolve_openssl(),
                "pkeyutl",
                "-verify",
                "-rawin",
                "-pubin",
                "-inkey",
                str(public_path),
                "-keyform",
                "DER",
                "-in",
                str(payload_path),
                "-sigfile",
                str(signature_path),
            ],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if result.returncode != 0:
            fail("Ed25519 signature is invalid")


def require_exact_keys(value: object, expected: set[str], label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        fail(f"{label} must be a JSON object")
    actual = set(value)
    if actual != expected:
        fail(f"{label} keys differ: missing={sorted(expected - actual)}, unknown={sorted(actual - expected)}")
    return value


def main() -> None:
    args = parse_args()
    try:
        envelope_value = json.loads(args.manifest.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        fail(f"cannot read envelope: {error}")
    envelope = require_exact_keys(envelope_value, ENVELOPE_KEYS, "envelope")
    if envelope["schema_version"] != 1 or envelope["algorithm"] != "Ed25519":
        fail("unsupported envelope schema or algorithm")
    payload_bytes = decode_base64(envelope["payload"], "payload")
    signature = decode_base64(envelope["signature"], "signature")
    public_key = decode_base64(args.public_key_base64, "public key")
    verify_signature(payload_bytes, signature, public_key)

    try:
        payload_value = json.loads(payload_bytes)
    except (UnicodeError, json.JSONDecodeError) as error:
        fail(f"signed payload is invalid JSON: {error}")
    payload = require_exact_keys(payload_value, PAYLOAD_KEYS, "payload")
    if payload["schema_version"] != 1:
        fail("unsupported payload schema")
    if args.release_tag != f"v{args.version}" or payload["version"] != args.version:
        fail("release tag, requested version, and signed version do not match")
    if not isinstance(payload["paused"], bool):
        fail("paused must be boolean")
    rollout = payload["rollout_percent"]
    if not isinstance(rollout, int) or isinstance(rollout, bool) or not 0 <= rollout <= 100:
        fail("rollout_percent must be an integer from 0 through 100")
    if args.expected_rollout_percent is not None and rollout != args.expected_rollout_percent:
        fail("signed rollout_percent does not match the requested value")
    if args.expect_paused is not None and payload["paused"] != (args.expect_paused == "true"):
        fail("signed paused state does not match the requested value")

    expected_release = f"https://github.com/kongweiguang/gmark/releases/tag/{args.release_tag}"
    if payload["release_url"] != expected_release:
        fail("release_url is not the expected official release")
    artifacts = require_exact_keys(payload["artifacts"], set(ARTIFACT_SUFFIXES), "artifacts")
    download_root = f"https://github.com/kongweiguang/gmark/releases/download/{args.release_tag}"
    for artifact_id, suffix in ARTIFACT_SUFFIXES.items():
        entry = require_exact_keys(artifacts[artifact_id], {"url", "sha256"}, artifact_id)
        filename = f"gmark-{args.release_tag}-{suffix}"
        if entry["url"] != f"{download_root}/{filename}":
            fail(f"{artifact_id} URL is not the expected official asset")
        path = args.dist / filename
        if not path.is_file():
            fail(f"required artifact is missing: {path}")
        digest = entry["sha256"]
        if not isinstance(digest, str) or len(digest) != 64:
            fail(f"{artifact_id} sha256 is malformed")
        if sha256_file(path) != digest:
            fail(f"{artifact_id} sha256 does not match final artifact bytes")

    print("signed update manifest and all release artifacts verified")


if __name__ == "__main__":
    main()
