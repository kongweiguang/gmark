// @author kongweiguang

use gmark_fuzz_support::{run_recovery_frame_program, run_source_document_program};

const CORPUS: &[(&str, &[u8])] = &[
    (
        "ascii",
        include_bytes!("../../../fuzz/corpus/source_document_transactions/ascii.seed"),
    ),
    (
        "unicode",
        include_bytes!("../../../fuzz/corpus/source_document_transactions/unicode.seed"),
    ),
    (
        "markdown",
        include_bytes!("../../../fuzz/corpus/source_document_transactions/markdown.seed"),
    ),
    (
        "newlines",
        include_bytes!("../../../fuzz/corpus/source_document_transactions/newlines.seed"),
    ),
    (
        "history",
        include_bytes!("../../../fuzz/corpus/source_document_transactions/history.seed"),
    ),
    (
        "invalid",
        include_bytes!("../../../fuzz/corpus/source_document_transactions/invalid.seed"),
    ),
];

const RECOVERY_CORPUS: &[(&str, &[u8])] = &[
    (
        "valid-base",
        include_bytes!("../../../fuzz/corpus/recovery_journal_frames/valid-base.seed"),
    ),
    (
        "valid-edit",
        include_bytes!("../../../fuzz/corpus/recovery_journal_frames/valid-edit.seed"),
    ),
    (
        "concatenated",
        include_bytes!("../../../fuzz/corpus/recovery_journal_frames/concatenated.seed"),
    ),
    (
        "crc-tail",
        include_bytes!("../../../fuzz/corpus/recovery_journal_frames/crc-tail.seed"),
    ),
    (
        "truncated",
        include_bytes!("../../../fuzz/corpus/recovery_journal_frames/truncated.seed"),
    ),
    (
        "bad-version",
        include_bytes!("../../../fuzz/corpus/recovery_journal_frames/bad-version.seed"),
    ),
    (
        "bad-kind",
        include_bytes!("../../../fuzz/corpus/recovery_journal_frames/bad-kind.seed"),
    ),
    (
        "bad-flags",
        include_bytes!("../../../fuzz/corpus/recovery_journal_frames/bad-flags.seed"),
    ),
    (
        "oversized-length",
        include_bytes!("../../../fuzz/corpus/recovery_journal_frames/oversized-length.seed"),
    ),
];

#[test]
fn replays_persistent_transaction_corpus() {
    for (name, input) in CORPUS {
        std::panic::catch_unwind(|| run_source_document_program(input))
            .unwrap_or_else(|_| panic!("transaction corpus seed '{name}' failed"));
    }
}

#[test]
fn deterministic_random_edit_trajectories_match_oracle() {
    for seed in 0_u64..128 {
        let mut state = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut input = vec![0_u8; 512];
        for byte in &mut input {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        std::panic::catch_unwind(|| run_source_document_program(&input))
            .unwrap_or_else(|_| panic!("deterministic trajectory seed {seed} failed"));
    }
}

#[test]
fn replays_recovery_frame_corpus_with_expected_prefixes() {
    let expected = [
        ("valid-base", 1, 97),
        ("valid-edit", 1, 78),
        ("concatenated", 2, 175),
        ("crc-tail", 1, 97),
        ("truncated", 1, 97),
        ("bad-version", 0, 0),
        ("bad-kind", 0, 0),
        ("bad-flags", 0, 0),
        ("oversized-length", 0, 0),
    ];
    for ((name, input), (expected_name, frames, bytes)) in RECOVERY_CORPUS.iter().zip(expected) {
        assert_eq!(*name, expected_name);
        let run = run_recovery_frame_program(input);
        assert_eq!(run.accepted_frames, frames, "seed '{name}' frame count");
        assert_eq!(run.accepted_bytes, bytes, "seed '{name}' prefix length");
    }
}

#[test]
fn deterministic_random_recovery_streams_never_violate_decoder_invariants() {
    for seed in 0_u64..256 {
        let mut state = seed.wrapping_add(0xD1B5_4A32_D192_ED03);
        let mut input = vec![0_u8; 4_096];
        for byte in &mut input {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        std::panic::catch_unwind(|| run_recovery_frame_program(&input))
            .unwrap_or_else(|_| panic!("deterministic recovery stream seed {seed} failed"));
    }
}
