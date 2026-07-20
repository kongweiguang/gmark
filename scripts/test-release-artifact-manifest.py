# @author kongweiguang

"""Behavior smoke tests for production and explicit unsigned-dev artifact manifests."""

from __future__ import annotations

import base64
import copy
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

from release_crypto import resolve_openssl


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release-artifact-manifest.py"
SUFFIXES = (
    "windows-x86_64-setup.exe",
    "macos-x86_64.dmg",
    "macos-aarch64.dmg",
    "linux-x86_64.AppImage",
    "linux-x86_64.deb",
)


def run(*arguments: str, succeeds: bool = True, env: dict[str, str] | None = None) -> None:
    result = subprocess.run(
        [sys.executable, str(SCRIPT), *arguments],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        env=env,
    )
    if (result.returncode == 0) != succeeds:
        raise AssertionError(f"unexpected exit {result.returncode}: {' '.join(arguments)}")


def write_resigned_manifest(path: Path, payload: dict[str, object], private_key: Path) -> None:
    payload_bytes = json.dumps(
        payload, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    payload_path = path.with_suffix(".payload")
    signature_path = path.with_suffix(".signature")
    payload_path.write_bytes(payload_bytes)
    subprocess.run(
        [
            resolve_openssl(), "pkeyutl", "-sign", "-rawin", "-inkey", str(private_key),
            "-in", str(payload_path), "-out", str(signature_path),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    envelope = {
        "schema_version": 1,
        "algorithm": "Ed25519",
        "payload": base64.b64encode(payload_bytes).decode(),
        "signature": base64.b64encode(signature_path.read_bytes()).decode(),
    }
    path.write_text(json.dumps(envelope), encoding="utf-8")


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="gmark-release-manifest-test-") as temporary:
        root = Path(temporary)
        dist = root / "dist"
        dist.mkdir()
        for suffix in SUFFIXES:
            (dist / f"gmark-v0.1.0-{suffix}").write_bytes(f"artifact:{suffix}\n".encode())

        private_key = root / "private.pem"
        public_der = root / "public.der"
        subprocess.run(
            [resolve_openssl(), "genpkey", "-algorithm", "Ed25519", "-out", str(private_key)],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        subprocess.run(
            [
                resolve_openssl(), "pkey", "-in", str(private_key), "-pubout", "-outform", "DER",
                "-out", str(public_der),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        public_key = base64.b64encode(public_der.read_bytes()[-32:]).decode()
        signed = root / "signed.json"
        common = (
            "--version", "0.1.0", "--release-tag", "v0.1.0", "--channel", "stable",
            "--rollout-percent", "25", "--dist", str(dist),
        )
        run(
            "create", *common, "--private-key", str(private_key),
            "--public-key-base64", public_key, "--output", str(signed),
            "--published-at", "2026-01-01T00:00:00Z",
        )
        run(
            "verify", *common, "--manifest", str(signed),
            "--public-key-base64", public_key, "--expect-paused", "false",
        )
        envelope = json.loads(signed.read_text(encoding="utf-8"))
        payload = json.loads(base64.b64decode(envelope["payload"], validate=True))
        assert payload["channel"] == "stable"
        assert payload["rollout_percent"] == 25
        assert payload["paused"] is False
        assert [entry["id"] for entry in payload["artifacts"]] == [
            "windows-x86_64",
            "macos-x86_64",
            "macos-aarch64",
            "linux-x86_64",
            "linux-x86_64-deb",
        ]
        assert [entry["package_format"] for entry in payload["artifacts"]] == [
            "inno-setup-exe",
            "unsigned-dmg",
            "unsigned-dmg",
            "appimage",
            "deb",
        ]
        assert all(entry["size"] > 0 and len(entry["sha256"]) == 64 for entry in payload["artifacts"])

        identity_only = (
            "--version", "0.1.0", "--release-tag", "v0.1.0", "--channel", "stable",
            "--dist", str(dist),
        )
        malformed_payloads = []
        for field, value in (
            ("paused", "false"),
            ("rollout_percent", True),
            ("rollout_percent", 101),
            ("published_at", ""),
        ):
            malformed = copy.deepcopy(payload)
            malformed[field] = value
            malformed_payloads.append(malformed)
        malformed = copy.deepcopy(payload)
        malformed["artifacts"][0]["size"] = "1"
        malformed_payloads.append(malformed)
        malformed = copy.deepcopy(payload)
        malformed["artifacts"] = {"not": "a list"}
        malformed_payloads.append(malformed)
        for index, malformed in enumerate(malformed_payloads):
            malformed_path = root / f"malformed-{index}.json"
            write_resigned_manifest(malformed_path, malformed, private_key)
            run(
                "verify", *identity_only, "--manifest", str(malformed_path),
                "--public-key-base64", public_key, succeeds=False,
            )

        unsigned = root / "unsigned.json"
        run("create", *common, "--unsigned-dev", "--output", str(unsigned))
        run(
            "verify", *common, "--manifest", str(unsigned), "--expect-paused", "false",
            succeeds=False,
        )
        run(
            "verify", *common, "--manifest", str(unsigned), "--expect-paused", "false",
            "--allow-unsigned-dev",
        )
        production = dict(os.environ)
        production["GMARK_RELEASE_MODE"] = "production"
        run(
            "create", *common, "--unsigned-dev", "--output", str(root / "forbidden.json"),
            succeeds=False, env=production,
        )
        run(
            "verify", *common, "--manifest", str(unsigned), "--expect-paused", "false",
            "--allow-unsigned-dev", succeeds=False, env=production,
        )

        target = dist / "gmark-v0.1.0-linux-x86_64.AppImage"
        target.write_bytes(target.read_bytes() + b"tampered")
        run(
            "verify", *common, "--manifest", str(signed),
            "--public-key-base64", public_key, "--expect-paused", "false",
            succeeds=False,
        )

    print("release artifact manifest tests passed")


if __name__ == "__main__":
    main()
