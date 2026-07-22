// @author kongweiguang

use std::fs;

use super::*;

fn open_document(contents: &[u8]) -> (tempfile::TempDir, PieceDocument) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("piece-tree.txt");
    fs::write(&path, contents).unwrap();
    let source = FileSource::open(&path).unwrap();
    let index = LineIndex::build(&source).unwrap();
    (dir, PieceDocument::open(source, index).unwrap())
}

#[test]
fn undo_and_redo_keep_exact_persistent_root_snapshots() {
    let (_dir, mut document) = open_document(b"alpha\nbeta\ngamma");
    let pristine_root = document.pieces.root_identity();

    document.replace_text(6..10, "one\ntwo").unwrap();
    let edited_root = document.pieces.root_identity();
    assert_ne!(edited_root, pristine_root);
    assert_eq!(
        document.undo.last().unwrap().0.root_identity(),
        pristine_root
    );

    assert!(document.undo());
    assert_eq!(document.pieces.root_identity(), pristine_root);
    assert_eq!(document.redo.last().unwrap().0.root_identity(), edited_root);
    assert!(document.redo());
    assert_eq!(document.pieces.root_identity(), edited_root);
}

#[test]
fn fragmented_edit_clones_only_boundary_paths() {
    let bytes = vec![b'x'; 16_384];
    let (_dir, mut document) = open_document(&bytes);
    document.pieces = PieceTree {
        root: SumTree::from_iter(
            (0..document.len).map(|offset| Piece {
                source: PieceSource::Base,
                range: offset..offset + 1,
                newlines: 0,
            }),
            (),
        ),
    };
    PIECE_CLONE_COUNT.with(|count| count.set(0));

    document
        .replace_text(document.len / 2..document.len / 2, "insert")
        .unwrap();

    let cloned_pieces = PIECE_CLONE_COUNT.with(std::cell::Cell::get);
    assert!(
        cloned_pieces < 256,
        "path-copy edit cloned {cloned_pieces} of 16,384 pieces"
    );
    // split 两端会各与相邻的连续 base piece 合并，再加入一个 Add piece。
    assert_eq!(document.pieces.piece_count(), 16_383);
}

#[test]
fn adjacent_physical_pieces_are_coalesced() {
    let (_dir, mut document) = open_document(b"base");
    document
        .replace_text_chunks(4..4, [" one", " two", " three"])
        .unwrap();
    assert_eq!(document.pieces.piece_count(), 2);
    assert_eq!(
        document.read_range(0..document.len()).unwrap(),
        b"base one two three"
    );
}

#[test]
fn persistent_history_evicts_oldest_roots_at_the_production_limit() {
    let (_dir, mut document) = open_document(b"x");
    for _ in 0..DEFAULT_HISTORY_LIMIT + 5 {
        let end = document.len();
        document.replace_text(end..end, "a").unwrap();
    }
    assert_eq!(document.undo.len(), DEFAULT_HISTORY_LIMIT);

    for _ in 0..DEFAULT_HISTORY_LIMIT {
        assert!(document.undo());
    }
    assert!(!document.undo());
    assert_eq!(
        document.len(),
        6,
        "the five evicted edits remain in the base"
    );

    for _ in 0..DEFAULT_HISTORY_LIMIT {
        assert!(document.redo());
    }
    assert_eq!(document.len(), (DEFAULT_HISTORY_LIMIT + 6) as u64);
}
