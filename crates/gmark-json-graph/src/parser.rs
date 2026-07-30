// @author kongweiguang

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use crate::{
    CancellationSignal, DocumentSnapshot, JsonGraphError, JsonGraphItemId, JsonGraphProjection,
};

mod cursor;
mod lexer;
mod model;
mod semantic;

use cursor::SnapshotCursor;
use model::{CandidateKey, ProjectedItem};

const DISPLAY_TEXT_BYTES: usize = 120;

struct GraphParser<'a> {
    cursor: SnapshotCursor<'a>,
    item_limit: usize,
    next_sequence: u64,
    truncated: bool,
    items: BTreeMap<CandidateKey, ProjectedItem>,
    item_keys: HashMap<JsonGraphItemId, CandidateKey>,
    cancellation: &'a dyn CancellationSignal,
    root_path: String,
    root_label: String,
}

pub(super) fn parse(
    document: &dyn DocumentSnapshot,
    range: Range<u64>,
    item_limit: usize,
    cancellation: &dyn CancellationSignal,
    root_path: String,
    root_label: String,
) -> Result<JsonGraphProjection, JsonGraphError> {
    GraphParser::new(
        document,
        range,
        item_limit,
        cancellation,
        root_path,
        root_label,
    )?
    .parse()
}
