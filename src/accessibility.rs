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
const FIRST_LINE_ID: u64 = 1_000;
const FIRST_TEXT_RUN_ID: u64 = 100_000;
const MAX_EXPOSED_LINES: usize = 512;
const MAX_EXPOSED_LINE_BYTES: usize = 8 * 1024;
const MAX_EXPOSED_TEXT_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EditorAccessibilitySnapshot {
    pub title: String,
    pub dirty: bool,
    pub status: String,
    pub error: Option<String>,
    pub busy: bool,
    pub search_visible: bool,
    pub navigation_visible: bool,
    pub caret: Option<(u64, usize)>,
    pub lines: Vec<(u64, String)>,
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
        self
    }
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
    let mut nodes = Vec::with_capacity(snapshot.lines.len() + 12);
    let mut root = Node::new(Role::Window);
    root.set_label("gmark");
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

    let mut tab_list = Node::new(Role::TabList);
    tab_list.set_children(vec![TAB_ID]);
    let mut tab = Node::new(Role::Tab);
    tab.set_label(snapshot.title.as_str());
    tab.set_selected(true);
    if snapshot.dirty {
        tab.set_description("Modified");
    }

    let mut document = Node::new(Role::MultilineTextInput);
    document.set_label("Source editor");
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

    nodes.push((ROOT_ID, root));
    nodes.push((TAB_LIST_ID, tab_list));
    nodes.push((TAB_ID, tab));
    nodes.push((DOCUMENT_ID, document));

    for (index, ((line, text), id)) in snapshot.lines.iter().zip(line_ids).enumerate() {
        let text_id = NodeId(FIRST_TEXT_RUN_ID + index as u64);
        let mut paragraph = Node::new(Role::Paragraph);
        paragraph.set_label(format!("Line {}", line + 1));
        paragraph.set_children(vec![text_id]);
        let mut text_run = Node::new(Role::TextRun);
        let mut value = text.clone();
        value.push('\n');
        let mut lengths = accesskit_character_lengths(text);
        lengths.push(1);
        text_run.set_value(value);
        text_run.set_character_lengths(lengths);
        nodes.push((id, paragraph));
        nodes.push((text_id, text_run));
    }

    let mut mode = Node::new(Role::Label);
    mode.set_label("Mode");
    mode.set_value("Source");
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
        focus: DOCUMENT_ID,
    }
}

fn action_button(label: &str) -> Node {
    let mut node = Node::new(Role::Button);
    node.set_label(label);
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

#[cfg(test)]
#[path = "../tests/unit/accessibility.rs"]
mod tests;
