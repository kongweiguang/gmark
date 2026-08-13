// @author kongweiguang

//! 版本化、多窗口工作区会话的领域类型与文件 store。

use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use gmark_document_core::{SourceAffinity, SourceAnchor, SourceSelection};
use serde::{Deserialize, Serialize};

use crate::{AppDirs, persistence::atomic_write_private, workspace_codec};

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
    /// Converts the document-core selection used by the existing UI adapter
    /// into the versioned persistence representation.
    #[must_use]
    pub fn from_source_selection(selection: SourceSelection) -> Self {
        let range = selection.range();
        Self {
            start: range.start.min(usize::MAX as u64) as usize,
            end: range.end.min(usize::MAX as u64) as usize,
            reversed: selection.reversed(),
            anchor_affinity: Some(selection.anchor.affinity.into()),
            head_affinity: Some(selection.head.affinity.into()),
        }
    }

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

    /// Restores the document-core selection expected by the UI adapter.
    #[must_use]
    pub fn source_selection_for_range(&self, range: std::ops::Range<usize>) -> SourceSelection {
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

/// 稳定的工作区 pane 标识。
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct WorkspaceSessionPaneId(pub uuid::Uuid);

impl WorkspaceSessionPaneId {
    /// 生成一个新的 pane 标识。
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// 返回底层 UUID。
    #[must_use]
    pub const fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}

impl From<uuid::Uuid> for WorkspaceSessionPaneId {
    fn from(value: uuid::Uuid) -> Self {
        Self(value)
    }
}

impl From<WorkspaceSessionPaneId> for uuid::Uuid {
    fn from(value: WorkspaceSessionPaneId) -> Self {
        value.0
    }
}

/// pane tree 分割方向。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSessionSplitAxis {
    /// 左右分割。
    #[default]
    Horizontal,
    /// 上下分割。
    Vertical,
}

/// 工作区文档引用；恢复文档没有文件路径。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSessionDocumentRef {
    /// 文件文档。
    File(PathBuf),
    /// 恢复区文档。
    Recovery(uuid::Uuid),
}

impl WorkspaceSessionDocumentRef {
    /// 返回文件引用的路径。
    #[must_use]
    pub fn file_path(&self) -> Option<&std::path::Path> {
        match self {
            Self::File(path) => Some(path),
            Self::Recovery(_) => None,
        }
    }
}

/// Markdown 折叠状态的稳定、宿主无关 JSON 值。
pub type WorkspaceSessionMarkdownFold = serde_json::Value;

/// 表格布局状态的稳定、宿主无关 JSON 值。
pub type WorkspaceSessionTableLayout = serde_json::Value;

/// 历史栈条目的稳定、宿主无关 JSON 值。
pub type WorkspaceSessionHistoryEntry = serde_json::Value;

/// 一个 pane 中的视图状态；不依赖 GPUI 或具体文档实现。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSessionPaneViewState {
    /// 文本选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<WorkspaceSessionSelection>,
    /// 水平滚动偏移。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_x: Option<f32>,
    /// 垂直滚动偏移。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_y: Option<f32>,
    /// 宿主解释的视图模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_mode: Option<String>,
    /// Split view 在源码/预览两列之间的比例（区别于 pane tree ratio）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_ratio: Option<f32>,
    /// Markdown 折叠快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markdown_fold: Option<WorkspaceSessionMarkdownFold>,
    /// 多区域 Markdown 折叠快照；与单值字段兼容不同宿主。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markdown_folds: Vec<WorkspaceSessionMarkdownFold>,
    /// 表格列布局快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_layout: Option<WorkspaceSessionTableLayout>,
    /// 前进历史栈，写入时最多保留 32 项。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forward: Vec<WorkspaceSessionHistoryEntry>,
    /// 后退历史栈，写入时最多保留 32 项。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub back: Vec<WorkspaceSessionHistoryEntry>,
}

impl WorkspaceSessionPaneViewState {}

/// 会话中保存的一个文档标签。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSessionTab {
    /// view instance 的稳定标识。
    #[serde(default = "uuid::Uuid::new_v4")]
    pub id: uuid::Uuid,
    /// 文档引用。
    #[serde(default = "default_document_ref")]
    pub document: WorkspaceSessionDocumentRef,
    /// 是否固定标签。
    #[serde(default)]
    pub pinned: bool,
    /// 中立视图状态。
    #[serde(default)]
    pub state: WorkspaceSessionPaneViewState,
}

fn default_document_ref() -> WorkspaceSessionDocumentRef {
    WorkspaceSessionDocumentRef::File(PathBuf::new())
}

impl Default for WorkspaceSessionTab {
    fn default() -> Self {
        Self::new(PathBuf::new(), false)
    }
}

impl WorkspaceSessionTab {
    /// 用默认视图状态构造文件标签。
    #[must_use]
    pub fn new(path: PathBuf, pinned: bool) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            document: WorkspaceSessionDocumentRef::File(path.clone()),
            pinned,
            state: WorkspaceSessionPaneViewState::default(),
        }
    }

    /// 用恢复文档引用构造标签。
    #[must_use]
    pub fn recovery(document_id: uuid::Uuid, pinned: bool) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            document: WorkspaceSessionDocumentRef::Recovery(document_id),
            pinned,
            state: WorkspaceSessionPaneViewState::default(),
        }
    }

    /// 返回文档路径（恢复文档返回 `None`）。
    #[must_use]
    pub fn document_path(&self) -> Option<&std::path::Path> {
        self.document.file_path()
    }
}

/// 一个 pane 的标签集合和活动 view instance。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSessionPane {
    /// pane 中的标签。
    #[serde(default)]
    pub tabs: Vec<WorkspaceSessionTab>,
    /// 活动标签的 view instance ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tab: Option<uuid::Uuid>,
}

impl WorkspaceSessionPane {
    /// 构造 pane。
    #[must_use]
    pub fn new(tabs: Vec<WorkspaceSessionTab>, active_tab: Option<uuid::Uuid>) -> Self {
        Self { tabs, active_tab }
    }
}

/// 递归 pane 树；叶子通过 `panes` map 查找 pane 内容。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSessionPaneNode {
    /// 一个 pane 叶子。
    Leaf(WorkspaceSessionPaneId),
    /// 两个子树的分割。
    Split {
        /// 分割方向。
        axis: WorkspaceSessionSplitAxis,
        /// 第一子树比例，归一化到 `.1..=.9`。
        ratio: f32,
        /// 第一子树。
        first: Box<Self>,
        /// 第二子树。
        second: Box<Self>,
    },
}

pub type WorkspaceSessionPaneTree = WorkspaceSessionPaneNode;

/// 一个窗口的可恢复工作区会话。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSession {
    /// 稳定窗口 ID。
    pub id: uuid::Uuid,
    /// pane tree 根节点。
    pub root: WorkspaceSessionPaneNode,
    /// pane 内容 map。
    #[serde(default)]
    pub panes: BTreeMap<WorkspaceSessionPaneId, WorkspaceSessionPane>,
    /// 当前聚焦 pane。
    pub focused_pane: WorkspaceSessionPaneId,
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
    /// 旧版分栏比例字段；v10 tree ratio 才是 pane 布局真值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_pane_ratio: Option<f32>,
}

impl WorkspaceSession {
    /// 构造一个空单叶 pane。
    #[must_use]
    pub fn single_pane(id: uuid::Uuid, workspace_root: Option<PathBuf>) -> Self {
        let pane_id = WorkspaceSessionPaneId::new();
        let mut panes = BTreeMap::new();
        panes.insert(pane_id, WorkspaceSessionPane::default());
        Self {
            id,
            root: WorkspaceSessionPaneNode::Leaf(pane_id),
            panes,
            focused_pane: pane_id,
            workspace_root,
            window: None,
            workspace_panel_width: None,
            workspace_docked_open: None,
            document_sidebar_width: None,
            document_sidebar_docked_open: None,
            split_pane_ratio: None,
        }
    }

    /// 返回当前聚焦 pane。
    #[must_use]
    pub fn focused(&self) -> Option<&WorkspaceSessionPane> {
        self.panes.get(&self.focused_pane)
    }

    /// 返回当前聚焦 pane 的可变引用。
    pub fn focused_mut(&mut self) -> Option<&mut WorkspaceSessionPane> {
        self.panes.get_mut(&self.focused_pane)
    }

    /// 返回会话是否包含至少一个标签。
    #[must_use]
    pub fn has_tabs(&self) -> bool {
        self.panes.values().any(|pane| !pane.tabs.is_empty())
    }

    /// 移除指定路径；没有可恢复标签时返回 `None`。
    #[must_use]
    pub fn without_paths(mut self, excluded: &[PathBuf]) -> Option<Self> {
        let excluded = excluded
            .iter()
            .map(|path| workspace_codec::path_identity(path))
            .collect::<HashSet<_>>();
        for pane in self.panes.values_mut() {
            pane.tabs.retain(|tab| {
                !tab.document
                    .file_path()
                    .is_some_and(|path| excluded.contains(&workspace_codec::path_identity(path)))
            });
            normalize_active_tab(pane);
        }
        if !self.has_tabs() {
            return None;
        }
        Some(self)
    }
}

fn normalize_active_tab(pane: &mut WorkspaceSessionPane) {
    pane.active_tab = pane
        .active_tab
        .filter(|id| pane.tabs.iter().any(|tab| tab.id == *id))
        .or_else(|| pane.tabs.first().map(|tab| tab.id));
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct WorkspaceSessionRegistry {
    pub(crate) version: u32,
    pub(crate) windows: Vec<WorkspaceSession>,
}

/// 文件系统边界上的工作区会话 store。
#[derive(Clone, Debug)]
pub struct WorkspaceSessionStore {
    dirs: AppDirs,
}

impl WorkspaceSessionStore {
    /// 为显式配置目录构造 store。
    #[must_use]
    pub fn new(dirs: AppDirs) -> Self {
        Self { dirs }
    }

    /// 为系统配置目录构造 store。
    pub fn from_system() -> Result<Self> {
        Ok(Self::new(AppDirs::from_system()?))
    }

    /// 返回关联的配置目录。
    #[must_use]
    pub fn dirs(&self) -> &AppDirs {
        &self.dirs
    }

    /// 返回旧会话的首次迁移备份路径。
    #[must_use]
    pub fn pre_v10_backup_path(&self) -> PathBuf {
        self.dirs.workspace_session_pre_v10_file()
    }

    /// 读取并归一化 registry 中的所有会话。
    pub fn read(&self) -> Result<Vec<WorkspaceSession>> {
        self.dirs.validate_state_root()?;
        let path = self.dirs.workspace_session_file();
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "workspace session file '{}' must not be a symbolic link",
                    path.display()
                );
            }
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
        if session.has_tabs() {
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

    /// 从所有窗口会话移除一组文件路径并修复活动标签。
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
            for pane in session.panes.values_mut() {
                pane.tabs.retain(|tab| {
                    !tab.document.file_path().is_some_and(|path| {
                        excluded.contains(&workspace_codec::path_identity(path))
                    })
                });
                normalize_active_tab(pane);
            }
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
        let bytes = serde_json::to_vec_pretty(&registry)?;
        if bytes.len() as u64 > workspace_codec::SESSION_FILE_LIMIT {
            bail!("workspace session registry exceeds the 1 MiB safety limit");
        }
        self.dirs.ensure_state_parent(&path)?;
        self.backup_pre_v10_if_needed(&path)?;
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
        atomic_write_private(&path, &bytes)
    }

    fn backup_pre_v10_if_needed(&self, path: &Path) -> Result<()> {
        let source = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to read '{}' for migration backup", path.display())
                });
            }
        };
        if !workspace_codec::is_pre_v10_bytes(&source) {
            return Ok(());
        }
        let backup = self.pre_v10_backup_path();
        match std::fs::symlink_metadata(&backup) {
            Ok(metadata) if metadata.is_file() => return Ok(()),
            Ok(_) => bail!(
                "migration backup path '{}' is not a regular file",
                backup.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to inspect migration backup '{}'", backup.display())
                });
            }
        }
        self.dirs.ensure_state_parent(&backup)?;
        atomic_write_noclobber(&backup, &source)
            .with_context(|| format!("failed to create migration backup '{}'", backup.display()))
    }
}

/// Write a migration backup without a replace-on-race window. The final hard-link
/// creation is exclusive on the same filesystem: a concurrent creator wins and
/// this call fails without touching its contents.
fn atomic_write_noclobber(path: &Path, contents: &[u8]) -> Result<()> {
    if path.file_name().is_none() {
        bail!("atomic write target has no file name");
    }
    let parent = path
        .parent()
        .filter(|candidate| !candidate.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary = parent.join(format!(
        ".gmark-config-pre-v10-{}.tmp",
        uuid::Uuid::new_v4()
    ));
    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .with_context(|| format!("failed to create '{}'", temporary.display()))?;
        file.write_all(contents)
            .with_context(|| format!("failed to write '{}'", temporary.display()))?;
        file.flush()
            .with_context(|| format!("failed to flush '{}'", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync '{}'", temporary.display()))?;
        drop(file);
        fs::hard_link(&temporary, path)
            .with_context(|| format!("failed to install exclusive backup '{}'", path.display()))?;
        fs::remove_file(&temporary).with_context(|| {
            format!(
                "failed to remove temporary backup '{}'",
                temporary.display()
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
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
