// @author kongweiguang

//! 从不可变 SourceBacked 快照生成有界 JSON 图投影。
//!
//! 本 crate 只描述 JSON 格式能力，不知道文件大小、PieceTree、磁盘 IO 或 GPUI。
//! 宿主通过窄快照与取消契约接入任意存储引擎。

mod model;
mod parser;
mod provider;

pub use gmark_document_core::{
    DocumentSnapshot, ProjectionCancellation as CancellationSignal, SnapshotError, SourceLocator,
};
pub use model::{
    DEFAULT_JSON_GRAPH_ITEM_LIMIT, JsonGraphEdge, JsonGraphEdgeKind, JsonGraphError,
    JsonGraphField, JsonGraphItemId, JsonGraphNode, JsonGraphProjection, JsonGraphRequest,
    JsonGraphRoot, JsonValueKind,
};
pub use provider::{JsonGraphProvider, JsonGraphSnapshot};
