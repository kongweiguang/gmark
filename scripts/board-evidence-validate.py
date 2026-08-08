#!/usr/bin/env python3
# @author kongweiguang
"""Strictly validate a Board technical-capture manifest and its PNG artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import struct
import sys
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 3
EXPECTED_FIXTURES = (
    "light",
    "dark",
    "narrow",
    "selected",
    "text-edit",
    "dense",
    "wizard",
    "export",
    "error",
    "missing-asset",
    "conflict",
    "recovery",
)
AUTOMATED_REVIEWER = "automated technical capture"
MANUAL_VISUAL_NOT_VERIFIED = "NOT VERIFIED"
TOP_LEVEL_KEYS = {
    "schema_version",
    "status",
    "platform",
    "capture_method",
    "capture_backends",
    "output_directory",
    "build_metadata",
    "review",
    "fixtures",
}
BUILD_KEYS = {"git_sha", "workspace_dirty"}
REVIEW_KEYS = {"reviewer", "date", "manual_visual_status"}
ARTIFACT_KEYS = {
    "fixture",
    "file",
    "width",
    "height",
    "bytes",
    "sha256",
    "capture_method",
    "unique_colors",
    "non_background_pixels",
}
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


class ValidationError(ValueError):
    """The manifest is absent, malformed, incomplete, or not traceable."""


def require_keys(value: dict[str, Any], expected: set[str], context: str) -> None:
    actual = set(value)
    if actual != expected:
        missing = sorted(expected - actual)
        extra = sorted(actual - expected)
        raise ValidationError(f"{context} keys differ; missing={missing} extra={extra}")


def require_mapping(value: Any, context: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValidationError(f"{context} must be an object")
    return value


def require_list(value: Any, context: str) -> list[Any]:
    if not isinstance(value, list):
        raise ValidationError(f"{context} must be an array")
    return value


def require_string(value: Any, context: str) -> str:
    if not isinstance(value, str) or not value:
        raise ValidationError(f"{context} must be a non-empty string")
    return value


def require_non_negative_integer(value: Any, context: str) -> int:
    if type(value) is not int or value < 0:
        raise ValidationError(f"{context} must be a non-negative integer")
    return value


def require_positive_integer(value: Any, context: str) -> int:
    value = require_non_negative_integer(value, context)
    if value == 0:
        raise ValidationError(f"{context} must be positive")
    return value


def require_boolean(value: Any, context: str) -> bool:
    if type(value) is not bool:
        raise ValidationError(f"{context} must be a boolean")
    return value


def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValidationError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def load_manifest(path: Path) -> dict[str, Any]:
    if path.is_dir():
        path = path / "manifest.json"
    if path.is_symlink() or not path.is_file():
        raise ValidationError(f"manifest does not exist as a regular file: {path}")
    try:
        document = json.loads(
            path.read_text(encoding="utf-8"), object_pairs_hook=reject_duplicate_keys
        )
    except (OSError, UnicodeError, json.JSONDecodeError, ValidationError) as error:
        raise ValidationError(f"cannot read manifest {path}: {error}") from error
    return require_mapping(document, "manifest")


def validate_date(value: Any, context: str) -> str:
    date = require_string(value, context)
    if not DATE_RE.fullmatch(date):
        raise ValidationError(f"{context} must use YYYY-MM-DD format")
    year, month, day = (int(part) for part in date.split("-"))
    if not 1 <= month <= 12:
        raise ValidationError(f"{context} has an invalid calendar value")
    leap = year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)
    days_in_month = (31, 29 if leap else 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31)
    if not 1 <= day <= days_in_month[month - 1]:
        raise ValidationError(f"{context} has an invalid calendar value")
    return date


def resolve_artifact(manifest_path: Path, file_name: str) -> Path:
    relative = Path(file_name)
    if relative.is_absolute() or relative.name != file_name or relative.parts != (file_name,):
        raise ValidationError(f"fixture file must be a single relative filename: {file_name!r}")
    if relative.suffix.lower() != ".png":
        raise ValidationError(f"fixture file must have a .png suffix: {file_name!r}")
    candidate = manifest_path.parent / relative
    if candidate.is_symlink() or not candidate.is_file():
        raise ValidationError(f"fixture PNG is not a regular file: {candidate}")
    return candidate


def png_dimensions(path: Path) -> tuple[int, int]:
    try:
        with path.open("rb") as stream:
            header = stream.read(33)
    except OSError as error:
        raise ValidationError(f"cannot read PNG {path}: {error}") from error
    if len(header) < 33 or header[:8] != PNG_SIGNATURE:
        raise ValidationError(f"fixture is not a PNG: {path}")
    chunk_length = struct.unpack(">I", header[8:12])[0]
    if chunk_length != 13 or header[12:16] != b"IHDR":
        raise ValidationError(f"PNG has no canonical IHDR chunk: {path}")
    width, height = struct.unpack(">II", header[16:24])
    if width == 0 or height == 0:
        raise ValidationError(f"PNG dimensions must be positive: {path}")
    # Require a standard RGBA/RGB image header and verify the IHDR CRC. The capture writer emits
    # an 8-bit PNG; rejecting exotic colour/depth combinations keeps the evidence contract clear.
    bit_depth = header[24]
    color_type = header[25]
    if bit_depth != 8 or color_type not in (2, 6):
        raise ValidationError(f"PNG has unsupported bit depth or colour type: {path}")
    expected_crc = struct.unpack(">I", header[29:33])[0]
    import zlib

    actual_crc = zlib.crc32(header[12:29]) & 0xFFFFFFFF
    if actual_crc != expected_crc:
        raise ValidationError(f"PNG IHDR CRC mismatch: {path}")
    return width, height


def sha256_and_size(path: Path) -> tuple[str, int]:
    digest = hashlib.sha256()
    size = 0
    try:
        with path.open("rb") as stream:
            while chunk := stream.read(1024 * 1024):
                size += len(chunk)
                digest.update(chunk)
    except OSError as error:
        raise ValidationError(f"cannot hash fixture PNG {path}: {error}") from error
    return digest.hexdigest(), size


def validate_build_metadata(value: Any) -> None:
    build = require_mapping(value, "manifest.build_metadata")
    require_keys(build, BUILD_KEYS, "manifest.build_metadata")
    git_sha = require_string(build["git_sha"], "manifest.build_metadata.git_sha")
    if not GIT_SHA_RE.fullmatch(git_sha):
        raise ValidationError(
            "manifest.build_metadata.git_sha must be a full lowercase 40-character commit SHA"
        )
    require_boolean(build["workspace_dirty"], "manifest.build_metadata.workspace_dirty")


def validate_review(value: Any) -> None:
    review = require_mapping(value, "manifest.review")
    require_keys(review, REVIEW_KEYS, "manifest.review")
    # The capture process defaults to the explicit automated reviewer, while an external human
    # audit may record a different name later. The technical manifest still cannot turn that name
    # into a manual visual PASS.
    require_string(review["reviewer"], "manifest.review.reviewer")
    validate_date(review["date"], "manifest.review.date")
    manual_status = require_string(
        review["manual_visual_status"], "manifest.review.manual_visual_status"
    )
    if manual_status != MANUAL_VISUAL_NOT_VERIFIED:
        raise ValidationError(
            "technical capture must never claim a manual visual PASS; expected NOT VERIFIED"
        )


def validate_artifact(manifest_path: Path, value: Any, index: int) -> str:
    artifact = require_mapping(value, f"manifest.fixtures[{index}]")
    require_keys(artifact, ARTIFACT_KEYS, f"manifest.fixtures[{index}]")
    fixture = require_string(artifact["fixture"], f"manifest.fixtures[{index}].fixture")
    if fixture not in EXPECTED_FIXTURES:
        raise ValidationError(f"unsupported evidence fixture: {fixture!r}")
    file_name = require_string(artifact["file"], f"manifest.fixtures[{index}].file")
    expected_file = f"{fixture}.png"
    if file_name != expected_file:
        raise ValidationError(
            f"manifest.fixtures[{index}].file must be {expected_file!r}, got {file_name!r}"
        )
    width = require_positive_integer(artifact["width"], f"manifest.fixtures[{index}].width")
    height = require_positive_integer(artifact["height"], f"manifest.fixtures[{index}].height")
    expected_bytes = require_positive_integer(
        artifact["bytes"], f"manifest.fixtures[{index}].bytes"
    )
    sha256 = require_string(artifact["sha256"], f"manifest.fixtures[{index}].sha256")
    if not SHA256_RE.fullmatch(sha256):
        raise ValidationError(f"manifest.fixtures[{index}].sha256 must be lowercase SHA-256")
    require_string(artifact["capture_method"], f"manifest.fixtures[{index}].capture_method")
    unique_colors = require_non_negative_integer(
        artifact["unique_colors"], f"manifest.fixtures[{index}].unique_colors"
    )
    non_background_pixels = require_non_negative_integer(
        artifact["non_background_pixels"],
        f"manifest.fixtures[{index}].non_background_pixels",
    )
    pixel_count = width * height
    if unique_colors == 0 or non_background_pixels == 0 or non_background_pixels > pixel_count:
        raise ValidationError(f"manifest.fixtures[{index}] image metrics are inconsistent")

    path = resolve_artifact(manifest_path, file_name)
    actual_dimensions = png_dimensions(path)
    if actual_dimensions != (width, height):
        raise ValidationError(
            f"manifest.fixtures[{index}] dimensions {width}x{height} do not match PNG "
            f"{actual_dimensions[0]}x{actual_dimensions[1]}"
        )
    actual_sha256, actual_bytes = sha256_and_size(path)
    if actual_bytes != expected_bytes:
        raise ValidationError(
            f"manifest.fixtures[{index}] bytes {expected_bytes} do not match PNG size {actual_bytes}"
        )
    if actual_sha256 != sha256:
        raise ValidationError(
            f"manifest.fixtures[{index}] sha256 does not match PNG bytes ({actual_sha256})"
        )
    return fixture


def validate_manifest(manifest_path: Path) -> dict[str, Any]:
    if manifest_path.is_dir():
        manifest_path = manifest_path / "manifest.json"
    document = load_manifest(manifest_path)
    manifest_path = manifest_path.resolve()
    require_keys(document, TOP_LEVEL_KEYS, "manifest")
    if require_non_negative_integer(document["schema_version"], "manifest.schema_version") != SCHEMA_VERSION:
        raise ValidationError(f"unsupported manifest.schema_version (expected {SCHEMA_VERSION})")
    if require_string(document["status"], "manifest.status") != "VERIFIED":
        raise ValidationError("manifest.status must be VERIFIED for technical capture")
    require_string(document["platform"], "manifest.platform")
    require_string(document["capture_method"], "manifest.capture_method")
    capture_backends = require_list(document["capture_backends"], "manifest.capture_backends")
    backend_values = [
        require_string(value, f"manifest.capture_backends[{index}]")
        for index, value in enumerate(capture_backends)
    ]
    if len(set(backend_values)) != len(backend_values):
        raise ValidationError("manifest.capture_backends contains duplicates")
    require_string(document["output_directory"], "manifest.output_directory")
    validate_build_metadata(document["build_metadata"])
    validate_review(document["review"])

    fixtures = require_list(document["fixtures"], "manifest.fixtures")
    if len(fixtures) != len(EXPECTED_FIXTURES):
        raise ValidationError(
            f"manifest.fixtures must contain exactly {len(EXPECTED_FIXTURES)} artifacts"
        )
    seen: list[str] = []
    for index, artifact in enumerate(fixtures):
        seen.append(validate_artifact(manifest_path, artifact, index))
    if seen != list(EXPECTED_FIXTURES):
        raise ValidationError(
            "manifest.fixtures must contain the required fixtures in deterministic order; "
            f"got {seen}"
        )
    artifact_methods = {
        require_string(artifact["capture_method"], f"manifest.fixtures[{index}].capture_method")
        for index, artifact in enumerate(fixtures)
    }
    if not artifact_methods.issubset(set(backend_values)):
        raise ValidationError("manifest.capture_backends is missing an artifact capture method")
    return document


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "manifest",
        nargs="?",
        type=Path,
        help="manifest.json or its evidence directory",
    )
    parser.add_argument(
        "--manifest",
        dest="manifest_option",
        type=Path,
        help="explicit manifest.json path (alternative to the positional argument)",
    )
    arguments = parser.parse_args(argv)
    manifest_path = arguments.manifest_option or arguments.manifest
    if manifest_path is None:
        parser.error("a manifest path is required")
    try:
        if manifest_path.is_dir():
            manifest_path = manifest_path / "manifest.json"
        validate_manifest(manifest_path)
    except ValidationError as error:
        print(f"board evidence validation failed: {error}", file=sys.stderr)
        return 1
    print(f"board evidence manifest validated: {manifest_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
