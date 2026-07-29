// @author kongweiguang

//! `workspace-session.json` 的版本兼容解码和安全归一化。

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use serde::Deserialize;

use crate::workspace_session::{
    WorkspaceSession, WorkspaceSessionRegistry, WorkspaceSessionTab, WorkspaceSessionWindow,
};

const LEGACY_SESSION_VERSION: u32 = 1;
const REGISTRY_VERSION_V2: u32 = 2;
const REGISTRY_VERSION_V3: u32 = 3;
const REGISTRY_VERSION_V4: u32 = 4;
const REGISTRY_VERSION_V5: u32 = 5;
const REGISTRY_VERSION_V6: u32 = 6;
const PREVIOUS_REGISTRY_VERSION: u32 = 7;
pub(crate) const CURRENT_REGISTRY_VERSION: u32 = 8;
pub(crate) const SESSION_FILE_LIMIT: u64 = 1024 * 1024;
pub(crate) const SESSION_TAB_LIMIT: usize = 100;
pub(crate) const SESSION_WINDOW_LIMIT: usize = 20;

#[derive(Deserialize)]
struct LegacyWorkspaceSession {
    version: u32,
    tabs: Vec<WorkspaceSessionTab>,
    active_index: usize,
    #[serde(default)]
    workspace_root: Option<PathBuf>,
}

pub(crate) fn decode_registry(bytes: &[u8]) -> Result<WorkspaceSessionRegistry> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .unwrap_or_default();
    match version {
        CURRENT_REGISTRY_VERSION => Ok(serde_json::from_value(value)?),
        PREVIOUS_REGISTRY_VERSION
        | REGISTRY_VERSION_V6
        | REGISTRY_VERSION_V5
        | REGISTRY_VERSION_V4
        | REGISTRY_VERSION_V3
        | REGISTRY_VERSION_V2 => {
            let mut registry: WorkspaceSessionRegistry = serde_json::from_value(value)?;
            registry.version = CURRENT_REGISTRY_VERSION;
            Ok(registry)
        }
        LEGACY_SESSION_VERSION => {
            let legacy: LegacyWorkspaceSession = serde_json::from_value(value)?;
            if legacy.version != LEGACY_SESSION_VERSION {
                bail!("invalid legacy workspace session version");
            }
            Ok(WorkspaceSessionRegistry {
                version: CURRENT_REGISTRY_VERSION,
                windows: vec![WorkspaceSession::new(
                    uuid::Uuid::new_v4(),
                    legacy.tabs,
                    legacy.active_index,
                    legacy.workspace_root,
                )],
            })
        }
        version => bail!("unsupported workspace session registry version {version}"),
    }
}

pub(crate) fn normalize_registry(
    mut registry: WorkspaceSessionRegistry,
) -> Result<WorkspaceSessionRegistry> {
    if registry.version != CURRENT_REGISTRY_VERSION {
        bail!(
            "unsupported workspace session registry version {}",
            registry.version
        );
    }
    if registry.windows.len() > SESSION_WINDOW_LIMIT {
        bail!("workspace session registry exceeds the 20 window safety limit");
    }

    let mut seen_windows = HashSet::new();
    let mut seen_paths = HashSet::new();
    let mut windows = Vec::with_capacity(registry.windows.len());
    for session in registry.windows.into_iter().rev() {
        if !seen_windows.insert(session.id) {
            continue;
        }
        let mut session = normalize_session(session)?;
        let active_path = session
            .tabs
            .get(session.active_index)
            .map(|tab| tab.path.clone());
        // 后写窗口拥有重复文档，避免恢复时在两个窗口同时打开同一路径。
        session
            .tabs
            .retain(|tab| seen_paths.insert(path_identity(&tab.path)));
        session.active_index = active_path
            .as_ref()
            .and_then(|path| session.tabs.iter().position(|tab| tab.path == *path))
            .unwrap_or(0)
            .min(session.tabs.len().saturating_sub(1));
        if !session.tabs.is_empty() {
            windows.push(session);
        }
    }
    windows.reverse();
    registry.windows = windows;
    Ok(registry)
}

pub(crate) fn normalize_session(mut session: WorkspaceSession) -> Result<WorkspaceSession> {
    if session.tabs.len() > SESSION_TAB_LIMIT {
        bail!("workspace session exceeds the 100 tab safety limit");
    }
    let active_path = session
        .tabs
        .get(session.active_index)
        .map(|tab| tab.path.clone());
    let mut seen_paths = HashSet::new();
    for tab in &mut session.tabs {
        if tab
            .view_mode
            .as_deref()
            .is_some_and(|mode| !matches!(mode, "live" | "source" | "preview" | "split"))
        {
            tab.view_mode = None;
        }
        if let Some(selection) = tab.selection.as_mut()
            && selection.start > selection.end
        {
            std::mem::swap(&mut selection.start, &mut selection.end);
        }
        tab.scroll_x = tab
            .scroll_x
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(-10_000_000.0, 10_000_000.0));
        tab.scroll_y = tab
            .scroll_y
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(-10_000_000.0, 10_000_000.0));
    }
    session
        .tabs
        .retain(|tab| !path_is_empty(&tab.path) && seen_paths.insert(path_identity(&tab.path)));
    session.tabs.sort_by_key(|tab| !tab.pinned);
    session.active_index = active_path
        .as_ref()
        .and_then(|path| session.tabs.iter().position(|tab| tab.path == *path))
        .unwrap_or(0)
        .min(session.tabs.len().saturating_sub(1));
    if session
        .workspace_root
        .as_ref()
        .is_some_and(|root| path_is_empty(root))
    {
        session.workspace_root = None;
    }
    session.window = session.window.and_then(normalize_window);
    session.workspace_panel_width = normalize_width(session.workspace_panel_width);
    session.document_sidebar_width = normalize_width(session.document_sidebar_width);
    session.split_pane_ratio = session
        .split_pane_ratio
        .filter(|ratio| ratio.is_finite())
        .map(|ratio| ratio.clamp(0.3, 0.7));
    Ok(session)
}

fn normalize_width(width: Option<f32>) -> Option<f32> {
    width
        .filter(|width| width.is_finite())
        .map(|width| width.clamp(200.0, 360.0))
}

fn normalize_window(mut window: WorkspaceSessionWindow) -> Option<WorkspaceSessionWindow> {
    if !window.x.is_finite()
        || !window.y.is_finite()
        || !window.width.is_finite()
        || !window.height.is_finite()
        || window.width <= 0.0
        || window.height <= 0.0
    {
        return None;
    }
    // 会话文件来自崩溃恢复边界，先限制异常坐标与尺寸，显示器级裁剪由宿主完成。
    window.x = window.x.clamp(-1_000_000.0, 1_000_000.0);
    window.y = window.y.clamp(-1_000_000.0, 1_000_000.0);
    window.width = window.width.clamp(720.0, 32_768.0);
    window.height = window.height.clamp(520.0, 32_768.0);
    Some(window)
}

fn path_is_empty(path: &Path) -> bool {
    path.as_os_str().is_empty() || path.to_string_lossy().trim().is_empty()
}

pub(crate) fn path_identity(path: &Path) -> String {
    let value = path.to_string_lossy();
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value.into_owned()
    }
}
