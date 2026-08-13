// @author kongweiguang

use super::*;

/// Builds a valid transaction layout so result/log tests exercise the same
/// path validation that protects production helper persistence.
fn persistence_plan(root: &Path) -> ApplyPlanV2 {
    let transaction_id = uuid::Uuid::new_v4();
    let transaction = root
        .join("v1.0.1")
        .join("transactions")
        .join(transaction_id.to_string());
    fs::create_dir_all(&transaction).expect("transaction directory");
    ApplyPlanV2 {
        schema_version: ApplyPlanV2::SCHEMA_VERSION,
        parent_pid: 0,
        current_version: "1.0.0".to_owned(),
        target_version: "1.0.1".to_owned(),
        artifact_path: transaction.join("artifact.ready"),
        artifact_url: "https://example.test/gmark-1.0.1.AppImage".to_owned(),
        artifact_size: 1,
        artifact_sha256: "00".repeat(32),
        artifact_format: "linux-app-image".to_owned(),
        signed_envelope_path: transaction.join("manifest.envelope.json"),
        expected_install_root: root.join("gmark.AppImage"),
        target_path: root.join("gmark.AppImage"),
        backup_path: root.join(format!(
            "gmark.AppImage.gmark-update-backup-{transaction_id}"
        )),
        relaunch_path: root.join("gmark.AppImage"),
        acknowledgement_path: transaction.join("startup-ack"),
        cancellation_path: transaction.join("cancel-install"),
        result_path: transaction.join("result.json"),
        helper_log_path: transaction.join("helper.log"),
        transaction_id,
        lifetime_lock_path: transaction.join("lifetime.lock"),
        progress_path: transaction.join("progress.json"),
        installer_log_path: transaction.join("installer.log"),
        feedback_mode: gmark_update_core::ApplyFeedbackModeV1::ProgressFile,
    }
}

#[test]
fn post_install_failure_explains_manual_recovery_without_rollback() {
    // The user-facing failure must make the committed-new-version policy
    // actionable instead of suggesting a replay of the terminal transaction.
    let root = tempfile::tempdir().expect("temporary update root");
    let plan = persistence_plan(root.path());
    let failure = post_install_failure(
        &plan,
        ApplyFailureCode::RelaunchFailed,
        "updated process did not acknowledge",
    );
    assert_eq!(failure.code, ApplyFailureCode::RelaunchFailed);
    assert_eq!(failure.recovery_action, RecoveryAction::Manual);
    assert!(
        failure
            .message
            .contains("new version 1.0.1 is installed and will not be rolled back automatically")
    );
    assert!(failure.message.contains("manual action:"));
    assert!(failure.message.contains("open helper log:"));
    assert!(failure.message.contains("open installer log:"));

    let mut progress = ProgressWriter::new(&plan);
    progress
        .publish(ApplyPhaseV1::WaitingForExit, "waiting")
        .expect("initial progress");
    persist_failure(&plan, &mut progress, failure.clone());
    let log = fs::read_to_string(&plan.helper_log_path).expect("helper log");
    assert!(log.contains("will not be rolled back automatically"));
    let result = read_apply_result_v2(&plan.result_path).expect("failed result");
    assert_eq!(result.recovery_action, Some(RecoveryAction::Manual));
    assert!(result.message.contains("manual action:"));
}

#[test]
fn reporting_a_failure_does_not_replay_an_existing_terminal_result() {
    // A terminal result is authoritative even when a late helper error is
    // reported again by the process entry point.
    let root = tempfile::tempdir().expect("temporary update root");
    let plan = persistence_plan(root.path());
    let terminal = ApplyResultV2::succeeded(
        plan.transaction_id,
        plan.current_version.clone(),
        plan.target_version.clone(),
    );
    write_apply_result_for_plan(&plan, &terminal).expect("terminal result");

    let failure = V2Failure::new(
        ApplyFailureCode::RelaunchFailed,
        RecoveryAction::Manual,
        "late startup acknowledgement",
    );
    report_v2_failure(&plan, &failure);

    let persisted = read_apply_result_v2(&plan.result_path).expect("existing terminal result");
    assert_eq!(persisted.status, "succeeded");
    assert_eq!(persisted.transaction_id, plan.transaction_id);
}

#[test]
fn execution_claim_makes_a_transaction_single_use_without_a_result() {
    // 结果盘故障不能成为重跑安装器的旁路，因此 durable claim 必须独立于 result.json 生效。
    let root = tempfile::tempdir().expect("temporary update root");
    let plan = persistence_plan(root.path());

    let claim = claim_install_attempt(&plan).expect("first execution claim");
    assert_eq!(
        fs::read_to_string(claim).expect("claim contents").trim(),
        plan.transaction_id.to_string()
    );
    let error = claim_install_attempt(&plan).expect_err("second claim must fail closed");
    assert!(error.contains("already claimed"));
    assert!(!plan.result_path.exists());
}

#[test]
fn committed_platform_failure_is_manual_and_never_retryable() {
    // 提交后的同步/校验错误必须保留新版并引导人工处理，不能让客户端重新执行同一安装。
    let root = tempfile::tempdir().expect("temporary update root");
    let plan = persistence_plan(root.path());
    let platform = PlatformInstallFailure::committed_or_unknown("post-commit validation failed");
    let failure = post_install_failure(
        &plan,
        classify_install_failure(&platform.message),
        platform.message,
    );

    assert_eq!(
        platform.commit_state,
        InstallCommitState::CommittedOrUnknown
    );
    assert_eq!(failure.recovery_action, RecoveryAction::Manual);
    assert!(
        failure
            .message
            .contains("will not be rolled back automatically")
    );
}
