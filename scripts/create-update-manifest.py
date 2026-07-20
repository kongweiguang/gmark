# @author kongweiguang

"""Create and self-verify the signed gmark update-manifest envelope."""

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


ARTIFACT_SUFFIXES = {
    "windows-x86_64": "windows-x86_64-setup.exe",
    "macos-x86_64": "macos-x86_64.dmg",
    "macos-aarch64": "macos-aarch64.dmg",
    "linux-x86_64": "linux-x86_64.AppImage",
    "linux-x86_64-deb": "linux-x86_64.deb",
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
    parser.add_argument("--rollout-percent", type=int, default=100)
    parser.add_argument("--paused", action="store_true")
    return parser.parse_args()


def run(command: list[str]) -> None:
    if command and command[0] == "openssl":
        command = [resolve_openssl(), *command[1:]]
    subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def main() -> None:
    args = parse_args()
    if args.release_tag != f"v{args.version}":
        raise SystemExit("release tag must exactly match v<version>")
    if not 0 <= args.rollout_percent <= 100:
        raise SystemExit("rollout percent must be between 0 and 100")
    try:
        configured_public_key = base64.b64decode(
            args.public_key_base64,
            validate=True,
        )
    except ValueError as error:
        raise SystemExit(f"invalid public key base64: {error}") from error
    if len(configured_public_key) != 32:
        raise SystemExit("Ed25519 public key must decode to exactly 32 bytes")

    artifacts: dict[str, dict[str, str]] = {}
    release_download = (
        f"https://github.com/kongweiguang/gmark/releases/download/{args.release_tag}"
    )
    for artifact_id, suffix in ARTIFACT_SUFFIXES.items():
        filename = f"gmark-{args.release_tag}-{suffix}"
        path = args.dist / filename
        if not path.is_file():
            raise SystemExit(f"required release artifact is missing: {path}")
        artifacts[artifact_id] = {
            "url": f"{release_download}/{filename}",
            "sha256": sha256_file(path),
        }

    payload = {
        "schema_version": 1,
        "version": args.version,
        "published_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "paused": args.paused,
        "rollout_percent": args.rollout_percent,
        "release_url": (
            f"https://github.com/kongweiguang/gmark/releases/tag/{args.release_tag}"
        ),
        "artifacts": artifacts,
    }
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
        run(
            [
                "openssl",
                "pkeyutl",
                "-sign",
                "-rawin",
                "-inkey",
                str(args.private_key),
                "-in",
                str(payload_path),
                "-out",
                str(signature_path),
            ]
        )
        run(
            [
                "openssl",
                "pkey",
                "-in",
                str(args.private_key),
                "-pubout",
                "-outform",
                "DER",
                "-out",
                str(public_der_path),
            ]
        )
        public_der = public_der_path.read_bytes()
        if len(public_der) < 32 or public_der[-32:] != configured_public_key:
            raise SystemExit("private key does not match the configured update public key")
        run(
            [
                "openssl",
                "pkeyutl",
                "-verify",
                "-rawin",
                "-pubin",
                "-inkey",
                str(public_der_path),
                "-keyform",
                "DER",
                "-in",
                str(payload_path),
                "-sigfile",
                str(signature_path),
            ]
        )
        signature = signature_path.read_bytes()

    if len(signature) != 64:
        raise SystemExit(f"Ed25519 signature must be 64 bytes, got {len(signature)}")
    envelope = {
        "schema_version": 1,
        "algorithm": "Ed25519",
        "payload": base64.b64encode(payload_bytes).decode("ascii"),
        "signature": base64.b64encode(signature).decode("ascii"),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(envelope, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )


if __name__ == "__main__":
    main()
