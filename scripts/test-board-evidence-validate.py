#!/usr/bin/env python3
# @author kongweiguang
"""Regression tests for the strict Board evidence manifest validator."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import struct
import tempfile
import unittest
import zlib
from pathlib import Path


SCRIPT = Path(__file__).with_name("board-evidence-validate.py")
SPEC = importlib.util.spec_from_file_location("board_evidence_validate", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot import validator {SCRIPT}")
validator = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(validator)


def png(width: int, height: int) -> bytes:
    signature = b"\x89PNG\r\n\x1a\n"
    ihdr_data = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    ihdr = struct.pack(">I", len(ihdr_data)) + b"IHDR" + ihdr_data
    ihdr += struct.pack(">I", zlib.crc32(ihdr[4:]) & 0xFFFFFFFF)
    # The validator intentionally only needs the canonical IHDR for traceability checks; include
    # an IEND so the fixture is also recognizable by ordinary PNG tooling.
    iend = struct.pack(">I", 0) + b"IEND" + struct.pack(">I", zlib.crc32(b"IEND") & 0xFFFFFFFF)
    return signature + ihdr + iend


def manifest_for(root: Path) -> dict[str, object]:
    method = "test-capture"
    fixtures: list[dict[str, object]] = []
    for index, fixture in enumerate(validator.EXPECTED_FIXTURES):
        path = root / f"{fixture}.png"
        content = png(index + 1, 2)
        path.write_bytes(content)
        fixtures.append(
            {
                "fixture": fixture,
                "file": path.name,
                "width": index + 1,
                "height": 2,
                "bytes": len(content),
                "sha256": hashlib.sha256(content).hexdigest(),
                "capture_method": method,
                "unique_colors": 2,
                "non_background_pixels": 1,
            }
        )
    return {
        "schema_version": validator.SCHEMA_VERSION,
        "status": "VERIFIED",
        "platform": "test",
        "capture_method": method,
        "capture_backends": [method],
        "output_directory": str(root),
        "build_metadata": {
            "git_sha": "0123456789abcdef0123456789abcdef01234567",
            "workspace_dirty": True,
        },
        "review": {
            "reviewer": validator.AUTOMATED_REVIEWER,
            "date": "2026-08-06",
            "manual_visual_status": validator.MANUAL_VISUAL_NOT_VERIFIED,
        },
        "fixtures": fixtures,
    }


class BoardEvidenceValidatorTests(unittest.TestCase):
    def write_manifest(self, root: Path, manifest: dict[str, object]) -> Path:
        path = root / "manifest.json"
        path.write_text(json.dumps(manifest), encoding="utf-8")
        return path

    def test_valid_manifest_round_trips_all_fixture_traceability(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = self.write_manifest(root, manifest_for(root))
            validator.validate_manifest(path)

    def test_invalid_build_sha_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = manifest_for(root)
            manifest["build_metadata"]["git_sha"] = "unknown"  # type: ignore[index]
            path = self.write_manifest(root, manifest)
            with self.assertRaisesRegex(validator.ValidationError, "commit SHA"):
                validator.validate_manifest(path)

    def test_workspace_dirty_must_be_a_real_boolean(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = manifest_for(root)
            manifest["build_metadata"]["workspace_dirty"] = 1  # type: ignore[index]
            path = self.write_manifest(root, manifest)
            with self.assertRaisesRegex(validator.ValidationError, "must be a boolean"):
                validator.validate_manifest(path)

    def test_png_dimensions_and_hash_are_bound_to_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = manifest_for(root)
            manifest["fixtures"][0]["width"] = 99  # type: ignore[index]
            path = self.write_manifest(root, manifest)
            with self.assertRaisesRegex(validator.ValidationError, "dimensions"):
                validator.validate_manifest(path)

    def test_png_byte_count_and_sha_are_bound_to_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = manifest_for(root)
            manifest["fixtures"][0]["bytes"] += 1  # type: ignore[operator]
            path = self.write_manifest(root, manifest)
            with self.assertRaisesRegex(validator.ValidationError, "bytes"):
                validator.validate_manifest(path)

    def test_png_sha_is_bound_to_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = manifest_for(root)
            manifest["fixtures"][0]["sha256"] = "0" * 64  # type: ignore[index]
            path = self.write_manifest(root, manifest)
            with self.assertRaisesRegex(validator.ValidationError, "sha256"):
                validator.validate_manifest(path)

    def test_fixture_path_traversal_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = manifest_for(root)
            manifest["fixtures"][0]["file"] = "../light.png"  # type: ignore[index]
            path = self.write_manifest(root, manifest)
            with self.assertRaisesRegex(validator.ValidationError, "must be 'light.png'"):
                validator.validate_manifest(path)

    def test_fixture_order_is_deterministic(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = manifest_for(root)
            manifest["fixtures"] = list(reversed(manifest["fixtures"]))  # type: ignore[assignment]
            path = self.write_manifest(root, manifest)
            with self.assertRaisesRegex(validator.ValidationError, "deterministic order"):
                validator.validate_manifest(path)

    def test_automated_capture_cannot_claim_manual_visual_pass(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = manifest_for(root)
            manifest["review"]["manual_visual_status"] = "PASS"  # type: ignore[index]
            path = self.write_manifest(root, manifest)
            with self.assertRaisesRegex(validator.ValidationError, "never claim"):
                validator.validate_manifest(path)

    def test_explicit_human_reviewer_remains_not_verified(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = manifest_for(root)
            manifest["review"]["reviewer"] = "Ada Lovelace"  # type: ignore[index]
            path = self.write_manifest(root, manifest)
            validator.validate_manifest(path)

    def test_review_date_must_be_a_real_calendar_date(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = manifest_for(root)
            manifest["review"]["date"] = "2026-02-29"  # type: ignore[index]
            path = self.write_manifest(root, manifest)
            with self.assertRaisesRegex(validator.ValidationError, "calendar"):
                validator.validate_manifest(path)

    def test_duplicate_json_keys_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / "manifest.json"
            path.write_text('{"schema_version":3,"schema_version":3}', encoding="utf-8")
            with self.assertRaisesRegex(validator.ValidationError, "duplicate JSON key"):
                validator.validate_manifest(path)

    def test_manifest_symlink_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "real-manifest.json"
            target.write_text(json.dumps(manifest_for(root)), encoding="utf-8")
            link = root / "manifest.json"
            try:
                link.symlink_to(target)
            except (OSError, NotImplementedError):
                self.skipTest("symlink creation is unavailable")
            with self.assertRaisesRegex(validator.ValidationError, "regular file"):
                validator.validate_manifest(link)


if __name__ == "__main__":
    unittest.main()
