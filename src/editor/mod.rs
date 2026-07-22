// @author kongweiguang

//! Top-level editor controller and window state.
//!
//! [`Editor`] owns window-level concerns such as view mode, save/close flow,
//! scroll state, and focus deferral. The runtime block tree itself lives in
//! [`DocumentTree`], which centralizes structural mutations and cached visible
//! order metadata.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::StreamExt;
use gmark_document::{Revision, SourceDocument};
use gmark_paged_document::{SourceAffinity, SourceSelection};
use gpui::*;

use self::context_menu::{ContextMenuState, TableInsertDialogState};
use self::tree::DocumentTree;
use crate::components::{
    Block, BlockKind, BlockRecord, FootnoteDefinitionBinding, FootnoteReferenceLocation,
    FootnoteRegistry, FootnoteResolvedOccurrence, ImageReferenceDefinitions, InlineTextTree,
    LinkReferenceDefinitions, parse_image_reference_definitions, parse_link_reference_definitions,
};
use crate::components::{
    TableAxisHighlight, TableAxisKind, TableAxisMarker, TableCellPosition, TableColumnAlignment,
    TableData, TableRuntime, UndoCaptureKind, serialize_table_cell_markdown,
};
mod auto_save;
mod close;
mod command_palette;
mod context_menu;
mod diagram_overlay;
mod document;
mod document_session;
mod encoding;
mod events;
mod export;
mod file_drop;
mod file_watch;
mod find_replace;
mod focus_modes;
mod history;
mod link_completion;
pub(crate) use crate::perf;
mod persistence;
mod projection;
mod recovery;
mod render;
pub(crate) use render::source_editor_top_padding;
mod runtime_context;
mod selection;
mod source_format;
mod source_mapping;
mod spellcheck;
mod status_bar;
mod table_edit;
mod table_fragment;
mod table_selection;
mod tabs;
pub(crate) use tabs::{DetachedTab, RestoredTab};
#[cfg(test)]
#[path = "../../tests/unit/editor/scenarios.rs"]
mod tests;
mod tree;
mod update;
mod virtual_surface;
mod window_state;
mod workspace;
mod workspace_file_ops;

use self::document_session::EditorDocumentSession;
use self::projection::PreparedSplitProjection;
use self::status_bar::StatusBarState;
use self::virtual_surface::VirtualSurfaceState;
use self::workspace::WorkspaceState;

/// Link navigation request deferred until a `Window` is available.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingOpenLink {
    pub(crate) prompt_target: String,
    pub(crate) open_target: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableFragmentMergeDirection {
    IntoPrevious,
    IntoNext,
}

#[derive(Clone)]
struct TableFragmentMergeTarget {
    table_id: EntityId,
    direction: TableFragmentMergeDirection,
    rows: Vec<Vec<InlineTextTree>>,
}

#[derive(Clone)]
struct TableFragmentMergeState {
    base_revision: Revision,
    parent_id: Option<EntityId>,
    fragment_ids: Vec<EntityId>,
    targets: Vec<TableFragmentMergeTarget>,
}

#[derive(Clone)]
struct DiagramOverlayState {
    block_id: EntityId,
    preview_key: u64,
    rendered: crate::components::MermaidSvgRender,
    actual_size: bool,
    close_focus_handle: FocusHandle,
    focus_close_on_render: bool,
}

#[derive(Clone, Debug)]
struct WorkspaceLinkCandidate {
    path: PathBuf,
    relative_workspace_path: String,
    title: String,
    disambiguate: bool,
}

#[derive(Clone, Debug)]
struct WorkspaceLinkCompletionState {
    block_id: EntityId,
    base_revision: Revision,
    trigger_range: std::ops::Range<usize>,
    selected: usize,
    candidates: Vec<WorkspaceLinkCandidate>,
}

#[derive(Clone, Copy, Debug)]
struct SplitResizeSession {
    start_x: Pixels,
    start_ratio: f32,
    available_width: f32,
}

/// Top-level controller that owns editor-wide state and delegates tree
/// mutations to [`DocumentTree`].
///
/// The editor subscribes to every [`BlockEvent`](crate::components::BlockEvent)
/// emitted by child blocks. Structural changes are handled centrally so focus,
/// scrolling, dirty tracking, and serialization stay synchronized.
pub struct Editor {
    accessibility_bridge: Option<crate::accessibility::AccessibilityBridge>,
    accessibility_wake_task: Option<Task<()>>,
    accessibility_revision: Option<u64>,
    /// 非 Markdown 格式与 Paged Source 的唯一应用视图宿主；Resident Markdown 由本 Editor 控制。
    document_host: Option<Entity<crate::document_host::DocumentHost>>,
    /// Markdown 源文本真值；块树只负责当前视图的可重建投影。
    source_document: EditorDocumentSession,
    /// 非 UTF-8 文件只读打开；用户明确转换后才允许编辑或保存。
    source_encoding: crate::document_io::DocumentEncoding,
    /// 无路径文档也必须保留显式类型；不能再用 `file_path == None` 偷换成 Markdown。
    document_kind: DocumentKind,
    /// 每次整文档替换递增；后台保存完成时用它拒绝回写到另一份文档。
    document_epoch: u64,
    /// Live、Preview 与 Split 共用的最近一次纯语义投影，可落后于正在编辑的源码 revision。
    projection_cache: Option<Arc<PreparedSplitProjection>>,
    document: DocumentTree,
    split_preview: Option<SplitPreviewState>,
    /// Split 分隔比例属于窗口布局状态；拖动期间不得进入文档事务或投影 revision。
    split_pane_ratio: f32,
    split_resize_session: Option<SplitResizeSession>,
    split_divider_focus_handle: FocusHandle,
    /// 主画布工具栏按钮跨 render 保持焦点身份，避免文档投影刷新打断键盘导航。
    document_toolbar_focus_handles: [FocusHandle; 3],
    file_open_failure_focus_handles: [FocusHandle; 2],
    table_cells: HashMap<EntityId, TableCellBinding>,
    /// Which view the editor is currently presenting.
    pub(crate) view_mode: ViewMode,
    /// Deferred focus target applied during render when a [`Window`] is
    /// available.
    pending_focus: Option<EntityId>,
    active_entity_id: Option<EntityId>,
    pending_scroll_active_block_into_view: bool,
    pending_scroll_recheck_after_layout: bool,
    pending_save: bool,
    pending_save_as: bool,
    /// 已有路径保存的后台任务；同时只允许一个 writer，后续请求合并到完成后的下一帧。
    save_task: Option<Task<()>>,
    save_queued: bool,
    /// 每次编辑替换 Task 即重置 idle 计时；关闭设置或保存成功会取消。
    auto_save_task: Option<Task<()>>,
    /// 活动块检查按输入 debounce；后台结果以 display text 快照拒绝过期回写。
    spellcheck_task: Option<Task<()>>,
    export_task: Option<Task<()>>,
    export_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    export_in_progress: bool,
    export_cancel_requested: bool,
    pending_open_link: Option<PendingOpenLink>,
    pending_window_edited: bool,
    pending_window_title_refresh: bool,
    document_dirty: bool,
    file_path: Option<PathBuf>,
    /// 打不开的文件仍占用真实 Tab；错误与后续动作必须留在内容区，不能污染工作区扫描状态。
    file_open_failure: Option<FileOpenFailure>,
    saved_file_fingerprint: Option<crate::recovery::FileFingerprint>,
    /// 父目录 watcher 跨原子替换存活；事件仍按当前文档 path 过滤。
    file_watch_guard: Option<file_watch::FileWatchGuard>,
    file_watch_task: Option<Task<()>>,
    external_file_conflict: bool,
    recovered_session: bool,
    show_external_conflict_dialog: bool,
    show_encoding_conversion_dialog: bool,
    external_conflict_preview: Option<ExternalConflictPreview>,
    external_conflict_restore_focus: Option<EntityId>,
    allow_external_overwrite_once: bool,
    scroll_handle: ScrollHandle,
    last_scroll_viewport_size: Option<Size<Pixels>>,
    /// Last frame's visible block ids, to detect structural edits so the height
    /// cache is refreshed only when the row/block mapping is unchanged.
    prev_visible_block_ids: Vec<EntityId>,
    /// Per-row footprint (height plus trailing gap), keyed by the row's first
    /// block. Scroll-invariant, unlike raw painted positions, so windowing from
    /// their running sum stays correct as the document scrolls. Filled as rows
    /// paint; unknown rows use a minimum-height estimate.
    row_stride_cache: HashMap<EntityId, f32>,
    /// 结构或分组语义未变化时跨滚动帧复用，避免逐 Entity 重扫行分组。
    render_row_cache: Option<render::RenderedRowCache>,
    /// Row range mounted last frame; only those rows shared one scroll offset, so
    /// their adjacent-top differences are valid footprints for the cache.
    prev_render_window: Option<(usize, usize)>,
    close_guard_installed: bool,
    show_unsaved_changes_dialog: bool,
    /// When true, the window will close after the next successful save.
    pending_close_after_save: bool,
    /// Focus target to restore when the close dialog is dismissed.
    close_dialog_restore_focus: Option<EntityId>,
    pending_drop_replace_path: Option<PathBuf>,
    show_drop_replace_dialog: bool,
    pending_drop_replace_after_save: bool,
    drop_replace_restore_focus: Option<EntityId>,
    /// Optional informational dialog shown from the Help menu.
    info_dialog: Option<InfoDialogKind>,
    /// True while an online update check is running in the background.
    update_check_in_progress: bool,
    workspace: WorkspaceState,
    tabs: tabs::TabState,
    focus_mode: bool,
    typewriter_mode: bool,
    status_bar: StatusBarState,
    context_menu: Option<ContextMenuState>,
    /// Logical selection for editor, workspace, and tab context menus. It is
    /// independent from GPUI focus so dismissing a menu preserves the caret.
    context_menu_keyboard_item: Option<usize>,
    context_menu_keyboard_submenu_item: Option<usize>,
    context_menu_scroll_handle: ScrollHandle,
    command_palette: Option<command_palette::CommandPaletteState>,
    find_panel: Option<find_replace::FindPanelState>,
    table_insert_dialog: Option<TableInsertDialogState>,
    context_menu_submenu_close_task: Option<Task<()>>,
    table_axis_preview: Option<TableAxisSelection>,
    table_axis_selection: Option<TableAxisSelection>,
    table_cell_rectangle: Option<table_selection::TableCellRectangle>,
    table_cell_drag_anchor: Option<(EntityId, TableCellPosition)>,
    table_fragment_merge: Option<TableFragmentMergeState>,
    diagram_overlay: Option<DiagramOverlayState>,
    workspace_link_completion: Option<WorkspaceLinkCompletionState>,
    cross_block_selection: Option<CrossBlockSelection>,
    cross_block_drag: Option<CrossBlockDrag>,
    rendered_select_all_cycle: Option<RenderedSelectAllCycle>,
    /// 应用图标独立控制一级导航是否展开，不与任一下拉面板的生命周期耦合。
    menu_bar_expanded: bool,
    menu_window_activation_subscription: Option<Subscription>,
    /// Open top-level menu in the in-window fallback menu bar.
    menu_bar_open: Option<usize>,
    /// Open child submenu inside the in-window fallback menu panel.
    menu_submenu_open: Option<usize>,
    /// Keyboard-only focus within the open main menu. Pointer hover keeps its
    /// own visual state and must not fabricate a persistent keyboard focus.
    menu_keyboard_item: Option<usize>,
    /// Keyboard focus within the currently open child submenu.
    menu_keyboard_submenu_item: Option<usize>,
    menu_bar_hovered: bool,
    menu_panel_hovered: bool,
    menu_submenu_panel_hovered: bool,
    /// Hover state for the invisible bridge spanning the gap between the menu
    /// panel and an open submenu. Tracked separately from
    /// `menu_submenu_panel_hovered` so the handoff between the two regions
    /// cannot clobber a single shared flag and tear the menu down.
    menu_submenu_bridge_hovered: bool,
    menu_close_task: Option<Task<()>>,
    scrollbar_hovered: bool,
    /// 仅表示指针位于 scrollbar hitbox；编辑画布 hover 只控制淡入淡出，不得放大 thumb。
    scrollbar_thumb_hovered: bool,
    scrollbar_visible_until: Instant,
    scrollbar_fade_task: Option<Task<()>>,
    split_preview_scrollbar_hovered: bool,
    split_preview_scrollbar_visible_until: Instant,
    split_preview_scrollbar_fade_task: Option<Task<()>>,
    /// Forces a repaint shortly after a pending scroll-into-view that could
    /// not be satisfied yet (the target block has no measured bounds), so the
    /// scroll lands on the next frame instead of waiting for the cursor blink.
    scroll_recheck_task: Option<Task<()>>,
    /// Live/Source 只发布纯投影缓存，不触碰当前 GPUI Entity 树。
    projection_cache_task: Option<Task<()>>,
    projection_cache_scheduled_revision: Option<Revision>,
    /// 合并连续输入的 Split 投影刷新任务；替换 Task 会取消旧定时器。
    split_projection_task: Option<Task<()>>,
    split_projection_scheduled_revision: Option<Revision>,
    recovery_journal: Option<Arc<Mutex<crate::recovery::RecoveryJournal>>>,
    recovery_task: Option<Task<()>>,
    recovery_generation: u64,
    scrollbar_drag: Option<ScrollbarDragSession>,
    split_preview_scrollbar_drag: Option<ScrollbarDragSession>,
    undo_history: Vec<HistoryEntry>,
    redo_history: Vec<HistoryEntry>,
    pending_undo_capture: Option<PendingUndoCapture>,
    /// 虚拟模式由 SourceDocument 保存逆补丁；Editor 只保存选择，不复制全文。
    virtual_undo_selections: Vec<UndoSelectionSnapshot>,
    virtual_redo_selections: Vec<UndoSelectionSnapshot>,
    pending_virtual_undo_selection: Option<UndoSelectionSnapshot>,
    last_selection_snapshot: UndoSelectionSnapshot,
    last_stable_source_text: String,
    /// `mark_dirty` 已物化的当前源码，供同一事务的历史收尾复用，避免长文档重复序列化。
    pending_dirty_source: Option<String>,
    history_restore_in_progress: bool,
    image_reference_definitions: Arc<ImageReferenceDefinitions>,
    link_reference_definitions: Arc<LinkReferenceDefinitions>,
    footnote_registry: Arc<FootnoteRegistry>,
    pending_virtual_global_runtime_refresh: bool,
    pending_virtual_footnote_focus: Option<String>,
    pending_virtual_footnote_backref_focus: Option<String>,
    /// 大文档以源码区域为全局所有权，只把 viewport 与 pinned region 物化为 Entity。
    virtual_surface: Option<VirtualSurfaceState>,
    /// 仅在性能追踪开启时存在，默认渲染路径不分配采样记录。
    first_render_started: Option<Instant>,
    /// 从块输入开始跨 Entity 事件保留到下一次 Editor render 的时间戳。
    pending_input_trace: Option<perf::PendingInputTrace>,
}

impl Drop for Editor {
    fn drop(&mut self) {
        if let Some(cancelled) = self.export_cancel.as_ref() {
            cancelled.store(true, std::sync::atomic::Ordering::Release);
        }
    }
}

/// Runtime binding between a table block and one cell editor.
#[derive(Clone)]
struct TableCellBinding {
    table_block: Entity<Block>,
    cell: Entity<Block>,
    position: TableCellPosition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExternalConflictPreview {
    path: String,
    first_difference_line: Option<usize>,
    local_line: String,
    disk_line: String,
    local_line_count: usize,
    disk_line_count: usize,
    local_bytes: usize,
    disk_bytes: usize,
    disk_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileOpenFailure {
    path: PathBuf,
    reason: String,
    action_error: Option<String>,
}

/// Split 模式右侧只读投影的独立运行时状态。
struct SplitPreviewState {
    document: DocumentTree,
    virtual_surface: Option<VirtualSurfaceState>,
    table_cells: HashMap<EntityId, TableCellBinding>,
    source_ranges: HashMap<EntityId, std::ops::Range<usize>>,
    scroll_handle: ScrollHandle,
    scroll_driver: Option<SplitScrollDriver>,
    row_stride_cache: HashMap<EntityId, f32>,
    previous_visible_ids: Vec<EntityId>,
    previous_render_window: Option<(usize, usize)>,
    revision: Revision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SplitScrollDriver {
    Source,
    Preview,
}

/// Selected row or column in a rendered native table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TableAxisSelection {
    table_block_id: EntityId,
    kind: TableAxisKind,
    index: usize,
}

/// Pixel geometry for the custom editor scrollbar.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarGeometry {
    track_height: f32,
    thumb_height: f32,
    thumb_top: f32,
    max_scroll_y: f32,
}

/// Windowing result: the run of rows to mount, plus the top/bottom spacer
/// heights standing in for the culled rows.
#[derive(Clone, Copy, Debug, PartialEq)]
struct RenderWindow {
    run_start: usize,
    run_end: usize,
    top_h: f32,
    bottom_h: f32,
}

/// Active drag session for the custom scrollbar thumb.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarDragSession {
    pointer_offset_y: f32,
    track_height: f32,
    thumb_height: f32,
    max_scroll_y: f32,
}

/// Source-mode selection snapshot stored with undo history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UndoSelectionSnapshot {
    selection: SourceSelection,
}

impl UndoSelectionSnapshot {
    /// Resident Rope 与磁盘 PieceTree 共用同一套 anchor/affinity 真值；调用方仍可在
    /// Rope 边界使用 usize，但不能把恢复、history 和视图切换状态退化回 Range。
    fn from_range(range: std::ops::Range<usize>, reversed: bool) -> Self {
        let start = range.start.min(range.end);
        let end = range.start.max(range.end);
        Self {
            selection: SourceSelection::from_range(start as u64..end as u64, reversed),
        }
    }

    fn collapsed(byte_offset: usize, affinity: SourceAffinity) -> Self {
        Self {
            selection: SourceSelection::collapsed(byte_offset as u64, affinity),
        }
    }

    fn from_source_selection(selection: SourceSelection) -> Self {
        Self { selection }
    }

    fn source_selection(self) -> SourceSelection {
        self.selection
    }

    fn range(self) -> std::ops::Range<usize> {
        let range = self.selection.range();
        saturating_source_offset(range.start)..saturating_source_offset(range.end)
    }

    fn reversed(self) -> bool {
        self.selection.reversed()
    }
}

fn saturating_source_offset(offset: u64) -> usize {
    offset.min(usize::MAX as u64) as usize
}

/// One undo history entry containing source text and selection state.
#[derive(Clone, Debug)]
struct HistoryEntry {
    source_text: String,
    source_format: gmark_document::SourceFormatSnapshot,
    selection: UndoSelectionSnapshot,
    timestamp: Instant,
    kind: UndoCaptureKind,
}

/// Deferred undo capture used to coalesce adjacent typing edits.
#[derive(Clone, Debug)]
struct PendingUndoCapture {
    snapshot: HistoryEntry,
}

/// Cross-block selection endpoint in visible block order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CrossBlockSelectionEndpoint {
    pub(super) entity_id: EntityId,
    pub(super) offset: usize,
}

/// Editor-level selection spanning two visible block endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CrossBlockSelection {
    pub(super) anchor: CrossBlockSelectionEndpoint,
    pub(super) focus: CrossBlockSelectionEndpoint,
}

/// Drag state while creating or extending a cross-block selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CrossBlockDrag {
    pub(super) anchor: CrossBlockSelectionEndpoint,
}

/// Short-lived Ctrl/Cmd+A press counter for rendered-mode selection upgrade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RenderedSelectAllCycle {
    entity_id: EntityId,
    count: u8,
    last_pressed_at: Instant,
}

/// Mapping from one visible block's text range to canonical Markdown offsets.
#[derive(Clone)]
pub(super) struct SourceTargetMapping {
    entity: Entity<Block>,
    full_source_range: std::ops::Range<usize>,
    content_to_source: Vec<usize>,
    source_to_content: Vec<usize>,
}

/// The two editing views the editor can present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    /// Rich rendered view where each block is styled by its semantic kind.
    Rendered,
    /// Plain source view where the full Markdown document is edited as a
    /// single raw buffer.
    Source,
    /// Read-only rendered document for reading and presentation.
    Preview,
    /// Editable source and read-only projection shown side by side.
    Split,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DocumentKind {
    Unspecified,
    Markdown,
    Json,
    Csv,
}

impl DocumentKind {
    fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("md" | "markdown") => Self::Markdown,
            Some("json" | "jsonl" | "ndjson") => Self::Json,
            Some("csv") => Self::Csv,
            _ => Self::Unspecified,
        }
    }

    fn initial_source(self) -> &'static str {
        match self {
            Self::Json => "{\n}\n",
            Self::Csv => "Column 1,Column 2\n",
            Self::Unspecified | Self::Markdown => "",
        }
    }

    fn initial_view_mode(self) -> ViewMode {
        match self {
            Self::Markdown => ViewMode::Rendered,
            Self::Unspecified | Self::Json | Self::Csv => ViewMode::Source,
        }
    }

    fn untitled_name(self) -> &'static str {
        match self {
            Self::Unspecified => "Untitled",
            Self::Markdown => "Untitled.md",
            Self::Json => "Untitled.json",
            Self::Csv => "Untitled.csv",
        }
    }

    fn default_extension(self) -> Option<&'static str> {
        match self {
            Self::Unspecified => None,
            Self::Markdown => Some("md"),
            Self::Json => Some("json"),
            Self::Csv => Some("csv"),
        }
    }

    fn apply_default_extension(self, path: &mut PathBuf) {
        if path.extension().is_none()
            && let Some(extension) = self.default_extension()
        {
            path.set_extension(extension);
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Unspecified => "icon/ui/file.svg",
            Self::Markdown => "icon/workspace/markdown.svg",
            Self::Json => "icon/ui/code.svg",
            Self::Csv => "icon/ui/table.svg",
        }
    }
}

/// The informational dialogs that can be shown from the Help menu.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum InfoDialogKind {
    /// Dialog describing update-check availability.
    CheckForUpdates,
    /// Dialog with app name and version information.
    About,
}

#[path = "core_parts/editor_construction.rs"]
mod construction;
