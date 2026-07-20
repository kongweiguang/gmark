# @author kongweiguang

"""Generate deterministic binary seeds for the recovery journal frame decoder."""

from pathlib import Path
import struct
import zlib


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "fuzz" / "corpus" / "recovery_journal_frames"
MAX_RECORD_BYTES = 128 * 1024 * 1024


def frame(kind: int, payload: bytes) -> bytes:
    header = struct.pack("<4sHBBQI", b"GMRJ", 1, kind, 0, len(payload), zlib.crc32(payload))
    return header + payload


def main() -> None:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    base = frame(
        1,
        b'{"document_id":"seed","path":null,"source":"# Recovery seed\\n","mode":"live"}',
    )
    edit = frame(2, b'{"start":2,"end":10,"replacement":"Journal","selection":9}')
    corrupt_edit = bytearray(edit)
    corrupt_edit[-1] ^= 0x80

    bad_version = bytearray(base)
    bad_version[4:6] = struct.pack("<H", 2)
    bad_kind = bytearray(base)
    bad_kind[6] = 0xFF
    bad_flags = bytearray(base)
    bad_flags[7] = 0x01
    oversized = bytearray(frame(1, b""))
    oversized[8:16] = struct.pack("<Q", MAX_RECORD_BYTES + 1)

    seeds = {
        "valid-base.seed": base,
        "valid-edit.seed": edit,
        "concatenated.seed": base + edit,
        "crc-tail.seed": base + corrupt_edit,
        "truncated.seed": base + edit[:-7],
        "bad-version.seed": bytes(bad_version),
        "bad-kind.seed": bytes(bad_kind),
        "bad-flags.seed": bytes(bad_flags),
        "oversized-length.seed": bytes(oversized),
    }
    for name, contents in seeds.items():
        (OUTPUT / name).write_bytes(contents)


if __name__ == "__main__":
    main()
