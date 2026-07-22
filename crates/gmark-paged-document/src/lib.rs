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

pub use backend::{
    DEFAULT_VIEWPORT_COLUMN_BYTES, MAX_SYSTEM_CLIPBOARD_BYTES, MAX_VIEWPORT_OVERSCAN_ROWS,
    MAX_VIEWPORT_ROWS, PagedDocument, PagedDocumentBackend, SelectionTransfer, SourceAffinity,
    SourceAnchor, SourceSelection, ViewportLine, ViewportRequest, ViewportSnapshot,
    selection_transfer_for_len,
};
pub use delimited::{
    DelimitedEdit, DelimitedFilterOptions, DelimitedIndex, DelimitedIndexOptions, DelimitedRecord,
    apply_delimited_column_edit, serialize_delimited_record,
};
pub use encoding::{EncodedSavePlan, PreparedUtf8Source, prepare_utf8_source};
pub use gmark_document_core::DocumentSnapshot;
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
    DEFAULT_MAX_RESIDENT_BYTES, DocumentFormat, OpenProbe, OpenStrategy, ProbeOptions,
    TextEncoding, probe_file,
};
pub use recovery::{
    PagedRecoveryBase, PagedRecoveryCommand, PagedRecoveryJournal, PagedRecoveryReadStatus,
    PagedRecoverySelection, RecoveredPagedDocument, inspect_paged_recovery_base,
    list_paged_recovery_journals, paged_recovery_has_edits, replay_paged_recovery,
};
pub use source::{FileCacheStats, FileIdentity, FileSource, PagedDocumentError};
