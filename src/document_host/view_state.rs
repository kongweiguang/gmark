// @author kongweiguang

//! Pure pane-facing state for a detached `DocumentHost` view.
//!
//! The state deliberately contains no GPUI entities, render rows, tasks, or
//! watcher handles. A detached view keeps exactly one registry lease and a
//! controller view id; reactivation registers that id again before any view
//! task is scheduled.

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

use gmark_document_core::{DocumentViewId, DocumentViewState};
use gmark_document_runtime::{
    ControllerError, DocumentHandle, DocumentLease, DocumentViewInstanceId,
};
use gmark_json_graph::JsonGraphItemId;

use super::contracts::DocumentHostViewMode;

pub(crate) const MAX_VIEW_HISTORY: usize = 32;

#[derive(Clone)]
pub(crate) struct ViewPresentationState {
    pub(crate) tab_view_state: DocumentViewState,
    pub(crate) search_query: String,
    pub(crate) structured_filter_query: String,
    pub(crate) structured_filter_column: Option<usize>,
    pub(crate) hidden_structured_columns: BTreeSet<usize>,
    pub(crate) structured_column_window_start: usize,
    pub(crate) structured_selected_cell: Option<(Option<u64>, usize)>,
    pub(crate) source_window_start: u64,
    pub(crate) source_collapsed_folds: BTreeSet<u64>,
    pub(crate) source_scroll_y: f32,
    pub(crate) structured_scroll_y: f32,
    pub(crate) structured_scroll_x: f32,
    pub(crate) view_mode: DocumentHostViewMode,
    pub(crate) json_split_ratio: f32,
    pub(crate) json_expanded_nodes: BTreeSet<Vec<u64>>,
    pub(crate) selected_projection_view: Option<DocumentViewId>,
    pub(crate) graph_selected_item: Option<JsonGraphItemId>,
    pub(crate) graph_search_collapsed_before: Option<Vec<Arc<str>>>,
}

impl Default for ViewPresentationState {
    fn default() -> Self {
        Self {
            tab_view_state: DocumentViewState::default(),
            search_query: String::new(),
            structured_filter_query: String::new(),
            structured_filter_column: None,
            hidden_structured_columns: BTreeSet::new(),
            structured_column_window_start: 0,
            structured_selected_cell: None,
            source_window_start: 0,
            source_collapsed_folds: BTreeSet::new(),
            source_scroll_y: 0.0,
            structured_scroll_y: 0.0,
            structured_scroll_x: 0.0,
            view_mode: DocumentHostViewMode::Source,
            json_split_ratio: 0.5,
            json_expanded_nodes: BTreeSet::new(),
            selected_projection_view: None,
            graph_selected_item: None,
            graph_search_collapsed_before: None,
        }
    }
}

/// Canonical pane-facing state.  It contains only cloneable presentation data
/// and bounded navigation history; no Controller body, Entity, task, or
/// watcher is retained here.
#[derive(Clone, Default)]
pub(crate) struct DocumentHostViewPresentation {
    pub(crate) current: ViewPresentationState,
    pub(crate) back_history: Vec<ViewPresentationState>,
    pub(crate) forward_history: Vec<ViewPresentationState>,
}

impl DocumentHostViewPresentation {
    pub(crate) fn bounded(
        current: ViewPresentationState,
        back_history: impl IntoIterator<Item = ViewPresentationState>,
        forward_history: impl IntoIterator<Item = ViewPresentationState>,
    ) -> Self {
        Self {
            current,
            back_history: bounded_history(back_history),
            forward_history: bounded_history(forward_history),
        }
    }
}

fn bounded_history(
    history: impl IntoIterator<Item = ViewPresentationState>,
) -> Vec<ViewPresentationState> {
    let mut history = VecDeque::from_iter(history);
    while history.len() > MAX_VIEW_HISTORY {
        history.pop_front();
    }
    history.into_iter().collect()
}

/// Detached pane state. The lease is intentionally not `Clone`; ownership of
/// the registry lifetime moves linearly from a live host into this value.
pub(crate) struct DocumentHostViewState {
    handle: DocumentHandle,
    lease: DocumentLease,
    view_id: DocumentViewInstanceId,
    presentation: ViewPresentationState,
    back_history: VecDeque<ViewPresentationState>,
    forward_history: VecDeque<ViewPresentationState>,
}

/// Public(crate) wrapper used by pane integration to retain an inactive view
/// without retaining its Entity or any host-local worker.
pub(crate) struct DetachedDocumentHostView(DocumentHostViewState);

impl DocumentHostViewState {
    pub(super) fn new(
        handle: DocumentHandle,
        lease: DocumentLease,
        view_id: DocumentViewInstanceId,
        presentation: ViewPresentationState,
    ) -> Self {
        Self {
            handle,
            lease,
            view_id,
            presentation,
            back_history: VecDeque::new(),
            forward_history: VecDeque::new(),
        }
    }

    pub(super) fn with_presentation(
        handle: DocumentHandle,
        lease: DocumentLease,
        view_id: DocumentViewInstanceId,
        presentation: DocumentHostViewPresentation,
    ) -> Self {
        Self {
            handle,
            lease,
            view_id,
            presentation: presentation.current,
            back_history: VecDeque::from_iter(presentation.back_history),
            forward_history: VecDeque::from_iter(presentation.forward_history),
        }
    }

    pub(crate) fn handle(&self) -> DocumentHandle {
        self.handle.clone()
    }

    pub(crate) fn view_id(&self) -> DocumentViewInstanceId {
        self.view_id
    }

    pub(crate) fn lease_count(&self) -> usize {
        self.handle.lease_count()
    }

    pub(crate) fn presentation(&self) -> &ViewPresentationState {
        &self.presentation
    }

    pub(super) fn presentation_mut(&mut self) -> &mut ViewPresentationState {
        &mut self.presentation
    }

    pub(super) fn back_len(&self) -> usize {
        self.back_history.len()
    }

    pub(super) fn forward_len(&self) -> usize {
        self.forward_history.len()
    }

    pub(crate) fn presentation_snapshot(&self) -> DocumentHostViewPresentation {
        DocumentHostViewPresentation::bounded(
            self.presentation.clone(),
            self.back_history.iter().cloned(),
            self.forward_history.iter().cloned(),
        )
    }

    pub(super) fn push_history(&mut self, previous: ViewPresentationState) {
        self.back_history.push_back(previous);
        while self.back_history.len() > MAX_VIEW_HISTORY {
            self.back_history.pop_front();
        }
        self.forward_history.clear();
    }

    pub(super) fn go_back(&mut self) -> bool {
        let Some(previous) = self.back_history.pop_back() else {
            return false;
        };
        let current = self.presentation.clone();
        self.forward_history.push_back(current);
        while self.forward_history.len() > MAX_VIEW_HISTORY {
            self.forward_history.pop_front();
        }
        self.presentation = previous;
        true
    }

    pub(super) fn go_forward(&mut self) -> bool {
        let Some(next) = self.forward_history.pop_back() else {
            return false;
        };
        let current = self.presentation.clone();
        self.back_history.push_back(current);
        while self.back_history.len() > MAX_VIEW_HISTORY {
            self.back_history.pop_front();
        }
        self.presentation = next;
        true
    }

    pub(super) fn into_parts(
        self,
    ) -> (
        DocumentHandle,
        DocumentLease,
        DocumentViewInstanceId,
        DocumentHostViewPresentation,
    ) {
        let presentation = self.presentation_snapshot();
        (self.handle, self.lease, self.view_id, presentation)
    }
}

impl DetachedDocumentHostView {
    pub(super) fn from_state(state: DocumentHostViewState) -> Self {
        Self(state)
    }

    /// Build an inactive view directly from a service-owned handle.  This
    /// path is deliberately pure: it validates the persisted identity and
    /// moves the linear lease, but does not register a Controller view or
    /// create any GPUI Entity/task.  Registration happens only in `activate`.
    pub(crate) fn from_shared_with_view_id(
        handle: DocumentHandle,
        lease: DocumentLease,
        view_id: DocumentViewInstanceId,
        presentation: DocumentHostViewPresentation,
    ) -> Result<Self, ControllerError> {
        if view_id.uuid().is_nil() {
            return Err(ControllerError::Mutation(
                "persisted document view id must not be nil".into(),
            ));
        }
        Ok(Self(DocumentHostViewState::with_presentation(
            handle,
            lease,
            view_id,
            presentation,
        )))
    }

    pub(crate) fn state(&self) -> &DocumentHostViewState {
        &self.0
    }

    pub(crate) fn state_mut(&mut self) -> &mut DocumentHostViewState {
        &mut self.0
    }

    pub(crate) fn presentation_snapshot(&self) -> DocumentHostViewPresentation {
        self.0.presentation_snapshot()
    }

    pub(crate) fn handle(&self) -> DocumentHandle {
        self.0.handle()
    }

    pub(crate) fn view_id(&self) -> DocumentViewInstanceId {
        self.0.view_id()
    }

    pub(crate) fn lease_count(&self) -> usize {
        self.0.lease_count()
    }

    /// Create a second active pane view over the same Controller body.  The
    /// new lease is acquired explicitly and the fresh view id is registered;
    /// the original detached state remains the owner of its own lease.
    pub(crate) fn fork_view(&self) -> Result<Self, ControllerError> {
        let handle = self.0.handle.clone();
        let lease = handle.lease();
        let view_id = DocumentViewInstanceId::new();
        {
            let mut controller = handle.lock()?;
            controller.register_view(view_id);
            let len = controller.session().len();
            let mut selection = self.0.presentation.tab_view_state.source.selection;
            selection.anchor.byte_offset = selection.anchor.byte_offset.min(len);
            selection.head.byte_offset = selection.head.byte_offset.min(len);
            controller.set_view_selection(view_id, selection);
        }
        let mut state =
            DocumentHostViewState::new(handle, lease, view_id, self.0.presentation.clone());
        state.back_history = self.0.back_history.clone();
        state.forward_history = self.0.forward_history.clone();
        Ok(Self(state))
    }

    pub(super) fn activate(
        self,
    ) -> Result<
        (
            DocumentHandle,
            DocumentLease,
            DocumentViewInstanceId,
            DocumentHostViewPresentation,
        ),
        ControllerError,
    > {
        let (handle, lease, view_id, presentation) = self.0.into_parts();
        if view_id.uuid().is_nil() {
            return Err(ControllerError::Mutation(
                "persisted document view id must not be nil".into(),
            ));
        }
        {
            let mut controller = handle.lock()?;
            if controller.view_selection(view_id).is_some() {
                return Err(ControllerError::Mutation(format!(
                    "persisted document view id is already registered: {}",
                    view_id.uuid()
                )));
            }
            controller.register_view(view_id);
            let len = controller.session().len();
            let mut selection = presentation.current.tab_view_state.source.selection;
            selection.anchor.byte_offset = selection.anchor.byte_offset.min(len);
            selection.head.byte_offset = selection.head.byte_offset.min(len);
            controller.set_view_selection(view_id, selection);
        }
        Ok((handle, lease, view_id, presentation))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/document_host_view_state_private.rs"]
mod tests;
