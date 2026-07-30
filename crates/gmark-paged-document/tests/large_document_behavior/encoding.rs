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
