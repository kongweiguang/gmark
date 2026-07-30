// @author kongweiguang

use super::*;

#[test]
fn line_ranges_keep_final_empty_line_and_utf8_viewport_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resident.txt");
    std::fs::write(&path, "甲乙\nlast\n").unwrap();
    let source = FileSource::open(&path).unwrap();
    let mut document = ResidentDocument::new(
        "甲乙\nlast\n",
        TextEncoding::Utf8 { bom: false },
        source.identity().unwrap(),
    );
    assert_eq!(document.line_count(), 3);
    assert_eq!(document.line_range(2), Some(12..12));
    document.replace_text(7..11, "done").unwrap();
    assert_eq!(
        document.read_range(0..document.len()).unwrap(),
        "甲乙\ndone\n".as_bytes()
    );
    assert!(document.undo());
    assert!(document.is_pristine());
}

#[test]
fn resident_edits_preserve_crlf_serialization() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resident.csv");
    let text = "name,score\r\nAda,10\r\nBob,20\r\n";
    std::fs::write(&path, text).unwrap();
    let source = FileSource::open(&path).unwrap();
    let mut document = ResidentDocument::new(
        text,
        TextEncoding::Utf8 { bom: false },
        source.identity().unwrap(),
    );
    document.replace_text(15..17, "11").unwrap();
    assert_eq!(
        document.encoded_bytes().unwrap(),
        b"name,score\r\nAda,11\r\nBob,20\r\n"
    );
}

#[test]
fn pristine_baseline_shares_rope_and_preserves_stale_save_semantics() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resident-baseline.txt");
    std::fs::write(&path, "alpha").unwrap();
    let source_identity = FileSource::open(&path).unwrap().identity().unwrap();
    let mut document =
        ResidentDocument::new("alpha", TextEncoding::Utf8 { bom: false }, source_identity);

    assert!(document.is_pristine());
    assert!(matches!(
        document.persisted_content,
        PersistedContent::Snapshot(_)
    ));
    document.replace_text(5..5, " beta").unwrap();
    assert!(!document.is_pristine());

    document.mark_persisted();
    assert!(document.is_pristine());
    assert!(matches!(
        document.persisted_content,
        PersistedContent::Snapshot(_)
    ));

    document.replace_text(10..10, " gamma").unwrap();
    document.mark_persisted_text("alpha beta");
    assert!(!document.is_pristine());
    assert!(matches!(
        document.persisted_content,
        PersistedContent::Materialized(_)
    ));
    assert!(document.undo());
    assert!(document.is_pristine());
}
