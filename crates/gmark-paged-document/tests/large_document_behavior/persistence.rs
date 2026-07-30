// @author kongweiguang

use super::*;

#[test]
fn atomic_save_refuses_to_overwrite_an_external_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.txt");
    fs::write(&path, b"base").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    document.replace_text(0..4, "local").unwrap();
    fs::write(&path, b"external change").unwrap();

    assert!(document.save_atomic(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"external change");
}

#[test]
fn cancelled_streaming_saves_leave_utf8_and_encoded_targets_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let utf8_path = dir.path().join("cancelled-save.txt");
    fs::write(&utf8_path, b"base").unwrap();
    let source = FileSource::open(&utf8_path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    document.replace_text(0..4, "local").unwrap();
    let cancellation = SearchCancellation::default();
    cancellation.cancel();
    assert!(matches!(
        document.save_atomic_cancellable(&utf8_path, &cancellation),
        Err(gmark_paged_document::PagedDocumentError::Cancelled)
    ));
    assert_eq!(fs::read(&utf8_path).unwrap(), b"base");

    let utf16_path = dir.path().join("cancelled-save-utf16.txt");
    let mut encoded = vec![0xff, 0xfe];
    for unit in "base".encode_utf16() {
        encoded.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&utf16_path, &encoded).unwrap();
    let prepared = prepare_utf8_source(
        FileSource::open(&utf16_path).unwrap(),
        TextEncoding::Utf16Le,
    )
    .unwrap();
    let shadow = prepared.source().clone();
    let shadow_index = LineIndex::build(&shadow).unwrap();
    let encoded_document = PieceDocument::open(shadow, shadow_index).unwrap();
    assert!(matches!(
        prepared.save_plan().unwrap().save_atomic_cancellable(
            &encoded_document,
            &utf16_path,
            &cancellation
        ),
        Err(gmark_paged_document::PagedDocumentError::Cancelled)
    ));
    assert_eq!(fs::read(&utf16_path).unwrap(), encoded);
}

#[test]
fn clean_document_accepts_a_pure_append_with_incremental_line_indexing() {
    use std::io::Write as _;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tail.log");
    fs::write(&path, b"alpha").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index.clone()).unwrap();

    let mut writer = fs::OpenOptions::new().append(true).open(&path).unwrap();
    writer.write_all(b"\nbeta\n").unwrap();
    writer.sync_all().unwrap();
    assert!(matches!(
        document.external_change().unwrap(),
        ExternalChange::Appended { .. }
    ));

    let appended_source = FileSource::open(&path).unwrap();
    let appended_index = index.extend_for_append(&appended_source).unwrap();
    assert_eq!(appended_index.line_range(0), Some(0..6));
    assert_eq!(appended_index.line_range(1), Some(6..11));
    assert_eq!(appended_index.line_range(2), Some(11..11));
    document
        .accept_external_append(appended_source, appended_index)
        .unwrap();
    assert_eq!(document.line_count(), 3);
    assert_eq!(
        document.read_range(0..document.len()).unwrap(),
        b"alpha\nbeta\n"
    );
    assert_eq!(
        document.external_change().unwrap(),
        ExternalChange::Unchanged
    );
}

#[test]
fn larger_same_file_with_rewritten_prefix_is_not_misclassified_as_append() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rewritten.log");
    fs::write(&path, b"alpha\n").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let document = PieceDocument::open(source, index).unwrap();

    fs::write(&path, b"omega\nlonger\n").unwrap();
    assert_eq!(
        document.external_change().unwrap(),
        ExternalChange::Modified
    );
}

#[test]
fn line_ranges_follow_newlines_inserted_and_removed_by_piece_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.txt");
    fs::write(&path, b"alpha\nbeta\ngamma").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    assert_eq!(document.line_for_offset(0), Some(0));
    assert_eq!(document.line_for_offset(5), Some(0));
    assert_eq!(document.line_for_offset(6), Some(1));

    document.replace_text(6..10, "one\ntwo").unwrap();
    assert_eq!(document.line_count(), 4);
    assert_eq!(document.line_for_offset(10), Some(2));
    assert_eq!(
        document
            .read_range(document.line_range(1).unwrap())
            .unwrap(),
        b"one\n"
    );
    assert_eq!(
        document
            .read_range(document.line_range(2).unwrap())
            .unwrap(),
        b"two\n"
    );
    document.replace_text(5..14, " ").unwrap();
    assert_eq!(document.line_count(), 1);
    assert_eq!(
        document.read_range(0..document.len()).unwrap(),
        b"alpha gamma"
    );
}

#[test]
fn fragmented_piece_document_preserves_public_behavior_across_history_and_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fragmented.txt");
    let mut expected = (0..2_048)
        .map(|line| format!("row-{line:04}\n"))
        .collect::<String>();
    fs::write(&path, expected.as_bytes()).unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    let mut snapshots = Vec::new();

    for edit in 0..512usize {
        snapshots.push(expected.clone());
        let start = (edit * 37) % (expected.len() + 1);
        let remove = (edit % 3).min(expected.len() - start);
        let replacement = match edit % 4 {
            0 => "X\n",
            1 => "yz",
            2 => "",
            _ => "Q",
        };
        document
            .replace_text(start as u64..(start + remove) as u64, replacement)
            .unwrap();
        expected.replace_range(start..start + remove, replacement);
    }

    assert_eq!(
        document.read_range(0..document.len()).unwrap(),
        expected.as_bytes()
    );
    for offset in [0, expected.len() / 3, expected.len() / 2, expected.len()] {
        let expected_line = expected.as_bytes()[..offset]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u64;
        assert_eq!(document.line_for_offset(offset as u64), Some(expected_line));
    }
    let expected_match = memchr::memmem::find(expected.as_bytes(), b"row-1500")
        .expect("untouched search marker") as u64;
    assert_eq!(
        document.search_literal(b"row-1500", 1).unwrap()[0].range,
        expected_match..expected_match + 8
    );

    for snapshot in snapshots.iter().rev() {
        assert!(document.undo());
        assert_eq!(
            document.read_range(0..document.len()).unwrap(),
            snapshot.as_bytes()
        );
    }
    for _ in 0..snapshots.len() {
        assert!(document.redo());
    }
    assert_eq!(
        document.read_range(0..document.len()).unwrap(),
        expected.as_bytes()
    );
    document.save_atomic(&path).unwrap();
    assert_eq!(fs::read(&path).unwrap(), expected.as_bytes());
}
