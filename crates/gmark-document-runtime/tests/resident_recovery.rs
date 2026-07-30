// @author kongweiguang

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use gmark_document::{SourceDocument, TextEdit, Transaction as DocumentTransaction};
use gmark_document_core::{
    DocumentRevision, DocumentViewId, RecoveryAction, RecoveryBackend, RecoveryRecord,
    SourceAffinity, SourceAnchor, SourceEdit, SourceSelection, Transaction,
};
use gmark_document_runtime::{
    ResidentRecoveryJournal, ResidentRecoveryReadStatus, cleanup_resident_recovery_artifacts,
    load_resident_recovery_documents, load_resident_recovery_journals,
    replay_resident_recovery_journal,
};
use gmark_recovery_codec::{RecordKind, decode_record, encode_record_payload};
use serde_json::json;

fn selection(offset: u64) -> SourceSelection {
    SourceSelection::collapsed(offset, SourceAffinity::After)
}

fn legacy_base_frame() -> Vec<u8> {
    // This literal is the V1 root-crate BaseRecord schema, not a runtime-private type.
    const LEGACY_BASE_PAYLOAD: &[u8] = br#"{"document_id":"legacy","file_path":null,"fingerprint":null,"source":"a\nb","selection":null,"view_mode":null}"#;
    encode_record_payload(RecordKind::Base, LEGACY_BASE_PAYLOAD).unwrap()
}

fn append_json_frame(path: &Path, kind: RecordKind, value: serde_json::Value) {
    let payload = serde_json::to_vec(&value).unwrap();
    let frame = encode_record_payload(kind, &payload).unwrap();
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(&frame).unwrap();
    file.sync_data().unwrap();
}

fn frame_counts(path: &Path) -> (usize, usize) {
    let bytes = fs::read(path).unwrap();
    let mut cursor = 0usize;
    let mut bases = 0usize;
    let mut edits = 0usize;
    while cursor < bytes.len() {
        let record = decode_record(&bytes, cursor).unwrap().unwrap();
        match record.kind {
            RecordKind::Base => bases += 1,
            RecordKind::Edit => edits += 1,
        }
        cursor = record.next;
    }
    (bases, edits)
}

#[test]
fn recovery_preserves_source_anchor_affinity_and_direction() {
    let temporary = tempfile::tempdir().unwrap();
    let source_selection = SourceSelection {
        anchor: SourceAnchor::new(12, SourceAffinity::After),
        head: SourceAnchor::new(4, SourceAffinity::Before),
    };
    let mut journal =
        ResidentRecoveryJournal::create(temporary.path(), None, "0123456789abcdef").unwrap();
    journal
        .record("0123456789abcdef!", source_selection, "source")
        .unwrap();

    let recovered = replay_resident_recovery_journal(journal.path()).unwrap();
    assert_eq!(recovered.selection, source_selection);
}

#[test]
fn directory_loader_retains_absent_legacy_selection_affinities() {
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = ResidentRecoveryJournal::create(temporary.path(), None, "base").unwrap();
    let source = SourceDocument::new("edited");
    journal
        .record_formatted_with_affinities(
            &source.text(),
            source.source_format(),
            selection(6),
            None,
            None,
            "source",
        )
        .unwrap();

    let recovered = load_resident_recovery_journals(temporary.path()).unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].document.source, "edited");
    assert_eq!(recovered[0].anchor_affinity, None);
    assert_eq!(recovered[0].head_affinity, None);

    let bytes = fs::read(journal.path()).unwrap();
    let base = decode_record(&bytes, 0).unwrap().unwrap();
    let edit = decode_record(&bytes, base.next).unwrap().unwrap();
    let payload: serde_json::Value = serde_json::from_slice(edit.payload).unwrap();
    assert!(payload["selection"].get("anchor_affinity").is_none());
    assert!(payload["selection"].get("head_affinity").is_none());
}

#[test]
fn recovery_replays_utf8_edits_selection_and_view_mode() {
    let temporary = tempfile::tempdir().unwrap();
    let mut journal =
        ResidentRecoveryJournal::create(temporary.path(), None, "alpha 中文").unwrap();
    assert!(
        journal
            .record("alpha 中文!", selection(13), "rendered")
            .unwrap()
    );
    assert!(
        journal
            .record("alpha 世界!", selection(12), "source")
            .unwrap()
    );

    let recovered = replay_resident_recovery_journal(journal.path()).unwrap();
    assert_eq!(recovered.source, "alpha 世界!");
    assert_eq!(recovered.selection, selection(12));
    assert_eq!(recovered.view_mode, "source");
    assert_eq!(recovered.read_status, ResidentRecoveryReadStatus::Complete);
}

#[test]
fn recovery_restores_bom_and_mixed_line_ending_patches() {
    let temporary = tempfile::tempdir().unwrap();
    let original = "\u{feff}a\r\nb\nc\rd";
    let mut document = SourceDocument::new(original);
    let mut journal = ResidentRecoveryJournal::create(temporary.path(), None, original).unwrap();
    document
        .apply_transaction(DocumentTransaction::new(
            document.revision(),
            vec![TextEdit::new(4..5, "B\nX")],
        ))
        .unwrap();
    journal
        .record_formatted(
            &document.text(),
            document.source_format(),
            selection(7),
            "source",
        )
        .unwrap();

    let recovered = replay_resident_recovery_journal(journal.path()).unwrap();
    let restored = SourceDocument::from_normalized(
        &recovered.source,
        recovered.source_format,
        SourceDocument::DEFAULT_HISTORY_LIMIT,
    )
    .unwrap();
    assert_eq!(restored.serialized_bytes(), document.serialized_bytes());
}

#[test]
fn format_only_edit_replays_exact_serialization_format() {
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = ResidentRecoveryJournal::create(temporary.path(), None, "a\nb").unwrap();
    let crlf_format = SourceDocument::new("a\r\nb").source_format();
    assert!(
        journal
            .record_formatted("a\nb", crlf_format, selection(3), "source")
            .unwrap()
    );

    let recovered = replay_resident_recovery_journal(journal.path()).unwrap();
    let restored =
        SourceDocument::from_normalized(&recovered.source, recovered.source_format, 0).unwrap();
    assert_eq!(restored.serialized_bytes(), b"a\r\nb");
}

#[test]
fn legacy_v1_base_bytes_default_to_lf_format() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("legacy.journal");
    fs::write(&path, legacy_base_frame()).unwrap();

    let recovered = replay_resident_recovery_journal(&path).unwrap();
    assert_eq!(recovered.source, "a\nb");
    assert_eq!(
        recovered.source_format,
        SourceDocument::new("a\nb").source_format()
    );
}

#[test]
fn emitted_frames_keep_the_v1_base_and_edit_json_schema() {
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = ResidentRecoveryJournal::create(temporary.path(), None, "base").unwrap();
    journal.record("edited", selection(6), "source").unwrap();
    let bytes = fs::read(journal.path()).unwrap();

    let base_frame = decode_record(&bytes, 0).unwrap().unwrap();
    assert_eq!(base_frame.kind, RecordKind::Base);
    let base: serde_json::Value = serde_json::from_slice(base_frame.payload).unwrap();
    assert!(
        base["document_id"]
            .as_str()
            .is_some_and(|document_id| !document_id.is_empty())
    );
    assert_eq!(base["file_path"], json!(null));
    assert_eq!(base["fingerprint"], json!(null));
    assert_eq!(base["source"], json!("base"));
    assert_eq!(
        base["source_format"],
        json!({"utf8_bom": false, "endings": [], "dominant": "lf"})
    );
    assert_eq!(base["selection"], json!(null));
    assert_eq!(base["view_mode"], json!(null));

    let edit_frame = decode_record(&bytes, base_frame.next).unwrap().unwrap();
    assert_eq!(edit_frame.kind, RecordKind::Edit);
    let edit: serde_json::Value = serde_json::from_slice(edit_frame.payload).unwrap();
    assert_eq!(
        edit,
        json!({
            "start": 0,
            "end": 4,
            "replacement": "edited",
            "selection": {
                "start": 6,
                "end": 6,
                "reversed": false,
                "anchor_affinity": "after",
                "head_affinity": "after"
            },
            "view_mode": "source",
            "format_patch": {
                "start": 0,
                "removed": 0,
                "inserted": [],
                "utf8_bom": false,
                "dominant": "lf"
            }
        })
    );
}

#[test]
fn truncated_tail_recovers_last_crc_valid_edit() {
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = ResidentRecoveryJournal::create(temporary.path(), None, "one").unwrap();
    journal.record("one two", selection(7), "rendered").unwrap();
    journal
        .record("one two three", selection(13), "rendered")
        .unwrap();
    let path = journal.path().to_path_buf();
    let mut bytes = fs::read(&path).unwrap();
    bytes.truncate(bytes.len() - 8);
    fs::write(&path, bytes).unwrap();

    let recovered = replay_resident_recovery_journal(&path).unwrap();
    assert_eq!(recovered.source, "one two");
    assert_eq!(recovered.selection, selection(7));
    assert_eq!(
        recovered.read_status,
        ResidentRecoveryReadStatus::TruncatedTail
    );
}

#[test]
fn invalid_format_patch_recovers_previous_valid_record() {
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = ResidentRecoveryJournal::create(temporary.path(), None, "a\r\nb").unwrap();
    let mut document = SourceDocument::new("a\r\nb");
    document
        .apply_transaction(DocumentTransaction::new(
            document.revision(),
            vec![TextEdit::new(2..3, "B")],
        ))
        .unwrap();
    journal
        .record_formatted(
            &document.text(),
            document.source_format(),
            selection(3),
            "source",
        )
        .unwrap();
    append_json_frame(
        journal.path(),
        RecordKind::Edit,
        json!({
            "start": 0,
            "end": 0,
            "replacement": "\n",
            "selection": {
                "start": 0,
                "end": 0,
                "reversed": false,
                "anchor_affinity": null,
                "head_affinity": null
            },
            "view_mode": "source",
            "format_patch": {
                "start": 99,
                "removed": 0,
                "inserted": ["lf"],
                "utf8_bom": false,
                "dominant": "cr_lf"
            }
        }),
    );

    let recovered = replay_resident_recovery_journal(journal.path()).unwrap();
    assert_eq!(recovered.source, "a\nB");
    assert_eq!(
        recovered.read_status,
        ResidentRecoveryReadStatus::TruncatedTail
    );
}

#[test]
fn checkpoint_restarts_base_and_discard_removes_the_session() {
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = ResidentRecoveryJournal::create(temporary.path(), None, "one").unwrap();
    journal.record("two", selection(3), "rendered").unwrap();
    let path = journal.path().to_path_buf();
    assert!(path.exists());
    journal.checkpoint(None, "two").unwrap();
    assert!(!path.exists());
    journal.record("three", selection(5), "rendered").unwrap();
    assert_eq!(
        replay_resident_recovery_journal(&path).unwrap().source,
        "three"
    );
    journal.discard().unwrap();
    assert!(!path.exists());
}

#[test]
fn resume_appends_to_the_replayed_baseline() {
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = ResidentRecoveryJournal::create(temporary.path(), None, "one").unwrap();
    journal.record("two", selection(3), "rendered").unwrap();
    let path = journal.path().to_path_buf();
    let recovered = replay_resident_recovery_journal(&path).unwrap();
    let mut resumed = ResidentRecoveryJournal::resume(&recovered);
    resumed.record("three", selection(5), "source").unwrap();

    let recovered = replay_resident_recovery_journal(&path).unwrap();
    assert_eq!(recovered.source, "three");
    assert_eq!(recovered.selection, selection(5));
    assert_eq!(recovered.view_mode, "source");
}

#[test]
fn fingerprint_marks_external_base_change_without_losing_recovery() {
    let temporary = tempfile::tempdir().unwrap();
    let file = temporary.path().join("doc.md");
    fs::write(&file, "base").unwrap();
    let mut journal =
        ResidentRecoveryJournal::create(temporary.path(), Some(file.clone()), "base").unwrap();
    fs::write(&file, "external").unwrap();
    journal.record("edited", selection(6), "rendered").unwrap();

    let recovered = replay_resident_recovery_journal(journal.path()).unwrap();
    assert_eq!(recovered.source, "edited");
    assert!(recovered.base_file_changed);
}

#[test]
fn directory_loader_quarantines_bad_neighbor_and_keeps_live_sessions() {
    let temporary = tempfile::tempdir().unwrap();
    let mut valid = ResidentRecoveryJournal::create(temporary.path(), None, "").unwrap();
    valid.record("valid", selection(5), "rendered").unwrap();
    let invalid = temporary.path().join("old.journal");
    let mut bytes = legacy_base_frame();
    bytes[4..6].copy_from_slice(&99u16.to_le_bytes());
    fs::write(&invalid, bytes).unwrap();

    let recovered = load_resident_recovery_journals(temporary.path()).unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].document.source, "valid");
    let quarantined = temporary.path().join("old.journal.invalid");
    assert!(!invalid.exists());
    assert!(quarantined.exists());
    let projected = load_resident_recovery_documents(temporary.path()).unwrap();
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].source, "valid");
    assert_eq!(
        cleanup_resident_recovery_artifacts(temporary.path()).unwrap(),
        1
    );
    assert!(!quarantined.exists());
    assert!(valid.path().exists());
}

#[test]
fn directory_loader_skips_a_suppressed_stale_journal_and_keeps_the_active_session()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let mut stale = ResidentRecoveryJournal::create(temporary.path(), None, "stale")?;
    stale.record("stale edit", selection(10), "rendered")?;
    let stale_marker = stale.path().with_extension("journal.suppressed");
    fs::write(&stale_marker, b"gmark-recovery-suppressed-v1\n")?;
    let blocked = temporary.path().join("blocked.journal");
    fs::create_dir(&blocked)?;
    let blocked_marker = blocked.with_extension("journal.suppressed");
    fs::write(&blocked_marker, b"gmark-recovery-suppressed-v1\n")?;

    let mut active = ResidentRecoveryJournal::create(temporary.path(), None, "active")?;
    active.record("active edit", selection(11), "source")?;

    let recovered = load_resident_recovery_journals(temporary.path())?;
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].document.source, "active edit");
    assert!(!stale.path().exists());
    assert!(!stale_marker.exists());
    assert!(blocked.exists());
    assert!(blocked_marker.exists());
    Ok(())
}

#[test]
fn append_failure_keeps_in_memory_baseline_retryable() {
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = ResidentRecoveryJournal::create(temporary.path(), None, "a").unwrap();
    journal.record("ab", selection(2), "rendered").unwrap();
    let path = journal.path().to_path_buf();
    let valid_prefix = fs::read(&path).unwrap();

    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(journal.record("abc", selection(3), "rendered").is_err());

    fs::remove_dir(&path).unwrap();
    fs::write(&path, valid_prefix).unwrap();
    assert!(journal.record("abc", selection(3), "rendered").unwrap());
    assert_eq!(
        replay_resident_recovery_journal(&path).unwrap().source,
        "abc"
    );
}

#[test]
fn long_sessions_compact_to_one_base_and_a_short_edit_tail() {
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = ResidentRecoveryJournal::create(temporary.path(), None, "").unwrap();
    let mut source = String::new();
    for index in 0..270 {
        source.push(char::from(b'a' + (index % 26) as u8));
        journal
            .record(&source, selection(source.len() as u64), "rendered")
            .unwrap();
    }

    let recovered = replay_resident_recovery_journal(journal.path()).unwrap();
    assert_eq!(recovered.source, source);
    assert_eq!(recovered.selection, selection(270));
    let (bases, edits) = frame_counts(journal.path());
    assert_eq!(bases, 1);
    assert!(edits < 20);
}

#[test]
fn recovery_backend_accepts_shared_source_transactions() {
    let temporary = tempfile::tempdir().unwrap();
    let mut journal = ResidentRecoveryJournal::create(temporary.path(), None, "base").unwrap();
    let source_selection = selection(6);
    RecoveryBackend::record(
        &mut journal,
        &RecoveryRecord {
            action: RecoveryAction::Transaction(Transaction::new(
                DocumentRevision(0),
                vec![SourceEdit::new(0..4, "edited")],
            )),
            selection: Some(source_selection),
            view_id: DocumentViewId::source(),
        },
    )
    .unwrap();

    let recovered = replay_resident_recovery_journal(journal.path()).unwrap();
    assert_eq!(recovered.source, "edited");
    assert_eq!(recovered.selection, source_selection);
    assert_eq!(recovered.view_mode, "source");
}
