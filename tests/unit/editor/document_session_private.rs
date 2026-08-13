// @author kongweiguang

use super::*;
use gmark_document_core::{SourceAffinity, SourceAnchor};

fn selection(offset: u64) -> SourceSelection {
    SourceSelection {
        anchor: SourceAnchor::new(offset, SourceAffinity::After),
        head: SourceAnchor::new(offset, SourceAffinity::After),
    }
}

#[test]
fn clone_shares_a_single_view_lease() {
    let document = EditorDocumentSession::new(SourceDocument::new("alpha"));
    let handle = document.handle().expect("shared handle");
    assert_eq!(handle.lease_count(), 1);
    let clone = document.clone();
    assert_eq!(clone.view_id(), document.view_id());
    assert_eq!(handle.lease_count(), 1);
    drop(clone);
    assert_eq!(handle.lease_count(), 1);
    drop(document);
    assert_eq!(handle.lease_count(), 0);
    assert!(handle.lock().is_ok());
}

#[test]
fn active_adapter_from_shared_lease_does_not_increment_registry_count() {
    let document = EditorDocumentSession::new(SourceDocument::new("alpha"));
    let handle = document.handle().expect("shared handle");
    let adapter_view = DocumentViewInstanceId::new();
    let adapter = EditorDocumentSession::from_lease_arc_with_view_id(
        document.lease_arc().expect("shared lease"),
        adapter_view,
    )
    .expect("shared active adapter");
    assert_ne!(adapter.view_id(), document.view_id());
    assert_eq!(handle.lease_count(), 1);
    drop(adapter);
    assert_eq!(handle.lease_count(), 1);
    drop(document);
    assert_eq!(handle.lease_count(), 0);
}

#[test]
fn fork_view_clones_lease_and_registers_a_distinct_view() {
    let document = EditorDocumentSession::new(SourceDocument::new("alpha"));
    let handle = document.handle().expect("shared handle");
    let fork = document.fork_view().expect("fork view");
    assert_ne!(document.view_id(), fork.view_id());
    assert_eq!(handle.lease_count(), 2);
    assert!(
        handle
            .lock()
            .expect("controller lock")
            .view_selection(document.view_id())
            .is_some()
    );
    drop(fork);
    assert_eq!(handle.lease_count(), 1);
    assert!(
        handle
            .lock()
            .expect("controller lock")
            .view_selection(document.view_id())
            .is_some()
    );
}

#[test]
fn persisted_view_id_is_reused_and_duplicate_is_rejected() {
    let document = EditorDocumentSession::new(SourceDocument::new("alpha"));
    let handle = document.handle().expect("shared handle");
    let persisted = DocumentViewInstanceId::new();
    let restored = EditorDocumentSession::from_handle_with_view_id(handle.clone(), persisted)
        .expect("persisted view id");
    assert_eq!(restored.view_id(), persisted);
    let duplicate = EditorDocumentSession::from_handle_with_view_id(handle, persisted);
    assert!(matches!(
        duplicate,
        Err(EditorDocumentSessionError::ViewAlreadyRegistered(view_id)) if view_id == persisted
    ));
    assert!(
        restored
            .handle()
            .expect("restored handle")
            .lock()
            .expect("controller lock")
            .view_selection(persisted)
            .is_some(),
        "a rejected duplicate must not unregister the existing view"
    );
}

#[test]
fn pane_arc_view_registers_without_an_extra_registry_lease() {
    let document = EditorDocumentSession::new(SourceDocument::new("alpha"));
    let handle = document.handle().expect("shared handle");
    let lease = document.lease_arc().expect("shared lease");
    let view_id = DocumentViewInstanceId::new();
    let pane = EditorDocumentSession::from_lease_arc_with_view_id(lease, view_id)
        .expect("pane view registration");
    assert_eq!(handle.lease_count(), 1);
    assert_eq!(pane.view_id(), view_id);
    assert!(
        handle
            .lock()
            .expect("controller lock")
            .view_selection(view_id)
            .is_some()
    );
    drop(pane);
    assert_eq!(handle.lease_count(), 1);
    assert!(
        handle
            .lock()
            .expect("controller lock")
            .view_selection(view_id)
            .is_none()
    );
    drop(document);
    assert_eq!(handle.lease_count(), 0);
}

#[test]
fn adapters_from_one_handle_share_edits_immediately() {
    let first = EditorDocumentSession::new(SourceDocument::new("alpha"));
    let second = EditorDocumentSession::from_handle(first.handle().expect("shared handle"))
        .expect("second view");
    let before = selection(0);
    let after = selection(1);
    first.sync_source_selection(before);
    first
        .apply_transaction_with_selection(
            Transaction::new(
                Revision::INITIAL,
                vec![gmark_document::TextEdit::new(0..0, "x")],
            ),
            before,
            after,
        )
        .expect("apply transaction");
    assert_eq!(second.text(), "xalpha");
    assert_eq!(second.revision(), Revision::from_u64(1));
}

#[test]
fn undo_and_redo_restore_the_origin_view_selection() {
    let document = EditorDocumentSession::new(SourceDocument::new("alpha"));
    let before = selection(0);
    let after = selection(1);
    document.sync_source_selection(before);
    document
        .apply_transaction_with_selection(
            Transaction::new(
                Revision::INITIAL,
                vec![gmark_document::TextEdit::new(0..0, "x")],
            ),
            before,
            after,
        )
        .expect("apply transaction");
    assert_eq!(document.source_selection(), after);
    document.undo().expect("undo").expect("undo snapshot");
    assert_eq!(document.source_selection(), before);
    document.redo().expect("redo").expect("redo snapshot");
    assert_eq!(document.source_selection(), after);
}

#[test]
fn dropping_a_non_last_view_keeps_controller_alive() {
    let document = EditorDocumentSession::new(SourceDocument::new("alpha"));
    let handle = document.handle().expect("shared handle");
    let clone = document.clone();
    drop(document);
    assert_eq!(handle.lease_count(), 1);
    assert_eq!(clone.text(), "alpha");
    drop(clone);
    assert_eq!(handle.lease_count(), 0);
    assert!(handle.lock().is_ok());
}

#[test]
fn forked_view_cursor_observes_shared_revision_event() {
    let first = EditorDocumentSession::new(SourceDocument::new("alpha"));
    let second = first.fork_view().expect("fork view");
    first
        .apply_transaction_with_selection(
            Transaction::new(
                Revision::INITIAL,
                vec![gmark_document::TextEdit::new(0..0, "x")],
            ),
            selection(0),
            selection(1),
        )
        .expect("apply transaction");
    let poll = second.poll_events().expect("poll shared events");
    assert!(poll.snapshot.is_none());
    assert!(poll.events.iter().any(|event| matches!(
        event,
        DocumentEvent::RevisionChanged { revision, .. }
            if revision.0 == 1
    )));
}

#[test]
fn event_readiness_is_quiet_until_a_shared_mutation() {
    let first = EditorDocumentSession::new(SourceDocument::new("alpha"));
    let second = first.fork_view().expect("fork view");
    assert!(!second.has_pending_events().expect("event readiness"));
    first
        .apply_transaction_with_selection(
            Transaction::new(
                Revision::INITIAL,
                vec![gmark_document::TextEdit::new(0..0, "x")],
            ),
            selection(0),
            selection(1),
        )
        .expect("apply transaction");
    assert!(second.has_pending_events().expect("event readiness"));
    let _ = second.poll_events().expect("poll shared events");
    assert!(!second.has_pending_events().expect("event readiness"));
}

#[test]
fn shell_has_no_controller_or_registry_lease() {
    let shell = EditorDocumentSession::shell();

    assert_eq!(shell.lease_count(), 0);
    assert!(shell.lease_arc().is_none());
    assert!(matches!(
        shell.handle(),
        Err(EditorDocumentSessionError::Shell)
    ));
    assert!(matches!(
        shell.document_id(),
        Err(EditorDocumentSessionError::Shell)
    ));
    assert!(matches!(
        shell.try_text(),
        Err(EditorDocumentSessionError::Shell)
    ));
    assert!(matches!(
        shell.try_snapshot(),
        Err(EditorDocumentSessionError::Shell)
    ));
    assert!(!shell.is_dirty());
    assert!(shell.text().is_empty());
}
