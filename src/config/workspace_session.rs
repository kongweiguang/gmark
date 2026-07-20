// @author kongweiguang

//! Versioned, bounded persistence for all open multi-document windows.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use gmark_large_document::{SourceAffinity, SourceAnchor, SourceSelection};
use serde::{Deserialize, Serialize};

use super::GmarkConfigDirs;

const LEGACY_SESSION_VERSION: u32 = 1;
const REGISTRY_VERSION_V2: u32 = 2;
const REGISTRY_VERSION_V3: u32 = 3;
const REGISTRY_VERSION_V4: u32 = 4;
const REGISTRY_VERSION_V5: u32 = 5;
const REGISTRY_VERSION_V6: u32 = 6;
const PREVIOUS_REGISTRY_VERSION: u32 = 7;
const REGISTRY_VERSION: u32 = 8;
const SESSION_FILE_LIMIT: u64 = 1024 * 1024;
const SESSION_TAB_LIMIT: usize = 100;
const SESSION_WINDOW_LIMIT: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkspaceSessionSelection {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) reversed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) anchor_affinity: Option<WorkspaceSessionAffinity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) head_affinity: Option<WorkspaceSessionAffinity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceSessionAffinity {
    Before,
    After,
}

impl WorkspaceSessionSelection {
    pub(crate) fn from_source_selection(selection: SourceSelection) -> Self {
        let range = selection.range();
        Self {
            start: range.start.min(usize::MAX as u64) as usize,
            end: range.end.min(usize::MAX as u64) as usize,
            reversed: selection.reversed(),
            anchor_affinity: Some(selection.anchor.affinity.into()),
            head_affinity: Some(selection.head.affinity.into()),
        }
    }

    pub(crate) fn source_selection_for_range(
        &self,
        range: std::ops::Range<usize>,
    ) -> SourceSelection {
        let fallback = SourceSelection::from_range(
            range.start as u64..range.end.max(range.start) as u64,
            self.reversed,
        );
        SourceSelection {
            anchor: SourceAnchor::new(
                fallback.anchor.byte_offset,
                self.anchor_affinity
                    .map(Into::into)
                    .unwrap_or(fallback.anchor.affinity),
            ),
            head: SourceAnchor::new(
                fallback.head.byte_offset,
                self.head_affinity
                    .map(Into::into)
                    .unwrap_or(fallback.head.affinity),
            ),
        }
    }
}

impl From<SourceAffinity> for WorkspaceSessionAffinity {
    fn from(value: SourceAffinity) -> Self {
        match value {
            SourceAffinity::Before => Self::Before,
            SourceAffinity::After => Self::After,
        }
    }
}

impl From<WorkspaceSessionAffinity> for SourceAffinity {
    fn from(value: WorkspaceSessionAffinity) -> Self {
        match value {
            WorkspaceSessionAffinity::Before => Self::Before,
            WorkspaceSessionAffinity::After => Self::After,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceSessionWindowState {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct WorkspaceSessionWindow {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    #[serde(default)]
    pub(crate) state: WorkspaceSessionWindowState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) display_uuid: Option<uuid::Uuid>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct WorkspaceSessionTab {
    pub(crate) path: PathBuf,
    #[serde(default)]
    pub(crate) pinned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) view_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) selection: Option<WorkspaceSessionSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) scroll_x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) scroll_y: Option<f32>,
}

impl WorkspaceSessionTab {
    pub(crate) fn new(path: PathBuf, pinned: bool) -> Self {
        Self {
            path,
            pinned,
            view_mode: None,
            selection: None,
            scroll_x: None,
            scroll_y: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct WorkspaceSession {
    pub(crate) id: uuid::Uuid,
    pub(crate) tabs: Vec<WorkspaceSessionTab>,
    pub(crate) active_index: usize,
    #[serde(default)]
    pub(crate) workspace_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) window: Option<WorkspaceSessionWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) workspace_panel_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) workspace_docked_open: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) split_pane_ratio: Option<f32>,
}

impl WorkspaceSession {
    pub(crate) fn new(
        id: uuid::Uuid,
        tabs: Vec<WorkspaceSessionTab>,
        active_index: usize,
        workspace_root: Option<PathBuf>,
    ) -> Self {
        Self {
            id,
            tabs,
            active_index,
            workspace_root,
            window: None,
            workspace_panel_width: None,
            workspace_docked_open: None,
            split_pane_ratio: None,
        }
    }

    pub(crate) fn without_paths(mut self, excluded: &[PathBuf]) -> Option<Self> {
        let active_path = self.tabs.get(self.active_index).map(|tab| tab.path.clone());
        let excluded = excluded
            .iter()
            .map(|path| path_identity(path))
            .collect::<HashSet<_>>();
        self.tabs
            .retain(|tab| !excluded.contains(&path_identity(&tab.path)));
        if self.tabs.is_empty() {
            return None;
        }
        self.active_index = active_path
            .as_ref()
            .and_then(|path| self.tabs.iter().position(|tab| tab.path == *path))
            .unwrap_or(0);
        Some(self)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct WorkspaceSessionRegistry {
    version: u32,
    windows: Vec<WorkspaceSession>,
}

#[derive(Deserialize)]
struct LegacyWorkspaceSession {
    version: u32,
    tabs: Vec<WorkspaceSessionTab>,
    active_index: usize,
    #[serde(default)]
    workspace_root: Option<PathBuf>,
}

pub(crate) fn read_workspace_sessions() -> anyhow::Result<Vec<WorkspaceSession>> {
    read_workspace_sessions_with_dirs(&GmarkConfigDirs::from_system()?)
}

// reason: 测试构建替换持久化 adapter，生产异步会话写入仍调用此入口；remove when adapter is injected in tests.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn upsert_workspace_session(session: &WorkspaceSession) -> anyhow::Result<()> {
    upsert_workspace_session_with_dirs(session, &GmarkConfigDirs::from_system()?)
}

// reason: 测试使用显式 dirs 验证删除语义，生产关闭窗口仍调用系统目录入口；remove when adapter is injected in tests.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn remove_workspace_session(id: uuid::Uuid) -> anyhow::Result<()> {
    remove_workspace_session_with_dirs(id, &GmarkConfigDirs::from_system()?)
}

pub(crate) fn remove_paths_from_workspace_sessions(paths: &[PathBuf]) -> anyhow::Result<()> {
    remove_paths_from_workspace_sessions_with_dirs(paths, &GmarkConfigDirs::from_system()?)
}

fn read_workspace_sessions_with_dirs(
    dirs: &GmarkConfigDirs,
) -> anyhow::Result<Vec<WorkspaceSession>> {
    let path = dirs.workspace_session_file();
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect '{}'", path.display()));
        }
    };
    if metadata.len() > SESSION_FILE_LIMIT {
        bail!("workspace session registry exceeds the 1 MiB safety limit");
    }
    let bytes =
        std::fs::read(&path).with_context(|| format!("failed to read '{}'", path.display()))?;
    decode_registry(&bytes)
        .with_context(|| format!("failed to parse '{}'", path.display()))
        .and_then(normalize_registry)
        .map(|registry| registry.windows)
}

fn upsert_workspace_session_with_dirs(
    session: &WorkspaceSession,
    dirs: &GmarkConfigDirs,
) -> anyhow::Result<()> {
    let mut registry = load_registry_for_update(dirs)?;
    registry.windows.retain(|window| window.id != session.id);
    let session = normalize_session(session.clone())?;
    if !session.tabs.is_empty() {
        registry.windows.push(session);
    }
    if registry.windows.len() > SESSION_WINDOW_LIMIT {
        let excess = registry.windows.len() - SESSION_WINDOW_LIMIT;
        registry.windows.drain(0..excess);
    }
    write_registry(&registry, dirs)
}

fn remove_workspace_session_with_dirs(
    id: uuid::Uuid,
    dirs: &GmarkConfigDirs,
) -> anyhow::Result<()> {
    let mut registry = load_registry_for_update(dirs)?;
    registry.windows.retain(|window| window.id != id);
    write_registry(&registry, dirs)
}

fn remove_paths_from_workspace_sessions_with_dirs(
    paths: &[PathBuf],
    dirs: &GmarkConfigDirs,
) -> anyhow::Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let excluded = paths
        .iter()
        .map(|path| path_identity(path))
        .collect::<HashSet<_>>();
    let mut registry = load_registry_for_update(dirs)?;
    for session in &mut registry.windows {
        let active_path = session
            .tabs
            .get(session.active_index)
            .map(|tab| tab.path.clone());
        session
            .tabs
            .retain(|tab| !excluded.contains(&path_identity(&tab.path)));
        session.active_index = active_path
            .as_ref()
            .and_then(|path| session.tabs.iter().position(|tab| tab.path == *path))
            .unwrap_or(0)
            .min(session.tabs.len().saturating_sub(1));
    }
    write_registry(&registry, dirs)
}

fn load_registry_for_update(dirs: &GmarkConfigDirs) -> anyhow::Result<WorkspaceSessionRegistry> {
    Ok(WorkspaceSessionRegistry {
        version: REGISTRY_VERSION,
        windows: read_workspace_sessions_with_dirs(dirs)?,
    })
}

fn write_registry(
    registry: &WorkspaceSessionRegistry,
    dirs: &GmarkConfigDirs,
) -> anyhow::Result<()> {
    let registry = normalize_registry(registry.clone())?;
    let path = dirs.workspace_session_file();
    if registry.windows.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to remove '{}'", path.display()));
            }
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(&registry)?;
    if bytes.len() as u64 > SESSION_FILE_LIMIT {
        bail!("workspace session registry exceeds the 1 MiB safety limit");
    }
    gmark_document::atomic_write(&path, &bytes)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("failed to atomically write '{}'", path.display()))
}

fn decode_registry(bytes: &[u8]) -> anyhow::Result<WorkspaceSessionRegistry> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .unwrap_or_default();
    match version {
        REGISTRY_VERSION => Ok(serde_json::from_value(value)?),
        PREVIOUS_REGISTRY_VERSION
        | REGISTRY_VERSION_V6
        | REGISTRY_VERSION_V5
        | REGISTRY_VERSION_V4
        | REGISTRY_VERSION_V3
        | REGISTRY_VERSION_V2 => {
            let mut registry: WorkspaceSessionRegistry = serde_json::from_value(value)?;
            registry.version = REGISTRY_VERSION;
            Ok(registry)
        }
        LEGACY_SESSION_VERSION => {
            let legacy: LegacyWorkspaceSession = serde_json::from_value(value)?;
            if legacy.version != LEGACY_SESSION_VERSION {
                bail!("invalid legacy workspace session version");
            }
            Ok(WorkspaceSessionRegistry {
                version: REGISTRY_VERSION,
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

fn normalize_registry(
    mut registry: WorkspaceSessionRegistry,
) -> anyhow::Result<WorkspaceSessionRegistry> {
    if registry.version != REGISTRY_VERSION {
        bail!(
            "unsupported workspace session registry version {}",
            registry.version
        );
    }
    if registry.windows.len() > SESSION_WINDOW_LIMIT {
        bail!("workspace session registry exceeds the 20 window safety limit");
    }
    let mut seen = HashSet::new();
    let mut seen_paths = HashSet::new();
    let mut windows = Vec::with_capacity(registry.windows.len());
    for session in registry.windows.into_iter().rev() {
        if seen.insert(session.id) {
            let mut session = normalize_session(session)?;
            let active_path = session
                .tabs
                .get(session.active_index)
                .map(|tab| tab.path.clone());
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
    }
    windows.reverse();
    registry.windows = windows;
    Ok(registry)
}

fn normalize_session(mut session: WorkspaceSession) -> anyhow::Result<WorkspaceSession> {
    if session.tabs.len() > SESSION_TAB_LIMIT {
        bail!("workspace session exceeds the 100 tab safety limit");
    }
    let active_path = session
        .tabs
        .get(session.active_index)
        .map(|tab| tab.path.clone());
    let mut seen = HashSet::new();
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
        .retain(|tab| !path_is_empty(&tab.path) && seen.insert(path_identity(&tab.path)));
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
    session.workspace_panel_width = session
        .workspace_panel_width
        .filter(|width| width.is_finite())
        .map(|width| width.clamp(200.0, 360.0));
    session.split_pane_ratio = session
        .split_pane_ratio
        .filter(|ratio| ratio.is_finite())
        .map(|ratio| ratio.clamp(0.3, 0.7));
    Ok(session)
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

    // 会话文件来自崩溃恢复边界，先限制异常坐标与尺寸，显示器级裁剪在窗口创建时完成。
    window.x = window.x.clamp(-1_000_000.0, 1_000_000.0);
    window.y = window.y.clamp(-1_000_000.0, 1_000_000.0);
    window.width = window.width.clamp(720.0, 32_768.0);
    window.height = window.height.clamp(520.0, 32_768.0);
    Some(window)
}

fn path_is_empty(path: &Path) -> bool {
    path.as_os_str().is_empty() || path.to_string_lossy().trim().is_empty()
}

fn path_identity(path: &Path) -> String {
    let value = path.to_string_lossy();
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value.into_owned()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/config/workspace_session.rs"]
mod tests;
