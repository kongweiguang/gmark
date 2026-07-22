// @author kongweiguang

//! Multi-document tab sessions with ownership-preserving state migration.

use super::document_session::EditorDocumentSession;
use std::mem;
use std::path::{Path, PathBuf};
#[cfg(not(test))]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
#[cfg(not(test))]
use std::time::Duration;
use std::{collections::HashMap, collections::HashSet};

use gmark_document::SourceDocument;
use gpui::prelude::FluentBuilder;
use gpui::*;

use super::{
    DocumentKind, Editor, ExternalConflictPreview, FileOpenFailure, HistoryEntry,
    PendingUndoCapture, UndoSelectionSnapshot, ViewMode,
    render::{
        DialogButtonKind, DialogTitleIcon, DocumentToolbarAction, clamped_floating_panel_origin,
        compact_menu_panel_height, dialog_actions, dialog_body, dialog_button, dialog_content,
        dialog_panel, dialog_title_with_icon, menu_icon_slot, modal_overlay,
    },
};
use crate::config::EditorSettings;
use crate::window_chrome::middle_ellipsis;

const CLOSED_TAB_LIMIT: usize = 20;
const TAB_CLOSE_ICON: &str = "icon/ui/close.svg";
const NEW_TAB_ICON: &str = "icon/ui/plus.svg";
const TAB_DOCUMENT_ICON: &str = "icon/workspace/markdown.svg";
const TAB_PIN_ICON: &str = "icon/editor/tab-pin.svg";
const QUICK_OPEN_ICON: &str = "icon/ui/files.svg";
const FIND_ICON: &str = "icon/ui/search.svg";
const COMMAND_PALETTE_ICON: &str = "icon/ui/keyboard.svg";
const TAB_STRIP_HEIGHT: f32 = 36.0;
const TAB_TOOL_BUTTON_SIZE: f32 = 28.0;
const TAB_TOOL_GROUP_PADDING: f32 = 4.0;
const TAB_MIN_WIDTH: f32 = 96.0;
const TAB_MAX_WIDTH: f32 = 220.0;
#[cfg(not(test))]
static SESSION_WRITE_GENERATIONS: OnceLock<Mutex<HashMap<uuid::Uuid, u64>>> = OnceLock::new();
#[cfg(not(test))]
static SESSION_WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone)]
struct TabDragPayload {
    id: uuid::Uuid,
    title: String,
    background: Hsla,
    text: Hsla,
}

struct TabDragPreview {
    payload: TabDragPayload,
    position: Point<Pixels>,
}

impl Render for TabDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .left(self.position.x + px(10.0))
            .top(self.position.y + px(10.0))
            .w(px(180.0))
            .h(px(32.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .overflow_hidden()
            .truncate()
            .rounded(px(6.0))
            .shadow_md()
            .bg(self.payload.background)
            .text_color(self.payload.text)
            .child(self.payload.title.clone())
    }
}

struct TabContextMenu {
    index: usize,
    position: Point<Pixels>,
}

struct NewTabMenu {
    position: Point<Pixels>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SessionViewSignature {
    tab_id: uuid::Uuid,
    mode: u8,
    selection_start: usize,
    selection_end: usize,
    selection_reversed: bool,
    scroll_x_bits: u32,
    scroll_y_bits: u32,
}

#[derive(Clone)]
pub(crate) struct RestoredTab {
    pub(crate) opened: crate::document_io::OpenedDocument,
    pub(crate) path: PathBuf,
    pub(crate) pinned: bool,
    pub(crate) view_mode: Option<String>,
    pub(crate) selection: Option<crate::config::workspace_session::WorkspaceSessionSelection>,
    pub(crate) scroll_x: Option<f32>,
    pub(crate) scroll_y: Option<f32>,
}

pub(crate) struct DetachedTab {
    snapshot: DocumentTabSnapshot,
}

impl DetachedTab {
    pub(crate) fn file_path(&self) -> Option<&Path> {
        self.snapshot.file_path.as_deref()
    }
}

struct DocumentTabSnapshot {
    document_host: Option<Entity<crate::document_host::DocumentHost>>,
    source_document: EditorDocumentSession,
    source_encoding: crate::document_io::DocumentEncoding,
    document_kind: DocumentKind,
    file_path: Option<PathBuf>,
    file_open_failure: Option<FileOpenFailure>,
    saved_file_fingerprint: Option<crate::recovery::FileFingerprint>,
    document_dirty: bool,
    view_mode: ViewMode,
    selection: UndoSelectionSnapshot,
    scroll_offset: Point<Pixels>,
    undo_history: Vec<HistoryEntry>,
    redo_history: Vec<HistoryEntry>,
    pending_undo_capture: Option<PendingUndoCapture>,
    virtual_undo_selections: Vec<UndoSelectionSnapshot>,
    virtual_redo_selections: Vec<UndoSelectionSnapshot>,
    pending_virtual_undo_selection: Option<UndoSelectionSnapshot>,
    last_stable_source_text: String,
    recovery_journal: Option<Arc<Mutex<crate::recovery::RecoveryJournal>>>,
    external_file_conflict: bool,
    recovered_session: bool,
    show_encoding_conversion_dialog: bool,
    external_conflict_preview: Option<ExternalConflictPreview>,
    allow_external_overwrite_once: bool,
}

struct TabRecord {
    id: uuid::Uuid,
    pinned: bool,
    snapshot: Option<DocumentTabSnapshot>,
}

pub(super) struct TabState {
    records: Vec<TabRecord>,
    active: usize,
    open_generation: u64,
    open_task: Option<Task<()>>,
    closed: Vec<DocumentTabSnapshot>,
    show_close_dialog: bool,
    close_after_save: bool,
    continue_window_close_after_save: bool,
    close_others_keep: Option<uuid::Uuid>,
    context_menu: Option<TabContextMenu>,
    new_tab_menu: Option<NewTabMenu>,
    // reason: 测试构建禁用真实会话落盘任务；remove when session writer is injected as a test adapter.
    #[cfg_attr(test, allow(dead_code))]
    session_generation: u64,
    // reason: 测试构建禁用真实会话落盘任务；remove when session writer is injected as a test adapter.
    #[cfg_attr(test, allow(dead_code))]
    session_task: Option<Task<()>>,
    dragging_tab: Option<uuid::Uuid>,
    session_id: uuid::Uuid,
    remove_session_after_window_close: bool,
    last_session_view_signature: Option<SessionViewSignature>,
    window: Option<crate::config::workspace_session::WorkspaceSessionWindow>,
    window_bounds_subscription: Option<Subscription>,
    /// UUID 绑定焦点身份，标签重排或关闭时不会把键盘焦点错误复用给另一份文档。
    focus_handles: HashMap<uuid::Uuid, FocusHandle>,
    new_tab_focus_handle: Option<FocusHandle>,
}

impl TabState {
    pub(super) fn new() -> Self {
        Self {
            records: vec![TabRecord {
                id: uuid::Uuid::new_v4(),
                pinned: false,
                snapshot: None,
            }],
            active: 0,
            open_generation: 0,
            open_task: None,
            closed: Vec::new(),
            show_close_dialog: false,
            close_after_save: false,
            continue_window_close_after_save: false,
            close_others_keep: None,
            context_menu: None,
            new_tab_menu: None,
            session_generation: 0,
            session_task: None,
            dragging_tab: None,
            session_id: uuid::Uuid::new_v4(),
            remove_session_after_window_close: false,
            last_session_view_signature: None,
            window: None,
            window_bounds_subscription: None,
            focus_handles: HashMap::new(),
            new_tab_focus_handle: None,
        }
    }
}

impl Editor {}

#[path = "tabs_parts/lifecycle.rs"]
mod lifecycle;
#[path = "tabs_parts/session.rs"]
mod session;
#[path = "tabs_parts/view.rs"]
mod view;

#[cfg(test)]
#[path = "../../tests/unit/editor/tabs.rs"]
mod tests;
