// @author kongweiguang

use super::*;

fn selection(offset: usize) -> RecoverySelection {
    RecoverySelection {
        start: offset,
        end: offset,
        reversed: false,
        anchor_affinity: None,
        head_affinity: None,
    }
}

#[test]
fn recovery_journal_preserves_source_anchor_affinity_and_direction() {
    let temp = tempfile::tempdir().unwrap();
    let source_selection = gmark_large_document::SourceSelection {
        anchor: gmark_large_document::SourceAnchor::new(
            12,
            gmark_large_document::SourceAffinity::After,
        ),
        head: gmark_large_document::SourceAnchor::new(
            4,
            gmark_large_document::SourceAffinity::Before,
        ),
    };
    let persisted = RecoverySelection::from_source_selection(source_selection);
    let mut journal =
        RecoveryJournal::create(temp.path(), None, "0123456789abcdef".to_owned()).unwrap();
    journal
        .record("0123456789abcdef!", persisted, "source")
        .unwrap();

    let recovered = replay_journal(journal.path()).unwrap();
    assert_eq!(recovered.selection.source_selection(), source_selection);
}

#[test]
fn journal_replays_utf8_edits_and_selection() {
    let temp = tempfile::tempdir().unwrap();
    let mut journal = RecoveryJournal::create(temp.path(), None, "alpha 中文".to_owned()).unwrap();
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

    let recovered = replay_journal(&journal.journal_path).unwrap();
    assert_eq!(recovered.source, "alpha 世界!");
    assert_eq!(recovered.selection, selection(12));
    assert_eq!(recovered.view_mode, "source");
    assert_eq!(recovered.read_status, RecoveryReadStatus::Complete);
}

#[test]
fn journal_restores_bom_and_mixed_line_ending_patches() {
    let temp = tempfile::tempdir().unwrap();
    let original = "\u{feff}a\r\nb\nc\rd";
    let mut document = gmark_document::SourceDocument::new(original);
    let mut journal = RecoveryJournal::create(temp.path(), None, original.to_owned()).unwrap();
    document
        .apply_transaction(gmark_document::Transaction::new(
            document.revision(),
            vec![gmark_document::TextEdit::new(4..5, "B\nX")],
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

    let recovered = replay_journal(&journal.journal_path).unwrap();
    let restored = gmark_document::SourceDocument::from_normalized(
        &recovered.source,
        recovered.source_format,
        gmark_document::SourceDocument::DEFAULT_HISTORY_LIMIT,
    )
    .unwrap();
    assert_eq!(restored.serialized_bytes(), document.serialized_bytes());
}

#[test]
fn old_journal_without_format_defaults_to_lf() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("legacy.journal");
    let base = BaseRecord {
        document_id: "legacy".to_owned(),
        file_path: None,
        fingerprint: None,
        source: "a\nb".to_owned(),
        source_format: None,
        selection: None,
        view_mode: None,
    };
    fs::write(&path, encode_record(RecordKind::Base, &base).unwrap()).unwrap();

    let recovered = replay_journal(&path).unwrap();
    assert_eq!(recovered.source_format, default_source_format("a\nb"));
}

#[test]
fn truncated_or_corrupt_tail_recovers_last_crc_valid_record() {
    let temp = tempfile::tempdir().unwrap();
    let mut journal = RecoveryJournal::create(temp.path(), None, "one".to_owned()).unwrap();
    journal.record("one two", selection(7), "rendered").unwrap();
    journal
        .record("one two three", selection(13), "rendered")
        .unwrap();
    let mut bytes = fs::read(&journal.journal_path).unwrap();
    bytes.truncate(bytes.len() - 8);
    fs::write(&journal.journal_path, bytes).unwrap();

    let recovered = replay_journal(&journal.journal_path).unwrap();
    assert_eq!(recovered.source, "one two");
    assert_eq!(recovered.selection, selection(7));
    assert_eq!(recovered.read_status, RecoveryReadStatus::TruncatedTail);
}

#[test]
fn forced_process_termination_recovers_last_synced_record() {
    const CHILD_DIR_ENV: &str = "GMARK_TEST_RECOVERY_CRASH_DIR";

    if let Some(recovery_dir) = std::env::var_os(CHILD_DIR_ENV) {
        let recovery_dir = PathBuf::from(recovery_dir);
        let mut journal = RecoveryJournal::create(&recovery_dir, None, "one".to_owned())
            .expect("create child recovery journal");
        journal
            .record("one two", selection(7), "rendered")
            .expect("sync last complete recovery record");

        // 模拟进程在下一帧只写入一半时被系统强制终止；析构和清理不会运行。
        let partial = encode_record(
            RecordKind::Edit,
            &EditRecord {
                start: 7,
                end: 7,
                replacement: " three".to_owned(),
                selection: selection(13),
                view_mode: "source".to_owned(),
                format_patch: None,
            },
        )
        .expect("encode partial recovery record");
        let mut file = OpenOptions::new()
            .append(true)
            .open(journal.path())
            .expect("open child journal tail");
        file.write_all(&partial[..partial.len() / 2])
            .expect("write partial recovery record");
        file.flush().expect("flush partial recovery record");
        std::process::abort();
    }

    let temp = tempfile::tempdir().expect("create parent recovery directory");
    let current_test = std::env::current_exe().expect("resolve current test executable");
    let status = std::process::Command::new(current_test)
        .arg("--exact")
        .arg("recovery::tests::forced_process_termination_recovers_last_synced_record")
        .arg("--nocapture")
        .env(CHILD_DIR_ENV, temp.path())
        .status()
        .expect("launch crash child");
    assert!(!status.success(), "crash child unexpectedly exited cleanly");

    let recovered = load_recovery_documents(temp.path()).expect("scan crash recovery directory");
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].source, "one two");
    assert_eq!(recovered[0].selection, selection(7));
    assert_eq!(recovered[0].read_status, RecoveryReadStatus::TruncatedTail);
}

#[test]
fn invalid_format_patch_recovers_previous_valid_record() {
    let temp = tempfile::tempdir().unwrap();
    let mut journal = RecoveryJournal::create(temp.path(), None, "a\r\nb".to_owned()).unwrap();
    let mut document = gmark_document::SourceDocument::new("a\r\nb");
    document
        .apply_transaction(gmark_document::Transaction::new(
            document.revision(),
            vec![gmark_document::TextEdit::new(2..3, "B")],
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
    append_record(
        &journal.journal_path,
        RecordKind::Edit,
        &EditRecord {
            start: 0,
            end: 0,
            replacement: "\n".to_owned(),
            selection: selection(0),
            view_mode: "source".to_owned(),
            format_patch: Some(RecoveryFormatPatch {
                start: 99,
                removed: 0,
                inserted: vec![RecoveryLineEnding::Lf],
                utf8_bom: false,
                dominant: RecoveryLineEnding::CrLf,
            }),
        },
    )
    .unwrap();

    let recovered = replay_journal(&journal.journal_path).unwrap();
    assert_eq!(recovered.source, "a\nB");
    assert_eq!(recovered.read_status, RecoveryReadStatus::TruncatedTail);
}

#[test]
fn checkpoint_removes_session_and_restarts_from_saved_base() {
    let temp = tempfile::tempdir().unwrap();
    let mut journal = RecoveryJournal::create(temp.path(), None, "one".to_owned()).unwrap();
    journal.record("two", selection(3), "rendered").unwrap();
    assert!(journal.journal_path.exists());
    journal.checkpoint(None, "two".to_owned()).unwrap();
    assert!(!journal.journal_path.exists());
    journal.record("three", selection(5), "rendered").unwrap();
    assert_eq!(
        replay_journal(&journal.journal_path).unwrap().source,
        "three"
    );
}

#[test]
fn fingerprint_marks_external_base_change_without_losing_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("doc.md");
    fs::write(&file, "base").unwrap();
    let mut journal =
        RecoveryJournal::create(temp.path(), Some(file.clone()), "base".to_owned()).unwrap();
    // 外部改动发生在 2 秒 debounce 的首次 journal 写入前，基线仍必须是打开时版本。
    fs::write(&file, "external").unwrap();
    journal.record("edited", selection(6), "rendered").unwrap();

    let recovered = replay_journal(&journal.journal_path).unwrap();
    assert_eq!(recovered.source, "edited");
    assert!(recovered.base_file_changed);
}

#[test]
fn unsupported_version_is_rejected_instead_of_misparsed() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("old.journal");
    let mut bytes = encode_record(
        RecordKind::Base,
        &BaseRecord {
            document_id: "old".to_owned(),
            file_path: None,
            fingerprint: None,
            source: String::new(),
            source_format: None,
            selection: None,
            view_mode: None,
        },
    )
    .unwrap();
    bytes[4..6].copy_from_slice(&99u16.to_le_bytes());
    fs::write(&path, bytes).unwrap();
    assert!(
        replay_journal(&path)
            .unwrap_err()
            .to_string()
            .contains("version 99")
    );
}

#[test]
fn scan_quarantines_unreadable_versions_and_keeps_valid_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let mut valid = RecoveryJournal::create(temp.path(), None, String::new()).unwrap();
    valid.record("valid", selection(5), "rendered").unwrap();
    let invalid = temp.path().join("old.journal");
    let mut bytes = encode_record(
        RecordKind::Base,
        &BaseRecord {
            document_id: "old".to_owned(),
            file_path: None,
            fingerprint: None,
            source: String::new(),
            source_format: None,
            selection: None,
            view_mode: None,
        },
    )
    .unwrap();
    bytes[4..6].copy_from_slice(&99u16.to_le_bytes());
    fs::write(&invalid, bytes).unwrap();

    let recovered = load_recovery_documents(temp.path()).unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].source, "valid");
    assert!(!invalid.exists());
    assert!(temp.path().join("old.journal.invalid").exists());
}

#[test]
fn append_failure_does_not_advance_in_memory_source_and_retry_is_complete() {
    let temp = tempfile::tempdir().unwrap();
    let mut journal = RecoveryJournal::create(temp.path(), None, "a".to_owned()).unwrap();
    journal.record("ab", selection(2), "rendered").unwrap();
    let path = journal.journal_path.clone();
    let valid_prefix = fs::read(&path).unwrap();

    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(journal.record("abc", selection(3), "rendered").is_err());

    fs::remove_dir(&path).unwrap();
    fs::write(&path, valid_prefix).unwrap();
    assert!(journal.record("abc", selection(3), "rendered").unwrap());
    assert_eq!(replay_journal(&path).unwrap().source, "abc");
}

#[test]
fn base_write_failure_remains_retryable_without_creating_false_session() {
    let temp = tempfile::tempdir().unwrap();
    let recovery_dir = temp.path().join("recovery");
    let mut journal = RecoveryJournal::create(&recovery_dir, None, "base".to_owned()).unwrap();
    fs::remove_dir(&recovery_dir).unwrap();
    fs::write(&recovery_dir, "not a directory").unwrap();
    assert!(journal.record("edited", selection(6), "rendered").is_err());

    fs::remove_file(&recovery_dir).unwrap();
    fs::create_dir(&recovery_dir).unwrap();
    assert!(journal.record("edited", selection(6), "rendered").unwrap());
    assert_eq!(
        replay_journal(&journal.journal_path).unwrap().source,
        "edited"
    );
}

#[test]
fn long_session_compacts_atomically_and_preserves_latest_mode_and_selection() {
    let temp = tempfile::tempdir().unwrap();
    let mut journal = RecoveryJournal::create(temp.path(), None, String::new()).unwrap();
    let mut source = String::new();
    for index in 0..270 {
        source.push(char::from(b'a' + (index % 26) as u8));
        journal
            .record(
                &source,
                selection(source.len()),
                if index == 256 { "split" } else { "rendered" },
            )
            .unwrap();
    }

    let recovered = replay_journal(&journal.journal_path).unwrap();
    assert_eq!(recovered.source, source);
    assert_eq!(recovered.selection, selection(source.len()));
    assert_eq!(recovered.view_mode, "rendered");
    assert!(journal.edit_count < 20);
}
