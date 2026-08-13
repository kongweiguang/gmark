// @author kongweiguang

//! Runtime document references and detached-host presentation conversion.

use std::cell::RefCell;
use std::fmt;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gmark_document_runtime::{DocumentHandle, DocumentId, DocumentLease, DocumentViewInstanceId};
use gmark_paged_document::OpenProbe;

use crate::document_host::{
    DetachedDocumentHostView, DocumentHostViewMode, DocumentHostViewPresentation,
    ViewPresentationState,
};
use crate::editor::panes::{PaneReadOnlyKind, PaneViewStateSnapshot};

/// Kind of runtime-backed content held by one pane tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneDocumentKind {
    Markdown,
    DocumentHost,
    Image,
    Error,
}

/// Convert the host's complete detached presentation DTO into the versioned
/// workspace-session shape shared by every pane tab.  The conversion copies
/// only view metadata; document text, entities, leases, and workers stay in
/// their owning runtime objects.
pub(crate) fn host_presentation_to_pane_view_state(
    presentation: &DocumentHostViewPresentation,
) -> PaneViewStateSnapshot {
    let current = host_presentation_state(&presentation.current);
    let mut back = presentation
        .back_history
        .iter()
        .map(host_history_value)
        .collect::<Vec<_>>();
    let mut forward = presentation
        .forward_history
        .iter()
        .map(host_history_value)
        .collect::<Vec<_>>();
    back.truncate(32);
    forward.truncate(32);
    PaneViewStateSnapshot {
        selection: current.selection,
        scroll_x: current.scroll_x,
        scroll_y: current.scroll_y,
        view_mode: current.view_mode,
        split_ratio: current.split_ratio,
        markdown_fold: current.markdown_fold,
        markdown_folds: current.markdown_folds,
        table_layout: current.table_layout,
        forward,
        back,
    }
}

fn host_presentation_state(state: &ViewPresentationState) -> PaneViewStateSnapshot {
    let selection =
        crate::config::workspace_session::WorkspaceSessionSelection::from_source_selection(
            state.tab_view_state.source.selection,
        );
    let markdown_folds = state
        .source_collapsed_folds
        .iter()
        .map(|line| serde_json::json!({ "line": line, "collapsed": true }))
        .collect::<Vec<_>>();
    let table_layout = serde_json::json!({
        "filter_query": state.structured_filter_query,
        "filter_column": state.structured_filter_column,
        "hidden_columns": state.hidden_structured_columns,
        "column_window_start": state.structured_column_window_start,
        "selected_cell": state.structured_selected_cell,
    });
    PaneViewStateSnapshot {
        selection: Some(selection),
        scroll_x: Some(state.structured_scroll_x),
        scroll_y: Some(state.source_scroll_y),
        view_mode: Some(host_view_mode(state.view_mode).to_owned()),
        split_ratio: Some(state.json_split_ratio.clamp(0.1, 0.9)),
        markdown_fold: markdown_folds.first().cloned(),
        markdown_folds,
        table_layout: Some(table_layout),
        forward: Vec::new(),
        back: Vec::new(),
    }
}

fn host_history_value(state: &ViewPresentationState) -> serde_json::Value {
    let snapshot = host_presentation_state(state);
    serde_json::json!({
        "selection": snapshot.selection,
        "scroll_x": snapshot.scroll_x,
        "scroll_y": snapshot.scroll_y,
        "view_mode": snapshot.view_mode,
        "split_ratio": snapshot.split_ratio,
        "markdown_folds": snapshot.markdown_folds,
        "table_layout": snapshot.table_layout,
    })
}

fn host_view_mode(mode: DocumentHostViewMode) -> &'static str {
    match mode {
        DocumentHostViewMode::Live => "live",
        DocumentHostViewMode::Source => "source",
        DocumentHostViewMode::Structure => "structure",
        DocumentHostViewMode::Split => "split",
    }
}

/// Runtime reference held by one pane tab.
///
/// Markdown tabs share an [`Arc<DocumentLease>`] with their active child
/// canvas.  Source-backed hosts use a linear [`DetachedDocumentHostView`]
/// token while inactive; activation moves that token into a host Entity and
/// deactivation moves it back.  No tab state owns an Entity or worker.
#[derive(Clone)]
pub struct PaneDocumentRef {
    document_id: DocumentId,
    lease: Option<Arc<DocumentLease>>,
    view_id: DocumentViewInstanceId,
    title: String,
    path: Option<PathBuf>,
    kind: PaneDocumentKind,
    view_state: Rc<RefCell<PaneViewStateSnapshot>>,
    readonly: Option<PaneReadOnlyKind>,
    host: Option<Rc<RefCell<Option<DetachedDocumentHostView>>>>,
    host_path: Option<PathBuf>,
    host_probe: Option<OpenProbe>,
}

impl fmt::Debug for PaneDocumentRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PaneDocumentRef")
            .field("document_id", &self.document_id)
            .field("view_id", &self.view_id)
            .field("title", &self.title)
            .field("kind", &self.kind)
            .finish()
    }
}

impl PartialEq for PaneDocumentRef {
    fn eq(&self, other: &Self) -> bool {
        self.document_id == other.document_id && self.view_id == other.view_id
    }
}

impl Eq for PaneDocumentRef {}

impl PaneDocumentRef {
    pub fn new(
        document_id: DocumentId,
        lease: DocumentLease,
        view_id: DocumentViewInstanceId,
    ) -> Self {
        Self {
            document_id,
            lease: Some(Arc::new(lease)),
            view_id,
            title: String::new(),
            path: None,
            kind: PaneDocumentKind::Markdown,
            view_state: Rc::new(RefCell::new(PaneViewStateSnapshot::default())),
            readonly: None,
            host: None,
            host_path: None,
            host_probe: None,
        }
    }

    pub fn with_title(
        document_id: DocumentId,
        lease: DocumentLease,
        view_id: DocumentViewInstanceId,
        title: impl Into<String>,
    ) -> Self {
        Self {
            document_id,
            lease: Some(Arc::new(lease)),
            view_id,
            title: title.into(),
            path: None,
            kind: PaneDocumentKind::Markdown,
            view_state: Rc::new(RefCell::new(PaneViewStateSnapshot::default())),
            readonly: None,
            host: None,
            host_path: None,
            host_probe: None,
        }
    }

    /// Build a pane reference from a view-owned lease token. Cloning the Arc
    /// shares one lease; it does not increment the runtime registry count.
    pub fn from_lease_arc(
        document_id: DocumentId,
        lease: Arc<DocumentLease>,
        view_id: DocumentViewInstanceId,
    ) -> Self {
        Self {
            document_id,
            lease: Some(lease),
            view_id,
            title: String::new(),
            path: None,
            kind: PaneDocumentKind::Markdown,
            view_state: Rc::new(RefCell::new(PaneViewStateSnapshot::default())),
            readonly: None,
            host: None,
            host_path: None,
            host_probe: None,
        }
    }

    pub fn from_lease_arc_with_title(
        document_id: DocumentId,
        lease: Arc<DocumentLease>,
        view_id: DocumentViewInstanceId,
        title: impl Into<String>,
    ) -> Self {
        Self {
            document_id,
            lease: Some(lease),
            view_id,
            title: title.into(),
            path: None,
            kind: PaneDocumentKind::Markdown,
            view_state: Rc::new(RefCell::new(PaneViewStateSnapshot::default())),
            readonly: None,
            host: None,
            host_path: None,
            host_probe: None,
        }
    }

    pub fn from_lease_arc_with_title_and_path(
        document_id: DocumentId,
        lease: Arc<DocumentLease>,
        view_id: DocumentViewInstanceId,
        title: impl Into<String>,
        path: Option<PathBuf>,
    ) -> Self {
        let mut document = Self::from_lease_arc_with_title(document_id, lease, view_id, title);
        document.path = path;
        document
    }

    /// Build a source-backed pane tab from a detached host view.  The
    /// detached state is wrapped in a shared cell solely so model clones can
    /// retain one linear token; it is consumed exactly once on activation.
    pub fn from_detached_host(
        document_id: DocumentId,
        detached: DetachedDocumentHostView,
        path: PathBuf,
        probe: OpenProbe,
        title: impl Into<String>,
    ) -> Self {
        let view_id = detached.view_id();
        let view_state = host_presentation_to_pane_view_state(&detached.presentation_snapshot());
        Self {
            document_id,
            lease: None,
            view_id,
            title: title.into(),
            path: Some(path.clone()),
            kind: PaneDocumentKind::DocumentHost,
            view_state: Rc::new(RefCell::new(view_state)),
            readonly: None,
            host: Some(Rc::new(RefCell::new(Some(detached)))),
            host_path: Some(path),
            host_probe: Some(probe),
        }
    }

    pub fn from_image(
        document_id: DocumentId,
        path: PathBuf,
        view_id: DocumentViewInstanceId,
        title: impl Into<String>,
    ) -> Self {
        Self {
            document_id,
            lease: None,
            view_id,
            title: title.into(),
            path: Some(path.clone()),
            kind: PaneDocumentKind::Image,
            view_state: Rc::new(RefCell::new(PaneViewStateSnapshot::default())),
            readonly: Some(PaneReadOnlyKind::Image { path }),
            host: None,
            host_path: None,
            host_probe: None,
        }
    }

    pub fn from_error(
        document_id: DocumentId,
        path: PathBuf,
        message: impl Into<String>,
        view_id: DocumentViewInstanceId,
        title: impl Into<String>,
    ) -> Self {
        Self {
            document_id,
            lease: None,
            view_id,
            title: title.into(),
            path: Some(path.clone()),
            kind: PaneDocumentKind::Error,
            view_state: Rc::new(RefCell::new(PaneViewStateSnapshot::default())),
            readonly: Some(PaneReadOnlyKind::Error {
                path,
                message: message.into(),
            }),
            host: None,
            host_path: None,
            host_probe: None,
        }
    }

    pub const fn document_id(&self) -> DocumentId {
        self.document_id
    }

    pub const fn view_id(&self) -> DocumentViewInstanceId {
        self.view_id
    }

    pub const fn kind(&self) -> PaneDocumentKind {
        self.kind
    }

    pub fn readonly_kind(&self) -> Option<&PaneReadOnlyKind> {
        self.readonly.as_ref()
    }

    /// Return the last pure presentation snapshot captured for this tab.
    /// Active entities are synchronized into this cell before they are
    /// detached; inactive tabs therefore retain all fields without an Entity.
    pub fn view_state_snapshot(&self) -> PaneViewStateSnapshot {
        self.view_state.borrow().clone()
    }

    /// Replace the canonical pure presentation snapshot after an active view
    /// has captured its current selection/scroll/fold/table/history state.
    pub fn set_view_state_snapshot(&self, snapshot: PaneViewStateSnapshot) {
        *self.view_state.borrow_mut() = snapshot;
    }

    pub fn lease(&self) -> Option<&DocumentLease> {
        self.lease.as_deref()
    }

    pub fn lease_arc(&self) -> Option<&Arc<DocumentLease>> {
        self.lease.as_ref()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn display_title(&self) -> String {
        if !self.title.trim().is_empty() {
            return self.title.clone();
        }
        match self.kind {
            PaneDocumentKind::Markdown => "Untitled.md".to_owned(),
            PaneDocumentKind::DocumentHost => "Untitled".to_owned(),
            PaneDocumentKind::Image => "Image".to_owned(),
            PaneDocumentKind::Error => "Document error".to_owned(),
        }
    }

    pub const fn icon(&self) -> &'static str {
        match self.kind {
            PaneDocumentKind::Markdown => "icon/workspace/markdown.svg",
            PaneDocumentKind::DocumentHost | PaneDocumentKind::Error => "icon/ui/file.svg",
            PaneDocumentKind::Image => "icon/ui/image.svg",
        }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn host_path(&self) -> Option<&PathBuf> {
        self.host_path.as_ref()
    }

    pub fn host_probe(&self) -> Option<&OpenProbe> {
        self.host_probe.as_ref()
    }

    pub fn take_detached_host(&self) -> Option<DetachedDocumentHostView> {
        self.host.as_ref()?.borrow_mut().take()
    }

    pub fn put_detached_host(&self, detached: DetachedDocumentHostView) -> bool {
        let Some(host) = self.host.as_ref() else {
            return false;
        };
        let mut slot = host.borrow_mut();
        if slot.is_some() {
            return false;
        }
        *slot = Some(detached);
        true
    }

    pub fn has_detached_host(&self) -> bool {
        self.host
            .as_ref()
            .is_some_and(|host| host.borrow().is_some())
    }

    pub fn host_handle(&self) -> Option<DocumentHandle> {
        self.host
            .as_ref()?
            .borrow()
            .as_ref()
            .map(DetachedDocumentHostView::handle)
    }

    pub fn host_lease_count(&self) -> Option<usize> {
        self.host
            .as_ref()?
            .borrow()
            .as_ref()
            .map(DetachedDocumentHostView::lease_count)
    }

    pub fn host_dirty(&self) -> Option<bool> {
        let handle = self.host_handle()?;
        handle
            .lock()
            .ok()
            .map(|controller| controller.session().dirty)
    }

    /// Report the shared document dirty state for pane-local tab chrome.
    ///
    /// Resident Markdown tabs keep their lease directly while document-host
    /// tabs retain a detached host handle. Both routes must render the same
    /// unsaved-state signal because the body is shared across pane views.
    pub fn is_dirty(&self) -> bool {
        if let Some(dirty) = self.host_dirty() {
            return dirty;
        }
        let Some(lease) = self.lease() else {
            return false;
        };
        let handle = lease.handle();
        handle
            .lock()
            .ok()
            .map(|controller| controller.session().dirty)
            .unwrap_or(false)
    }
}
