// @author kongweiguang

use gmark_document::{
    DocumentError, LineEnding, LineEndingStatus, Revision, SourceDocument, TextEdit, Transaction,
};

#[test]
fn transaction_uses_base_coordinates_and_supports_undo_redo() {
    let mut document = SourceDocument::new("alpha beta gamma");
    let transaction = Transaction::new(
        document.revision(),
        vec![TextEdit::new(0..5, "A"), TextEdit::new(11..16, "G")],
    );

    let applied = document
        .apply_transaction(transaction)
        .expect("合法 transaction 应成功");
    assert_eq!(applied.text(), "A beta G");
    assert_eq!(applied.revision().get(), 1);

    let undone = document
        .undo()
        .expect("撤销不应失败")
        .expect("存在可撤销历史");
    assert_eq!(undone.text(), "alpha beta gamma");
    assert_eq!(undone.revision().get(), 2);

    let redone = document
        .redo()
        .expect("重做不应失败")
        .expect("存在可重做历史");
    assert_eq!(redone.text(), "A beta G");
    assert_eq!(redone.revision().get(), 3);
}

#[test]
fn stale_transaction_is_rejected_without_mutation() {
    let mut document = SourceDocument::new("text");
    let stale = Transaction::new(Revision::from_u64(9), vec![TextEdit::new(0..4, "other")]);

    let error = document
        .apply_transaction(stale)
        .expect_err("过期 transaction 必须失败");

    assert!(matches!(error, DocumentError::StaleRevision { .. }));
    assert_eq!(document.text(), "text");
    assert_eq!(document.revision(), Revision::INITIAL);
    assert!(!document.can_undo());
}

#[test]
fn all_ranges_are_validated_before_any_mutation() {
    let mut document = SourceDocument::new("abcdef");
    let transaction = Transaction::new(
        document.revision(),
        vec![TextEdit::new(0..1, "A"), TextEdit::new(4..9, "invalid")],
    );

    let error = document
        .apply_transaction(transaction)
        .expect_err("包含越界范围的 transaction 必须整体失败");

    assert!(matches!(error, DocumentError::InvalidRange { .. }));
    assert_eq!(document.text(), "abcdef");
}

#[test]
fn overlapping_and_unordered_edits_are_rejected() {
    let mut document = SourceDocument::new("abcdef");
    let overlap = Transaction::new(
        document.revision(),
        vec![TextEdit::new(1..4, "x"), TextEdit::new(3..5, "y")],
    );
    assert!(matches!(
        document.apply_transaction(overlap),
        Err(DocumentError::OverlappingEdits { .. })
    ));

    let unordered = Transaction::new(
        document.revision(),
        vec![TextEdit::new(4..5, "x"), TextEdit::new(1..2, "y")],
    );
    assert!(matches!(
        document.apply_transaction(unordered),
        Err(DocumentError::UnorderedEdits { .. })
    ));
    assert_eq!(document.text(), "abcdef");
}

#[test]
fn utf8_character_boundaries_are_enforced() {
    let mut document = SourceDocument::new("A中B");
    let transaction = Transaction::new(document.revision(), vec![TextEdit::new(2..4, "x")]);

    assert!(matches!(
        document.apply_transaction(transaction),
        Err(DocumentError::InvalidCharacterBoundary { .. })
    ));
    assert_eq!(document.text(), "A中B");
}

#[test]
fn snapshot_remains_stable_after_later_edits() {
    let mut document = SourceDocument::new("before");
    let snapshot = document.snapshot();
    document
        .apply_transaction(Transaction::new(
            document.revision(),
            vec![TextEdit::new(0..6, "after")],
        ))
        .expect("合法替换应成功");

    assert_eq!(snapshot.text(), "before");
    assert_eq!(snapshot.revision(), Revision::INITIAL);
    assert_eq!(document.text(), "after");
}

#[test]
fn history_limit_evicts_oldest_transaction() {
    let mut document = SourceDocument::with_history_limit("a", 1);
    document
        .apply_transaction(Transaction::new(
            document.revision(),
            vec![TextEdit::new(1..1, "b")],
        ))
        .expect("第一次编辑应成功");
    document
        .apply_transaction(Transaction::new(
            document.revision(),
            vec![TextEdit::new(2..2, "c")],
        ))
        .expect("第二次编辑应成功");

    document
        .undo()
        .expect("撤销不应失败")
        .expect("最近历史应保留");
    assert_eq!(document.text(), "ab");
    assert!(document.undo().expect("空撤销栈不应失败").is_none());
}

#[test]
fn bom_and_mixed_line_endings_round_trip_exactly() {
    let original = "\u{feff}alpha\r\nbeta\ngamma\rdelta";
    let document = SourceDocument::new(original);

    assert_eq!(document.text(), "alpha\nbeta\ngamma\ndelta");
    assert_eq!(document.serialized_bytes(), original.as_bytes());
    assert_eq!(
        document.source_format().endings,
        vec![LineEnding::CrLf, LineEnding::Lf, LineEnding::Cr]
    );
}

#[test]
fn immutable_snapshot_restores_formatted_bytes_for_background_save() {
    let document = SourceDocument::new("\u{feff}a\r\nb\nc\rd");
    let snapshot = document.snapshot();
    let (text, bytes) = snapshot
        .formatted_text_and_bytes(document.source_format())
        .expect("snapshot format should match normalized text");

    assert_eq!(text, "a\nb\nc\nd");
    assert_eq!(bytes, b"\xef\xbb\xbfa\r\nb\nc\rd");
}

#[test]
fn content_edit_preserves_existing_line_endings() {
    let mut document = SourceDocument::new("alpha\r\nbeta\ngamma\r");
    document
        .apply_transaction(Transaction::new(
            document.revision(),
            vec![TextEdit::new(6..10, "BETA")],
        ))
        .unwrap();

    assert_eq!(document.serialized_bytes(), b"alpha\r\nBETA\ngamma\r");
}

#[test]
fn inserted_newline_uses_nearest_left_ending_then_dominant() {
    let mut document = SourceDocument::new("alpha\r\nbeta\ngamma");
    document
        .apply_transaction(Transaction::new(
            document.revision(),
            vec![TextEdit::new(11..11, "one\ntwo\n")],
        ))
        .unwrap();

    assert_eq!(
        document.serialized_bytes(),
        b"alpha\r\nbeta\none\ntwo\ngamma"
    );
}

#[test]
fn deleting_and_undoing_restores_removed_ending_styles() {
    let mut document = SourceDocument::new("a\r\nb\nc\rd");
    document
        .apply_transaction(Transaction::new(
            document.revision(),
            vec![TextEdit::new(1..5, "\n")],
        ))
        .unwrap();
    let edited = document.serialized_bytes();

    document.undo().unwrap().unwrap();
    assert_eq!(document.serialized_bytes(), b"a\r\nb\nc\rd");
    document.redo().unwrap().unwrap();
    assert_eq!(document.serialized_bytes(), edited);
}

#[test]
fn editing_updates_dominant_ending_counts_across_undo_and_redo() {
    let mut document = SourceDocument::new("a\r\nb\r\nc\n");
    assert_eq!(document.source_format().dominant, LineEnding::CrLf);
    assert_eq!(
        document.source_format_summary().line_endings,
        LineEndingStatus::Mixed
    );

    document
        .apply_transaction(Transaction::new(
            document.revision(),
            vec![TextEdit::new(1..4, "")],
        ))
        .unwrap();
    assert_eq!(document.source_format().dominant, LineEnding::Lf);
    assert_eq!(
        document.source_format_summary().line_endings,
        LineEndingStatus::Uniform(LineEnding::Lf)
    );

    document.undo().unwrap().unwrap();
    assert_eq!(document.source_format().dominant, LineEnding::CrLf);
    assert_eq!(
        document.source_format_summary().line_endings,
        LineEndingStatus::Mixed
    );
    document.redo().unwrap().unwrap();
    assert_eq!(document.source_format().dominant, LineEnding::Lf);
}

#[test]
fn multi_edit_utf8_transaction_keeps_unaffected_ending_styles() {
    let mut document = SourceDocument::new("中\r\na\nb\rc");
    document
        .apply_transaction(Transaction::new(
            document.revision(),
            vec![TextEdit::new(0..3, "文"), TextEdit::new(6..7, "B\nX")],
        ))
        .unwrap();

    assert_eq!(document.serialized_bytes(), "文\r\na\nB\nX\rc".as_bytes());
    document.undo().unwrap().unwrap();
    assert_eq!(document.serialized_bytes(), "中\r\na\nb\rc".as_bytes());
}

#[test]
fn deleting_all_newlines_keeps_dominant_style_for_later_insertions() {
    let mut document = SourceDocument::new("a\r\nb");
    document
        .apply_transaction(Transaction::new(
            document.revision(),
            vec![TextEdit::new(1..2, "")],
        ))
        .unwrap();
    document
        .apply_transaction(Transaction::new(
            document.revision(),
            vec![TextEdit::new(1..1, "\n")],
        ))
        .unwrap();

    assert_eq!(document.serialized_bytes(), b"a\r\nb");
}

#[test]
fn explicit_line_ending_normalization_is_revisioned_and_undoable() {
    let original = "\u{feff}a\r\nb\nc\rd";
    let mut document = SourceDocument::new(original);
    let initial_revision = document.revision();

    let normalized = document
        .normalize_line_endings(LineEnding::CrLf)
        .unwrap()
        .expect("mixed document should change");
    assert!(normalized.revision() > initial_revision);
    assert_eq!(document.serialized_bytes(), b"\xef\xbb\xbfa\r\nb\r\nc\r\nd");

    document.undo().unwrap().unwrap();
    assert_eq!(document.serialized_bytes(), original.as_bytes());
    document.redo().unwrap().unwrap();
    assert_eq!(document.serialized_bytes(), b"\xef\xbb\xbfa\r\nb\r\nc\r\nd");
    assert!(
        document
            .normalize_line_endings(LineEnding::CrLf)
            .unwrap()
            .is_none()
    );
}
