// @author kongweiguang

use std::sync::Arc;

use gmark_document_core::{SnapshotError, SourceLocator};
use thiserror::Error;

pub const DEFAULT_JSON_GRAPH_ITEM_LIMIT: usize = 1_500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphRequest {
    pub document_epoch: u64,
    pub revision: u64,
    pub generation: u64,
    pub root: Option<JsonGraphRoot>,
    pub item_limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphRoot {
    pub source: SourceLocator,
    pub json_path: Arc<str>,
    pub label: Arc<str>,
}

impl JsonGraphRoot {
    pub fn new(
        source: SourceLocator,
        json_path: impl Into<Arc<str>>,
        label: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            source,
            json_path: json_path.into(),
            label: label.into(),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum JsonGraphError {
    #[error("operation was cancelled")]
    Cancelled,
    #[error("the immutable source snapshot changed")]
    SourceChanged,
    #[error("invalid byte range {start}..{end} for a {len}-byte source")]
    InvalidRange { start: u64, end: u64, len: u64 },
    #[error("byte range length does not fit this platform")]
    RangeTooLarge,
    #[error("invalid JSON near byte {offset}: {message}")]
    InvalidJson { offset: u64, message: String },
    #[error(transparent)]
    Read(#[from] SnapshotError),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JsonGraphItemId(Arc<str>);

impl JsonGraphItemId {
    pub fn new(value: impl Into<Arc<str>>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonValueKind {
    Object,
    Array,
    String,
    Number,
    Boolean,
    Null,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphNode {
    pub id: JsonGraphItemId,
    pub json_path: Arc<str>,
    pub source: SourceLocator,
    pub kind: JsonValueKind,
    pub label: Arc<str>,
    pub fields: Arc<[JsonGraphField]>,
    pub child_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphField {
    pub id: JsonGraphItemId,
    pub json_path: Arc<str>,
    pub label: Arc<str>,
    pub display_value: Arc<str>,
    pub source: SourceLocator,
    pub kind: JsonValueKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonGraphEdgeKind {
    ObjectMember,
    ArrayItem,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphEdge {
    pub id: JsonGraphItemId,
    pub from: JsonGraphItemId,
    pub to: JsonGraphItemId,
    /// 父卡片中承载该容器字段的稳定端口；UI 用它绑定字段行和连线，不能按边序号猜测。
    pub parent_port: JsonGraphItemId,
    pub source: SourceLocator,
    pub kind: JsonGraphEdgeKind,
    pub label: Arc<str>,
}

/// 投影严格受 item_limit 限制；达到预算后仍完成语法验证，但不再保留新图项目。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphProjection {
    pub nodes: Arc<[JsonGraphNode]>,
    pub edges: Arc<[JsonGraphEdge]>,
    pub truncated: bool,
}
