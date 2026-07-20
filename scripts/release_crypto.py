# @author kongweiguang

"""Resolve release cryptography tools without relying on an interactive shell PATH."""

from __future__ import annotations

import os
import shutil
from pathlib import Path


def resolve_openssl() -> str:
    explicit = os.environ.get("GMARK_OPENSSL")
    if explicit:
        path = Path(explicit)
        if path.is_file():
            return str(path)
        raise SystemExit("GMARK_OPENSSL does not point to an existing executable")
    discovered = shutil.which("openssl")
    if discovered:
        return discovered
    candidates = []
    for variable in ("ProgramFiles", "ProgramFiles(x86)"):
        root = os.environ.get(variable)
        if root:
            candidates.append(Path(root) / "Git" / "usr" / "bin" / "openssl.exe")
    candidates.append(Path("C:/Program Files/Git/usr/bin/openssl.exe"))
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)
    raise SystemExit(
        "OpenSSL was not found; install it, add it to PATH, or set GMARK_OPENSSL to the executable"
    )
