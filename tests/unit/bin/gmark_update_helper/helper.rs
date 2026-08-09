// @author kongweiguang

use super::*;

#[test]
fn rollback_failure_is_structured_and_manual() {
    let failure = V2Failure::new(
        ApplyFailureCode::RelaunchFailed,
        RecoveryAction::ReattemptInstall,
        "updated process did not acknowledge",
    )
    .after_install()
    .rollback_failed("backup could not be restored");
    assert_eq!(failure.code, ApplyFailureCode::RollbackFailed);
    assert_eq!(failure.recovery_action, RecoveryAction::Manual);
    assert!(!failure.rollback_required);
    assert!(failure.message.contains("rollback failed"));
}
