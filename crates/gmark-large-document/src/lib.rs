// @author kongweiguang

//! 磁盘后备的大文本探测、索引、编辑和搜索内核。

mod backend;
mod delimited;
mod encoding;
mod index;
mod json;
mod markdown_table;
mod piece;
mod probe;
mod recovery;
mod source;
mod view;

pub use backend::{
    DEFAULT_VIEWPORT_COLUMN_BYTES, LargeDocumentAdapter, LargeDocumentBackend,
    MAX_SYSTEM_CLIPBOARD_BYTES, MAX_VIEWPORT_OVERSCAN_ROWS, MAX_VIEWPORT_ROWS, SelectionTransfer,
    SourceAffinity, SourceAnchor, SourceSelection, ViewportLine, ViewportRequest, ViewportSnapshot,
    selection_transfer_for_len,
};
pub use delimited::{
    DelimitedFilterOptions, DelimitedIndex, DelimitedIndexOptions, DelimitedRecord,
};
pub use encoding::{EncodedSavePlan, PreparedUtf8Source, prepare_utf8_source};
pub use index::LineIndex;
pub use json::{
    JsonIndex, JsonIndexOptions, JsonRootKind, validate_json_lines_cancellable,
    validate_json_lines_from_cancellable,
};
pub use markdown_table::{MarkdownTableIndex, MarkdownTableRow};
pub use piece::{
    ExternalChange, PieceDocument, SearchCancellation, SearchMatch, SearchOptions,
    search_file_source,
};
pub use probe::{
    DEFAULT_LARGE_FILE_THRESHOLD, DocumentFormat, OpenProbe, OpenStrategy, ProbeOptions,
    TextEncoding, probe_file,
};
pub use recovery::{
    LargeRecoveryBase, LargeRecoveryCommand, LargeRecoveryJournal, LargeRecoveryReadStatus,
    LargeRecoverySelection, RecoveredLargeDocument, inspect_large_recovery_base,
    large_recovery_has_edits, list_large_recovery_journals, replay_large_recovery,
};
pub use source::{FileCacheStats, FileIdentity, FileSource, LargeDocumentError};
pub use view::{
    DEFAULT_DELIMITED_COLUMN_WINDOW, DEFAULT_DELIMITED_ROW_WINDOW, DEFAULT_JSON_GRAPH_NODE_LIMIT,
    DelimitedCellProjection, DelimitedWindowProjection, DerivedEdit, DerivedProjectionProvider,
    DerivedProjectionRequest, DerivedProjectionSnapshot, DerivedProjectionStatus, DerivedTextEdit,
    DerivedViewState, DocumentViewId, DocumentViewRegistry, DocumentViewState,
    ImmutableDocumentSnapshot, JsonGraphEdge, JsonGraphEdgeKind, JsonGraphItemId, JsonGraphNode,
    JsonGraphProjection, JsonValueKind, SourceLocator, SourceViewState, ViewDescriptor, ViewFormat,
};
