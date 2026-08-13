// @author kongweiguang

use gmark_document_core::{
    DocumentMutationMap, DocumentRevision, SourceAffinity, SourceAnchor, SourceEdit, Transaction,
};

// Reason: keep the anchor-affinity regression in the test target so production
// source remains separate from fixtures while the public transaction contract
// still guards the coordinate mapping behavior.
#[test]
fn mutation_map_moves_affinity_aware_anchors_without_retaining_text() {
    let transaction = Transaction::new(
        DocumentRevision(0),
        vec![SourceEdit::new(2..4, "replacement")],
    );
    let map = DocumentMutationMap::from_transaction(&transaction);
    assert_eq!(
        map.map_anchor(SourceAnchor::new(1, SourceAffinity::After))
            .byte_offset,
        1
    );
    assert_eq!(
        map.map_anchor(SourceAnchor::new(2, SourceAffinity::Before))
            .byte_offset,
        2
    );
    assert_eq!(
        map.map_anchor(SourceAnchor::new(2, SourceAffinity::After))
            .byte_offset,
        13
    );
    assert_eq!(
        map.map_anchor(SourceAnchor::new(5, SourceAffinity::Before))
            .byte_offset,
        14
    );
    assert_eq!(map.edits()[0].replacement_len, 11);
}
