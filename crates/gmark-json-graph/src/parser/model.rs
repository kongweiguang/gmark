// @author kongweiguang

use std::ops::Range;
use std::sync::Arc;

use crate::{JsonGraphEdgeKind, JsonGraphField, JsonGraphItemId, JsonValueKind};

#[derive(Clone)]
pub(super) struct StringToken {
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) display: String,
}

pub(super) enum Token {
    ObjectStart(u64),
    ObjectEnd(u64),
    ArrayStart(u64),
    ArrayEnd(u64),
    Colon(u64),
    Comma(u64),
    String(StringToken),
    Scalar {
        start: u64,
        end: u64,
        display: String,
        kind: JsonValueKind,
    },
    Eof(u64),
}

#[derive(Clone, Copy)]
pub(super) enum ContainerKind {
    Object,
    Array,
}

pub(super) enum ContainerState {
    ObjectKeyOrEnd { allow_end: bool },
    ObjectColon,
    ObjectValue,
    ObjectCommaOrEnd,
    ArrayValueOrEnd { allow_end: bool },
    ArrayCommaOrEnd,
}

pub(super) struct Frame {
    pub(super) kind: ContainerKind,
    pub(super) state: ContainerState,
    pub(super) node_id: JsonGraphItemId,
    pub(super) depth: usize,
    pub(super) path: String,
    pub(super) next_ordinal: usize,
    pub(super) pending_key: Option<StringToken>,
}

#[derive(Clone)]
pub(super) struct ParentContext {
    pub(super) id: JsonGraphItemId,
    pub(super) depth: usize,
    pub(super) kind: ContainerKind,
}

pub(super) struct NodeBuild {
    pub(super) id: JsonGraphItemId,
    pub(super) json_path: Arc<str>,
    pub(super) source: Range<u64>,
    pub(super) kind: JsonValueKind,
    pub(super) label: Arc<str>,
    pub(super) child_count: usize,
    pub(super) root_field: Option<JsonGraphField>,
    pub(super) parent: Option<JsonGraphItemId>,
    pub(super) edge_kind: Option<JsonGraphEdgeKind>,
    pub(super) edge_label: Arc<str>,
}

pub(super) enum ProjectedItem {
    Node(NodeBuild),
    Field {
        parent: JsonGraphItemId,
        field: JsonGraphField,
    },
}

impl ProjectedItem {
    pub(super) fn id(&self) -> &JsonGraphItemId {
        match self {
            Self::Node(node) => &node.id,
            Self::Field { field, .. } => &field.id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct CandidateKey {
    pub(super) depth: usize,
    pub(super) kind_rank: u8,
    pub(super) sequence: u64,
}
