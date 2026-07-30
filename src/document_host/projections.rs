// @author kongweiguang

//! Derived-projection providers that keep format models independent of GPUI.

use super::*;

pub(super) struct RegisteredStructuredProvider {
    descriptor: ViewDescriptor,
}

impl RegisteredStructuredProvider {
    pub(super) fn for_format(format: &DocumentFormat) -> Option<Self> {
        let (id, label, icon, supported_formats, max_items) = match format {
            DocumentFormat::Markdown => (
                DocumentViewId::markdown_tables(),
                "Markdown Tables",
                "table",
                Arc::from([ViewFormat::Markdown]),
                None,
            ),
            DocumentFormat::JsonLines => (
                DocumentViewId::json_structure(),
                "JSON Structure",
                "braces",
                Arc::from([ViewFormat::JsonLines]),
                Some(DEFAULT_JSON_GRAPH_ITEM_LIMIT),
            ),
            DocumentFormat::Json => return None,
            DocumentFormat::Delimited { .. } => (
                DocumentViewId::delimited_table(),
                "Delimited Table",
                "table",
                Arc::from([ViewFormat::Delimited]),
                Some(DEFAULT_DELIMITED_ROW_WINDOW * DEFAULT_DELIMITED_COLUMN_WINDOW),
            ),
            DocumentFormat::PlainText => return None,
        };
        Some(Self {
            descriptor: ViewDescriptor {
                id,
                label: Arc::from(label),
                icon: Arc::from(icon),
                supported_formats,
                available: true,
                // 图编辑只生成带 revision 的 Source transaction，不直接修改 projection。
                read_only: false,
                max_items,
            },
        })
    }

    pub(super) fn view_id(&self) -> DocumentViewId {
        self.descriptor.id.clone()
    }
}

impl DerivedProjectionProvider for RegisteredStructuredProvider {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn build(
        &self,
        document: &dyn DocumentSnapshot,
        request: &DerivedProjectionRequest,
        cancellation: &dyn ProjectionCancellation,
    ) -> Result<Arc<dyn DerivedProjectionSnapshot>, ProjectionError> {
        if cancellation.is_cancelled() {
            return Err(ProjectionError::Cancelled);
        }
        if document.revision().0 != request.revision {
            return Err(ProjectionError::SourceChanged);
        }
        let locator = request
            .root
            .clone()
            .unwrap_or_else(|| SourceLocator::new(0..document.len()));
        if locator.range.start > locator.range.end || locator.range.end > document.len() {
            return Err(ProjectionError::InvalidSourceRange {
                start: locator.range.start,
                end: locator.range.end,
                len: document.len(),
            });
        }
        Ok(Arc::new(RegisteredStructuredSnapshot {
            document_epoch: request.document_epoch,
            revision: request.revision,
            generation: request.generation,
            locators: vec![locator],
        }))
    }
}

struct RegisteredStructuredSnapshot {
    document_epoch: u64,
    revision: u64,
    generation: u64,
    locators: Vec<SourceLocator>,
}

/// JSON 格式 Provider：把后端无关快照投影为 Registry 可安装的图状态。
/// 适配仅发生在格式边界，JSON crate 与 Provider 都不感知 GPUI 或具体存储后端。
pub(super) struct JsonGraphProjectionProvider {
    descriptor: ViewDescriptor,
    focused_roots: JsonFocusedRoots,
}

pub(super) type JsonFocusedRoots = Arc<Mutex<HashMap<(u64, u64), JsonGraphRoot>>>;

impl JsonGraphProjectionProvider {
    pub(super) fn new(focused_roots: JsonFocusedRoots) -> Self {
        Self {
            descriptor: ViewDescriptor {
                id: DocumentViewId::json_graph(),
                label: Arc::from("JSON Graph"),
                icon: Arc::from("graph"),
                supported_formats: Arc::from([ViewFormat::Json]),
                available: true,
                // 图本身不持有可变 JSON；编辑始终提交带 revision 的 Source transaction。
                read_only: false,
                max_items: Some(DEFAULT_JSON_GRAPH_ITEM_LIMIT),
            },
            focused_roots,
        }
    }
}

impl DerivedProjectionProvider for JsonGraphProjectionProvider {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn build(
        &self,
        document: &dyn DocumentSnapshot,
        request: &DerivedProjectionRequest,
        cancellation: &dyn ProjectionCancellation,
    ) -> Result<Arc<dyn DerivedProjectionSnapshot>, ProjectionError> {
        let json_request = JsonGraphRequest {
            document_epoch: request.document_epoch,
            revision: request.revision,
            generation: request.generation,
            root: self
                .focused_roots
                .lock()
                .ok()
                .and_then(|mut roots| roots.remove(&(request.document_epoch, request.generation)))
                .or_else(|| {
                    request.root.as_ref().map(|root| {
                        JsonGraphRoot::new(JsonSourceLocator::new(root.range.clone()), "$", "$")
                    })
                }),
            item_limit: request.item_limit,
        };
        let snapshot = JsonGraphProvider
            .build(document, &json_request, cancellation)
            .map_err(map_json_graph_error)?;
        Ok(Arc::new(snapshot))
    }
}

fn map_json_graph_error(error: JsonGraphError) -> ProjectionError {
    match error {
        JsonGraphError::Cancelled => ProjectionError::Cancelled,
        JsonGraphError::SourceChanged => ProjectionError::SourceChanged,
        JsonGraphError::InvalidRange { start, end, len } => {
            ProjectionError::InvalidSourceRange { start, end, len }
        }
        JsonGraphError::RangeTooLarge => ProjectionError::Build("source range is too large".into()),
        JsonGraphError::InvalidJson { offset, message } => {
            ProjectionError::InvalidJson { offset, message }
        }
        JsonGraphError::Read(error) => ProjectionError::Build(error.to_string()),
    }
}

impl DerivedProjectionSnapshot for RegisteredStructuredSnapshot {
    fn document_epoch(&self) -> u64 {
        self.document_epoch
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn status(&self) -> DerivedProjectionStatus {
        DerivedProjectionStatus::Ready
    }

    fn source_locators(&self) -> &[SourceLocator] {
        &self.locators
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
