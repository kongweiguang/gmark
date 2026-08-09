// @author kongweiguang

//! Small AccessKit tree for the standalone update feedback window.
//!
//! GPUI 0.2.2 does not derive accessibility semantics from element ids, so
//! the agent publishes a two-node tree directly.  The platform adapter is
//! intentionally local to the agent; it has no action path back to the
//! installer and therefore cannot turn UI interaction into cancellation.

use std::sync::{Arc, Mutex};

use accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, Live, Node, NodeId, Role, Tree, TreeId,
    TreeUpdate,
};
use gpui::Window;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

const ROOT_ID: NodeId = NodeId(0);
const STATUS_ID: NodeId = NodeId(1);
const MESSAGE_ID: NodeId = NodeId(2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub(crate) phase: String,
    pub(crate) message: String,
    pub(crate) failure: bool,
}

#[derive(Clone)]
struct SharedSnapshot(Arc<Mutex<Snapshot>>);

impl ActivationHandler for SharedSnapshot {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let snapshot = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        Some(build_tree(snapshot))
    }
}

struct NoopActionHandler;

impl ActionHandler for NoopActionHandler {
    fn do_action(&mut self, _request: ActionRequest) {}
}

#[cfg(all(unix, not(target_os = "macos")))]
struct DeactivationHandler;

#[cfg(all(unix, not(target_os = "macos")))]
impl accesskit::DeactivationHandler for DeactivationHandler {
    fn deactivate_accessibility(&mut self) {}
}

#[cfg(target_os = "windows")]
type PlatformAdapter = accesskit_windows::SubclassingAdapter;

#[cfg(target_os = "macos")]
type PlatformAdapter = accesskit_macos::SubclassingAdapter;

#[cfg(all(unix, not(target_os = "macos")))]
type PlatformAdapter = accesskit_unix::Adapter;

pub(crate) struct Bridge {
    state: Arc<Mutex<Snapshot>>,
    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        all(unix, not(target_os = "macos"))
    ))]
    platform: PlatformAdapter,
}

impl Bridge {
    /// Installs before the first frame is shown, as required by the native
    /// AccessKit adapters.  Unsupported targets simply omit the bridge.
    // 原因：AccessKit 需要读取 GPUI 原生窗口句柄；当 GPUI 提供安全句柄 API 后移除 unsafe。
    #[allow(unsafe_code)]
    pub(crate) fn new(window: &Window, initial: Snapshot) -> Option<Self> {
        // GPUI visual tests use a synthetic window without a native handle.
        if cfg!(test) {
            return None;
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
        {
            let _ = (window, initial);
            return None;
        }

        let state = Arc::new(Mutex::new(initial));
        let activation = SharedSnapshot(Arc::clone(&state));
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
            let was_visible = unsafe { IsWindowVisible(hwnd).as_bool() };
            if was_visible {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            let adapter =
                accesskit_windows::SubclassingAdapter::new(hwnd, activation, NoopActionHandler);
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
            unsafe {
                accesskit_macos::SubclassingAdapter::new(
                    handle.ns_view.as_ptr(),
                    activation,
                    NoopActionHandler,
                )
            }
        };

        #[cfg(all(unix, not(target_os = "macos")))]
        let platform = {
            let _ = window;
            accesskit_unix::Adapter::new(activation, NoopActionHandler, DeactivationHandler)
        };

        #[cfg(any(
            target_os = "windows",
            target_os = "macos",
            all(unix, not(target_os = "macos"))
        ))]
        {
            Some(Self { state, platform })
        }

        #[cfg(not(any(
            target_os = "windows",
            target_os = "macos",
            all(unix, not(target_os = "macos"))
        )))]
        {
            let _ = state;
            None
        }
    }

    pub(crate) fn update(&mut self, snapshot: Snapshot) {
        *self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = snapshot.clone();
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        if let Some(events) = self
            .platform
            .update_if_active(|| build_tree(snapshot.clone()))
        {
            events.raise();
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        self.platform
            .update_if_active(|| build_tree(snapshot.clone()));
    }
}

pub(crate) fn build_tree(snapshot: Snapshot) -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_label("GMark update");
    root.set_children(vec![STATUS_ID, MESSAGE_ID]);

    let mut status = Node::new(if snapshot.failure {
        Role::Alert
    } else if snapshot.phase == "Succeeded" {
        Role::Status
    } else {
        Role::ProgressIndicator
    });
    status.set_label(snapshot.phase.as_str());
    status.set_value(snapshot.message.as_str());
    status.set_live(if snapshot.failure {
        Live::Assertive
    } else {
        Live::Polite
    });

    let mut message = Node::new(Role::Label);
    message.set_label("Update details");
    message.set_value(snapshot.message.as_str());

    TreeUpdate {
        nodes: vec![(ROOT_ID, root), (STATUS_ID, status), (MESSAGE_ID, message)],
        tree: Some(Tree::new(ROOT_ID)),
        tree_id: TreeId::ROOT,
        focus: STATUS_ID,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/update_agent/accessibility.rs"]
mod tests;
