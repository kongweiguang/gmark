# @author kongweiguang

"""Create and self-verify legacy and v2 signed gmark updater manifests."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import subprocess
import tempfile
from datetime import datetime, timezone
from pathlib import Path

from release_crypto import resolve_openssl


LEGACY_ARTIFACT_SUFFIXES = {
    "windows-x86_64": "windows-x86_64-setup.exe",
    "macos-x86_64": "macos-x86_64.dmg",
    "macos-aarch64": "macos-aarch64.dmg",
    "linux-x86_64": "linux-x86_64.AppImage",
    "linux-x86_64-deb": "linux-x86_64.deb",
}

V2_ARTIFACTS = {
    "windows-x86_64": ("windows-x86_64-setup.exe", "windows-setup-exe", "windows"),
    "macos-x86_64": ("macos-x86_64.app.tar.gz", "macos-app-tar-gz", "macos"),
    "macos-aarch64": ("macos-aarch64.app.tar.gz", "macos-app-tar-gz", "macos"),
    "linux-x86_64": ("linux-x86_64.AppImage", "linux-app-image", "linux"),
}

VELOPACK_ARTIFACTS = {
    "windows-x86_64": ("windows-x86_64-full.nupkg", "windows-velopack-nupkg", "windows"),
    "macos-x86_64": ("macos-x86_64-full.nupkg", "macos-velopack-nupkg", "macos"),
    "macos-aarch64": ("macos-aarch64-full.nupkg", "macos-velopack-nupkg", "macos"),
    "linux-x86_64": ("linux-x86_64-full.nupkg", "linux-velopack-nupkg", "linux"),
}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as artifact:
        for chunk in iter(lambda: artifact.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--release-tag", required=True)
    parser.add_argument("--dist", type=Path, required=True)
    parser.add_argument("--private-key", type=Path, required=True)
    parser.add_argument("--public-key-base64", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--v2-output", type=Path)
    parser.add_argument("--velopack-output", type=Path)
    parser.add_argument("--notes", default="")
    parser.add_argument("--rollout-percent", type=int, default=100)
    parser.add_argument("--paused", action="store_true")
    parser.add_argument(
        "--windows-system-trust",
        choices=("unsigned", "authenticode"),
        default="unsigned",
    )
    parser.add_argument(
        "--macos-system-trust",
        choices=("unsigned", "developer-id-notarized"),
        default="unsigned",
    )
    return parser.parse_args()


def run(command: list[str]) -> None:
    if command and command[0] == "openssl":
        command = [resolve_openssl(), *command[1:]]
    subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def configured_public_key(encoded: str) -> bytes:
    try:
        key = base64.b64decode(encoded, validate=True)
    except ValueError as error:
        raise SystemExit(f"invalid public key base64: {error}") from error
    if len(key) != 32:
        raise SystemExit("Ed25519 public key must decode to exactly 32 bytes")
    return key


def signed_envelope(payload: dict[str, object], args: argparse.Namespace, public_key: bytes) -> dict[str, object]:
    payload_bytes = json.dumps(
        payload,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    with tempfile.TemporaryDirectory(prefix="gmark-update-sign-") as temporary:
        temporary = Path(temporary)
        payload_path = temporary / "payload.json"
        signature_path = temporary / "signature.bin"
        public_der_path = temporary / "public.der"
        payload_path.write_bytes(payload_bytes)
        run([
            "openssl", "pkeyutl", "-sign", "-rawin", "-inkey", str(args.private_key),
            "-in", str(payload_path), "-out", str(signature_path),
        ])
        run([
            "openssl", "pkey", "-in", str(args.private_key), "-pubout", "-outform", "DER",
            "-out", str(public_der_path),
        ])
        public_der = public_der_path.read_bytes()
        if len(public_der) < 32 or public_der[-32:] != public_key:
            raise SystemExit("private key does not match the configured update public key")
        run([
            "openssl", "pkeyutl", "-verify", "-rawin", "-pubin", "-inkey",
            str(public_der_path), "-keyform", "DER", "-in", str(payload_path),
            "-sigfile", str(signature_path),
        ])
        signature = signature_path.read_bytes()
    if len(signature) != 64:
        raise SystemExit(f"Ed25519 signature must be 64 bytes, got {len(signature)}")
    return {
        "schema_version": 1,
        "algorithm": "Ed25519",
        "payload": base64.b64encode(payload_bytes).decode("ascii"),
        "signature": base64.b64encode(signature).decode("ascii"),
    }


def write_envelope(path: Path, envelope: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(envelope, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def require_artifact(dist: Path, release_tag: str, suffix: str) -> tuple[Path, str]:
    filename = f"gmark-{release_tag}-{suffix}"
    path = dist / filename
    if not path.is_file():
        raise SystemExit(f"required release artifact is missing: {path}")
    return path, filename


def v2_artifacts(
    definitions: dict[str, tuple[str, str, str]],
    args: argparse.Namespace,
) -> dict[str, dict[str, object]]:
    """Build one signed artifact set so compatibility and Velopack feeds cannot mix formats."""
    release_download = f"https://github.com/kongweiguang/gmark/releases/download/{args.release_tag}"
    artifacts: dict[str, dict[str, object]] = {}
    for artifact_id, (suffix, package_format, platform) in definitions.items():
        path, filename = require_artifact(args.dist, args.release_tag, suffix)
        system_trust = {
            "windows": args.windows_system_trust,
            "macos": args.macos_system_trust,
            "linux": "not-applicable",
        }[platform]
        artifacts[artifact_id] = {
            "url": f"{release_download}/{filename}",
            "size": path.stat().st_size,
            "sha256": sha256_file(path),
            "format": package_format,
            "system_trust": system_trust,
        }
    return artifacts


def write_v2_manifest(
    path: Path,
    definitions: dict[str, tuple[str, str, str]],
    args: argparse.Namespace,
    public_key: bytes,
    published_at: str,
) -> None:
    """Sign each endpoint independently so legacy clients never consume Velopack packages."""
    payload = {
        "schema_version": 2,
        "channel": "stable",
        "version": args.version,
        "published_at": published_at,
        "notes": args.notes,
        "paused": args.paused,
        "rollout_percent": args.rollout_percent,
        "release_url": f"https://github.com/kongweiguang/gmark/releases/tag/{args.release_tag}",
        "artifacts": v2_artifacts(definitions, args),
    }
    write_envelope(path, signed_envelope(payload, args, public_key))


def main() -> None:
    args = parse_args()
    if args.release_tag != f"v{args.version}":
        raise SystemExit("release tag must exactly match v<version>")
    if (args.v2_output or args.velopack_output) and "-" in args.version:
        raise SystemExit("automatic updater manifests currently support stable SemVer only")
    if not 0 <= args.rollout_percent <= 100:
        raise SystemExit("rollout percent must be between 0 and 100")
    public_key = configured_public_key(args.public_key_base64)
    release_download = f"https://github.com/kongweiguang/gmark/releases/download/{args.release_tag}"
    published_at = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")

    legacy_artifacts: dict[str, dict[str, str]] = {}
    for artifact_id, suffix in LEGACY_ARTIFACT_SUFFIXES.items():
        path, filename = require_artifact(args.dist, args.release_tag, suffix)
        legacy_artifacts[artifact_id] = {
            "url": f"{release_download}/{filename}",
            "sha256": sha256_file(path),
        }
    legacy_payload = {
        "schema_version": 1,
        "version": args.version,
        "published_at": published_at,
        "paused": args.paused,
        "rollout_percent": args.rollout_percent,
        "release_url": f"https://github.com/kongweiguang/gmark/releases/tag/{args.release_tag}",
        "artifacts": legacy_artifacts,
    }
    write_envelope(args.output, signed_envelope(legacy_payload, args, public_key))

    if args.v2_output:
        write_v2_manifest(
            args.v2_output, V2_ARTIFACTS, args, public_key, published_at
        )
    if args.velopack_output:
        write_v2_manifest(
            args.velopack_output, VELOPACK_ARTIFACTS, args, public_key, published_at
        )


if __name__ == "__main__":
    main()
