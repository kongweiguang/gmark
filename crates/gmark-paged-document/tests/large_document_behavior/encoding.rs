// @author kongweiguang

use super::*;

#[test]
fn utf16_shadow_edits_and_streams_back_to_the_original_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("utf16.txt");
    let mut encoded = vec![0xff, 0xfe];
    for unit in "alpha\n世界".encode_utf16() {
        encoded.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&path, encoded).unwrap();
    let original = FileSource::open(&path).unwrap();
    let mut prepared = prepare_utf8_source(original, TextEncoding::Utf16Le).unwrap();
    let source = prepared.source().clone();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    document.replace_text(0..5, "bravo").unwrap();
    let plan = prepared.save_plan().unwrap();
    let copy_path = dir.path().join("utf16-copy.txt");
    plan.save_atomic_as(&document, &copy_path).unwrap();
    assert!(fs::read(&copy_path).unwrap().starts_with(&[0xff, 0xfe]));
    let identity = plan.save_atomic(&document, &path).unwrap();
    prepared.mark_original_saved(identity);

    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(&[0xff, 0xfe]));
    let units = bytes[2..]
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    assert_eq!(String::from_utf16(&units).unwrap(), "bravo\n世界");
}

#[test]
fn selection_export_reuses_original_encoding_and_is_atomic_on_unrepresentable_text() {
    let dir = tempfile::tempdir().unwrap();
    let utf16_path = dir.path().join("utf16-source.txt");
    let mut encoded = vec![0xff, 0xfe];
    for unit in "alpha\n世界\nomega".encode_utf16() {
        encoded.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&utf16_path, encoded).unwrap();
    let prepared = prepare_utf8_source(
        FileSource::open(&utf16_path).unwrap(),
        TextEncoding::Utf16Le,
    )
    .unwrap();
    let source = prepared.source().clone();
    let index = LineIndex::build(&source).unwrap();
    let document = PieceDocument::open(source, index).unwrap();
    let exported = dir.path().join("utf16-selection.txt");
    prepared
        .save_plan()
        .unwrap()
        .save_range_atomic_as_cancellable(
            &document,
            6..12,
            &exported,
            &SearchCancellation::default(),
        )
        .unwrap();
    let bytes = fs::read(&exported).unwrap();
    assert!(bytes.starts_with(&[0xff, 0xfe]));
    let units = bytes[2..]
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    assert_eq!(String::from_utf16(&units).unwrap(), "世界");

    let legacy_path = dir.path().join("legacy-source.txt");
    fs::write(&legacy_path, b"cafe\xe9").unwrap();
    let legacy = prepare_utf8_source(
        FileSource::open(&legacy_path).unwrap(),
        TextEncoding::Legacy("windows-1252".to_owned()),
    )
    .unwrap();
    let source = legacy.source().clone();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    let start = document.len();
    document.replace_text(start..start, " 文").unwrap();
    let failed_target = dir.path().join("legacy-selection.txt");
    fs::write(&failed_target, b"keep-me").unwrap();
    assert!(matches!(
        legacy
            .save_plan()
            .unwrap()
            .save_range_atomic_as_cancellable(
                &document,
                start..document.len(),
                &failed_target,
                &SearchCancellation::default(),
            ),
        Err(gmark_paged_document::PagedDocumentError::UnrepresentableEncoding { .. })
    ));
    assert_eq!(fs::read(&failed_target).unwrap(), b"keep-me");
}

#[test]
fn legacy_save_refuses_characters_the_original_encoding_cannot_represent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.txt");
    fs::write(&path, b"cafe\xe9").unwrap();
    let original = FileSource::open(&path).unwrap();
    let prepared =
        prepare_utf8_source(original, TextEncoding::Legacy("windows-1252".to_owned())).unwrap();
    let source = prepared.source().clone();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PieceDocument::open(source, index).unwrap();
    document
        .replace_text(document.len()..document.len(), " 文")
        .unwrap();

    assert!(
        prepared
            .save_plan()
            .unwrap()
            .save_atomic(&document, &path)
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), b"cafe\xe9");
}

#[test]
fn paged_document_owns_prepared_shadow_and_updates_plan_identity() {
    let dir = tempfile::tempdir().unwrap();
    let source_path = dir.path().join("owned-utf16.txt");
    let mut encoded = vec![0xff, 0xfe];
    for unit in "alpha\n世界".encode_utf16() {
        encoded.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&source_path, encoded).unwrap();

    let prepared = prepare_utf8_source(
        FileSource::open(&source_path).unwrap(),
        TextEncoding::Utf16Le,
    )
    .unwrap();
    let source = prepared.source().clone();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PagedDocument::from_prepared(prepared, index).unwrap();
    assert_eq!(document.encoding(), &TextEncoding::Utf16Le);
    assert_eq!(
        document
            .prepared_save_plan()
            .unwrap()
            .original_identity()
            .path
            .file_name(),
        source_path.file_name()
    );
    document.replace_text(0..5, "bravo").unwrap();

    let copy_path = dir.path().join("owned-copy.txt");
    document
        .save_prepared_atomic_as_cancellable(&copy_path, &SearchCancellation::default())
        .unwrap();
    assert!(fs::read(&copy_path).unwrap().starts_with(&[0xff, 0xfe]));
    assert_eq!(
        document
            .prepared_save_plan()
            .unwrap()
            .original_identity()
            .path
            .file_name(),
        copy_path.file_name()
    );

    // Cloning and dropping the original document must not remove the shadow
    // while another shared Controller/session still holds it.
    let retained = document.clone();
    drop(document);
    assert_eq!(retained.read_range(0..9).unwrap(), b"bravo\n\xe4\xb8\x96");
    let retained_source = retained.prepared_source().unwrap();
    assert_eq!(
        retained_source.read_range(0, 9).unwrap(),
        b"alpha\n\xe4\xb8\x96"
    );
}

#[test]
fn direct_utf8_documents_start_without_a_plan_and_encoding_changes_are_metadata_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("utf8.txt");
    fs::write(&path, b"alpha").unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    let mut document = PagedDocument::new(PieceDocument::open(source, index).unwrap());
    assert!(document.prepared_save_plan().is_none());
    let before = document.read_range(0..5).unwrap();

    assert!(document.set_encoding(TextEncoding::Utf16Le));
    assert!(document.prepared_save_plan().is_some());
    assert_eq!(document.read_range(0..5).unwrap(), before);
    assert!(document.set_encoding(TextEncoding::Utf8 { bom: false }));
    assert!(document.prepared_save_plan().is_none());
    assert_eq!(document.read_range(0..5).unwrap(), before);
}
