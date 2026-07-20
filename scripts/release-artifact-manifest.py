# @author kongweiguang

"""Create or verify the signed release-artifact manifest used by release CI."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import subprocess
import tempfile
from datetime import datetime, timezone
from pathlib import Path

from release_crypto import resolve_openssl


ARTIFACT_SUFFIXES = {
    "windows-x86_64": (
        "windows",
        "x86_64",
        "inno-setup-exe",
        "windows-x86_64-setup.exe",
    ),
    "macos-x86_64": ("macos", "x86_64", "unsigned-dmg", "macos-x86_64.dmg"),
    "macos-aarch64": ("macos", "aarch64", "unsigned-dmg", "macos-aarch64.dmg"),
    "linux-x86_64": ("linux", "x86_64", "appimage", "linux-x86_64.AppImage"),
    "linux-x86_64-deb": ("linux", "x86_64", "deb", "linux-x86_64.deb"),
}
SEMVER = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$")
ED25519_SPKI_PREFIX = bytes.fromhex("302a300506032b6570032100")


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    subparsers = root.add_subparsers(dest="command", required=True)
    create = subparsers.add_parser("create")
    create.add_argument("--version", required=True)
    create.add_argument("--release-tag", required=True)
    create.add_argument("--channel", choices=("stable", "beta", "nightly"), required=True)
    create.add_argument("--rollout-percent", type=int, required=True)
    create.add_argument("--paused", action="store_true")
    create.add_argument("--dist", type=Path, required=True)
    create.add_argument("--output", type=Path, required=True)
    create.add_argument("--private-key", type=Path)
    create.add_argument("--public-key-base64")
    create.add_argument("--unsigned-dev", action="store_true")
    create.add_argument("--published-at")

    verify = subparsers.add_parser("verify")
    verify.add_argument("--manifest", type=Path, required=True)
    verify.add_argument("--dist", type=Path, required=True)
    verify.add_argument("--version", required=True)
    verify.add_argument("--release-tag", required=True)
    verify.add_argument("--channel", choices=("stable", "beta", "nightly"), required=True)
    verify.add_argument("--rollout-percent", type=int)
    verify.add_argument("--expect-paused", choices=("true", "false"))
    verify.add_argument("--public-key-base64")
    verify.add_argument("--allow-unsigned-dev", action="store_true")
    return root


def fail(message: str) -> None:
    raise SystemExit(f"release artifact manifest: {message}")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as artifact:
        for chunk in iter(lambda: artifact.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def decode_public_key(encoded: str | None) -> bytes:
    if not encoded:
        fail("public key is required for a signed production manifest")
    try:
        key = base64.b64decode(encoded, validate=True)
    except ValueError as error:
        fail(f"public key is invalid base64: {error}")
    if len(key) != 32:
        fail(f"Ed25519 public key must be 32 bytes, got {len(key)}")
    return key


def validate_identity(version: str, release_tag: str, channel: str, rollout: int | None) -> None:
    if not SEMVER.fullmatch(version) or release_tag != f"v{version}":
        fail("version must be SemVer and release_tag must exactly equal v<version>")
    prerelease = "-" in version
    if channel == "stable" and prerelease:
        fail("stable channel cannot publish a prerelease version")
    if channel == "beta" and not prerelease:
        fail("beta channel requires a SemVer prerelease version")
    if rollout is not None and not 0 <= rollout <= 100:
        fail("rollout_percent must be from 0 through 100")


def artifact_entries(dist: Path, release_tag: str) -> list[dict[str, object]]:
    entries = []
    for artifact_id, (platform, arch, package_format, suffix) in ARTIFACT_SUFFIXES.items():
        filename = f"gmark-{release_tag}-{suffix}"
        path = dist / filename
        if not path.is_file():
            fail(f"required artifact is missing: {path}")
        entries.append(
            {
                "id": artifact_id,
                "platform": platform,
                "arch": arch,
                "package_format": package_format,
                "filename": filename,
                "size": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        )
    return entries


def run_openssl(command: list[str]) -> None:
    if command and command[0] == "openssl":
        command = [resolve_openssl(), *command[1:]]
    result = subprocess.run(command, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if result.returncode != 0:
        fail("OpenSSL signature operation failed")


def sign(payload: bytes, private_key: Path, configured_public_key: bytes) -> bytes:
    if not private_key.is_file():
        fail("private key file does not exist")
    with tempfile.TemporaryDirectory(prefix="gmark-release-manifest-") as temporary:
        root = Path(temporary)
        payload_path = root / "payload.json"
        signature_path = root / "signature.bin"
        public_path = root / "public.der"
        payload_path.write_bytes(payload)
        run_openssl(
            [
                "openssl", "pkeyutl", "-sign", "-rawin", "-inkey", str(private_key),
                "-in", str(payload_path), "-out", str(signature_path),
            ]
        )
        run_openssl(
            [
                "openssl", "pkey", "-in", str(private_key), "-pubout", "-outform", "DER",
                "-out", str(public_path),
            ]
        )
        derived = public_path.read_bytes()
        if not derived.endswith(configured_public_key):
            fail("private key does not match configured public key")
        signature = signature_path.read_bytes()
    if len(signature) != 64:
        fail(f"Ed25519 signature must be 64 bytes, got {len(signature)}")
    return signature


def verify_signature(payload: bytes, signature: bytes, public_key: bytes) -> None:
    if len(signature) != 64:
        fail("signature must be 64 bytes")
    with tempfile.TemporaryDirectory(prefix="gmark-release-verify-") as temporary:
        root = Path(temporary)
        payload_path = root / "payload.json"
        signature_path = root / "signature.bin"
        public_path = root / "public.der"
        payload_path.write_bytes(payload)
        signature_path.write_bytes(signature)
        public_path.write_bytes(ED25519_SPKI_PREFIX + public_key)
        run_openssl(
            [
                "openssl", "pkeyutl", "-verify", "-rawin", "-pubin", "-keyform", "DER",
                "-inkey", str(public_path), "-in", str(payload_path),
                "-sigfile", str(signature_path),
            ]
        )


def create(args: argparse.Namespace) -> None:
    validate_identity(args.version, args.release_tag, args.channel, args.rollout_percent)
    if args.unsigned_dev and os.environ.get("GMARK_RELEASE_MODE") == "production":
        fail("--unsigned-dev is forbidden when GMARK_RELEASE_MODE=production")
    if args.unsigned_dev and (args.private_key or args.public_key_base64):
        fail("--unsigned-dev cannot be combined with signing keys")
    if not args.unsigned_dev and not args.private_key:
        fail("production manifest creation requires --private-key")
    payload = {
        "schema_version": 1,
        "version": args.version,
        "release_tag": args.release_tag,
        "channel": args.channel,
        "published_at": args.published_at
        or datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "paused": args.paused,
        "rollout_percent": args.rollout_percent,
        "artifacts": artifact_entries(args.dist, args.release_tag),
    }
    payload_bytes = json.dumps(
        payload, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    if args.unsigned_dev:
        algorithm = "UNSIGNED-DEV"
        signature = b""
    else:
        public_key = decode_public_key(args.public_key_base64)
        algorithm = "Ed25519"
        signature = sign(payload_bytes, args.private_key, public_key)
    envelope = {
        "schema_version": 1,
        "algorithm": algorithm,
        "payload": base64.b64encode(payload_bytes).decode("ascii"),
        "signature": base64.b64encode(signature).decode("ascii"),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(envelope, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def require_keys(value: object, expected: set[str], label: str) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != expected:
        fail(f"{label} has missing or unknown fields")
    return value


def validate_signed_payload_types(payload: dict[str, object]) -> None:
    if type(payload["schema_version"]) is not int or payload["schema_version"] != 1:
        fail("payload schema_version must be integer 1")
    for field in ("version", "release_tag", "channel", "published_at"):
        if not isinstance(payload[field], str) or not payload[field]:
            fail(f"payload {field} must be a non-empty string")
    published_at = payload["published_at"]
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?Z", published_at):
        fail("published_at must be an ISO-8601 UTC timestamp ending in Z")
    try:
        parsed_time = datetime.fromisoformat(published_at[:-1] + "+00:00")
    except ValueError as error:
        fail(f"published_at is not a real UTC timestamp: {error}")
    if parsed_time.utcoffset() != timezone.utc.utcoffset(parsed_time):
        fail("published_at must use UTC")
    if type(payload["paused"]) is not bool:
        fail("paused must be boolean")
    rollout = payload["rollout_percent"]
    if type(rollout) is not int or not 0 <= rollout <= 100:
        fail("rollout_percent must be an integer from 0 through 100")
    artifacts = payload["artifacts"]
    if not isinstance(artifacts, list) or len(artifacts) != len(ARTIFACT_SUFFIXES):
        fail("artifacts must be the complete ordered artifact list")
    seen = set()
    expected_keys = {"id", "platform", "arch", "package_format", "filename", "size", "sha256"}
    for index, value in enumerate(artifacts):
        entry = require_keys(value, expected_keys, f"artifacts[{index}]")
        for field in ("id", "platform", "arch", "package_format", "filename", "sha256"):
            if not isinstance(entry[field], str) or not entry[field]:
                fail(f"artifacts[{index}].{field} must be a non-empty string")
        if type(entry["size"]) is not int or entry["size"] < 0:
            fail(f"artifacts[{index}].size must be a non-negative integer")
        if not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]):
            fail(f"artifacts[{index}].sha256 must be lowercase hexadecimal")
        artifact_id = entry["id"]
        if artifact_id in seen or artifact_id not in ARTIFACT_SUFFIXES:
            fail("artifact ids must be unique and recognized")
        seen.add(artifact_id)
        platform, arch, package_format, suffix = ARTIFACT_SUFFIXES[artifact_id]
        if (
            entry["platform"] != platform
            or entry["arch"] != arch
            or entry["package_format"] != package_format
            or entry["filename"] != f"gmark-{payload['release_tag']}-{suffix}"
        ):
            fail(f"artifact metadata does not match locked format for {artifact_id}")


def verify(args: argparse.Namespace) -> None:
    validate_identity(args.version, args.release_tag, args.channel, args.rollout_percent)
    try:
        envelope_value = json.loads(args.manifest.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        fail(f"cannot read manifest: {error}")
    envelope = require_keys(
        envelope_value, {"schema_version", "algorithm", "payload", "signature"}, "envelope"
    )
    if type(envelope["schema_version"]) is not int or envelope["schema_version"] != 1:
        fail("unsupported envelope schema")
    if not isinstance(envelope["algorithm"], str):
        fail("envelope algorithm must be a string")
    try:
        payload_bytes = base64.b64decode(envelope["payload"], validate=True)
        signature = base64.b64decode(envelope["signature"], validate=True)
    except (TypeError, ValueError) as error:
        fail(f"invalid envelope base64: {error}")
    if envelope["algorithm"] == "UNSIGNED-DEV":
        if not args.allow_unsigned_dev or os.environ.get("GMARK_RELEASE_MODE") == "production":
            fail("unsigned development manifest rejected")
        if signature:
            fail("unsigned development manifest must not contain a signature")
    elif envelope["algorithm"] == "Ed25519":
        verify_signature(payload_bytes, signature, decode_public_key(args.public_key_base64))
    else:
        fail("unsupported signature algorithm")
    try:
        payload_value = json.loads(payload_bytes)
    except (UnicodeError, json.JSONDecodeError) as error:
        fail(f"payload is invalid JSON: {error}")
    payload = require_keys(
        payload_value,
        {
            "schema_version", "version", "release_tag", "channel", "published_at",
            "paused", "rollout_percent", "artifacts",
        },
        "payload",
    )
    validate_signed_payload_types(payload)
    if (
        payload["schema_version"] != 1
        or payload["version"] != args.version
        or payload["release_tag"] != args.release_tag
        or payload["channel"] != args.channel
        or (
            args.expect_paused is not None
            and payload["paused"] is not (args.expect_paused == "true")
        )
        or (
            args.rollout_percent is not None
            and payload["rollout_percent"] != args.rollout_percent
        )
    ):
        fail("payload release controls do not match requested values")
    expected = artifact_entries(args.dist, args.release_tag)
    if payload["artifacts"] != expected:
        fail("artifact filename, package format, size, or sha256 differs from final bytes")
    print("release artifact manifest verified")


def main() -> None:
    args = parser().parse_args()
    if args.command == "create":
        create(args)
    else:
        verify(args)


if __name__ == "__main__":
    main()
