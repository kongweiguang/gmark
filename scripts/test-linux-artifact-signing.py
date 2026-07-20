# @author kongweiguang

"""Generate an ephemeral GPG key and exercise Linux release signing fail-closed."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "linux-artifact-signing.py"


def invoke(*arguments: str, succeeds: bool = True, env: dict[str, str] | None = None) -> None:
    result = subprocess.run(
        [sys.executable, str(SCRIPT), *arguments],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        env=env,
    )
    if (result.returncode == 0) != succeeds:
        raise AssertionError(f"unexpected exit {result.returncode}: {' '.join(arguments)}")


def gpg(home: Path, *arguments: str, capture: bool = False) -> str:
    executable = shutil.which("gpg")
    if not executable:
        raise SystemExit("gpg is required for this test")
    result = subprocess.run(
        [executable, "--batch", "--no-tty", "--homedir", str(home), *arguments],
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return result.stdout or ""


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="gmark-linux-sign-test-") as temporary:
        root = Path(temporary)
        artifact = root / "gmark-v0.1.0-linux-x86_64.tar.gz"
        artifact.write_bytes(b"release artifact\n")
        dry_signature = root / "dry.asc"
        invoke(
            "sign", "--artifact", str(artifact), "--signature", str(dry_signature), "--dry-run"
        )
        invoke(
            "sign", "--artifact", str(artifact), "--signature", str(dry_signature), "--unsigned-dev"
        )
        production = dict(os.environ)
        production["GMARK_RELEASE_MODE"] = "production"
        invoke(
            "sign", "--artifact", str(artifact), "--signature", str(dry_signature),
            "--unsigned-dev", succeeds=False, env=production,
        )
        if os.name == "nt":
            print("Linux GPG signed roundtrip deferred to Linux CI; Windows dry-run gates passed")
            return

        home = root / "gnupg"
        home.mkdir(mode=0o700)
        gpg(
            home,
            "--pinentry-mode", "loopback", "--passphrase", "", "--quick-generate-key",
            "gmark Release Test <release-test@example.invalid>", "ed25519", "sign", "1d",
        )
        listing = gpg(home, "--with-colons", "--list-secret-keys", "--fingerprint", capture=True)
        fingerprint = next(
            fields[9]
            for line in listing.splitlines()
            if (fields := line.split(":"))[0] == "fpr"
        )
        public_key = root / "public.asc"
        private_key = root / "private.asc"
        public_key.write_text(gpg(home, "--armor", "--export", fingerprint, capture=True))
        private_key.write_text(gpg(home, "--armor", "--export-secret-keys", fingerprint, capture=True))
        signature = root / "artifact.asc"
        common = (
            "--artifact", str(artifact), "--signature", str(signature),
            "--expected-fingerprint", fingerprint,
        )
        invoke(
            "sign", *common, "--private-key", str(private_key), "--public-key", str(public_key)
        )
        invoke("verify", *common, "--public-key", str(public_key))
        artifact.write_bytes(b"tampered\n")
        invoke("verify", *common, "--public-key", str(public_key), succeeds=False)

    print("Linux artifact signing tests passed")


if __name__ == "__main__":
    main()
