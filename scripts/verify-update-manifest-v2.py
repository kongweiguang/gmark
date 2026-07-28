# @author kongweiguang

"""Fail-closed verifier for the signed production updater v2 manifest."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

from release_crypto import resolve_openssl


ARTIFACTS = {
    "windows-x86_64": ("windows-x86_64-setup.exe", "windows-setup-exe"),
    "macos-x86_64": ("macos-x86_64.app.tar.gz", "macos-app-tar-gz"),
    "macos-aarch64": ("macos-aarch64.app.tar.gz", "macos-app-tar-gz"),
    "linux-x86_64": ("linux-x86_64.AppImage", "linux-app-image"),
}


def fail(message: str) -> None:
    raise SystemExit(f"v2 update manifest: {message}")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def exact_object(value: object, keys: set[str], label: str) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != keys:
        fail(f"{label} has missing or unknown fields")
    return value


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--public-key-base64", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--release-tag", required=True)
    parser.add_argument("--dist", type=Path, required=True)
    parser.add_argument("--expected-rollout-percent", type=int, required=True)
    parser.add_argument("--expect-paused", choices=("true", "false"), required=True)
    return parser.parse_args()


def verify_signature(payload: bytes, signature: bytes, key: bytes) -> None:
    if len(key) != 32 or len(signature) != 64:
        fail("Ed25519 key or signature has the wrong length")
    spki = bytes.fromhex("302a300506032b6570032100") + key
    with tempfile.TemporaryDirectory(prefix="gmark-update-v2-verify-") as temporary:
        root = Path(temporary)
        (root / "payload").write_bytes(payload)
        (root / "signature").write_bytes(signature)
        (root / "public.der").write_bytes(spki)
        result = subprocess.run(
            [
                resolve_openssl(), "pkeyutl", "-verify", "-rawin", "-pubin",
                "-keyform", "DER", "-inkey", str(root / "public.der"),
                "-in", str(root / "payload"), "-sigfile", str(root / "signature"),
            ],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if result.returncode != 0:
            fail("Ed25519 signature verification failed")


def main() -> None:
    args = parse_args()
    envelope = exact_object(
        json.loads(args.manifest.read_text(encoding="utf-8")),
        {"schema_version", "algorithm", "payload", "signature"},
        "envelope",
    )
    if envelope["schema_version"] != 1 or envelope["algorithm"] != "Ed25519":
        fail("unsupported envelope format")
    try:
        payload_bytes = base64.b64decode(str(envelope["payload"]), validate=True)
        signature = base64.b64decode(str(envelope["signature"]), validate=True)
        public_key = base64.b64decode(args.public_key_base64, validate=True)
    except ValueError as error:
        fail(f"invalid base64: {error}")
    verify_signature(payload_bytes, signature, public_key)
    payload = exact_object(
        json.loads(payload_bytes),
        {
            "schema_version", "channel", "version", "published_at", "notes", "paused",
            "rollout_percent", "release_url", "artifacts",
        },
        "payload",
    )
    if payload["schema_version"] != 2 or payload["channel"] != "stable":
        fail("payload must use stable schema v2")
    if payload["version"] != args.version or args.release_tag != f"v{args.version}":
        fail("release identity mismatch")
    if payload["rollout_percent"] != args.expected_rollout_percent:
        fail("rollout percent mismatch")
    if payload["paused"] != (args.expect_paused == "true"):
        fail("paused state mismatch")
    artifacts = exact_object(payload["artifacts"], set(ARTIFACTS), "artifacts")
    release_base = f"https://github.com/kongweiguang/gmark/releases/download/{args.release_tag}"
    for artifact_id, (suffix, package_format) in ARTIFACTS.items():
        entry = exact_object(
            artifacts[artifact_id],
            {"url", "size", "sha256", "format", "system_trust"},
            f"artifact {artifact_id}",
        )
        filename = f"gmark-{args.release_tag}-{suffix}"
        path = args.dist / filename
        if not path.is_file():
            fail(f"artifact is missing: {path}")
        if entry["url"] != f"{release_base}/{filename}":
            fail(f"artifact URL mismatch: {artifact_id}")
        if entry["size"] != path.stat().st_size or entry["sha256"] != sha256_file(path):
            fail(f"artifact bytes mismatch: {artifact_id}")
        if entry["format"] != package_format:
            fail(f"artifact format mismatch: {artifact_id}")
        if entry["system_trust"] not in {
            "unsigned", "authenticode", "developer-id-notarized", "not-applicable"
        }:
            fail(f"invalid system trust: {artifact_id}")

    print("signed v2 update manifest verified")


if __name__ == "__main__":
    main()
