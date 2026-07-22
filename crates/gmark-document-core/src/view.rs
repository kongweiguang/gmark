// @author kongweiguang

use std::any::Any;
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

use crate::{
    DocumentFormat, DocumentSnapshot, ProjectionError, SourceAffinity, SourceAnchor,
    SourceSelection,
};

pub const DEFAULT_DELIMITED_ROW_WINDOW: usize = 512;
pub const DEFAULT_DELIMITED_COLUMN_WINDOW: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocator {
    pub range: Range<u64>,
}

impl SourceLocator {
    pub fn new(range: Range<u64>) -> Self {
        Self { range }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelimitedCellProjection {
    pub record_index: u64,
    pub column_index: usize,
    pub source: SourceLocator,
    pub display_value: Arc<str>,
}

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

pub trait ProjectionCancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

pub trait DerivedProjectionProvider: Send + Sync {
    fn descriptor(&self) -> &ViewDescriptor;

    fn build(
        &self,
        document: &dyn DocumentSnapshot,
        request: &DerivedProjectionRequest,
        cancellation: &dyn ProjectionCancellation,
    ) -> Result<Arc<dyn DerivedProjectionSnapshot>, ProjectionError>;
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

    pub fn views_for(&self, format: &DocumentFormat) -> Vec<ViewDescriptor> {
        let mut views = vec![ViewDescriptor::source()];
        views.extend(self.available(format));
        views
    }
}

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
    pub fn markdown_preview() -> Self {
        Self::new("markdown-preview")
    }
    pub fn markdown_split() -> Self {
        Self::new("markdown-split")
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
    fn from(value: &DocumentFormat) -> Self {
        match value {
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

    fn format(id: DocumentViewId, format: ViewFormat, read_only: bool) -> Self {
        let (label, icon) = match id.as_str() {
            "markdown-live" => ("Live", "eye"),
            "markdown-preview" => ("Preview", "eye"),
            "markdown-split" => ("Split", "columns"),
            "json-graph" => ("JSON Graph", "graph"),
            "json-structure" => ("JSON Structure", "braces"),
            "delimited-table" => ("Delimited Table", "table"),
            "markdown-tables" => ("Markdown Tables", "table"),
            _ => ("View", "file"),
        };
        Self {
            id,
            label: Arc::from(label),
            icon: Arc::from(icon),
            supported_formats: Arc::from([format]),
            available: true,
            read_only,
            max_items: None,
        }
    }

    pub fn supports(&self, format: &DocumentFormat) -> bool {
        self.supported_formats.contains(&ViewFormat::from(format))
    }

    pub fn regular_views_for(format: &DocumentFormat) -> Vec<Self> {
        let mut views = vec![Self::source()];
        match format {
            DocumentFormat::Markdown => views.extend([
                Self::format(DocumentViewId::markdown_live(), ViewFormat::Markdown, false),
                Self::format(
                    DocumentViewId::markdown_preview(),
                    ViewFormat::Markdown,
                    true,
                ),
                Self::format(
                    DocumentViewId::markdown_split(),
                    ViewFormat::Markdown,
                    false,
                ),
            ]),
            DocumentFormat::Json => views.push(Self::format(
                DocumentViewId::json_graph(),
                ViewFormat::Json,
                false,
            )),
            DocumentFormat::JsonLines => views.push(Self::format(
                DocumentViewId::json_structure(),
                ViewFormat::JsonLines,
                false,
            )),
            DocumentFormat::Delimited { .. } => views.push(Self::format(
                DocumentViewId::delimited_table(),
                ViewFormat::Delimited,
                false,
            )),
            DocumentFormat::PlainText => {}
        }
        views
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DerivedViewState {
    pub camera_x: f32,
    pub camera_y: f32,
    pub zoom: f32,
    pub expanded_items: Vec<Arc<str>>,
    pub collapsed_items: Vec<Arc<str>>,
    pub selected_item: Option<Arc<str>>,
    pub filter: Arc<str>,
}

impl Default for DerivedViewState {
    fn default() -> Self {
        Self {
            camera_x: 0.0,
            camera_y: 0.0,
            zoom: 1.0,
            expanded_items: Vec::new(),
            collapsed_items: Vec::new(),
            selected_item: None,
            filter: Arc::from(""),
        }
    }
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

/// 会话级可恢复视图状态。Resident 与 Paged 共用同一结构，后端不得再定义副本。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentViewState {
    pub source: SourceViewState,
    pub active_view: Option<DocumentViewId>,
    pub derived: BTreeMap<DocumentViewId, DerivedViewState>,
}
