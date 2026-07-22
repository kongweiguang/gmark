// @author kongweiguang

use gmark_document::{Revision, SourceDocument, TextEdit, Transaction};
use gmark_document_core::DocumentSnapshot;
use gmark_paged_document::{FileSource, LineIndex, PagedDocument, PieceDocument};

#[test]
fn resident_and_paged_snapshots_share_revision_range_and_edit_semantics() {
    let original = "alpha\nbeta\n";
    let mut resident = SourceDocument::new(original);

    let temp = tempfile::tempdir().expect("paged contract tempdir");
    let path = temp.path().join("contract.txt");
    std::fs::write(&path, original).expect("paged contract fixture");
    let source = FileSource::open(&path).expect("paged source");
    let index = LineIndex::build(&source).expect("paged line index");
    let mut paged: PagedDocument = PieceDocument::open(source, index)
        .expect("paged document")
        .into();

    resident
        .apply_transaction(Transaction::new(
            Revision::INITIAL,
            vec![TextEdit::new(6..10, "BETA")],
        ))
        .expect("resident transaction");
    paged
        .replace_text(6..10, "BETA")
        .expect("paged transaction");

    let resident_snapshot = resident.snapshot();
    let expected = b"alpha\nBETA\n".to_vec();
    assert_eq!(resident_snapshot.read_range(0..11).unwrap(), expected);
    assert_eq!(paged.read_range(0..11).unwrap(), expected);
    assert_eq!(
        DocumentSnapshot::revision(&resident_snapshot).0,
        paged.revision()
    );

    resident.undo().expect("resident undo");
    assert!(paged.undo());
    assert_eq!(resident.text(), original);
    assert_eq!(
        paged.read_range(0..original.len() as u64).unwrap(),
        original.as_bytes()
    );
}
