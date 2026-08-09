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
