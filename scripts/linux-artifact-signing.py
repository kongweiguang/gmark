# @author kongweiguang

"""Fail-closed GPG detached signing and verification for Linux release archives."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import tempfile
from pathlib import Path


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    subparsers = root.add_subparsers(dest="command", required=True)
    sign = subparsers.add_parser("sign")
    sign.add_argument("--artifact", type=Path, required=True)
    sign.add_argument("--signature", type=Path, required=True)
    sign.add_argument("--private-key", type=Path)
    sign.add_argument("--public-key", type=Path)
    sign.add_argument("--passphrase-file", type=Path)
    sign.add_argument("--expected-fingerprint")
    sign.add_argument("--dry-run", action="store_true")
    sign.add_argument("--unsigned-dev", action="store_true")

    verify = subparsers.add_parser("verify")
    verify.add_argument("--artifact", type=Path, required=True)
    verify.add_argument("--signature", type=Path, required=True)
    verify.add_argument("--public-key", type=Path)
    verify.add_argument("--expected-fingerprint")
    verify.add_argument("--dry-run", action="store_true")
    verify.add_argument("--unsigned-dev", action="store_true")
    return root


def fail(message: str) -> None:
    raise SystemExit(f"Linux artifact signing: {message}")


def is_production() -> bool:
    return os.environ.get("GMARK_RELEASE_MODE") == "production"


def normalize_fingerprint(value: str | None) -> str:
    if not value:
        fail("--expected-fingerprint is required")
    fingerprint = "".join(value.split()).upper()
    if len(fingerprint) < 40 or any(character not in "0123456789ABCDEF" for character in fingerprint):
        fail("expected fingerprint must be at least 40 hexadecimal characters")
    return fingerprint


def run_gpg(home: Path, arguments: list[str], capture: bool = False) -> subprocess.CompletedProcess[str]:
    gpg = shutil.which("gpg")
    if not gpg:
        fail("gpg was not found")
    result = subprocess.run(
        [gpg, "--batch", "--no-tty", "--homedir", str(home), *arguments],
        check=False,
        text=True,
        stdout=subprocess.PIPE if capture else subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    if result.returncode != 0:
        fail("gpg operation failed")
    return result


def import_key(home: Path, path: Path, label: str) -> None:
    if not path.is_file():
        fail(f"{label} key file does not exist")
    run_gpg(home, ["--import", str(path)])


def fingerprints(home: Path, secret: bool = False) -> set[str]:
    command = ["--with-colons", "--fingerprint"]
    if secret:
        command.insert(0, "--list-secret-keys")
    else:
        command.insert(0, "--list-keys")
    output = run_gpg(home, command, capture=True).stdout
    return {
        fields[9].upper()
        for line in output.splitlines()
        if (fields := line.split(":"))[0] == "fpr" and len(fields) > 9
    }


def validate_development_mode(args: argparse.Namespace) -> bool:
    if args.dry_run and args.unsigned_dev:
        fail("--dry-run and --unsigned-dev are mutually exclusive")
    if is_production() and (args.dry_run or args.unsigned_dev):
        fail("dry-run and unsigned-dev are forbidden in production")
    if args.dry_run:
        print(f"dry-run: would {args.command} {args.artifact.name} with GPG")
        return True
    if args.unsigned_dev:
        if args.signature.exists():
            fail("refusing unsigned-dev while a stale signature file exists")
        print(f"UNSIGNED DEV: no signature emitted for {args.artifact.name}")
        return True
    return False


def sign(args: argparse.Namespace) -> None:
    if not args.artifact.is_file():
        fail("artifact does not exist")
    if validate_development_mode(args):
        return
    if not args.private_key or not args.public_key:
        fail("production signing requires private and public key files")
    if is_production() and (not args.passphrase_file or not args.passphrase_file.is_file()):
        fail("production signing requires --passphrase-file")
    if args.passphrase_file and not args.passphrase_file.is_file():
        fail("passphrase file does not exist")
    expected = normalize_fingerprint(args.expected_fingerprint)
    with tempfile.TemporaryDirectory(prefix="gmark-linux-sign-") as temporary:
        home = Path(temporary)
        home.chmod(0o700)
        import_key(home, args.public_key, "public")
        import_key(home, args.private_key, "private")
        if expected not in fingerprints(home) or expected not in fingerprints(home, secret=True):
            fail("imported public/private key does not match expected fingerprint")
        args.signature.parent.mkdir(parents=True, exist_ok=True)
        signing_arguments = ["--yes", "--armor", "--detach-sign", "--local-user", expected]
        if args.passphrase_file:
            signing_arguments.extend(
                ["--pinentry-mode", "loopback", "--passphrase-file", str(args.passphrase_file)]
            )
        signing_arguments.extend(["--output", str(args.signature), str(args.artifact)])
        run_gpg(home, signing_arguments)
    verify_signature(args.artifact, args.signature, args.public_key, expected)
    print(f"signed and verified {args.artifact.name}")


def verify_signature(artifact: Path, signature: Path, public_key: Path, expected: str) -> None:
    if not signature.is_file() or not public_key.is_file():
        fail("signature or public key file does not exist")
    with tempfile.TemporaryDirectory(prefix="gmark-linux-verify-") as temporary:
        home = Path(temporary)
        home.chmod(0o700)
        import_key(home, public_key, "public")
        if expected not in fingerprints(home):
            fail("public key does not match expected fingerprint")
        run_gpg(home, ["--status-fd", "1", "--verify", str(signature), str(artifact)], capture=True)


def verify(args: argparse.Namespace) -> None:
    if not args.artifact.is_file():
        fail("artifact does not exist")
    if validate_development_mode(args):
        return
    if not args.public_key:
        fail("verification requires --public-key")
    expected = normalize_fingerprint(args.expected_fingerprint)
    verify_signature(args.artifact, args.signature, args.public_key, expected)
    print(f"verified {args.artifact.name}")


def main() -> None:
    args = parser().parse_args()
    if args.command == "sign":
        sign(args)
    else:
        verify(args)


if __name__ == "__main__":
    main()
