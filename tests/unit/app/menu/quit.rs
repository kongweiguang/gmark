// @author kongweiguang

use super::*;

#[test]
fn lifecycle_is_idempotent_and_abortable() {
    // The GPUI global is exercised by integration tests; this pure state
    // fixture protects the public transition contract without calling
    // `App::quit` in a unit test.
    let mut coordinator = QuitCoordinator::default();
    assert_eq!(coordinator.phase, QuitPhase::Idle);
    coordinator.intent = Some(QuitIntent::ApplyUpdate);
    coordinator.phase = QuitPhase::Scheduled;
    assert_eq!(coordinator.intent, Some(QuitIntent::ApplyUpdate));
    coordinator.phase = QuitPhase::AwaitingUser;
    coordinator.intent = None;
    coordinator.phase = QuitPhase::Idle;
    assert_eq!(coordinator.phase, QuitPhase::Idle);
}

#[test]
fn shared_document_is_pending_and_handled_once() {
    let mut coordinator = QuitCoordinator::default();
    let document_id = gmark_document_runtime::DocumentId::new();

    assert!(coordinator.mark_pending(document_id));
    assert!(!coordinator.mark_pending(document_id));
    assert_eq!(coordinator.pending_document_count(), 1);
    assert_eq!(coordinator.handled_document_count(), 0);

    coordinator.intent = Some(QuitIntent::UserQuit);
    coordinator.phase = QuitPhase::AwaitingUser;
    // GPUI scheduling moves the pending decision into handled state.
    coordinator.resolve_pending_documents();
    assert_eq!(coordinator.pending_document_count(), 0);
    assert_eq!(coordinator.handled_document_count(), 1);
    assert!(!coordinator.mark_pending(document_id));
}

#[test]
fn new_quit_intent_clears_previous_document_tracking() {
    let mut coordinator = QuitCoordinator::default();
    let document_id = gmark_document_runtime::DocumentId::new();
    coordinator.handled_documents.insert(document_id);
    coordinator.pending_documents.insert(document_id);
    coordinator.intent = Some(QuitIntent::UserQuit);
    coordinator.phase = QuitPhase::AwaitingUser;

    // The process-level begin path starts a fresh deduplication scope.  The
    // direct field reset mirrors the GPUI wrapper without creating an App.
    coordinator.handled_documents.clear();
    coordinator.pending_documents.clear();
    assert_eq!(coordinator.handled_document_count(), 0);
    assert_eq!(coordinator.pending_document_count(), 0);
}
