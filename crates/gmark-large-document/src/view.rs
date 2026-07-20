// @author kongweiguang

//! Source-first 派生视图契约。派生投影永远不拥有文档真值。

use std::any::Any;
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

use crate::{
    DocumentFormat, LargeDocumentAdapter, LargeDocumentError, SearchCancellation, SourceAffinity,
    SourceAnchor, SourceSelection,
};

pub const DEFAULT_JSON_GRAPH_NODE_LIMIT: usize = 1_500;
pub const DEFAULT_DELIMITED_ROW_WINDOW: usize = 512;
pub const DEFAULT_DELIMITED_COLUMN_WINDOW: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentViewId(Arc<str>);

impl DocumentViewId {
    pub fn new(value: impl Into<Arc<str>>) -> Self {
        Self(value.into())
    }

    pub fn source() -> Self {
        Self::new("source")
    }

    pub fn markdown_live() -> Self {
        Self::new("markdown-live")
    }

    pub fn markdown_split() -> Self {
        Self::new("markdown-split")
    }

    pub fn markdown_preview() -> Self {
        Self::new("markdown-preview")
    }

    pub fn json_graph() -> Self {
        Self::new("json-graph")
    }

    pub fn json_structure() -> Self {
        Self::new("json-structure")
    }

    pub fn delimited_table() -> Self {
        Self::new("delimited-table")
    }

    pub fn markdown_tables() -> Self {
        Self::new("markdown-tables")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewFormat {
    Markdown,
    Json,
    JsonLines,
    Delimited,
    PlainText,
}

impl From<&DocumentFormat> for ViewFormat {
    fn from(format: &DocumentFormat) -> Self {
        match format {
            DocumentFormat::Markdown => Self::Markdown,
            DocumentFormat::Json => Self::Json,
            DocumentFormat::JsonLines => Self::JsonLines,
            DocumentFormat::Delimited { .. } => Self::Delimited,
            DocumentFormat::PlainText => Self::PlainText,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewDescriptor {
    pub id: DocumentViewId,
    pub label: Arc<str>,
    pub icon: Arc<str>,
    pub supported_formats: Arc<[ViewFormat]>,
    pub available: bool,
    pub read_only: bool,
    pub max_items: Option<usize>,
}

impl ViewDescriptor {
    pub fn source() -> Self {
        Self {
            id: DocumentViewId::source(),
            label: Arc::from("Source"),
            icon: Arc::from("file-code"),
            supported_formats: Arc::from([
                ViewFormat::Markdown,
                ViewFormat::Json,
                ViewFormat::JsonLines,
                ViewFormat::Delimited,
                ViewFormat::PlainText,
            ]),
            available: true,
            read_only: false,
            max_items: None,
        }
    }

    pub fn supports(&self, format: &DocumentFormat) -> bool {
        let format = ViewFormat::from(format);
        self.supported_formats.contains(&format)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocator {
    pub range: Range<u64>,
}

impl SourceLocator {
    pub fn new(range: Range<u64>) -> Self {
        Self { range }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JsonGraphItemId(Arc<str>);

impl JsonGraphItemId {
    pub fn new(value: impl Into<Arc<str>>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonValueKind {
    Object,
    Array,
    String,
    Number,
    Boolean,
    Null,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphNode {
    pub id: JsonGraphItemId,
    pub json_path: Arc<str>,
    pub source: SourceLocator,
    pub kind: JsonValueKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonGraphEdgeKind {
    ObjectMember,
    ArrayItem,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphEdge {
    pub id: JsonGraphItemId,
    pub from: JsonGraphItemId,
    pub to: JsonGraphItemId,
    pub source: SourceLocator,
    pub kind: JsonGraphEdgeKind,
}

/// 图投影只保存可回收的数据窗口；达到节点上限时由 provider 标记 truncated，
/// UI 必须要求筛选、按层展开或局部子树，不能继续构造完整巨图。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphProjection {
    pub nodes: Arc<[JsonGraphNode]>,
    pub edges: Arc<[JsonGraphEdge]>,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelimitedCellProjection {
    pub record_index: u64,
    pub column_index: usize,
    pub source: SourceLocator,
    pub display_value: Arc<str>,
}

/// 表格 provider 一次只发布二维窗口；排序和筛选属于派生状态，cell 仍以 source range
/// 回到唯一文档真值，未来编辑也只能生成 DerivedEdit。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelimitedWindowProjection {
    pub record_range: Range<u64>,
    pub column_range: Range<usize>,
    pub cells: Arc<[DelimitedCellProjection]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedProjectionRequest {
    pub document_epoch: u64,
    pub revision: u64,
    pub generation: u64,
    pub root: Option<SourceLocator>,
    pub item_limit: usize,
}

impl DerivedProjectionRequest {
    /// 派生结果必须与发起请求的文档、版本和任务代次完全一致才能安装。
    pub fn accepts(&self, snapshot: &dyn DerivedProjectionSnapshot) -> bool {
        snapshot.document_epoch() == self.document_epoch
            && snapshot.revision() == self.revision
            && snapshot.generation() == self.generation
    }
}

pub trait DerivedProjectionSnapshot: Any + Send + Sync {
    fn document_epoch(&self) -> u64;
    fn revision(&self) -> u64;
    fn generation(&self) -> u64;
    fn status(&self) -> DerivedProjectionStatus;
    fn source_locators(&self) -> &[SourceLocator];
    fn as_any(&self) -> &dyn Any;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DerivedProjectionStatus {
    Ready,
    Partial,
    LimitExceeded,
    Failed,
    Cancelled,
}

/// Provider 只能读取命令触发时捕获的持久 PieceTree 快照，不能持有 UI 文档实体。
pub trait ImmutableDocumentSnapshot: Send + Sync {
    fn revision(&self) -> u64;
    fn len(&self) -> u64;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>, LargeDocumentError>;
}

impl ImmutableDocumentSnapshot for LargeDocumentAdapter {
    fn revision(&self) -> u64 {
        LargeDocumentAdapter::revision(self)
    }

    fn len(&self) -> u64 {
        LargeDocumentAdapter::len(self)
    }

    fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>, LargeDocumentError> {
        LargeDocumentAdapter::read_range(self, range)
    }
}

pub trait DerivedProjectionProvider: Send + Sync {
    fn descriptor(&self) -> &ViewDescriptor;

    fn build(
        &self,
        document: &dyn ImmutableDocumentSnapshot,
        request: &DerivedProjectionRequest,
        cancellation: &SearchCancellation,
    ) -> Result<Arc<dyn DerivedProjectionSnapshot>, LargeDocumentError>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceViewState {
    pub selection: SourceSelection,
    pub top_byte_anchor: SourceAnchor,
    pub line_offset_y: f32,
}

impl Default for SourceViewState {
    fn default() -> Self {
        Self {
            selection: SourceSelection::default(),
            top_byte_anchor: SourceAnchor::new(0, SourceAffinity::Before),
            line_offset_y: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DerivedViewState {
    pub camera_x: f32,
    pub camera_y: f32,
    pub zoom: f32,
    pub expanded_items: Vec<Arc<str>>,
    pub filter: Arc<str>,
}

impl Default for DerivedViewState {
    fn default() -> Self {
        Self {
            camera_x: 0.0,
            camera_y: 0.0,
            zoom: 1.0,
            expanded_items: Vec::new(),
            filter: Arc::from(""),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentViewState {
    pub source: SourceViewState,
    pub derived: BTreeMap<DocumentViewId, DerivedViewState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedTextEdit {
    pub range: Range<u64>,
    pub replacement: Arc<str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedEdit {
    pub base_revision: u64,
    pub edits: Vec<DerivedTextEdit>,
}

impl DerivedEdit {
    /// 派生编辑只能提交到构建 projection 时对应的 Source revision。
    pub fn is_applicable_to(&self, current_revision: u64) -> bool {
        self.base_revision == current_revision
    }
}

#[derive(Default)]
pub struct DocumentViewRegistry {
    providers: BTreeMap<DocumentViewId, Arc<dyn DerivedProjectionProvider>>,
}

impl DocumentViewRegistry {
    pub fn register(&mut self, provider: Arc<dyn DerivedProjectionProvider>) -> bool {
        let id = provider.descriptor().id.clone();
        if id == DocumentViewId::source() || self.providers.contains_key(&id) {
            return false;
        }
        self.providers.insert(id, provider);
        true
    }

    pub fn provider(&self, id: &DocumentViewId) -> Option<Arc<dyn DerivedProjectionProvider>> {
        self.providers.get(id).cloned()
    }

    /// 只有 descriptor 对当前格式真实可用时才返回 provider，调用方不再按扩展名
    /// 猜测某个固定 view id。这样同一格式以后可以并存多个派生能力。
    pub fn available_provider(
        &self,
        id: &DocumentViewId,
        format: &DocumentFormat,
    ) -> Option<Arc<dyn DerivedProjectionProvider>> {
        self.providers
            .get(id)
            .filter(|provider| {
                let descriptor = provider.descriptor();
                descriptor.available && descriptor.supports(format)
            })
            .cloned()
    }

    pub fn first_available_provider(
        &self,
        format: &DocumentFormat,
    ) -> Option<Arc<dyn DerivedProjectionProvider>> {
        self.providers
            .values()
            .find(|provider| {
                let descriptor = provider.descriptor();
                descriptor.available && descriptor.supports(format)
            })
            .cloned()
    }

    pub fn available(&self, format: &DocumentFormat) -> Vec<ViewDescriptor> {
        self.providers
            .values()
            .map(|provider| provider.descriptor())
            .filter(|descriptor| descriptor.available && descriptor.supports(format))
            .cloned()
            .collect()
    }

    /// Source 永远是首个且不可注销的回退视图；其余入口仅来自已注册 provider。
    pub fn views_for(&self, format: &DocumentFormat) -> Vec<ViewDescriptor> {
        let mut views = vec![ViewDescriptor::source()];
        views.extend(self.available(format));
        views
    }
}
