// @author kongweiguang

use super::*;
use gmark_document_core::{DocumentFormat, DocumentProfile, LoadingPolicy, TextEncoding};
use gmark_document_runtime::{
    DocumentController, DocumentEvent, DocumentId, DocumentStore, FileIdentity, ResidentDocument,
};
use gmark_paged_document::FileIdentity as SourceIdentity;
use std::path::PathBuf;

fn session(text: &str) -> DocumentSession {
    let source_identity = SourceIdentity {
        path: PathBuf::from("shared-document-test.txt"),
        len: text.len() as u64,
        modified_nanos: None,
        os_file_id: None,
    };
    let profile = DocumentProfile {
        len: text.len() as u64,
        format: DocumentFormat::PlainText,
        encoding: TextEncoding::Utf8 { bom: false },
        estimated_lines: 1,
        estimated_structural_units: 0,
    };
    let plan = LoadingPolicy::default().resolve(&profile);
    match DocumentSession::new(
        profile.clone(),
        DocumentStore::Resident(Box::new(ResidentDocument::new(
            text,
            profile.encoding.clone(),
            source_identity.clone(),
        ))),
        plan,
        FileIdentity::from(&source_identity),
    ) {
        Ok(session) => session,
        Err(error) => panic!("shared document fixture must be valid: {error}"),
    }
}

fn host(text: &str) -> SharedDocument {
    let controller = DocumentController::new(DocumentId::new(), session(text));
    match SharedDocument::from_controller(controller) {
        Ok(document) => document,
        Err(error) => panic!("shared document fixture must register: {error}"),
    }
}

#[test]
fn hosts_sharing_a_handle_share_edits_undo_and_dirty_state() {
    let first = host("one");
    let handle = first.handle();
    let second = match SharedDocument::from_handle(
        handle.clone(),
        handle.lease(),
        DocumentViewInstanceId::new(),
    ) {
        Ok(document) => document,
        Err(error) => panic!("second shared host must register: {error}"),
    };

    match first.replace_range(0..0, "x") {
        Ok(_) => {}
        Err(error) => panic!("shared edit must succeed: {error}"),
    }
    match second.read_range(0..4) {
        Ok(bytes) => assert_eq!(bytes, b"xone".to_vec()),
        Err(error) => panic!("shared body must be readable: {error}"),
    }
    assert!(first.dirty());
    assert!(second.dirty());

    match second.undo_changed() {
        Ok(changed) => assert!(changed),
        Err(error) => panic!("shared undo must succeed: {error}"),
    }
    match first.read_range(0..3) {
        Ok(bytes) => assert_eq!(bytes, b"one".to_vec()),
        Err(error) => panic!("shared body must be readable after undo: {error}"),
    }
    assert!(!first.dirty());
    assert!(!second.dirty());
}

#[test]
fn stale_save_completion_cannot_clear_newer_revision_dirty_state() {
    let document = host("one");
    match document.replace_range(0..0, "x") {
        Ok(_) => {}
        Err(error) => panic!("first edit must succeed: {error}"),
    }
    let snapshot = match document.request_save_snapshot() {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => panic!("first save request must start"),
        Err(error) => panic!("save request must succeed: {error}"),
    };
    match document.replace_range(1..1, "y") {
        Ok(_) => {}
        Err(error) => panic!("second edit must succeed: {error}"),
    }
    match document.save_succeeded(snapshot.revision, snapshot.identity.clone()) {
        Ok(()) => {}
        Err(error) => panic!("stale save completion must be accepted: {error}"),
    }
    assert!(document.dirty());
    assert_eq!(document.revision(), snapshot.revision.0 + 1);
}

#[test]
fn discard_clears_dirty_without_changing_body_revision_or_undo_history() {
    let document = host("one");
    match document.replace_range(0..0, "x") {
        Ok(_) => {}
        Err(error) => panic!("discard fixture edit must succeed: {error}"),
    }
    let revision = document.revision();
    let body = match document.read_range(0..4) {
        Ok(body) => body,
        Err(error) => panic!("discard fixture body must be readable: {error}"),
    };
    assert!(document.dirty());

    match document.discard_current_changes() {
        Ok(changed) => assert!(changed),
        Err(error) => panic!("final lease may discard changes: {error}"),
    }
    assert!(!document.dirty());
    assert_eq!(document.revision(), revision);
    assert_eq!(document.read_range(0..4).unwrap_or_default(), body);

    match document.undo_changed() {
        Ok(changed) => assert!(changed),
        Err(error) => panic!("discard must retain undo history: {error}"),
    }
    assert_eq!(document.read_range(0..3).unwrap_or_default(), b"one");
}

#[test]
fn discard_is_rejected_while_another_view_holds_a_lease() {
    let first = host("one");
    let handle = first.handle();
    let second = match SharedDocument::from_handle(
        handle.clone(),
        handle.lease(),
        DocumentViewInstanceId::new(),
    ) {
        Ok(document) => document,
        Err(error) => panic!("second shared host must register: {error}"),
    };
    match first.replace_range(0..0, "x") {
        Ok(_) => {}
        Err(error) => panic!("shared discard fixture edit must succeed: {error}"),
    }
    assert!(matches!(
        first.discard_current_changes(),
        Err(ControllerError::SharedDocumentStillLeased)
    ));
    assert!(second.dirty());
}

#[test]
fn persisted_view_id_rejects_duplicate_and_nil_identities() {
    let first = host("one");
    let handle = first.handle();
    let persisted = DocumentViewInstanceId::new();
    let restored =
        match SharedDocument::from_handle_with_view_id(handle.clone(), handle.lease(), persisted) {
            Ok(document) => document,
            Err(error) => panic!("persisted view id must register once: {error}"),
        };
    assert_eq!(restored.view_id(), persisted);
    let duplicate =
        SharedDocument::from_handle_with_view_id(handle.clone(), handle.lease(), persisted);
    assert!(matches!(duplicate, Err(ControllerError::Mutation(_))));

    let nil = DocumentViewInstanceId::from_uuid(uuid::Uuid::nil());
    let nil_result = SharedDocument::from_handle_with_view_id(handle.clone(), handle.lease(), nil);
    assert!(matches!(nil_result, Err(ControllerError::Mutation(_))));
    assert_eq!(handle.lease_count(), 2);
}

#[test]
fn closing_one_host_keeps_shared_document_alive_for_another_host() {
    let first = host("one");
    let handle = first.handle();
    let second = match SharedDocument::from_handle(
        handle.clone(),
        handle.lease(),
        DocumentViewInstanceId::new(),
    ) {
        Ok(document) => document,
        Err(error) => panic!("second shared host must register: {error}"),
    };
    drop(first);

    match second.read_range(0..3) {
        Ok(bytes) => assert_eq!(bytes, b"one".to_vec()),
        Err(error) => panic!("remaining host must retain body: {error}"),
    }
    assert_eq!(second.lease_count(), 1);
}

#[test]
fn inactive_view_does_not_reappear_from_a_stale_worker_clone() {
    let document = host("one");
    let worker = document.clone();
    let view_id = document.view_id();
    match document.close_view() {
        Ok(()) => {}
        Err(error) => panic!("view close must succeed: {error}"),
    }
    match worker.set_source_selection(SourceSelection::collapsed(2, Default::default())) {
        Ok(()) => {}
        Err(error) => panic!("stale selection update must be ignored: {error}"),
    }
    let handle = document.handle();
    let controller = match handle.lock() {
        Ok(controller) => controller,
        Err(error) => panic!("controller lock must succeed: {error}"),
    };
    assert!(controller.view_selection(view_id).is_none());
    drop(controller);
    assert!(!document.is_view_registered());
    match document.register_view() {
        Ok(()) => {}
        Err(error) => panic!("view reactivation must succeed: {error}"),
    }
    assert!(document.is_view_registered());
}

#[test]
fn controller_event_subscription_observes_shared_body_revision() {
    let document = host("one");
    let (_, mut subscription) = match document.handle().subscribe_with_snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("controller subscription must start: {error}"),
    };
    match document.replace_range(0..0, "x") {
        Ok(_) => {}
        Err(error) => panic!("shared edit must succeed: {error}"),
    }
    let events = match subscription.poll() {
        Ok(events) => events,
        Err(error) => panic!("controller events must be readable: {error}"),
    };
    assert!(
        events
            .iter()
            .any(|event| matches!(event, DocumentEvent::RevisionChanged { .. }))
    );
}
