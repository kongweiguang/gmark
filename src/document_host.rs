// @author kongweiguang

//! GPUI shell for disk-backed SourceBacked text documents.

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;

use directories::ProjectDirs;
use gmark_document_core::{
    DEFAULT_DELIMITED_COLUMN_WINDOW, DEFAULT_DELIMITED_ROW_WINDOW, DerivedProjectionProvider,
    DerivedProjectionRequest, DerivedProjectionSnapshot, DerivedProjectionStatus, DocumentFormat,
    DocumentSnapshot, DocumentViewId, DocumentViewRegistry, DocumentViewState,
    ProjectionCancellation, ProjectionError, RecoveryAction, RecoveryBackend, RecoveryRecord,
    SourceAffinity, SourceAnchor, SourceEdit, SourceLocator, SourceSelection, TextEncoding,
    Transaction, ViewDescriptor, ViewFormat,
};
use gmark_document_runtime::DocumentSession;
#[cfg(test)]
use gmark_json_graph::JsonGraphEdgeKind;
use gmark_json_graph::{
    DEFAULT_JSON_GRAPH_ITEM_LIMIT, JsonGraphEdge, JsonGraphError, JsonGraphField, JsonGraphItemId,
    JsonGraphNode, JsonGraphProjection, JsonGraphProvider, JsonGraphRequest, JsonGraphRoot,
    JsonGraphSnapshot, JsonValueKind, SourceLocator as JsonSourceLocator,
};
use gmark_paged_document::{
    DelimitedEdit, DelimitedFilterOptions, DelimitedIndex, DelimitedIndexOptions, EncodedSavePlan,
    ExternalChange, FileSource, JsonIndex, JsonIndexOptions, LineIndex, MarkdownTableIndex,
    OpenProbe, OpenStrategy, PagedDocument as PagedDocumentAdapter, PagedDocumentError,
    PagedRecoveryJournal, PagedRecoverySelection, PieceDocument, PreparedUtf8Source,
    SearchCancellation, SearchMatch, SearchOptions, SelectionTransfer, ViewportRequest,
    prepare_utf8_source, replay_paged_recovery, search_file_source, selection_transfer_for_len,
    serialize_delimited_record, validate_json_lines_cancellable,
    validate_json_lines_from_cancellable,
};
use gpui::prelude::*;
use gpui::{
    AnyView, App, ClipboardItem, Context, Div, Entity, FocusHandle, Focusable, KeyDownEvent,
    MouseButton, MouseDownEvent, Pixels, Point, ScrollHandle, ScrollStrategy, ScrollWheelEvent,
    SharedString, Stateful, Task, UniformListScrollHandle, Window, div, hsla, point, px, relative,
    rems, svg, uniform_list,
};

use crate::components::{
    Block, BlockEvent, BlockHostAction, BlockKind, BlockRecord, Copy, Cut, Delete, DeleteBack,
    DismissTransientUi, ExportSelection, FindInDocument, FindNext, FindPrevious, GoToLine,
    JumpToBottom, JumpToTop, PageDown, PageUp, Paste, Redo, SaveDocument, SelectAll,
    SourceLayoutIdentity, Undo, source_line_number_gutter_width,
};
use crate::document_runtime::DocumentCoordinator;

use crate::i18n::{I18nManager, I18nStrings};
use crate::theme::ThemeManager;

const PREFIX_PREVIEW_BYTES: u64 = 256 * 1024;
const DOCUMENT_HOST_KEY_CONTEXT: &str = "BlockEditor";
const MAX_RENDERED_LINE_BYTES: u64 = 64 * 1024;
const SOURCE_SCROLL_BYTES_PER_PIXEL: f32 = 32.0;
const FALLBACK_SOURCE_ROW_HEIGHT: f32 = 25.6;
const SOURCE_OVERSCAN_ROWS: usize = 96;

/// GPUI/DirectWrite on Windows does not resolve the CSS-style generic `monospace` family.
/// Use a platform font that is part of the base OS so a fresh profile cannot panic on first paint.
pub(crate) fn source_monospace_font_family() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "Consolas"
    }
    #[cfg(target_os = "macos")]
    {
        "Menlo"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        "monospace"
    }
}

fn session_plan(
    profile: &gmark_document_core::DocumentProfile,
    probe: &OpenProbe,
    strategy: OpenStrategy,
    retain_resident_session: bool,
) -> gmark_document_core::OpenPlan {
    let mut policy = gmark_document_core::LoadingPolicy {
        max_resident_bytes: Some(probe.options.max_resident_bytes),
        max_resident_lines: Some(probe.options.max_resident_lines),
        max_structural_units: Some(probe.options.max_structural_units),
        force_safe_source: probe.force_safe_source,
        ..gmark_document_core::LoadingPolicy::default()
    };
    // 恢复日志可能显式要求 Paged，即使当前文件已缩小；仍沿用打开时阈值，
    // 只把本次会话强制为安全 Source，不污染偏好设置。
    if strategy == OpenStrategy::Paged
        && gmark_document_core::OpenPolicyResolver
            .resolve(policy, profile)
            .backend
            == gmark_document_core::DocumentBackendKind::Resident
    {
        policy.force_safe_source = true;
    }
    let plan = gmark_document_core::OpenPolicyResolver.resolve(policy, profile);
    if retain_resident_session
        && strategy == OpenStrategy::Resident
        && plan.backend == gmark_document_core::DocumentBackendKind::Paged
    {
        let mut resident = gmark_document_core::OpenPolicyResolver.resolve(
            gmark_document_core::LoadingPolicy {
                max_resident_bytes: Some(u64::MAX),
                max_resident_lines: Some(u64::MAX),
                max_structural_units: Some(u64::MAX),
                ..gmark_document_core::LoadingPolicy::default()
            },
            profile,
        );
        resident.limits = plan.limits;
        return resident;
    }
    plan
}

/// Probe 后只在这里安装正文后端；格式 Controller 之后只能持有统一 session。
fn build_document_session(
    probe: &OpenProbe,
    original_source: &FileSource,
    utf8_source: FileSource,
    index: LineIndex,
    retain_resident_session: bool,
) -> Result<DocumentSession, PagedDocumentError> {
    let profile = probe.profile();
    let plan = session_plan(&profile, probe, probe.strategy, retain_resident_session);
    let source_identity = original_source.identity()?;
    if source_identity != probe.identity {
        return Err(PagedDocumentError::SourceChanged);
    }
    let file_identity = gmark_document_runtime::FileIdentity::from(&source_identity);
    let store = if probe.strategy == OpenStrategy::Resident {
        let bytes = utf8_source.read_range(0, utf8_source.identity()?.len)?;
        if original_source.identity()? != probe.identity {
            return Err(PagedDocumentError::SourceChanged);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| PagedDocumentError::Binary)?;
        gmark_document_runtime::DocumentStore::Resident(Box::new(
            gmark_document_runtime::ResidentDocument::new(
                text,
                probe.encoding.clone(),
                source_identity,
            ),
        ))
    } else {
        let document = PieceDocument::open(utf8_source, index)?;
        gmark_document_runtime::DocumentStore::Paged(Box::new(PagedDocumentAdapter::new(document)))
    };
    if original_source.identity()? != probe.identity {
        return Err(PagedDocumentError::SourceChanged);
    }
    DocumentSession::new(profile, store, plan, file_identity)
        .map_err(|error| PagedDocumentError::InvalidTransaction(error.to_string()))
}

fn build_paged_session(
    probe: &OpenProbe,
    document: PieceDocument,
    identity: gmark_paged_document::FileIdentity,
) -> Result<DocumentSession, PagedDocumentError> {
    let profile = probe.profile();
    let plan = session_plan(&profile, probe, OpenStrategy::Paged, false);
    DocumentSession::new(
        profile,
        gmark_document_runtime::DocumentStore::Paged(Box::new(PagedDocumentAdapter::new(document))),
        plan,
        gmark_document_runtime::FileIdentity::from(&identity),
    )
    .map_err(|error| PagedDocumentError::InvalidTransaction(error.to_string()))
}

fn verify_saved_session_readback(
    expected: &DocumentSession,
    actual: &DocumentSession,
    cancellation: &SearchCancellation,
) -> Result<(), PagedDocumentError> {
    if expected.len() != actual.len() {
        return Err(PagedDocumentError::InvalidTransaction(
            "saved readback length differs from the save snapshot".into(),
        ));
    }
    const VERIFY_CHUNK_BYTES: u64 = 8 * 1024 * 1024;
    let mut start = 0u64;
    while start < expected.len() {
        if cancellation.is_cancelled() {
            return Err(PagedDocumentError::Cancelled);
        }
        let end = start.saturating_add(VERIFY_CHUNK_BYTES).min(expected.len());
        if expected.read_range(start..end)? != actual.read_range(start..end)? {
            return Err(PagedDocumentError::InvalidTransaction(
                "saved readback bytes differ from the save snapshot".into(),
            ));
        }
        start = end;
    }
    Ok(())
}

type StructureInput = (FileSource, LineIndex, Option<Arc<[u8]>>);

fn structure_input_for_session(
    document: &DocumentSession,
    prepared_source: &PreparedUtf8Source,
    prepared_index: &LineIndex,
    cancellation: &SearchCancellation,
) -> Result<StructureInput, PagedDocumentError> {
    if document.store.kind() == gmark_document_core::DocumentBackendKind::Paged {
        return Ok((
            prepared_source.source().clone(),
            prepared_index.clone(),
            None,
        ));
    }
    if cancellation.is_cancelled() {
        return Err(PagedDocumentError::Cancelled);
    }
    let bytes: Arc<[u8]> = document
        .snapshot()
        .read_range(0..document.len())
        .map_err(|error| PagedDocumentError::InvalidTransaction(error.to_string()))?
        .into();
    Ok((
        prepared_source.source().clone(),
        prepared_index.clone(),
        Some(bytes),
    ))
}

fn modifier_horizontal_wheel_delta(
    shift: bool,
    control: bool,
    delta_x: f32,
    delta_y: f32,
) -> Option<f32> {
    ((shift || control) && delta_y.abs() >= delta_x.abs()).then_some(delta_y)
}

fn recovery_view_id(mode: DocumentHostViewMode) -> DocumentViewId {
    match mode {
        DocumentHostViewMode::Source => DocumentViewId::source(),
        DocumentHostViewMode::Live => DocumentViewId::new("live"),
        DocumentHostViewMode::Structure => DocumentViewId::new("preview"),
        DocumentHostViewMode::Split => DocumentViewId::new("split"),
    }
}

fn record_recovery_transaction(
    backend: &mut dyn RecoveryBackend,
    base_revision: u64,
    range: Range<u64>,
    replacement: impl Into<Arc<str>>,
    selection: Option<SourceSelection>,
    view_id: DocumentViewId,
) -> Result<(), gmark_document_core::PersistenceError> {
    backend.record(&RecoveryRecord {
        action: RecoveryAction::Transaction(Transaction::new(
            gmark_document_core::DocumentRevision(base_revision),
            vec![SourceEdit::new(range, replacement)],
        )),
        selection,
        view_id,
    })
}

fn derived_views_enabled(strategy: OpenStrategy) -> bool {
    strategy == OpenStrategy::Resident
}

// GPUI 的滚动坐标是 f32；把数千万行直接乘行高会在文件尾产生 32–128px
// 量化，最终表现为行号重叠和跳行。uniform_list 永远只承载一个局部滑窗，
// 全局位置由 source_list_origin 和 SourceAnchor 保存。
pub(crate) const SOURCE_LIST_WINDOW_ROWS: usize = 65_536;
// 单行窗口最多 64 KiB；512 行同时给 row/entity/shaped-line 缓存提供 32 MiB
// 的硬上界，且低于契约允许的 2,048 行上限。
pub(crate) const MAX_SOURCE_CACHED_ROWS: usize = 512;
const STRUCTURED_OVERSCAN_ROWS: usize = 64;
const STRUCTURED_CELL_BYTES: usize = 8 * 1024;
const STRUCTURED_CELL_WIDTH: f32 = 220.0;
const STRUCTURED_COLUMN_WINDOW: usize = 16;
const FIND_CASE_ICON: &str = "icon/ui/case-sensitive.svg";
const FIND_WORD_ICON: &str = "icon/ui/whole-word.svg";
const FIND_REGEX_ICON: &str = "icon/ui/regex.svg";
const CHEVRON_UP_ICON: &str = "icon/ui/chevron-up.svg";
const CHEVRON_DOWN_ICON: &str = "icon/ui/chevron-down.svg";
const CLOSE_ICON: &str = "icon/ui/close.svg";

fn localized_document_error(error: &PagedDocumentError, cx: &App) -> SharedString {
    cx.global::<I18nManager>()
        .strings()
        .large_document_error(error)
        .into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DocumentHostViewMode {
    Live,
    Source,
    Structure,
    Split,
}

#[derive(Clone, Copy)]
enum SourceContextCommand {
    Copy,
    Cut,
    Paste,
    SelectAll,
    ExportSelection,
    ExportSelectionUtf8,
}

#[derive(Clone, Debug)]
pub(crate) enum DocumentHostEvent {
    SavedAs(PathBuf),
    StateChanged,
    ViewModeChanged(DocumentHostMode),
    SplitRatioChanged(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocumentHostMode {
    Live,
    Source,
    Preview,
    Split,
}

#[derive(Clone)]
enum StructuredIndex {
    Delimited(DelimitedIndex),
    MarkdownTables {
        tables: Vec<MarkdownTableIndex>,
        selected: usize,
    },
    Json {
        index: JsonIndex,
        source: FileSource,
    },
    JsonLines {
        lines: StructuredLines,
        source: StructuredTextSource,
        record_count: u64,
    },
}

#[derive(Clone)]
enum StructuredLines {
    File(LineIndex),
    Snapshot(Arc<[Range<u64>]>),
}

impl StructuredLines {
    fn line_range(&self, line: u64) -> Option<Range<u64>> {
        match self {
            Self::File(lines) => lines.line_range(line),
            Self::Snapshot(lines) => lines.get(usize::try_from(line).ok()?).cloned(),
        }
    }

    fn line_count(&self) -> u64 {
        match self {
            Self::File(lines) => lines.line_count(),
            Self::Snapshot(lines) => lines.len() as u64,
        }
    }
}

#[derive(Clone)]
enum StructuredTextSource {
    File(FileSource),
    Snapshot(Arc<[u8]>),
}

impl StructuredTextSource {
    fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>, PagedDocumentError> {
        match self {
            Self::File(source) => source.read_range(range.start, range.end),
            Self::Snapshot(bytes) => {
                let len = bytes.len() as u64;
                if range.start > range.end || range.end > len {
                    return Err(PagedDocumentError::InvalidRange {
                        start: range.start,
                        end: range.end,
                        len,
                    });
                }
                let start =
                    usize::try_from(range.start).map_err(|_| PagedDocumentError::RangeTooLarge)?;
                let end =
                    usize::try_from(range.end).map_err(|_| PagedDocumentError::RangeTooLarge)?;
                Ok(bytes[start..end].to_vec())
            }
        }
    }
}

struct RegisteredStructuredProvider {
    descriptor: ViewDescriptor,
}

impl RegisteredStructuredProvider {
    fn for_format(format: &DocumentFormat) -> Option<Self> {
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
struct JsonGraphProjectionProvider {
    descriptor: ViewDescriptor,
    focused_roots: JsonFocusedRoots,
}

type JsonFocusedRoots = Arc<Mutex<HashMap<(u64, u64), JsonGraphRoot>>>;

impl JsonGraphProjectionProvider {
    fn new(focused_roots: JsonFocusedRoots) -> Self {
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

#[derive(Clone, Debug)]
struct StructuredRow {
    index: u64,
    byte_range: Range<u64>,
    column_start: usize,
    cells: Vec<String>,
    depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct JsonNode {
    container_path: Vec<u64>,
    item: u64,
    depth: usize,
}

impl JsonNode {
    fn path(&self) -> Vec<u64> {
        let mut path = self.container_path.clone();
        path.push(self.item);
        path
    }
}

#[path = "document_views/structured_index.rs"]
mod structured_index;

struct SourceLineEdit {
    line: usize,
    range: std::ops::Range<u64>,
    ending: String,
    leading_truncated: bool,
    trailing_truncated: bool,
    block: Entity<Block>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BoundedLineWindow {
    content_range: Range<u64>,
    replace_range: Range<u64>,
    text: SharedString,
    ending: String,
    leading_truncated: bool,
    trailing_truncated: bool,
    display: SharedString,
    display_with_endings: OnceLock<SharedString>,
}

impl BoundedLineWindow {
    fn new(
        content_range: Range<u64>,
        replace_range: Range<u64>,
        text: String,
        ending: String,
        leading_truncated: bool,
        trailing_truncated: bool,
    ) -> Self {
        let text: SharedString = text.into();
        let display = if leading_truncated || trailing_truncated {
            let mut rendered = String::with_capacity(text.len().saturating_add(4));
            if leading_truncated {
                rendered.push_str("… ");
            }
            rendered.push_str(&text);
            if trailing_truncated {
                rendered.push_str(" …");
            }
            rendered.into()
        } else {
            // 常见路径直接复用 GPUI SharedString 的同一份 Arc backing storage。
            text.clone()
        };
        Self {
            content_range,
            replace_range,
            text,
            ending,
            leading_truncated,
            trailing_truncated,
            display,
            display_with_endings: OnceLock::new(),
        }
    }

    fn rendered(&self, show_line_endings: bool) -> SharedString {
        if show_line_endings {
            if self.trailing_truncated || self.ending.is_empty() {
                return self.display.clone();
            }
            self.display_with_endings
                .get_or_init(|| rendered_line_window_text(self, true).into())
                .clone()
        } else {
            self.display.clone()
        }
    }

    /// 前序编辑会平移本行的 source byte range，但可见文本不一定变化。此时 Block 仍是
    /// 有效的输入与布局宿主；独立 SourceLayoutIdentity 会更新坐标并按需失效 shaped layout。
    fn has_same_surface_text(&self, other: &Self) -> bool {
        self.text == other.text
            && self.ending == other.ending
            && self.leading_truncated == other.leading_truncated
            && self.trailing_truncated == other.trailing_truncated
    }
}

/// 一帧 Source 的原子行快照。正文、行号、选择映射、命中测试和无障碍树只能
/// 读取此对象；后台 row cache 仅用于组装下一帧，不能被渲染层半途观察。
#[derive(Clone, Debug)]
struct ScreenLines {
    document_revision: u64,
    generation: u64,
    cache_epoch: u64,
    column_window_start: u64,
    visible: Range<usize>,
    rows: Arc<BTreeMap<usize, Arc<BoundedLineWindow>>>,
}

impl Default for ScreenLines {
    fn default() -> Self {
        Self {
            document_revision: 0,
            generation: 0,
            cache_epoch: 0,
            column_window_start: 0,
            visible: 0..0,
            rows: Arc::new(BTreeMap::new()),
        }
    }
}

impl ScreenLines {
    fn row(&self, line: usize) -> Option<&BoundedLineWindow> {
        self.rows.get(&line).map(Arc::as_ref)
    }

    fn top_source_anchor(&self) -> Option<SourceAnchor> {
        self.row(self.visible.start)
            .map(|row| SourceAnchor::new(row.content_range.start, SourceAffinity::Before))
    }

    /// 随机远跳的新范围尚未读取时，按旧可见区的相对行序保留上一帧正文。
    /// 一旦新范围已有任意真实行，就不再混合两个坐标系，避免 selection/hit-test
    /// 错把旧文本映射到新的 source offset。
    fn should_retain_previous_frame(&self, requested_visible: &Range<usize>) -> bool {
        !self.rows.is_empty()
            && !requested_visible
                .clone()
                .any(|line| self.rows.contains_key(&line))
    }

    fn retained_rows(&self, show_line_endings: bool) -> Vec<(usize, SharedString)> {
        self.rows
            .range(self.visible.clone())
            .map(|(line, row)| (*line, row.rendered(show_line_endings)))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PagedDocumentMetrics {
    viewport_requests: u64,
    viewport_installs: u64,
    stale_viewport_results: u64,
    viewport_cancellations: u64,
    max_cached_rows: usize,
    layout_cache_hits: u64,
    layout_cache_misses: u64,
    max_layout_cache_entries: usize,
    blank_frames_after_content: u64,
    copy_requests: u64,
    copied_bytes: u64,
    export_requests: u64,
    exported_bytes: u64,
    projection_installs: u64,
    stale_projection_results: u64,
}

/// 所有后台结果携带同一组文档身份。只读快照任务可选择仅校验 epoch（例如 Copy
/// 允许正文继续编辑），会回写坐标或 UI 状态的任务必须同时校验 revision 与 generation。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DocumentTaskStamp {
    document_epoch: u64,
    document_revision: Option<u64>,
    generation: u64,
}

impl DocumentTaskStamp {
    fn capture(view: &DocumentHost, generation: u64) -> Self {
        Self {
            document_epoch: view.document_epoch,
            document_revision: view.document.as_ref().map(DocumentSession::revision),
            generation,
        }
    }

    fn accepts_identity(self, view: &DocumentHost, generation: u64) -> bool {
        self.document_epoch == view.document_epoch && self.generation == generation
    }

    fn accepts_strict(self, view: &DocumentHost, generation: u64) -> bool {
        self.accepts_identity(view, generation)
            && self.document_revision == view.document.as_ref().map(DocumentSession::revision)
    }
}

#[derive(Clone)]
enum SourceViewportReader {
    Indexed(Box<DocumentSession>),
    Provisional {
        source: FileSource,
        estimated_lines: u64,
        encoding: TextEncoding,
    },
}

#[derive(Clone)]
struct JsonGraphContextMenu {
    node: JsonGraphItemId,
    /// 相对 JSON 画布的坐标，避免工作区与 Tab 外壳偏移菜单位置。
    position: gpui::Point<gpui::Pixels>,
}

#[derive(Clone)]
struct JsonGraphEditTarget {
    item_id: JsonGraphItemId,
    range: Range<u64>,
    document_epoch: u64,
    base_revision: u64,
    label: Arc<str>,
    kind: JsonValueKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JsonGraphEditIssue {
    Invalid,
    Stale,
    TooLarge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StructuredCellEdit {
    record: Option<u64>,
    column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StructuredMenuTarget {
    Row(u64),
    Column(usize),
}

/// Tab 的格式文档 Host；内部 DocumentSession 是正文、revision 与后端选择的唯一权威状态。
pub(crate) struct DocumentHost {
    path: PathBuf,
    probe: OpenProbe,
    index: Option<LineIndex>,
    document: Option<DocumentSession>,
    prepared_source: Option<PreparedUtf8Source>,
    provisional_source: Option<FileSource>,
    structured_index: Option<StructuredIndex>,
    /// 未保存 CSV/TSV 的结构索引读取此临时快照，磁盘原文件仍由 Save 独占写入。
    structured_rows: BTreeMap<u64, StructuredRow>,
    structured_pending: Option<Range<u64>>,
    structured_generation: u64,
    structured_cancellation: Option<SearchCancellation>,
    structure_error: Option<SharedString>,
    structure_error_byte: Option<u64>,
    structured_filter_input: Entity<Block>,
    structured_cell_input: Entity<Block>,
    structured_cell_edit: Option<StructuredCellEdit>,
    structured_selected_cell: Option<StructuredCellEdit>,
    /// 单元格提交后，旧索引继续稳定渲染到后台快照完成；覆盖值与基线区间偏移
    /// 保证这段窗口内连续编辑相邻单元格仍读取当前文档，而不是闪回旧表格。
    structured_cell_overrides: BTreeMap<StructuredCellEdit, String>,
    structured_cell_source_edits: Vec<(Range<u64>, i64)>,
    structured_context_target: Option<StructuredMenuTarget>,
    structured_column_progress: Option<(Arc<AtomicU64>, u64)>,
    structured_filter_column: Option<usize>,
    structured_filtered_rows: Vec<u64>,
    structured_filter_generation: u64,
    structured_filter_cancellation: Option<SearchCancellation>,
    structured_filter_running: bool,
    hidden_structured_columns: BTreeSet<usize>,
    structured_column_window_start: usize,
    json_child_indexes: BTreeMap<Vec<u64>, JsonIndex>,
    json_expanded_nodes: BTreeSet<Vec<u64>>,
    json_rows: BTreeMap<Vec<u64>, StructuredRow>,
    json_expand_generation: u64,
    json_expand_cancellation: Option<SearchCancellation>,
    view_registry: DocumentViewRegistry,
    /// 仅在 session 尚未安装或被保存任务暂时移出时存在；Ready 状态的唯一视图真值
    /// 位于 `DocumentSession.view_state`，禁止 Host 再保留第二份副本。
    pending_view_state: Option<DocumentViewState>,
    /// 用户最近选择的投影 Provider；切回 Source 后仍保留偏好，但当前 active view 只在 session 中。
    selected_projection_view: Option<DocumentViewId>,
    document_epoch: u64,
    derived_projection_generation: u64,
    derived_projection_cancellation: Option<SearchCancellation>,
    derived_projection_snapshot: Option<Arc<dyn DerivedProjectionSnapshot>>,
    /// JSON 编辑失败时保留最后一次有效图；错误与 stale 标记描述的是当前源码 revision。
    derived_projection_error: Option<SharedString>,
    derived_projection_error_offset: Option<u64>,
    derived_projection_stale: bool,
    derived_projection_root: Option<JsonGraphRoot>,
    json_focused_roots: JsonFocusedRoots,
    graph_selected_item: Option<JsonGraphItemId>,
    graph_search_matches: Vec<JsonGraphItemId>,
    graph_search_selected: usize,
    graph_search_collapsed_before: Option<Vec<Arc<str>>>,
    graph_context_menu: Option<JsonGraphContextMenu>,
    graph_edit_target: Option<JsonGraphEditTarget>,
    graph_edit_input: Entity<Block>,
    graph_edit_error: Option<SharedString>,
    graph_edit_issue: Option<JsonGraphEditIssue>,
    graph_edit_original: Option<Arc<str>>,
    graph_state_initialized: bool,
    graph_needs_fit: bool,
    graph_last_viewport: Option<(f32, f32)>,
    graph_pan_session: Option<(gpui::Point<gpui::Pixels>, f32, f32)>,
    graph_pending_center: Option<JsonGraphItemId>,
    graph_recenter_anchor: Option<(JsonGraphItemId, gpui::Point<gpui::Pixels>)>,
    graph_focus_handle: FocusHandle,
    json_split_ratio: f32,
    json_split_drag: Option<(f32, f32)>,
    json_split_focus_handle: FocusHandle,
    derived_projection_task: Task<()>,
    view_mode: DocumentHostViewMode,
    preview_lines: Vec<SharedString>,
    source_rows: BTreeMap<usize, Arc<BoundedLineWindow>>,
    displayed_screen_lines: Arc<ScreenLines>,
    metrics: PagedDocumentMetrics,
    /// 从 Host 构造到首个真实 Source 窗口绘制的耗时；仅在本地诊断显式开启时分配。
    first_render_started: Option<Instant>,
    source_row_blocks: BTreeMap<usize, Entity<Block>>,
    source_row_epochs: BTreeMap<usize, u64>,
    source_cache_epoch: u64,
    soak_ready_published: bool,
    source_pending: Option<Range<usize>>,
    source_queued_visible: Option<Range<usize>>,
    source_last_visible: Option<Range<usize>>,
    source_list_origin: usize,
    source_cancel_in_flight: bool,
    source_row_height: f32,
    active_edit: Option<SourceLineEdit>,
    suppressed_line_edit_text: Option<String>,
    selection_anchor: Option<usize>,
    selected_lines: Option<Range<usize>>,
    source_drag_anchor: Option<SourceAnchor>,
    source_drag_autoscroll_direction: i8,
    source_drag_autoscroll_task: Task<()>,
    source_context_menu: Option<gpui::Point<gpui::Pixels>>,
    source_context_menu_focus_handle: FocusHandle,
    search_input: Entity<Block>,
    search_visible: bool,
    navigation_input: Entity<Block>,
    navigation_visible: bool,
    navigation_is_byte: bool,
    show_line_endings: bool,
    search_options: SearchOptions,
    search_results: Vec<SearchMatch>,
    search_selected: usize,
    search_running: bool,
    search_error: Option<SharedString>,
    mode_notice: Option<SharedString>,
    tail_enabled: bool,
    /// 仅在 session 尚未安装或保存任务暂时移出时存在；Ready 状态的 dirty 真值属于 session。
    pending_dirty: Option<bool>,
    saving: bool,
    reloading: bool,
    error: Option<SharedString>,
    coordinator: DocumentCoordinator,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    structured_scroll_handle: UniformListScrollHandle,
    structured_horizontal_scroll_handle: ScrollHandle,
    source_window_start: u64,
    provisional_anchor: Option<SourceAnchor>,
    /// 关闭标签仍会保留实体用于“重新打开关闭的标签”；挂起期间所有后台任务必须停止，
    /// 重新激活后再从当前不可变文档状态恢复，不允许关闭的标签改写剪贴板或缓存。
    closed_suspended: bool,
    structured_task: Task<()>,
    structured_progress_task: Task<()>,
    structured_filter_task: Task<()>,
    json_expand_task: Task<()>,
    clipboard_generation: u64,
    clipboard_cancellation: Option<SearchCancellation>,
    clipboard_task: Task<()>,
    selection_export_generation: u64,
    selection_export_cancellation: Option<SearchCancellation>,
    selection_export_task: Task<()>,
}

impl gpui::EventEmitter<DocumentHostEvent> for DocumentHost {}

impl DocumentHost {}

#[path = "document_runtime/host_controller.rs"]
mod controller;
#[path = "document_runtime/host_editing.rs"]
mod editing;
#[path = "document_views/navigation.rs"]
mod navigation;
#[path = "document_runtime/host_recovery.rs"]
mod recovery;
#[path = "document_views/view_mode.rs"]
mod view_mode;

fn search_document_reader(
    document: Option<&DocumentSession>,
    provisional_source: Option<&FileSource>,
    query: &str,
    options: SearchOptions,
    cancellation: &SearchCancellation,
) -> Result<Vec<SearchMatch>, gmark_paged_document::PagedDocumentError> {
    if let Some(document) = document {
        document.search(query, options, cancellation)
    } else if let Some(source) = provisional_source {
        search_file_source(source, query, options, cancellation)
    } else {
        Ok(Vec::new())
    }
}

fn build_structured_index(
    source: &FileSource,
    lines: &LineIndex,
    format: DocumentFormat,
    cancellation: &SearchCancellation,
    snapshot: Option<Arc<[u8]>>,
) -> Result<Option<StructuredIndex>, gmark_paged_document::PagedDocumentError> {
    match format {
        DocumentFormat::Delimited { delimiter } => {
            let options = DelimitedIndexOptions {
                delimiter,
                ..DelimitedIndexOptions::default()
            };
            let cache_dir = ProjectDirs::from("com", "kongweiguang", "gmark")
                .map(|dirs| dirs.cache_dir().join("large-document-indexes"));
            let index = if let Some(snapshot) = snapshot {
                DelimitedIndex::build_snapshot_cancellable(snapshot, options, cancellation)
            } else if let Some(cache_dir) = cache_dir {
                DelimitedIndex::build_cached_cancellable(source, options, cache_dir, cancellation)
            } else {
                DelimitedIndex::build_cancellable(source, options, cancellation)
            }?;
            Ok(Some(StructuredIndex::Delimited(index)))
        }
        DocumentFormat::Markdown => {
            MarkdownTableIndex::detect_all_cancellable(source, lines.clone(), cancellation).map(
                |tables| {
                    (!tables.is_empty()).then_some(StructuredIndex::MarkdownTables {
                        tables,
                        selected: 0,
                    })
                },
            )
        }
        DocumentFormat::Json => {
            let options = JsonIndexOptions::default();
            let cache_dir = ProjectDirs::from("com", "kongweiguang", "gmark")
                .map(|dirs| dirs.cache_dir().join("large-document-indexes"));
            let index = if let Some(cache_dir) = cache_dir {
                JsonIndex::build_cached_cancellable(source, options, cache_dir, cancellation)
            } else {
                JsonIndex::build_cancellable(source, options, cancellation)
            }?;
            Ok(Some(StructuredIndex::Json {
                index,
                source: source.clone(),
            }))
        }
        DocumentFormat::JsonLines => {
            let (lines, source, record_count) = if let Some(snapshot) = snapshot {
                let ranges = snapshot_line_ranges(&snapshot);
                validate_json_lines_snapshot(&snapshot, &ranges, cancellation)?;
                let lines = StructuredLines::Snapshot(ranges.into());
                let record_count = structured_json_lines_record_count(&lines);
                (
                    lines,
                    StructuredTextSource::Snapshot(snapshot),
                    record_count,
                )
            } else {
                validate_json_lines_cancellable(source, lines, cancellation)?;
                let lines = StructuredLines::File(lines.clone());
                let record_count = structured_json_lines_record_count(&lines);
                (
                    lines,
                    StructuredTextSource::File(source.clone()),
                    record_count,
                )
            };
            Ok(Some(StructuredIndex::JsonLines {
                lines,
                source,
                record_count,
            }))
        }
        DocumentFormat::PlainText => Ok(None),
    }
}

fn structured_json_lines_record_count(lines: &StructuredLines) -> u64 {
    lines
        .line_count()
        .checked_sub(1)
        .filter(|last| {
            lines
                .line_range(*last)
                .is_some_and(|range| range.start == range.end)
        })
        .unwrap_or_else(|| lines.line_count())
}

fn snapshot_line_ranges(bytes: &[u8]) -> Vec<Range<u64>> {
    let mut ranges = Vec::new();
    let mut start = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            ranges.push(start as u64..(index + 1) as u64);
            start = index + 1;
        }
    }
    ranges.push(start as u64..bytes.len() as u64);
    ranges
}

fn validate_json_lines_snapshot(
    bytes: &[u8],
    lines: &[Range<u64>],
    cancellation: &SearchCancellation,
) -> Result<(), PagedDocumentError> {
    for (line, range) in lines.iter().enumerate() {
        if line.is_multiple_of(1_024) && cancellation.is_cancelled() {
            return Err(PagedDocumentError::Cancelled);
        }
        let value = bytes[range.start as usize..range.end as usize]
            .strip_suffix(b"\n")
            .unwrap_or(&bytes[range.start as usize..range.end as usize]);
        let value = value.strip_suffix(b"\r").unwrap_or(value);
        if value.is_empty() && line + 1 == lines.len() {
            continue;
        }
        serde_json::from_slice::<serde_json::Value>(value).map_err(|error| {
            PagedDocumentError::InvalidJson {
                offset: range
                    .start
                    .saturating_add((error.column() as u64).saturating_sub(1)),
                message: error.to_string(),
            }
        })?;
    }
    Ok(())
}

fn read_json_cells(
    index: &JsonIndex,
    source: &FileSource,
    item: u64,
) -> Result<Vec<String>, gmark_paged_document::PagedDocumentError> {
    let Some((key, value)) = index.item_key_value_ranges(item)? else {
        return Ok(Vec::new());
    };
    let label = if let Some(key) = key {
        let complete = key.end.saturating_sub(key.start) <= STRUCTURED_CELL_BYTES as u64;
        let end = key.end.min(key.start + STRUCTURED_CELL_BYTES as u64);
        let bytes = source.read_range(key.start, end)?;
        if complete {
            serde_json::from_slice::<String>(&bytes)
                .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).into_owned())
        } else {
            let mut label = String::from_utf8_lossy(&bytes).into_owned();
            label.push('…');
            label
        }
    } else {
        item.to_string()
    };
    Ok(vec![label, read_json_preview(source, value)?])
}

fn read_json_preview(
    source: &FileSource,
    range: Range<u64>,
) -> Result<String, gmark_paged_document::PagedDocumentError> {
    let end = range.end.min(range.start + STRUCTURED_CELL_BYTES as u64);
    let bytes = source.read_range(range.start, end)?;
    let mut preview = String::from_utf8_lossy(&bytes).replace(['\r', '\n'], " ");
    if end < range.end {
        preview.push('…');
    }
    Ok(preview)
}

fn truncate_cell(mut value: String) -> String {
    if value.len() <= STRUCTURED_CELL_BYTES {
        return value;
    }
    let mut end = STRUCTURED_CELL_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value.push('…');
    value
}

/// 全文索引尚未完成时按估算行号映射到字节锚点。每一行最多两次 64 KiB 读取，
/// 因而首屏、滚动条拖动和关闭窗口都不依赖 O(file_size) 扫描。
fn read_provisional_source_rows(
    source: &FileSource,
    estimated_lines: u64,
    requested: Range<usize>,
    column_start: u64,
    encoding: &TextEncoding,
    cancellation: &SearchCancellation,
) -> Result<Vec<(usize, BoundedLineWindow)>, gmark_paged_document::PagedDocumentError> {
    let len = source.identity()?.len;
    if len == 0 {
        return Ok(vec![(
            requested.start,
            BoundedLineWindow::new(0..0, 0..0, String::new(), String::new(), false, false),
        )]);
    }
    requested
        .map(|logical_line| {
            if cancellation.is_cancelled() {
                return Err(PagedDocumentError::Cancelled);
            }
            let target = ((len as u128 * logical_line as u128) / estimated_lines.max(1) as u128)
                .min(len.saturating_sub(1) as u128) as u64;
            read_provisional_line_window(source, target, column_start, encoding)
                .map(|window| (logical_line, window))
        })
        .collect()
}

fn read_provisional_line_window(
    source: &FileSource,
    mut target: u64,
    column_start: u64,
    encoding: &TextEncoding,
) -> Result<BoundedLineWindow, gmark_paged_document::PagedDocumentError> {
    let len = source.identity()?.len;
    let utf16 = matches!(encoding, TextEncoding::Utf16Le | TextEncoding::Utf16Be);
    if utf16 {
        target -= target % 2;
    }
    let mut backward_start = target.saturating_sub(MAX_RENDERED_LINE_BYTES);
    if utf16 {
        backward_start -= backward_start % 2;
    }
    let backward = source.read_range(backward_start, target)?;
    let known_line_start = last_line_break_end(&backward, backward_start, encoding);
    let physical_start = known_line_start.unwrap_or(target);
    let aligned_column = if utf16 {
        column_start - column_start % 2
    } else {
        column_start
    };
    let mut start = physical_start.saturating_add(aligned_column).min(len);
    if start < len && matches!(encoding, TextEncoding::Utf8 { .. }) {
        let probe = source.read_range(start, (start + 4).min(len))?;
        start = start.saturating_add(
            probe
                .iter()
                .take_while(|byte| **byte & 0b1100_0000 == 0b1000_0000)
                .count() as u64,
        );
    }
    let read_end = start.saturating_add(MAX_RENDERED_LINE_BYTES).min(len);
    let mut bytes = source.read_range(start, read_end)?;
    let newline_end = first_line_break_end(&bytes, start, encoding);
    if let Some(newline_end) = newline_end {
        bytes.truncate(newline_end);
    }
    let source_end = start.saturating_add(bytes.len() as u64);
    let ending_len = encoded_line_ending_len(&bytes, encoding);
    let content_end = source_end.saturating_sub(ending_len as u64);
    let content_bytes = &bytes[..bytes.len().saturating_sub(ending_len)];
    let text = decode_provisional_bytes(content_bytes, encoding, start);
    let content_range = start..content_end;
    Ok(BoundedLineWindow::new(
        content_range,
        physical_start..source_end,
        text,
        decoded_line_ending(ending_len, utf16),
        known_line_start.is_none() && target > 0 || start > physical_start,
        newline_end.is_none() && source_end < len,
    ))
}

fn last_line_break_end(bytes: &[u8], absolute_start: u64, encoding: &TextEncoding) -> Option<u64> {
    match encoding {
        TextEncoding::Utf16Le => bytes
            .chunks_exact(2)
            .enumerate()
            .filter(|(_, pair)| u16::from_le_bytes([pair[0], pair[1]]) == b'\n' as u16)
            .map(|(index, _)| absolute_start + (index as u64 + 1) * 2)
            .next_back(),
        TextEncoding::Utf16Be => bytes
            .chunks_exact(2)
            .enumerate()
            .filter(|(_, pair)| u16::from_be_bytes([pair[0], pair[1]]) == b'\n' as u16)
            .map(|(index, _)| absolute_start + (index as u64 + 1) * 2)
            .next_back(),
        _ => bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map(|position| absolute_start + position as u64 + 1),
    }
}

fn first_line_break_end(
    bytes: &[u8],
    _absolute_start: u64,
    encoding: &TextEncoding,
) -> Option<usize> {
    match encoding {
        TextEncoding::Utf16Le => bytes
            .chunks_exact(2)
            .position(|pair| u16::from_le_bytes([pair[0], pair[1]]) == b'\n' as u16)
            .map(|index| (index + 1) * 2),
        TextEncoding::Utf16Be => bytes
            .chunks_exact(2)
            .position(|pair| u16::from_be_bytes([pair[0], pair[1]]) == b'\n' as u16)
            .map(|index| (index + 1) * 2),
        _ => bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1),
    }
}

fn encoded_line_ending_len(bytes: &[u8], encoding: &TextEncoding) -> usize {
    match encoding {
        TextEncoding::Utf16Le if bytes.ends_with(&[b'\r', 0, b'\n', 0]) => 4,
        TextEncoding::Utf16Be if bytes.ends_with(&[0, b'\r', 0, b'\n']) => 4,
        TextEncoding::Utf16Le if bytes.ends_with(&[b'\n', 0]) => 2,
        TextEncoding::Utf16Be if bytes.ends_with(&[0, b'\n']) => 2,
        _ if bytes.ends_with(b"\r\n") => 2,
        _ if bytes.ends_with(b"\n") || bytes.ends_with(b"\r") => 1,
        _ => 0,
    }
}

fn decoded_line_ending(ending_len: usize, utf16: bool) -> String {
    match (ending_len, utf16) {
        (4, true) | (2, false) => "\r\n".to_owned(),
        (2, true) | (1, false) => "\n".to_owned(),
        _ => String::new(),
    }
}

fn decode_provisional_bytes(bytes: &[u8], encoding: &TextEncoding, absolute_start: u64) -> String {
    match encoding {
        TextEncoding::Utf8 { bom } => {
            let bytes = if *bom && absolute_start == 0 {
                bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes)
            } else {
                bytes
            };
            String::from_utf8_lossy(bytes).into_owned()
        }
        TextEncoding::Utf16Le | TextEncoding::Utf16Be => {
            let bytes = if absolute_start == 0 {
                bytes
                    .strip_prefix(&[0xff, 0xfe])
                    .or_else(|| bytes.strip_prefix(&[0xfe, 0xff]))
                    .unwrap_or(bytes)
            } else {
                bytes
            };
            let units = bytes.chunks_exact(2).map(|pair| match encoding {
                TextEncoding::Utf16Le => u16::from_le_bytes([pair[0], pair[1]]),
                TextEncoding::Utf16Be => u16::from_be_bytes([pair[0], pair[1]]),
                _ => unreachable!(),
            });
            String::from_utf16_lossy(&units.collect::<Vec<_>>())
        }
        TextEncoding::Legacy(label) => encoding_rs::Encoding::for_label(label.as_bytes())
            .map(|encoding| encoding.decode(bytes).0.into_owned())
            .unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned()),
    }
}

fn read_bounded_line_window(
    document: &DocumentSession,
    line: u64,
    requested_start: u64,
) -> Result<Option<BoundedLineWindow>, gmark_paged_document::PagedDocumentError> {
    let Some(line_range) = document.line_range(line) else {
        return Ok(None);
    };
    let tail_start = line_range.end.saturating_sub(2).max(line_range.start);
    let tail = document.read_range(tail_start..line_range.end)?;
    let ending_len = if tail.ends_with(b"\r\n") {
        2u64
    } else if tail.ends_with(b"\n") || tail.ends_with(b"\r") {
        1
    } else {
        0
    };
    let content_end = line_range.end.saturating_sub(ending_len);
    let content_len = content_end.saturating_sub(line_range.start);
    let relative_start = requested_start.min(content_len.saturating_sub(MAX_RENDERED_LINE_BYTES));
    let mut start = line_range.start.saturating_add(relative_start);
    if start > line_range.start && start < content_end {
        // 横向窗口可能落在多字节码点内部；最多向前跳过三个 continuation byte。
        let probe_end = (start + 4).min(content_end);
        let probe = document.read_range(start..probe_end)?;
        let skipped = probe
            .iter()
            .take_while(|byte| **byte & 0b1100_0000 == 0b1000_0000)
            .count() as u64;
        start = start.saturating_add(skipped);
    }
    let requested_end = (start + MAX_RENDERED_LINE_BYTES).min(content_end);
    let mut bytes = document.read_range(start..requested_end)?;
    let mut end = requested_end;
    if let Err(error) = std::str::from_utf8(&bytes)
        && error.error_len().is_none()
    {
        bytes.truncate(error.valid_up_to());
        end = start.saturating_add(bytes.len() as u64);
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let ending = if end == content_end && ending_len > 0 {
        String::from_utf8_lossy(&tail[tail.len() - ending_len as usize..]).into_owned()
    } else {
        String::new()
    };
    let replace_end = if end == content_end {
        line_range.end
    } else {
        end
    };
    Ok(Some(BoundedLineWindow::new(
        start..end,
        start..replace_end,
        text,
        ending,
        start > line_range.start,
        end < content_end,
    )))
}

fn rendered_line_ending(ending: &str) -> &'static str {
    match ending {
        "\r\n" => "␍␊",
        "\n" => "␊",
        "\r" => "␍",
        _ => "",
    }
}

fn text_encoding_label(encoding: &TextEncoding) -> String {
    match encoding {
        TextEncoding::Utf8 { bom: false } => "UTF-8".to_owned(),
        TextEncoding::Utf8 { bom: true } => "UTF-8 BOM".to_owned(),
        TextEncoding::Utf16Le => "UTF-16 LE".to_owned(),
        TextEncoding::Utf16Be => "UTF-16 BE".to_owned(),
        TextEncoding::Legacy(label) => label.to_uppercase(),
    }
}

fn rendered_line_window_text(window: &BoundedLineWindow, show_line_endings: bool) -> String {
    let mut text = String::with_capacity(window.text.len().saturating_add(6));
    if window.leading_truncated {
        text.push_str("… ");
    }
    text.push_str(&window.text);
    if window.trailing_truncated {
        text.push_str(" …");
    } else if show_line_endings {
        text.push_str(rendered_line_ending(&window.ending));
    }
    text
}

fn shift_source_window_start(current: u64, delta: i64, maximum: u64) -> u64 {
    if delta >= 0 {
        current.saturating_add(delta as u64).min(maximum)
    } else {
        current.saturating_sub(delta.unsigned_abs())
    }
}

fn source_window_start_for_anchor(line_len: u64, relative_byte: u64) -> u64 {
    relative_byte
        .min(line_len)
        .saturating_sub(MAX_RENDERED_LINE_BYTES / 4)
        .min(line_len.saturating_sub(MAX_RENDERED_LINE_BYTES))
}

fn source_window_start_from_pointer(
    pointer_x: gpui::Pixels,
    track_left: gpui::Pixels,
    track_width: f32,
    thumb_width: f32,
    maximum: u64,
) -> u64 {
    let travel = (track_width - thumb_width).max(0.0);
    if travel <= 0.0 || maximum == 0 {
        return 0;
    }
    let thumb_left = (f32::from(pointer_x - track_left) - thumb_width * 0.5).clamp(0.0, travel);
    ((thumb_left / travel) as f64 * maximum as f64).round() as u64
}

fn source_list_origin_for_target(total: usize, target: usize) -> usize {
    if total <= SOURCE_LIST_WINDOW_ROWS {
        return 0;
    }
    target
        .saturating_sub(SOURCE_LIST_WINDOW_ROWS / 2)
        .min(total - SOURCE_LIST_WINDOW_ROWS)
}

fn source_line_from_scrollbar_pointer(
    pointer_y: gpui::Pixels,
    track_top: gpui::Pixels,
    track_height: f32,
    thumb_height: f32,
    max_top_line: usize,
) -> usize {
    let travel = (track_height - thumb_height).max(0.0);
    let thumb_top = (f32::from(pointer_y - track_top) - thumb_height * 0.5).clamp(0.0, travel);
    let progress = if travel > 0.0 {
        thumb_top / travel
    } else {
        0.0
    };
    (progress as f64 * max_top_line as f64).round() as usize
}

fn document_view_state_mut<'a>(
    document: &'a mut Option<DocumentSession>,
    pending: &'a mut Option<DocumentViewState>,
) -> &'a mut DocumentViewState {
    if let Some(document) = document.as_mut() {
        &mut document.view_state
    } else {
        pending
            .as_mut()
            .expect("DocumentHost without a session must own pending view state")
    }
}

fn document_dirty_state(document: &Option<DocumentSession>, pending: &Option<bool>) -> bool {
    document
        .as_ref()
        .map(|document| document.dirty)
        .or(*pending)
        .unwrap_or(false)
}

fn set_document_dirty_state(
    document: &mut Option<DocumentSession>,
    pending: &mut Option<bool>,
    dirty: bool,
) {
    if let Some(document) = document.as_mut() {
        document.dirty = dirty;
    } else {
        *pending = Some(dirty);
    }
}

impl DocumentHost {
    fn sync_session_active_view(&mut self) {
        let active_view = if self.view_mode == DocumentHostViewMode::Source {
            DocumentViewId::source()
        } else {
            self.selected_projection_view
                .clone()
                .unwrap_or_else(DocumentViewId::source)
        };
        if let Some(document) = self.document.as_mut() {
            let _ = document.set_active_view(active_view);
        } else if let Some(pending) = self.pending_view_state.as_mut() {
            pending.active_view = Some(active_view);
        }
    }

    /// 后台保存需要独占并移走 session；视图状态先转交 pending 槽，避免后台副本
    /// 与仍在渲染的 Host 同时成为权威状态。
    fn take_document_session(&mut self) -> Option<DocumentSession> {
        let mut document = self.document.take()?;
        debug_assert!(self.pending_view_state.is_none());
        debug_assert!(self.pending_dirty.is_none());
        self.pending_view_state = Some(std::mem::take(&mut document.view_state));
        self.pending_dirty = Some(std::mem::replace(&mut document.dirty, false));
        Some(document)
    }

    /// 安装 session 时原子接回 pending 状态。初次打开保留 OpenPlan 的 active view；
    /// 恢复/保存期间已有 active view 仅在新计划仍允许时提交。
    fn install_document_session(&mut self, mut document: DocumentSession) {
        if let Some(dirty) = self.pending_dirty.take() {
            document.dirty = dirty;
        }
        let Some(mut pending) = self.pending_view_state.take() else {
            // 同步编辑生成的 next session 已携带当前 view_state，可直接替换旧 session。
            self.document = Some(document);
            return;
        };
        if let Some(active_view) = pending.active_view.clone() {
            if document.set_active_view(active_view).is_err() {
                pending.active_view = Some(document.active_view.clone());
            }
        } else {
            pending.active_view = Some(document.active_view.clone());
        }
        document.view_state = pending;
        self.document = Some(document);
    }
}

impl Drop for DocumentHost {
    fn drop(&mut self) {
        self.coordinator.cancel_all();
        if let Some(cancellation) = self.structured_cancellation.take() {
            cancellation.cancel();
        }
        if let Some(cancellation) = self.structured_filter_cancellation.take() {
            cancellation.cancel();
        }
        if let Some(cancellation) = self.json_expand_cancellation.take() {
            cancellation.cancel();
        }
        if let Some(cancellation) = self.clipboard_cancellation.take() {
            cancellation.cancel();
        }
        if let Some(cancellation) = self.selection_export_cancellation.take() {
            cancellation.cancel();
        }
        if let Some(cancellation) = self.derived_projection_cancellation.take() {
            cancellation.cancel();
        }
        // 未编辑的预建日志只有身份帧，不应在下次启动伪装成恢复文档。
        if !document_dirty_state(&self.document, &self.pending_dirty)
            && let Some(journal) = self.coordinator.recovery_journal.take()
        {
            let _ = journal.checkpoint();
        }
    }
}

impl Focusable for DocumentHost {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[path = "document_views/json_graph.rs"]
mod json_graph;
#[path = "document_views/render.rs"]
mod render;
#[path = "document_views/source_surface.rs"]
mod source_surface;
#[path = "document_views/structured_view.rs"]
mod structured_view;

#[cfg(test)]
#[path = "../tests/unit/document_host.rs"]
mod bounded_line_tests;
