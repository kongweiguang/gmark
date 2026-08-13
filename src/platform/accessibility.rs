// @author kongweiguang

//! AccessKit bridge for GPUI's custom-rendered editor surface.

use std::sync::{Arc, Mutex};

#[cfg(all(unix, not(target_os = "macos")))]
use accesskit::DeactivationHandler;
use accesskit::{
    Action, ActionHandler, ActionRequest, ActivationHandler, Live, Node, NodeId, Role,
    TextPosition, TextSelection, Tree, TreeId, TreeUpdate,
};
use gpui::Window;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use raw_window_handle::HasWindowHandle;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use raw_window_handle::RawWindowHandle;

const ROOT_ID: NodeId = NodeId(0);
const TAB_LIST_ID: NodeId = NodeId(1);
const TAB_ID: NodeId = NodeId(2);
const DOCUMENT_ID: NodeId = NodeId(3);
const MODE_ID: NodeId = NodeId(4);
const STATUS_ID: NodeId = NodeId(5);
pub(crate) const SAVE_ID: NodeId = NodeId(6);
pub(crate) const FIND_ID: NodeId = NodeId(7);
pub(crate) const GO_TO_LINE_ID: NodeId = NodeId(8);
pub(crate) const ERROR_ID: NodeId = NodeId(9);
const SEARCH_INPUT_ID: NodeId = NodeId(10);
const NAVIGATION_INPUT_ID: NodeId = NodeId(11);
pub(crate) const MATH_ID: NodeId = NodeId(12);
pub(crate) const MATH_INPUT_ID: NodeId = NodeId(13);
pub(crate) const MATH_TAB_LIST_ID: NodeId = NodeId(14);
pub(crate) const MATH_SYMBOLS_TAB_ID: NodeId = NodeId(15);
pub(crate) const MATH_STRUCTURES_TAB_ID: NodeId = NodeId(16);
pub(crate) const MATH_PAGE_ID: NodeId = NodeId(17);
pub(crate) const MATH_GRID_ID: NodeId = NodeId(18);
const FIRST_LINE_ID: u64 = 1_000;
const FIRST_TEXT_RUN_ID: u64 = 100_000;
const FIRST_FOLD_ID: u64 = 1_000_000;
pub(crate) const FIRST_MATH_ACTION_ID: u64 = 2_000_000;
pub(crate) const FIRST_MATH_GRID_CELL_ID: u64 = 2_100_000;
const MATH_GRID_CELL_STRIDE: u64 = 1_024;
const MAX_EXPOSED_LINES: usize = 512;
const MAX_EXPOSED_LINE_BYTES: usize = 8 * 1024;
const MAX_EXPOSED_TEXT_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AccessibilityFold {
    pub(crate) start_line: u64,
    pub(crate) end_line: u64,
    pub(crate) collapsed: bool,
    pub(crate) target: Option<AccessibilityFoldTarget>,
}

/// Identifies the action that should be dispatched when an accessibility fold
/// button is activated. Paged/document-host folds use their source line;
/// rendered Markdown folds use the process-local presentation key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AccessibilityFoldTarget {
    SourceLine,
    Rendered { key: String, heading: bool },
}

/// Presentation mode exposed to assistive technology.
///
/// The rendered canvas still uses the same source-backed editing model, but
/// exposing the active projection is important: a screen reader must not
/// announce a Live/Preview canvas as a raw source editor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AccessibilityMode {
    #[default]
    Source,
    Live,
    Preview,
    Split,
}

impl AccessibilityMode {
    fn document_label(self) -> &'static str {
        match self {
            Self::Source => "Source editor",
            Self::Live => "Live rendered view",
            Self::Preview => "Preview",
            Self::Split => "Source editor and preview",
        }
    }

    fn value(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Live => "Live",
            Self::Preview => "Preview",
            Self::Split => "Split",
        }
    }
}

/// The two semantic pages of the formula palette. The visual palette may
/// eventually keep this state in the block runtime; the accessibility tree
/// carries the current page independently so assistive technology can still
/// expose a stable tab contract.
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub(crate) enum AccessibilityMathPage {
    #[default]
    Symbols,
    Structures,
}

impl AccessibilityMathPage {
    fn value(self) -> &'static str {
        match self {
            Self::Symbols => "symbols",
            Self::Structures => "structures",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AccessibilityMathControl {
    /// Stable command key used to route an AccessKit click back to the block.
    pub(crate) key: String,
    /// Already-localized accessible name for the graphical button.
    pub(crate) label: String,
    pub(crate) page: AccessibilityMathPage,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AccessibilityMathGridCell {
    pub(crate) row: usize,
    pub(crate) column: usize,
    pub(crate) value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AccessibilityMathGrid {
    pub(crate) rows: usize,
    pub(crate) columns: usize,
    pub(crate) active_row: usize,
    pub(crate) active_column: usize,
    pub(crate) cells: Vec<AccessibilityMathGridCell>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AccessibilityMath {
    /// Canonical formula source from the active structured session.
    pub(crate) source: String,
    /// Source text in the slot occupied by the two-dimensional cursor.
    pub(crate) slot_value: String,
    /// UTF-8 byte offset within `slot_value` (converted to an AccessKit
    /// character index when building the tree).
    pub(crate) slot_cursor: usize,
    pub(crate) slot_label: String,
    pub(crate) symbols_label: String,
    pub(crate) structures_label: String,
    pub(crate) page: AccessibilityMathPage,
    pub(crate) controls: Vec<AccessibilityMathControl>,
    pub(crate) grid: Option<AccessibilityMathGrid>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EditorAccessibilitySnapshot {
    pub title: String,
    pub mode: AccessibilityMode,
    pub dirty: bool,
    pub status: String,
    pub error: Option<String>,
    pub busy: bool,
    pub search_visible: bool,
    pub navigation_visible: bool,
    pub caret: Option<(u64, usize)>,
    pub lines: Vec<(u64, String)>,
    pub folds: Vec<AccessibilityFold>,
    pub math: Option<AccessibilityMath>,
}

impl EditorAccessibilitySnapshot {
    fn bounded(mut self) -> Self {
        self.lines.truncate(MAX_EXPOSED_LINES);
        let mut retained = Vec::with_capacity(self.lines.len());
        let mut total = 0usize;
        for (line, mut text) in self.lines {
            if text.len() > MAX_EXPOSED_LINE_BYTES {
                let mut end = MAX_EXPOSED_LINE_BYTES.saturating_sub('…'.len_utf8());
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.truncate(end);
                text.push('…');
            }
            if total.saturating_add(text.len()) > MAX_EXPOSED_TEXT_BYTES {
                break;
            }
            total += text.len();
            retained.push((line, text));
        }
        self.lines = retained;
        self.folds
            .retain(|fold| self.lines.iter().any(|(line, _)| *line == fold.start_line));
        self
    }
}

pub(crate) fn source_line_for_fold_node(node: NodeId) -> Option<usize> {
    node.0
        .checked_sub(FIRST_FOLD_ID)
        .and_then(|line| usize::try_from(line).ok())
}

pub(crate) fn math_action_index(node: NodeId) -> Option<usize> {
    let index = node.0.checked_sub(FIRST_MATH_ACTION_ID)?;
    (node.0 < FIRST_MATH_GRID_CELL_ID)
        .then(|| usize::try_from(index).ok())
        .flatten()
}

pub(crate) fn math_grid_cell_for_node(node: NodeId) -> Option<(usize, usize)> {
    let encoded = node.0.checked_sub(FIRST_MATH_GRID_CELL_ID)?;
    let row = usize::try_from(encoded / MATH_GRID_CELL_STRIDE).ok()?;
    let column = usize::try_from(encoded % MATH_GRID_CELL_STRIDE).ok()?;
    Some((row, column))
}

fn math_grid_cell_node(row: usize, column: usize) -> NodeId {
    let encoded = (row as u64)
        .saturating_mul(MATH_GRID_CELL_STRIDE)
        .saturating_add(column as u64);
    NodeId(FIRST_MATH_GRID_CELL_ID.saturating_add(encoded))
}

#[derive(Clone)]
struct SharedActivationState(Arc<Mutex<EditorAccessibilitySnapshot>>);

impl ActivationHandler for SharedActivationState {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let snapshot = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        Some(build_tree(snapshot))
    }
}

#[derive(Clone)]
struct SharedActionQueue {
    actions: Arc<Mutex<Vec<ActionRequest>>>,
    wake: futures::channel::mpsc::UnboundedSender<()>,
}

impl ActionHandler for SharedActionQueue {
    fn do_action(&mut self, request: ActionRequest) {
        self.actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(request);
        let _ = self.wake.unbounded_send(());
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
struct SharedDeactivationState;

#[cfg(all(unix, not(target_os = "macos")))]
impl DeactivationHandler for SharedDeactivationState {
    fn deactivate_accessibility(&mut self) {}
}

pub(crate) struct AccessibilityBridge {
    state: Arc<Mutex<EditorAccessibilitySnapshot>>,
    actions: Arc<Mutex<Vec<ActionRequest>>>,
    platform: PlatformAdapter,
}

#[cfg(target_os = "windows")]
type PlatformAdapter = accesskit_windows::SubclassingAdapter;

#[cfg(target_os = "macos")]
type PlatformAdapter = accesskit_macos::SubclassingAdapter;

#[cfg(all(unix, not(target_os = "macos")))]
type PlatformAdapter = accesskit_unix::Adapter;

impl AccessibilityBridge {
    /// 必须在原生窗口第一次显示前安装；调用方应在 `open_window` 构造闭包内执行。
    // reason: AccessKit 适配器必须接收原生句柄并调用平台 API；remove when: 上游提供覆盖三平台的安全构造接口。
    #[allow(unsafe_code)]
    pub(crate) fn new(
        window: &Window,
        initial: EditorAccessibilitySnapshot,
    ) -> Option<(Self, futures::channel::mpsc::UnboundedReceiver<()>)> {
        // GPUI 的测试窗口没有原生平台句柄，调用 `HasWindowHandle` 会直接 panic。
        // 语义树由本模块纯函数测试覆盖；真实 adapter 仅在非测试应用进程安装。
        if cfg!(test) {
            return None;
        }
        let initial = initial.bounded();
        let state = Arc::new(Mutex::new(initial));
        let actions = Arc::new(Mutex::new(Vec::new()));
        let (wake, receiver) = futures::channel::mpsc::unbounded();
        let activation = SharedActivationState(Arc::clone(&state));
        let action_handler = SharedActionQueue {
            actions: Arc::clone(&actions),
            wake,
        };
        #[cfg(target_os = "windows")]
        let platform = {
            let raw = HasWindowHandle::window_handle(window).ok()?.as_raw();
            let RawWindowHandle::Win32(handle) = raw else {
                return None;
            };
            let hwnd = windows::Win32::Foundation::HWND(handle.hwnd.get() as *mut _);
            use windows::Win32::UI::WindowsAndMessaging::{
                IsWindowVisible, SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow,
            };
            // GPUI marks the HWND visible before executing its construction closure, while the
            // official AccessKit subclass adapter requires a currently hidden HWND. No frame has
            // been presented yet here; hide only this target HWND, install, then restore without
            // activation so focus and user input cannot move to another application.
            let was_visible = unsafe { IsWindowVisible(hwnd).as_bool() };
            if was_visible {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            let adapter =
                accesskit_windows::SubclassingAdapter::new(hwnd, activation, action_handler);
            if was_visible {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
            }
            adapter
        };

        #[cfg(target_os = "macos")]
        let platform = {
            let raw = HasWindowHandle::window_handle(window).ok()?.as_raw();
            let RawWindowHandle::AppKit(handle) = raw else {
                return None;
            };
            // SAFETY: GPUI owns this NSView for at least as long as the Editor and its bridge.
            unsafe {
                accesskit_macos::SubclassingAdapter::new(
                    handle.ns_view.as_ptr(),
                    activation,
                    action_handler,
                )
            }
        };

        #[cfg(all(unix, not(target_os = "macos")))]
        let platform = {
            // AT-SPI 通过会话总线注册，不绑定 Wayland/X11 原生窗口句柄。
            let _ = window;
            accesskit_unix::Adapter::new(activation, action_handler, SharedDeactivationState)
        };

        Some((
            Self {
                state,
                actions,
                platform,
            },
            receiver,
        ))
    }

    pub(crate) fn update(&mut self, snapshot: EditorAccessibilitySnapshot) {
        let snapshot = snapshot.bounded();
        *self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = snapshot.clone();

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        if let Some(events) = self.platform.update_if_active(|| build_tree(snapshot)) {
            events.raise();
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        self.platform.update_if_active(|| build_tree(snapshot));
    }

    pub(crate) fn update_focus(&mut self, is_focused: bool) {
        #[cfg(target_os = "windows")]
        let _ = is_focused; // The subclass adapter consumes WM_SETFOCUS/WM_KILLFOCUS directly.

        #[cfg(target_os = "macos")]
        if let Some(events) = self.platform.update_view_focus_state(is_focused) {
            events.raise();
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        self.platform.update_window_focus_state(is_focused);
    }

    pub(crate) fn take_actions(&self) -> Vec<ActionRequest> {
        std::mem::take(
            &mut *self
                .actions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    }
}

fn build_tree(snapshot: EditorAccessibilitySnapshot) -> TreeUpdate {
    let math_node_count = snapshot.math.as_ref().map_or(0, |math| {
        7 + math.controls.len() + math.grid.as_ref().map_or(0, |grid| grid.cells.len())
    });
    let mut nodes =
        Vec::with_capacity(snapshot.lines.len() * 2 + snapshot.folds.len() + 12 + math_node_count);
    let mut root = Node::new(Role::Window);
    root.set_label("Gmark");
    root.set_children(vec![TAB_LIST_ID, DOCUMENT_ID, MODE_ID, STATUS_ID]);
    root.push_child(SAVE_ID);
    root.push_child(FIND_ID);
    root.push_child(GO_TO_LINE_ID);
    if snapshot.error.is_some() {
        root.push_child(ERROR_ID);
    }
    if snapshot.search_visible {
        root.push_child(SEARCH_INPUT_ID);
    }
    if snapshot.navigation_visible {
        root.push_child(NAVIGATION_INPUT_ID);
    }
    if snapshot.math.is_some() {
        root.push_child(MATH_ID);
    }
    for fold in snapshot.folds.iter().filter(|fold| {
        matches!(
            fold.target.as_ref(),
            Some(AccessibilityFoldTarget::Rendered { .. })
        )
    }) {
        // Rendered Markdown folds are presentation controls rather than
        // source-line children. Keep them reachable even when virtualization
        // omits the corresponding source row from the current tree window.
        root.push_child(NodeId(FIRST_FOLD_ID + fold.start_line));
    }

    let mut tab_list = Node::new(Role::TabList);
    tab_list.set_children(vec![TAB_ID]);
    let mut tab = Node::new(Role::Tab);
    tab.set_label(snapshot.title.as_str());
    tab.set_selected(true);
    if snapshot.dirty {
        tab.set_description("Modified");
    }

    let mut document = Node::new(Role::MultilineTextInput);
    document.set_label(snapshot.mode.document_label());
    let line_ids = snapshot
        .lines
        .iter()
        .enumerate()
        .map(|(index, _)| NodeId(FIRST_LINE_ID + index as u64))
        .collect::<Vec<_>>();
    document.set_children(line_ids.clone());
    document.set_value(
        snapshot
            .lines
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    );
    if let Some((caret_line, caret_column)) = snapshot.caret
        && let Some((line_index, (_, text))) = snapshot
            .lines
            .iter()
            .enumerate()
            .find(|(_, (line, _))| *line == caret_line)
    {
        let character_count = accesskit_character_lengths(text).len();
        let position = TextPosition {
            node: NodeId(FIRST_TEXT_RUN_ID + line_index as u64),
            character_index: caret_column.min(character_count),
        };
        document.set_text_selection(TextSelection {
            anchor: position,
            focus: position,
        });
    }

    let mut focus = DOCUMENT_ID;
    if let Some(math) = snapshot.math.as_ref() {
        let mut formula = Node::new(Role::Math);
        formula.set_label(math.slot_label.as_str());
        formula.set_value(math.source.as_str());
        formula.add_action(Action::Focus);
        formula.set_children(vec![MATH_INPUT_ID, MATH_TAB_LIST_ID, MATH_PAGE_ID]);

        let mut input = Node::new(Role::TextInput);
        input.set_label(math.slot_label.as_str());
        input.set_value(math.slot_value.as_str());
        input.add_action(Action::Focus);
        input.add_action(Action::SetTextSelection);
        input.add_action(Action::ReplaceSelectedText);
        let input_offset = accesskit_character_index_for_byte(&math.slot_value, math.slot_cursor);
        input.set_text_selection(TextSelection {
            anchor: TextPosition {
                node: MATH_INPUT_ID,
                character_index: input_offset,
            },
            focus: TextPosition {
                node: MATH_INPUT_ID,
                character_index: input_offset,
            },
        });

        let mut tabs = Node::new(Role::TabList);
        tabs.set_children(vec![MATH_SYMBOLS_TAB_ID, MATH_STRUCTURES_TAB_ID]);
        let mut symbols_tab = Node::new(Role::Tab);
        symbols_tab.set_label(math_label_for_page(math, AccessibilityMathPage::Symbols));
        symbols_tab.set_selected(math.page == AccessibilityMathPage::Symbols);
        symbols_tab.add_action(Action::Click);
        let mut structures_tab = Node::new(Role::Tab);
        structures_tab.set_label(math_label_for_page(math, AccessibilityMathPage::Structures));
        structures_tab.set_selected(math.page == AccessibilityMathPage::Structures);
        structures_tab.add_action(Action::Click);

        let mut page = Node::new(Role::Button);
        page.set_label(math_label_for_page(math, math.page));
        page.set_value(math.page.value());
        page.set_selected(true);
        page.add_action(Action::Click);

        nodes.push((MATH_INPUT_ID, input));
        nodes.push((MATH_TAB_LIST_ID, tabs));
        nodes.push((MATH_SYMBOLS_TAB_ID, symbols_tab));
        nodes.push((MATH_STRUCTURES_TAB_ID, structures_tab));
        nodes.push((MATH_PAGE_ID, page));

        for (index, control) in math
            .controls
            .iter()
            .filter(|control| control.page == math.page)
            .enumerate()
        {
            let mut button = Node::new(Role::Button);
            button.set_label(control.label.as_str());
            button.set_description(control.key.as_str());
            button.add_action(Action::Click);
            formula.push_child(NodeId(FIRST_MATH_ACTION_ID + index as u64));
            nodes.push((NodeId(FIRST_MATH_ACTION_ID + index as u64), button));
        }

        if let Some(grid) = math.grid.as_ref() {
            let mut grid_node = Node::new(Role::Grid);
            grid_node.set_label("Formula grid");
            grid_node.add_action(Action::Focus);
            grid_node.add_child_action(Action::Focus);
            grid_node.set_row_count(grid.rows);
            grid_node.set_column_count(grid.columns);
            let cell_ids = grid
                .cells
                .iter()
                .map(|cell| math_grid_cell_node(cell.row, cell.column))
                .collect::<Vec<_>>();
            grid_node.set_children(cell_ids);
            if let Some(active) = grid
                .cells
                .iter()
                .find(|cell| cell.row == grid.active_row && cell.column == grid.active_column)
            {
                grid_node.set_active_descendant(math_grid_cell_node(active.row, active.column));
            }
            formula.push_child(MATH_GRID_ID);
            nodes.push((MATH_GRID_ID, grid_node));
            for cell in &grid.cells {
                let mut cell_node = Node::new(Role::GridCell);
                cell_node.set_label(format!("Cell {}, {}", cell.row + 1, cell.column + 1));
                cell_node.set_value(cell.value.as_str());
                cell_node.set_row_index(cell.row + 1);
                cell_node.set_column_index(cell.column + 1);
                cell_node.add_action(Action::Focus);
                cell_node.add_action(Action::Click);
                cell_node.add_action(Action::SetSequentialFocusNavigationStartingPoint);
                nodes.push((math_grid_cell_node(cell.row, cell.column), cell_node));
            }
        }
        nodes.push((MATH_ID, formula));
        focus = MATH_INPUT_ID;
    }

    nodes.push((ROOT_ID, root));
    nodes.push((TAB_LIST_ID, tab_list));
    nodes.push((TAB_ID, tab));
    nodes.push((DOCUMENT_ID, document));

    for (index, ((line, text), id)) in snapshot.lines.iter().zip(line_ids).enumerate() {
        let text_id = NodeId(FIRST_TEXT_RUN_ID + index as u64);
        let mut paragraph = Node::new(Role::Paragraph);
        paragraph.set_label(format!("Line {}", line + 1));
        let fold = snapshot.folds.iter().find(|fold| {
            fold.start_line == *line
                && matches!(
                    fold.target.as_ref(),
                    Some(AccessibilityFoldTarget::SourceLine)
                )
        });
        let fold_id = fold.map(|fold| NodeId(FIRST_FOLD_ID + fold.start_line));
        paragraph.set_children(
            fold_id
                .into_iter()
                .chain([text_id])
                .collect::<Vec<NodeId>>(),
        );
        let mut text_run = Node::new(Role::TextRun);
        let mut value = text.clone();
        value.push('\n');
        let mut lengths = accesskit_character_lengths(text);
        lengths.push(1);
        text_run.set_value(value);
        text_run.set_character_lengths(lengths);
        nodes.push((id, paragraph));
        if let (Some(fold), Some(fold_id)) = (fold, fold_id) {
            nodes.push((fold_id, fold_button_node(fold)));
        }
        nodes.push((text_id, text_run));
    }

    for fold in snapshot.folds.iter().filter(|fold| {
        matches!(
            fold.target.as_ref(),
            Some(AccessibilityFoldTarget::Rendered { .. })
        )
    }) {
        nodes.push((
            NodeId(FIRST_FOLD_ID + fold.start_line),
            fold_button_node(fold),
        ));
    }

    let mut mode = Node::new(Role::Label);
    mode.set_label("Mode");
    mode.set_value(snapshot.mode.value());
    nodes.push((MODE_ID, mode));

    let mut status = Node::new(if snapshot.busy {
        Role::ProgressIndicator
    } else {
        Role::Status
    });
    status.set_label("Document status");
    status.set_value(snapshot.status.as_str());
    status.set_live(Live::Polite);
    nodes.push((STATUS_ID, status));

    nodes.push((SAVE_ID, action_button("Save")));
    nodes.push((FIND_ID, action_button("Find")));
    nodes.push((GO_TO_LINE_ID, action_button("Go to line")));

    if let Some(error) = snapshot.error {
        let mut node = Node::new(Role::Alert);
        node.set_label("Document error");
        node.set_value(error.as_str());
        node.set_description(error);
        node.set_live(Live::Assertive);
        node.add_action(Action::Click);
        nodes.push((ERROR_ID, node));
    }
    if snapshot.search_visible {
        let mut node = Node::new(Role::SearchInput);
        node.set_label("Find in document");
        nodes.push((SEARCH_INPUT_ID, node));
    }
    if snapshot.navigation_visible {
        let mut node = Node::new(Role::TextInput);
        node.set_label("Go to line or byte");
        nodes.push((NAVIGATION_INPUT_ID, node));
    }

    TreeUpdate {
        nodes,
        tree: Some(Tree::new(ROOT_ID)),
        tree_id: TreeId::ROOT,
        focus,
    }
}

fn math_label_for_page(math: &AccessibilityMath, page: AccessibilityMathPage) -> String {
    match page {
        AccessibilityMathPage::Symbols => math.symbols_label.clone(),
        AccessibilityMathPage::Structures => math.structures_label.clone(),
    }
}

fn action_button(label: &str) -> Node {
    let mut node = Node::new(Role::Button);
    node.set_label(label);
    node.add_action(Action::Click);
    node
}

fn fold_button_node(fold: &AccessibilityFold) -> Node {
    let mut node = Node::new(Role::Button);
    node.set_label(format!(
        "{} lines {} through {}",
        if fold.collapsed { "Expand" } else { "Collapse" },
        fold.start_line + 1,
        fold.end_line + 1
    ));
    node.set_expanded(!fold.collapsed);
    node.add_action(Action::Click);
    node
}

fn accesskit_character_lengths(text: &str) -> Vec<u8> {
    let mut lengths = Vec::new();
    for grapheme in unicode_segmentation::UnicodeSegmentation::graphemes(text, true) {
        let mut remaining = grapheme.len();
        while remaining > u8::MAX as usize {
            lengths.push(u8::MAX);
            remaining -= u8::MAX as usize;
        }
        if remaining > 0 {
            lengths.push(remaining as u8);
        }
    }
    lengths
}

fn accesskit_character_index_for_byte(text: &str, byte_offset: usize) -> usize {
    let end = byte_offset.min(text.len());
    let end = (0..=end)
        .rev()
        .find(|candidate| text.is_char_boundary(*candidate))
        .unwrap_or(0);
    unicode_segmentation::UnicodeSegmentation::graphemes(&text[..end], true).count()
}

#[cfg(test)]
#[path = "../../tests/unit/accessibility.rs"]
mod tests;
