// @author kongweiguang

use super::*;

thread_local! {
    static PIECE_CLONE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Clone for Piece {
    fn clone(&self) -> Self {
        PIECE_CLONE_COUNT.with(|count| count.set(count.get() + 1));
        Self {
            source: self.source,
            range: self.range.clone(),
            newlines: self.newlines,
        }
    }
}

pub(super) fn reset_piece_clone_count() {
    PIECE_CLONE_COUNT.with(|count| count.set(0));
}

pub(super) fn piece_clone_count() -> usize {
    PIECE_CLONE_COUNT.with(std::cell::Cell::get)
}

pub(super) fn root_identity(tree: &PieceTree) -> *const PieceSummary {
    tree.root.summary() as *const PieceSummary
}
