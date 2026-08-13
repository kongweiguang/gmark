// @author kongweiguang

use super::*;
use gmark_document_core::{DocumentFormat, DocumentProfile, LoadingPolicy, TextEncoding};
use gmark_document_runtime::{
    DocumentController, DocumentId, DocumentSession, DocumentStore, FileIdentity, ResidentDocument,
};
use gmark_paged_document::FileIdentity as SourceIdentity;
use std::path::PathBuf;

fn fixture_state() -> DocumentHostViewState {
    let source_identity = SourceIdentity {
        path: PathBuf::from("view-state-test.txt"),
        len: 3,
        modified_nanos: None,
        os_file_id: None,
    };
    let profile = DocumentProfile {
        len: 3,
        format: DocumentFormat::PlainText,
        encoding: TextEncoding::Utf8 { bom: false },
        estimated_lines: 1,
        estimated_structural_units: 0,
    };
    let session = DocumentSession::new(
        profile.clone(),
        DocumentStore::Resident(Box::new(ResidentDocument::new(
            "one",
            profile.encoding.clone(),
            source_identity.clone(),
        ))),
        LoadingPolicy::default().resolve(&profile),
        FileIdentity::from(&source_identity),
    )
    .unwrap_or_else(|error| panic!("view state fixture: {error}"));
    let handle = DocumentHandle::new(DocumentController::new(DocumentId::new(), session));
    let lease = handle.lease();
    DocumentHostViewState::new(
        handle,
        lease,
        DocumentViewInstanceId::new(),
        ViewPresentationState {
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
        },
    )
}

#[test]
fn history_is_bounded_and_forward_back_are_symmetric() {
    let mut state = fixture_state();
    for scroll in 0..(MAX_VIEW_HISTORY + 8) {
        let mut presentation = state.presentation().clone();
        presentation.source_scroll_y = scroll as f32;
        state.push_history(presentation);
    }
    assert_eq!(state.back_len(), MAX_VIEW_HISTORY);
    assert!(state.go_back());
    assert_eq!(state.forward_len(), 1);
    assert!(state.go_forward());
    assert_eq!(state.forward_len(), 0);
}

#[test]
fn persisted_presentation_round_trips_without_runtime_state() {
    let mut state = fixture_state();
    state.presentation_mut().search_query = "needle".into();
    state.presentation_mut().structured_filter_query = "active".into();
    state.presentation_mut().structured_filter_column = Some(2);
    state.presentation_mut().source_scroll_y = 48.0;
    state.presentation_mut().json_split_ratio = 0.62;
    let mut history = state.presentation().clone();
    history.source_window_start = 17;
    state.push_history(history);
    let (handle, lease, view_id, snapshot) = state.into_parts();
    let restored = DocumentHostViewState::with_presentation(handle, lease, view_id, snapshot);
    let restored_snapshot = restored.presentation_snapshot();

    assert_eq!(restored_snapshot.current.search_query, "needle");
    assert_eq!(restored_snapshot.current.structured_filter_query, "active");
    assert_eq!(restored_snapshot.current.structured_filter_column, Some(2));
    assert_eq!(restored_snapshot.current.source_scroll_y, 48.0);
    assert_eq!(restored_snapshot.current.json_split_ratio, 0.62);
    assert_eq!(restored_snapshot.back_history.len(), 1);
    assert_eq!(restored_snapshot.back_history[0].source_window_start, 17);
}

#[test]
fn fork_view_gets_a_new_lease_and_controller_view_id() {
    let state = fixture_state();
    let handle = state.handle();
    {
        let mut controller = handle
            .lock()
            .unwrap_or_else(|error| panic!("view fixture lock: {error}"));
        controller.register_view(state.view_id());
    }
    let detached = DetachedDocumentHostView::from_state(state);
    let fork = detached
        .fork_view()
        .unwrap_or_else(|error| panic!("detached view fork: {error}"));
    assert_eq!(detached.lease_count(), 2);
    assert_eq!(fork.lease_count(), 2);
    assert_ne!(detached.view_id(), fork.view_id());
    let controller = handle
        .lock()
        .unwrap_or_else(|error| panic!("fork controller lock: {error}"));
    assert!(controller.view_selection(detached.view_id()).is_some());
    assert!(controller.view_selection(fork.view_id()).is_some());
    assert_eq!(controller.session().len(), 3);
}

#[test]
fn activation_rejects_nil_or_duplicate_persisted_view_id() {
    let mut nil_state = fixture_state();
    nil_state.view_id = DocumentViewInstanceId::from_uuid(uuid::Uuid::nil());
    let nil = DetachedDocumentHostView::from_state(nil_state);
    assert!(matches!(nil.activate(), Err(ControllerError::Mutation(_))));

    let duplicate_state = fixture_state();
    let handle = duplicate_state.handle();
    let view_id = duplicate_state.view_id();
    handle
        .lock()
        .unwrap_or_else(|error| panic!("duplicate activation lock: {error}"))
        .register_view(view_id);
    let duplicate = DetachedDocumentHostView::from_state(duplicate_state);
    assert!(matches!(
        duplicate.activate(),
        Err(ControllerError::Mutation(_))
    ));
}

#[test]
fn pure_shared_detached_restore_has_no_controller_view_until_activation() {
    let state = fixture_state();
    let handle = state.handle();
    let lease = handle.lease();
    let view_id = DocumentViewInstanceId::new();
    let mut presentation = DocumentHostViewPresentation::default();
    presentation.current.tab_view_state.source.selection =
        gmark_document_core::SourceSelection::collapsed(2, Default::default());
    let detached = DetachedDocumentHostView::from_shared_with_view_id(
        handle.clone(),
        lease,
        view_id,
        presentation,
    )
    .unwrap_or_else(|error| panic!("pure detached restore: {error}"));
    assert_eq!(detached.lease_count(), 2);
    assert!(
        handle
            .lock()
            .unwrap_or_else(|error| panic!("inactive restore lock: {error}"))
            .view_selection(view_id)
            .is_none()
    );

    let (handle, lease, view_id, presentation) = detached
        .activate()
        .unwrap_or_else(|error| panic!("detached activation: {error}"));
    let mut controller = handle
        .lock()
        .unwrap_or_else(|error| panic!("active restore lock: {error}"));
    assert_eq!(
        controller.view_selection(view_id),
        Some(presentation.current.tab_view_state.source.selection)
    );
    controller.close_view(view_id);
    assert!(controller.view_selection(view_id).is_none());
    drop(controller);
    drop(lease);
}
