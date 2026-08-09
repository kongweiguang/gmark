// @author kongweiguang

use super::*;
use gmark_update_core::ApplyFeedbackModeV1;
use uuid::Uuid;

fn plan(root: &Path, target: &Path, backup: &Path, transaction_id: Uuid) -> ApplyPlanV2 {
    let transaction = root
        .join("v1.0.1/transactions")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&transaction).expect("transaction directory");
    ApplyPlanV2 {
        schema_version: ApplyPlanV2::SCHEMA_VERSION,
        parent_pid: 0,
        current_version: "1.0.0".to_owned(),
        target_version: "1.0.1".to_owned(),
        artifact_path: transaction.join("artifact.ready"),
        artifact_url: "https://github.com/kongweiguang/gmark/releases/download/v1.0.1/a".to_owned(),
        artifact_size: 1,
        artifact_sha256: "00".repeat(32),
        artifact_format: "linux-app-image".to_owned(),
        signed_envelope_path: transaction.join("manifest.envelope.json"),
        expected_install_root: target.to_owned(),
        target_path: target.to_owned(),
        backup_path: backup.to_owned(),
        relaunch_path: target.to_owned(),
        acknowledgement_path: transaction.join("startup-ack"),
        cancellation_path: transaction.join("cancel-install"),
        result_path: transaction.join("result.json"),
        helper_log_path: transaction.join("helper.log"),
        transaction_id,
        lifetime_lock_path: transaction.join("lifetime.lock"),
        progress_path: transaction.join("progress.json"),
        installer_log_path: transaction.join("installer.log"),
        feedback_mode: ApplyFeedbackModeV1::ProgressFile,
    }
}

#[test]
fn existing_uuid_backup_is_never_overwritten() {
    let root = tempfile::tempdir().expect("temporary root");
    let target = root.path().join("gmark.AppImage");
    let transaction_id = Uuid::new_v4();
    let backup = root.path().join(format!(
        "gmark.AppImage.gmark-update-backup-{transaction_id}"
    ));
    fs::write(&target, b"current").expect("target");
    fs::write(&backup, b"protected").expect("existing backup");
    let transaction = plan(root.path(), &target, &backup, transaction_id);
    let error = create_backup(&target, &backup, root.path(), 0o755, &transaction)
        .expect_err("existing backup must reject install");
    assert!(error.contains("already exists"));
    assert_eq!(fs::read(&backup).expect("backup bytes"), b"protected");
}

#[test]
fn rollback_atomically_replaces_target_and_retains_backup() {
    let root = tempfile::tempdir().expect("temporary root");
    let target = root.path().join("gmark.AppImage");
    let transaction_id = Uuid::new_v4();
    let backup = root.path().join(format!(
        "gmark.AppImage.gmark-update-backup-{transaction_id}"
    ));
    fs::write(&target, b"new").expect("target");
    fs::write(&backup, b"old").expect("backup");
    let transaction = plan(root.path(), &target, &backup, transaction_id);
    rollback(&transaction).expect("rollback");
    assert_eq!(fs::read(&target).expect("restored target"), b"old");
    assert_eq!(fs::read(&backup).expect("retained backup"), b"old");
}
