// @author kongweiguang

use super::*;

#[test]
fn progress_sequence_accepts_only_monotonic_lifecycle_phases() {
    assert!(legal_transition(
        ApplyPhaseV1::WaitingForExit,
        ApplyPhaseV1::Preparing
    ));
    assert!(legal_transition(
        ApplyPhaseV1::Confirming,
        ApplyPhaseV1::RollingBack
    ));
    assert!(legal_transition(
        ApplyPhaseV1::RollingBack,
        ApplyPhaseV1::Failed
    ));
    assert!(!legal_transition(
        ApplyPhaseV1::Succeeded,
        ApplyPhaseV1::Preparing
    ));
    assert!(!legal_transition(
        ApplyPhaseV1::Installing,
        ApplyPhaseV1::WaitingForExit
    ));
}

#[test]
fn lifecycle_lock_is_exclusive_and_releases_on_drop() {
    let root = tempfile::tempdir().expect("temporary lock root");
    let path = root.path().join(ApplyPlanV2::LIFETIME_LOCK_FILE_NAME);
    let first = acquire_lifetime_lock(&path, Duration::from_millis(100), Duration::from_millis(1))
        .expect("first helper obtains lock");
    let deadline = Instant::now() + Duration::from_millis(20);
    assert!(matches!(
        acquire_lifetime_lock_until(&path, deadline, Duration::from_millis(1)),
        Err(LockError::Timeout)
    ));
    drop(first);
    let second = acquire_lifetime_lock(&path, Duration::from_millis(100), Duration::from_millis(1));
    assert!(second.is_ok());
}

#[test]
fn cancellation_marker_is_idempotent_by_presence() {
    let root = tempfile::tempdir().expect("temporary cancellation root");
    let path = root.path().join("cancel-install");
    assert!(!cancellation_requested(&path).expect("missing marker is not cancellation"));
    fs::write(&path, b"cancelled\n").expect("write marker");
    assert!(cancellation_requested(&path).expect("marker requests cancellation"));
    assert!(cancellation_requested(&path).expect("repeated cancellation remains true"));
}
