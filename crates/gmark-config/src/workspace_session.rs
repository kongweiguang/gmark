// @author kongweiguang

//! 版本化、多窗口工作区会话的领域类型与文件 store。

use std::{collections::HashSet, path::PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{ConfigDirs, persistence::atomic_write, workspace_codec};

/// 当前写入的工作区 registry 版本。
pub const WORKSPACE_SESSION_VERSION: u32 = workspace_codec::CURRENT_REGISTRY_VERSION;

/// 选择锚点位于字符边界之前还是之后。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSessionAffinity {
    /// 锚点位于边界之前。
    #[default]
    Before,
    /// 锚点位于边界之后。
    After,
}

/// 与文档实现无关的文本锚点。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionAnchor {
    /// 字节偏移。
    pub byte_offset: u64,
    /// 相对于该偏移的亲和性。
    pub affinity: WorkspaceSessionAffinity,
}

impl SessionAnchor {
    /// 构造一个文本锚点。
    #[must_use]
    pub const fn new(byte_offset: u64, affinity: WorkspaceSessionAffinity) -> Self {
        Self {
            byte_offset,
            affinity,
        }
    }
}

/// 与文档实现无关的文本选择。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionSelection {
    /// 固定端锚点。
    pub anchor: SessionAnchor,
    /// 活动端锚点。
    pub head: SessionAnchor,
}

impl SessionSelection {
    /// 构造折叠选择。
    #[must_use]
    pub const fn collapsed(byte_offset: u64, affinity: WorkspaceSessionAffinity) -> Self {
        let anchor = SessionAnchor::new(byte_offset, affinity);
        Self {
            anchor,
            head: anchor,
        }
    }

    /// 根据范围与方向构造选择，并使用既有的方向性亲和性默认值。
    #[must_use]
    pub fn from_range(range: std::ops::Range<u64>, reversed: bool) -> Self {
        if range.is_empty() {
            return Self::collapsed(range.start, WorkspaceSessionAffinity::Before);
        }
        let start = SessionAnchor::new(range.start, WorkspaceSessionAffinity::Before);
        let end = SessionAnchor::new(range.end, WorkspaceSessionAffinity::After);
        if reversed {
            Self {
                anchor: end,
                head: start,
            }
        } else {
            Self {
                anchor: start,
                head: end,
            }
        }
    }

    /// 返回覆盖的无方向范围。
    #[must_use]
    pub fn range(self) -> std::ops::Range<u64> {
        self.anchor.byte_offset.min(self.head.byte_offset)
            ..self.anchor.byte_offset.max(self.head.byte_offset)
    }

    /// 返回活动端是否位于固定端之前。
    #[must_use]
    pub fn reversed(self) -> bool {
        self.head.byte_offset < self.anchor.byte_offset
    }
}

/// 兼容 `workspace-session.json` 的持久化选择。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSessionSelection {
    /// 所选范围起点。
    pub start: usize,
    /// 所选范围终点。
    pub end: usize,
    /// 是否为反向选择。
    pub reversed: bool,
    /// 锚点亲和性；v1-v7 的缺失字段保留为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_affinity: Option<WorkspaceSessionAffinity>,
    /// 活动端亲和性；v1-v7 的缺失字段保留为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_affinity: Option<WorkspaceSessionAffinity>,
}

impl WorkspaceSessionSelection {
    /// 从中立选择转换为持久化值。
    #[must_use]
    pub fn from_selection(selection: SessionSelection) -> Self {
        let range = selection.range();
        Self {
            start: range.start.min(usize::MAX as u64) as usize,
            end: range.end.min(usize::MAX as u64) as usize,
            reversed: selection.reversed(),
            anchor_affinity: Some(selection.anchor.affinity),
            head_affinity: Some(selection.head.affinity),
        }
    }

    /// 根据已恢复范围生成中立选择。
    #[must_use]
    pub fn selection_for_range(&self, range: std::ops::Range<usize>) -> SessionSelection {
        let fallback = SessionSelection::from_range(
            range.start as u64..range.end.max(range.start) as u64,
            self.reversed,
        );
        SessionSelection {
            anchor: SessionAnchor::new(
                fallback.anchor.byte_offset,
                self.anchor_affinity.unwrap_or(fallback.anchor.affinity),
            ),
            head: SessionAnchor::new(
                fallback.head.byte_offset,
                self.head_affinity.unwrap_or(fallback.head.affinity),
            ),
        }
    }
}

/// 窗口显示状态。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSessionWindowState {
    /// 普通窗口。
    #[default]
    Windowed,
    /// 最大化窗口。
    Maximized,
    /// 全屏窗口。
    Fullscreen,
}

/// 会话中保存的窗口几何信息。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSessionWindow {
    /// 屏幕坐标 x。
    pub x: f32,
    /// 屏幕坐标 y。
    pub y: f32,
    /// 窗口宽度。
    pub width: f32,
    /// 窗口高度。
    pub height: f32,
    /// 窗口状态。
    #[serde(default)]
    pub state: WorkspaceSessionWindowState,
    /// 关联显示器 UUID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_uuid: Option<uuid::Uuid>,
}

/// 会话中保存的一个文档标签。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSessionTab {
    /// 文档路径。
    pub path: PathBuf,
    /// 是否固定标签。
    #[serde(default)]
    pub pinned: bool,
    /// 宿主解释的视图模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_mode: Option<String>,
    /// 选择状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<WorkspaceSessionSelection>,
    /// 水平滚动偏移。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_x: Option<f32>,
    /// 垂直滚动偏移。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_y: Option<f32>,
}

impl WorkspaceSessionTab {
    /// 用默认视图状态构造标签。
    #[must_use]
    pub fn new(path: PathBuf, pinned: bool) -> Self {
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

/// 一个窗口的可恢复工作区会话。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSession {
    /// 稳定窗口 ID。
    pub id: uuid::Uuid,
    /// 打开的标签。
    pub tabs: Vec<WorkspaceSessionTab>,
    /// 活动标签索引。
    pub active_index: usize,
    /// 工作区根目录。
    #[serde(default)]
    pub workspace_root: Option<PathBuf>,
    /// 窗口几何状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WorkspaceSessionWindow>,
    /// 工作区面板宽度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_panel_width: Option<f32>,
    /// 工作区停靠面板是否打开。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_docked_open: Option<bool>,
    /// 文档侧栏宽度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_sidebar_width: Option<f32>,
    /// 文档侧栏是否打开。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_sidebar_docked_open: Option<bool>,
    /// 分栏比例。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_pane_ratio: Option<f32>,
}

impl WorkspaceSession {
    /// 用空的窗口、面板和滚动状态构造会话。
    #[must_use]
    pub fn new(
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
            document_sidebar_width: None,
            document_sidebar_docked_open: None,
            split_pane_ratio: None,
        }
    }

    /// 移除指定路径；没有可恢复标签时返回 `None`。
    #[must_use]
    pub fn without_paths(mut self, excluded: &[PathBuf]) -> Option<Self> {
        let active_path = self.tabs.get(self.active_index).map(|tab| tab.path.clone());
        let excluded = excluded
            .iter()
            .map(|path| workspace_codec::path_identity(path))
            .collect::<HashSet<_>>();
        self.tabs
            .retain(|tab| !excluded.contains(&workspace_codec::path_identity(&tab.path)));
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
pub(crate) struct WorkspaceSessionRegistry {
    pub(crate) version: u32,
    pub(crate) windows: Vec<WorkspaceSession>,
}

/// 文件系统边界上的工作区会话 store。
#[derive(Clone, Debug)]
pub struct WorkspaceSessionStore {
    dirs: ConfigDirs,
}

impl WorkspaceSessionStore {
    /// 为显式配置目录构造 store。
    #[must_use]
    pub fn new(dirs: ConfigDirs) -> Self {
        Self { dirs }
    }

    /// 为系统配置目录构造 store。
    pub fn from_system() -> Result<Self> {
        Ok(Self::new(ConfigDirs::from_system()?))
    }

    /// 返回关联的配置目录。
    #[must_use]
    pub fn dirs(&self) -> &ConfigDirs {
        &self.dirs
    }

    /// 读取并归一化 registry 中的所有会话。
    pub fn read(&self) -> Result<Vec<WorkspaceSession>> {
        let path = self.dirs.workspace_session_file();
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect '{}'", path.display()));
            }
        };
        if metadata.len() > workspace_codec::SESSION_FILE_LIMIT {
            bail!("workspace session registry exceeds the 1 MiB safety limit");
        }
        let bytes =
            std::fs::read(&path).with_context(|| format!("failed to read '{}'", path.display()))?;
        if bytes.len() as u64 > workspace_codec::SESSION_FILE_LIMIT {
            bail!("workspace session registry exceeds the 1 MiB safety limit");
        }
        workspace_codec::decode_registry(&bytes)
            .with_context(|| format!("failed to parse '{}'", path.display()))
            .and_then(workspace_codec::normalize_registry)
            .map(|registry| registry.windows)
    }

    /// 追加或替换一个窗口会话。
    pub fn upsert(&self, session: &WorkspaceSession) -> Result<()> {
        let mut registry = self.load_registry_for_update()?;
        registry.windows.retain(|window| window.id != session.id);
        let session = workspace_codec::normalize_session(session.clone())?;
        if !session.tabs.is_empty() {
            registry.windows.push(session);
        }
        if registry.windows.len() > workspace_codec::SESSION_WINDOW_LIMIT {
            let excess = registry.windows.len() - workspace_codec::SESSION_WINDOW_LIMIT;
            registry.windows.drain(0..excess);
        }
        self.write_registry(&registry)
    }

    /// 移除一个窗口会话。
    pub fn remove(&self, id: uuid::Uuid) -> Result<()> {
        let mut registry = self.load_registry_for_update()?;
        registry.windows.retain(|window| window.id != id);
        self.write_registry(&registry)
    }

    /// 从所有窗口会话移除一组路径并修复活动索引。
    pub fn remove_paths(&self, paths: &[PathBuf]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let excluded = paths
            .iter()
            .map(|path| workspace_codec::path_identity(path))
            .collect::<HashSet<_>>();
        let mut registry = self.load_registry_for_update()?;
        for session in &mut registry.windows {
            let active_path = session
                .tabs
                .get(session.active_index)
                .map(|tab| tab.path.clone());
            session
                .tabs
                .retain(|tab| !excluded.contains(&workspace_codec::path_identity(&tab.path)));
            session.active_index = active_path
                .as_ref()
                .and_then(|path| session.tabs.iter().position(|tab| tab.path == *path))
                .unwrap_or(0)
                .min(session.tabs.len().saturating_sub(1));
        }
        self.write_registry(&registry)
    }

    fn load_registry_for_update(&self) -> Result<WorkspaceSessionRegistry> {
        Ok(WorkspaceSessionRegistry {
            version: WORKSPACE_SESSION_VERSION,
            windows: self.read()?,
        })
    }

    fn write_registry(&self, registry: &WorkspaceSessionRegistry) -> Result<()> {
        let registry = workspace_codec::normalize_registry(registry.clone())?;
        let path = self.dirs.workspace_session_file();
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
        if bytes.len() as u64 > workspace_codec::SESSION_FILE_LIMIT {
            bail!("workspace session registry exceeds the 1 MiB safety limit");
        }
        atomic_write(&path, &bytes)
    }
}

/// 读取系统配置目录中的工作区会话。
pub fn read_workspace_sessions() -> Result<Vec<WorkspaceSession>> {
    WorkspaceSessionStore::from_system()?.read()
}

/// 在系统配置目录中追加或替换工作区会话。
pub fn upsert_workspace_session(session: &WorkspaceSession) -> Result<()> {
    WorkspaceSessionStore::from_system()?.upsert(session)
}

/// 从系统配置目录中移除一个窗口会话。
pub fn remove_workspace_session(id: uuid::Uuid) -> Result<()> {
    WorkspaceSessionStore::from_system()?.remove(id)
}

/// 从系统配置目录所有会话中移除路径。
pub fn remove_paths_from_workspace_sessions(paths: &[PathBuf]) -> Result<()> {
    WorkspaceSessionStore::from_system()?.remove_paths(paths)
}
