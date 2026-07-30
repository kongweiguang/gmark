// @author kongweiguang

//! GPUI shell for disk-backed SourceBacked text documents.

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;

use directories::ProjectDirs;
use gmark_document_core::{
    DEFAULT_DELIMITED_COLUMN_WINDOW, DEFAULT_DELIMITED_ROW_WINDOW, DerivedProjectionProvider,
    DerivedProjectionRequest, DerivedProjectionSnapshot, DerivedProjectionStatus, DocumentFormat,
    DocumentSnapshot, DocumentViewId, DocumentViewRegistry, DocumentViewState,
    ProjectionCancellation, ProjectionError, RecoveryAction, RecoveryBackend, RecoveryRecord,
    SourceAffinity, SourceAnchor, SourceEdit, SourceLocator, SourceSelection, TextEncoding,
    Transaction, ViewDescriptor, ViewFormat,
};
use gmark_document_runtime::{DocumentSession, ResidentRecoveryError, ResidentRecoveryJournal};
#[cfg(test)]
use gmark_json_graph::JsonGraphEdgeKind;
use gmark_json_graph::{
    DEFAULT_JSON_GRAPH_ITEM_LIMIT, JsonGraphEdge, JsonGraphError, JsonGraphField, JsonGraphItemId,
    JsonGraphNode, JsonGraphProjection, JsonGraphProvider, JsonGraphRequest, JsonGraphRoot,
    JsonGraphSnapshot, JsonValueKind, SourceLocator as JsonSourceLocator,
};
use gmark_paged_document::{
    DelimitedEdit, DelimitedFilterOptions, DelimitedIndex, DelimitedIndexOptions, EncodedSavePlan,
    ExternalChange, FileSource, JsonIndex, JsonIndexOptions, LineIndex, MarkdownTableIndex,
    OpenProbe, OpenStrategy, PagedDocument as PagedDocumentAdapter, PagedDocumentError,
    PagedRecoveryJournal, PieceDocument, PreparedUtf8Source, SearchCancellation, SearchMatch,
    SearchOptions, SelectionTransfer, ViewportRequest, prepare_utf8_source, replay_paged_recovery,
    search_file_source, selection_transfer_for_len, serialize_delimited_record,
    validate_json_lines_cancellable, validate_json_lines_from_cancellable,
};
use gmark_source_tools::SourceSyntaxContext;
use gpui::prelude::*;
use gpui::{
    AnyView, App, Bounds, ClipboardItem, Context, Div, Entity, FocusHandle, Focusable,
    KeyDownEvent, MouseButton, MouseDownEvent, Pixels, Point, ScrollHandle, ScrollStrategy,
    ScrollWheelEvent, SharedString, Stateful, Subscription, Task, UniformListScrollHandle, Window,
    canvas, div, hsla, point, px, relative, rems, svg, uniform_list,
};

use crate::components::{
    Block, BlockEvent, BlockHostAction, BlockKind, BlockRecord, CancelFormatting, CollapseAllFolds,
    CollapseFold, Copy, Cut, Delete, DeleteBack, DismissTransientUi, ExpandAllFolds, ExpandFold,
    ExportSelection, FindInDocument, FindNext, FindPrevious, FormatDocument, FormatSelection,
    GoToLine, JumpToBottom, JumpToTop, PageDown, PageUp, Paste, Redo, SaveDocument, SelectAll,
    SourceLayoutIdentity, Undo, source_line_number_gutter_width,
};
use crate::source_tools::{FoldProjectionIndex, ResidentFoldParser, SourceLanguageId};

use crate::i18n::{I18nManager, I18nStrings};
use crate::theme::ThemeManager;

#[path = "contracts.rs"]
mod contracts;

use contracts::{
    CHEVRON_DOWN_ICON, CHEVRON_UP_ICON, CLOSE_ICON, DOCUMENT_HOST_KEY_CONTEXT,
    DocumentHostViewMode, FALLBACK_SOURCE_ROW_HEIGHT, FIND_CASE_ICON, FIND_REGEX_ICON,
    FIND_WORD_ICON, MAX_RENDERED_LINE_BYTES, PREFIX_PREVIEW_BYTES, SOURCE_OVERSCAN_ROWS,
    SOURCE_SCROLL_BYTES_PER_PIXEL, STRUCTURED_CELL_BYTES, STRUCTURED_CELL_WIDTH,
    STRUCTURED_COLUMN_WINDOW, STRUCTURED_OVERSCAN_ROWS, SourceContextCommand, StructuredIndex,
    StructuredLines, StructuredTextSource, localized_document_error, source_surface_padding,
};
pub(crate) use contracts::{
    DocumentHostEvent, DocumentHostMode, DocumentMenuFormat, MAX_SOURCE_CACHED_ROWS,
    SOURCE_LIST_WINDOW_ROWS, source_monospace_font_family,
};

#[path = "session.rs"]
mod session;

pub(crate) use session::DocumentRecoveryJournal;
use session::{
    build_document_session, build_paged_session, derived_views_enabled,
    modifier_horizontal_wheel_delta, record_recovery_transaction, recovery_view_id, session_plan,
    structure_input_for_session, verify_saved_session_readback,
};

#[path = "projections.rs"]
mod projections;

use projections::{JsonFocusedRoots, JsonGraphProjectionProvider, RegisteredStructuredProvider};

#[path = "state.rs"]
mod state;
#[path = "views/structured_index.rs"]
mod structured_index;

pub(crate) use state::PagedDocumentMetrics;
use state::{
    BoundedLineWindow, DocumentTaskStamp, JsonGraphContextMenu, JsonGraphEditIssue,
    JsonGraphEditTarget, JsonNode, ScreenLines, SourceLineEdit, SourceViewportReader,
    StructuredCellEdit, StructuredMenuTarget, StructuredRow,
};

/// Tab 的格式文档 Host；内部 DocumentSession 是正文、revision 与后端选择的唯一权威状态。
pub(crate) struct DocumentHost {
    path: PathBuf,
    probe: OpenProbe,
    index: Option<LineIndex>,
    document: Option<DocumentSession>,
    prepared_source: Option<PreparedUtf8Source>,
    provisional_source: Option<FileSource>,
    structured_index: Option<StructuredIndex>,
    /// 未保存 CSV/TSV 的结构索引读取此临时快照，磁盘原文件仍由 Save 独占写入。
    structured_rows: BTreeMap<u64, StructuredRow>,
    structured_pending: Option<Range<u64>>,
    structured_generation: u64,
    structured_cancellation: Option<SearchCancellation>,
    structure_error: Option<SharedString>,
    structure_error_byte: Option<u64>,
    structured_filter_input: Entity<Block>,
    structured_cell_input: Entity<Block>,
    structured_cell_edit: Option<StructuredCellEdit>,
    structured_selected_cell: Option<StructuredCellEdit>,
    /// 单元格提交后，旧索引继续稳定渲染到后台快照完成；覆盖值与基线区间偏移
    /// 保证这段窗口内连续编辑相邻单元格仍读取当前文档，而不是闪回旧表格。
    structured_cell_overrides: BTreeMap<StructuredCellEdit, String>,
    structured_cell_source_edits: Vec<(Range<u64>, i64)>,
    structured_context_target: Option<StructuredMenuTarget>,
    structured_column_progress: Option<(Arc<AtomicU64>, u64)>,
    structured_filter_column: Option<usize>,
    structured_filtered_rows: Vec<u64>,
    structured_filter_generation: u64,
    structured_filter_cancellation: Option<SearchCancellation>,
    structured_filter_running: bool,
    hidden_structured_columns: BTreeSet<usize>,
    structured_column_window_start: usize,
    json_child_indexes: BTreeMap<Vec<u64>, JsonIndex>,
    json_expanded_nodes: BTreeSet<Vec<u64>>,
    json_rows: BTreeMap<Vec<u64>, StructuredRow>,
    json_expand_generation: u64,
    json_expand_cancellation: Option<SearchCancellation>,
    view_registry: DocumentViewRegistry,
    /// 此 Tab 的选择、滚动、投影展开状态和活动模式。它始终留在 Host，即使保存任务
    /// 暂时移出共享 Session，也不能把一个窗口的 UI 状态塞回正文会话。
    tab_view_state: DocumentViewState,
    /// 用户最近选择的投影 Provider；切回 Source 后仍保留偏好，活动模式属于 Tab。
    selected_projection_view: Option<DocumentViewId>,
    document_epoch: u64,
    derived_projection_generation: u64,
    derived_projection_cancellation: Option<SearchCancellation>,
    derived_projection_snapshot: Option<Arc<dyn DerivedProjectionSnapshot>>,
    /// JSON 编辑失败时保留最后一次有效图；错误与 stale 标记描述的是当前源码 revision。
    derived_projection_error: Option<SharedString>,
    derived_projection_error_offset: Option<u64>,
    derived_projection_stale: bool,
    derived_projection_root: Option<JsonGraphRoot>,
    json_focused_roots: JsonFocusedRoots,
    graph_selected_item: Option<JsonGraphItemId>,
    graph_search_matches: Vec<JsonGraphItemId>,
    graph_search_selected: usize,
    graph_search_collapsed_before: Option<Vec<Arc<str>>>,
    graph_context_menu: Option<JsonGraphContextMenu>,
    graph_edit_target: Option<JsonGraphEditTarget>,
    graph_edit_input: Entity<Block>,
    graph_edit_error: Option<SharedString>,
    graph_edit_issue: Option<JsonGraphEditIssue>,
    graph_edit_original: Option<Arc<str>>,
    graph_state_initialized: bool,
    graph_projection_identity: Option<(u64, u64, u64)>,
    graph_row_limits: HashMap<JsonGraphItemId, usize>,
    graph_layout_cache: Option<json_graph::GraphLayoutCache>,
    graph_needs_fit: bool,
    graph_fit_all_requested: bool,
    graph_last_viewport: Option<(f32, f32)>,
    graph_pan_session: Option<(gpui::Point<gpui::Pixels>, f32, f32)>,
    graph_pending_center: Option<JsonGraphItemId>,
    graph_recenter_anchor: Option<(JsonGraphItemId, gpui::Point<gpui::Pixels>)>,
    graph_focus_handle: FocusHandle,
    graph_focus_subscription: Option<Subscription>,
    json_split_ratio: f32,
    json_split_drag: Option<(f32, f32)>,
    json_split_focus_handle: FocusHandle,
    derived_projection_task: Task<()>,
    view_mode: DocumentHostViewMode,
    preview_lines: Vec<SharedString>,
    source_rows: BTreeMap<usize, Arc<BoundedLineWindow>>,
    displayed_screen_lines: Arc<ScreenLines>,
    metrics: PagedDocumentMetrics,
    /// 从 Host 构造到首个真实 Source 窗口绘制的耗时；仅在本地诊断显式开启时分配。
    first_render_started: Option<Instant>,
    source_row_blocks: BTreeMap<usize, Entity<Block>>,
    source_syntax_contexts: BTreeMap<usize, SourceSyntaxContext>,
    source_row_epochs: BTreeMap<usize, u64>,
    source_cache_epoch: u64,
    soak_ready_published: bool,
    source_pending: Option<Range<usize>>,
    source_queued_visible: Option<Range<usize>>,
    source_last_visible: Option<Range<usize>>,
    source_list_origin: usize,
    source_language: SourceLanguageId,
    fold_projection: FoldProjectionIndex,
    fold_parser: Arc<Mutex<ResidentFoldParser>>,
    fold_snapshot_revision: Option<u64>,
    fold_window: Option<Range<usize>>,
    fold_generation: u64,
    fold_cancellation: Option<SearchCancellation>,
    fold_task: Task<()>,
    folding_enabled: bool,
    format_generation: u64,
    format_cancellation: Option<SearchCancellation>,
    format_task: Task<()>,
    format_running: bool,
    save_after_format: Option<gpui::AnyWindowHandle>,
    source_cancel_in_flight: bool,
    source_row_height: f32,
    active_edit: Option<SourceLineEdit>,
    suppressed_line_edit_text: Option<String>,
    selection_anchor: Option<usize>,
    selected_lines: Option<Range<usize>>,
    source_drag_anchor: Option<SourceAnchor>,
    source_drag_autoscroll_direction: i8,
    source_drag_autoscroll_task: Task<()>,
    /// 右键事件使用窗口坐标；菜单位于宿主局部层，必须用最近一帧边界消除外壳偏移。
    document_host_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    source_context_menu: Option<gpui::Point<gpui::Pixels>>,
    source_context_menu_focus_handle: FocusHandle,
    search_input: Entity<Block>,
    search_visible: bool,
    navigation_input: Entity<Block>,
    navigation_visible: bool,
    navigation_is_byte: bool,
    show_line_endings: bool,
    search_options: SearchOptions,
    search_results: Vec<SearchMatch>,
    search_selected: usize,
    search_running: bool,
    search_error: Option<SharedString>,
    mode_notice: Option<SharedString>,
    tail_enabled: bool,
    /// 仅在 session 尚未安装或保存任务暂时移出时存在；Ready 状态的 dirty 真值属于 session。
    pending_dirty: Option<bool>,
    saving: bool,
    reloading: bool,
    error: Option<SharedString>,
    coordinator: DocumentCoordinator,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    structured_scroll_handle: UniformListScrollHandle,
    structured_horizontal_scroll_handle: ScrollHandle,
    source_window_start: u64,
    provisional_anchor: Option<SourceAnchor>,
    /// 关闭标签仍会保留实体用于“重新打开关闭的标签”；挂起期间所有后台任务必须停止，
    /// 重新激活后再从当前不可变文档状态恢复，不允许关闭的标签改写剪贴板或缓存。
    closed_suspended: bool,
    structured_task: Task<()>,
    structured_progress_task: Task<()>,
    structured_filter_task: Task<()>,
    json_expand_task: Task<()>,
    clipboard_generation: u64,
    clipboard_cancellation: Option<SearchCancellation>,
    clipboard_task: Task<()>,
    selection_export_generation: u64,
    selection_export_cancellation: Option<SearchCancellation>,
    selection_export_task: Task<()>,
}

impl gpui::EventEmitter<DocumentHostEvent> for DocumentHost {}

impl DocumentHost {}

#[path = "runtime/closed_tabs.rs"]
mod closed_tabs;
#[path = "runtime/construction.rs"]
mod construction;
#[path = "runtime/coordinator.rs"]
mod coordinator;
use coordinator::DocumentCoordinator;
#[path = "runtime/editing/export.rs"]
mod editing_export;
#[path = "runtime/editing/history.rs"]
mod editing_history;
#[path = "runtime/editing/reload.rs"]
mod editing_reload;
#[path = "runtime/editing/save.rs"]
mod editing_save;
#[path = "runtime/editing/source.rs"]
mod editing_source;
#[path = "runtime/editing/structured_cells.rs"]
mod editing_structured_cells;
#[path = "runtime/editing/structured_support.rs"]
mod editing_structured_support;
#[path = "runtime/indexing.rs"]
mod indexing;
use editing_save::{delimited_record_terminator, transform_delimited_adapter};
#[path = "views/navigation_contract.rs"]
mod navigation_contract;
pub(crate) use navigation_contract::DocumentSidebarTarget;
use navigation_contract::{MAX_STRUCTURED_CACHED_ROWS, prune_structured_row_cache};
#[path = "views/navigation_json.rs"]
mod navigation_json;
#[path = "views/navigation_search.rs"]
mod navigation_search;
#[path = "views/navigation_sidebar_overview.rs"]
mod navigation_sidebar_overview;
#[path = "views/navigation_sidebar_structure.rs"]
mod navigation_sidebar_structure;
#[path = "views/navigation_structured.rs"]
mod navigation_structured;
#[path = "runtime/recovery/metadata.rs"]
mod recovery_metadata;
#[path = "runtime/recovery/modes.rs"]
mod recovery_modes;
#[path = "runtime/recovery/restore.rs"]
mod recovery_restore;
#[cfg(test)]
#[path = "../../tests/unit/document_runtime/host_recovery_test_interactions.rs"]
mod recovery_test_interactions;
#[cfg(test)]
#[path = "../../tests/unit/document_runtime/host_recovery_test_state.rs"]
mod recovery_test_state;
#[cfg(test)]
pub(crate) use recovery_test_interactions::PagedDocumentMetricsSnapshot;
#[path = "views/mode_input.rs"]
mod mode_input;
#[path = "views/mode_projection.rs"]
mod mode_projection;
#[path = "views/mode_state.rs"]
mod mode_state;
#[path = "runtime/recovery/accessibility.rs"]
mod recovery_accessibility;
#[path = "runtime/recovery/monitor.rs"]
mod recovery_monitor;
#[path = "views/source_viewport.rs"]
mod source_viewport;

#[path = "structured_data.rs"]
mod structured_data;

use structured_data::{
    build_structured_index, read_json_cells, search_document_reader,
    structured_json_lines_record_count, truncate_cell,
};

#[path = "source_window.rs"]
mod source_window;

use source_window::{
    decode_provisional_bytes, read_bounded_line_window, read_provisional_source_rows,
    rendered_line_ending, rendered_line_window_text, shift_source_window_start,
    source_line_from_scrollbar_pointer, source_list_origin_for_target,
    source_window_start_for_anchor, source_window_start_from_pointer, text_encoding_label,
};

fn document_view_state_mut<'a>(
    _: &mut Option<DocumentSession>,
    tab_view_state: &'a mut DocumentViewState,
) -> &'a mut DocumentViewState {
    tab_view_state
}

fn document_dirty_state(document: &Option<DocumentSession>, pending: &Option<bool>) -> bool {
    document
        .as_ref()
        .map(|document| document.dirty)
        .or(*pending)
        .unwrap_or(false)
}

fn set_document_dirty_state(
    document: &mut Option<DocumentSession>,
    pending: &mut Option<bool>,
    dirty: bool,
) {
    if let Some(document) = document.as_mut() {
        document.dirty = dirty;
    } else {
        *pending = Some(dirty);
    }
}

impl DocumentHost {
    fn sync_tab_active_view(&mut self) {
        let mut active_view = if self.view_mode == DocumentHostViewMode::Source {
            DocumentViewId::source()
        } else {
            self.selected_projection_view
                .clone()
                .unwrap_or_else(DocumentViewId::source)
        };
        // Session 与 Tab 必须公开同一个活动视图；只更新 Tab 会让 Source/Live 往返后
        // 持久会话仍声称自己停留在旧表格，保存、恢复和测试都会读到错误状态。
        if let Some(document) = self.document.as_mut()
            && document.set_active_view(active_view.clone()).is_err()
        {
            self.view_mode = DocumentHostViewMode::Source;
            active_view = DocumentViewId::source();
            // Source 是所有文本会话的必备视图；若这里仍失败，说明构造契约已损坏。
            debug_assert!(document.set_active_view(active_view.clone()).is_ok());
        }
        self.tab_view_state.active_view = Some(active_view);
    }

    /// 后台保存需要独占并移走 session；Tab 视图状态始终留在 Host，避免后台副本
    /// 与仍在渲染的窗口同时成为权威状态。
    fn take_document_session(&mut self) -> Option<DocumentSession> {
        let mut document = self.document.take()?;
        debug_assert!(self.pending_dirty.is_none());
        self.pending_dirty = Some(std::mem::replace(&mut document.dirty, false));
        Some(document)
    }

    /// 安装或接回共享 Session 时只恢复正文状态；Tab 视图状态已经由 Host 持有。
    fn install_document_session(&mut self, mut document: DocumentSession) {
        if let Some(dirty) = self.pending_dirty.take() {
            document.dirty = dirty;
        }
        self.document = Some(document);
    }
}

impl Drop for DocumentHost {
    fn drop(&mut self) {
        self.coordinator.cancel_all();
        if let Some(cancellation) = self.structured_cancellation.take() {
            cancellation.cancel();
        }
        if let Some(cancellation) = self.structured_filter_cancellation.take() {
            cancellation.cancel();
        }
        if let Some(cancellation) = self.json_expand_cancellation.take() {
            cancellation.cancel();
        }
        if let Some(cancellation) = self.clipboard_cancellation.take() {
            cancellation.cancel();
        }
        if let Some(cancellation) = self.selection_export_cancellation.take() {
            cancellation.cancel();
        }
        if let Some(cancellation) = self.derived_projection_cancellation.take() {
            cancellation.cancel();
        }
        if let Some(cancellation) = self.fold_cancellation.take() {
            cancellation.cancel();
        }
        if let Some(cancellation) = self.format_cancellation.take() {
            cancellation.cancel();
        }
        // 未编辑的预建日志只有身份帧，不应在下次启动伪装成恢复文档。
        if !document_dirty_state(&self.document, &self.pending_dirty)
            && let Some(mut journal) = self.coordinator.recovery_journal.take()
        {
            if let Some(document) = self.document.as_ref() {
                let _ = journal.checkpoint(document);
            } else {
                let _ = journal.discard();
            }
        }
    }
}

impl Focusable for DocumentHost {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[path = "views/json_graph.rs"]
mod json_graph;
#[path = "views/render.rs"]
mod render;
#[path = "views/source_context.rs"]
mod source_context;
#[path = "views/source_folding.rs"]
mod source_folding;
#[path = "views/source_formatting.rs"]
mod source_formatting;
#[path = "views/source_rows.rs"]
mod source_rows;
#[path = "views/source_selection.rs"]
mod source_selection;
#[path = "views/structured_layout.rs"]
mod structured_layout;
use structured_layout::StructuredPanelLayout;
#[path = "views/structured_chrome.rs"]
mod structured_chrome;
#[path = "views/structured_header.rs"]
mod structured_header;
#[path = "views/structured_rows.rs"]
mod structured_rows;
#[path = "views/structured_scrollbar.rs"]
mod structured_scrollbar;
#[path = "views/structured_view.rs"]
mod structured_view;

#[cfg(test)]
#[path = "../../tests/unit/document_host.rs"]
mod bounded_line_tests;
