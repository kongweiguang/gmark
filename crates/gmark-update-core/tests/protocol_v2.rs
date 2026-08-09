// @author kongweiguang

use std::{fs, path::Path};

use gmark_update_core::{
    ApplyFailureCode, ApplyFeedbackModeV1, ApplyPhaseV1, ApplyPlanV1, ApplyPlanV2, ApplyProgressV1,
    ApplyResultV2, Platform, RecoveryAction, UpdateCoreError, parse_apply_progress,
    parse_apply_result_v2, read_apply_plan_v2, read_apply_progress, read_apply_result_v2,
    validate_apply_plan_v2, validate_apply_plan_v2_at_path, validate_apply_progress,
    validate_apply_result_v2, write_apply_plan_v2, write_apply_progress, write_apply_result_v2,
};
use uuid::Uuid;

const ARTIFACT_URL: &str =
    "https://github.com/kongweiguang/gmark/releases/download/v0.2.0/gmark.AppImage";

fn v1_plan(root: &Path) -> ApplyPlanV1 {
    let transaction = root.join("v0.2.0");
    let target = root.join("gmark.AppImage");
    ApplyPlanV1 {
        schema_version: ApplyPlanV1::SCHEMA_VERSION,
        parent_pid: 42,
        current_version: "0.1.0".to_owned(),
        target_version: "0.2.0".to_owned(),
        artifact_path: transaction.join("artifact.ready"),
        artifact_url: ARTIFACT_URL.to_owned(),
        artifact_size: 9,
        artifact_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_owned(),
        artifact_format: "linux-app-image".to_owned(),
        signed_envelope_path: transaction.join("manifest.envelope.json"),
        target_path: target.clone(),
        backup_path: root.join("gmark.AppImage.gmark-update-backup"),
        relaunch_path: target,
        acknowledgement_path: transaction.join("startup-ack"),
        cancellation_path: transaction.join("cancel-install"),
        result_path: root.join("last-result.json"),
        helper_log_path: root.join("last-helper.log"),
    }
}

fn v2_plan(root: &Path) -> ApplyPlanV2 {
    ApplyPlanV2::from_v1(&v1_plan(root), Uuid::from_u128(1))
}

#[test]
fn v2_plan_round_trip_uses_fixed_transaction_files() {
    let root = tempfile::tempdir().unwrap();
    let plan = v2_plan(root.path());
    let path = plan
        .transaction_dir()
        .unwrap()
        .join(ApplyPlanV2::PLAN_FILE_NAME);
    write_apply_plan_v2(&path, &plan).unwrap();
    assert_eq!(read_apply_plan_v2(&path).unwrap(), plan);
    validate_apply_plan_v2(&plan, &Platform::new("linux", "x86_64")).unwrap();
    validate_apply_plan_v2_at_path(&plan, &path, &Platform::new("linux", "x86_64")).unwrap();

    let json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(json["schema_version"], 2);
    assert_eq!(json["feedback_mode"], "progress_file");
    let transaction_dir = root
        .path()
        .join("v0.2.0")
        .join(ApplyPlanV2::TRANSACTIONS_DIR_NAME)
        .join(Uuid::from_u128(1).hyphenated().to_string());
    assert_eq!(plan.progress_path, transaction_dir.join("progress.json"));
    assert_eq!(
        plan.expected_install_root,
        root.path().join("gmark.AppImage")
    );
    assert!(
        plan.backup_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&Uuid::from_u128(1).hyphenated().to_string())
    );
}

#[test]
fn repeated_attempts_of_the_same_version_have_isolated_roots() {
    let root = tempfile::tempdir().unwrap();
    let first = ApplyPlanV2::from_v1(&v1_plan(root.path()), Uuid::from_u128(1));
    let second = ApplyPlanV2::from_v1(&v1_plan(root.path()), Uuid::from_u128(2));
    assert_ne!(first.transaction_dir(), second.transaction_dir());
    assert_ne!(first.backup_path, second.backup_path);
    validate_apply_plan_v2(&first, &Platform::new("linux", "x86_64")).unwrap();
    validate_apply_plan_v2(&second, &Platform::new("linux", "x86_64")).unwrap();
}

#[test]
fn v2_plan_rejects_unknown_fields_and_path_escape() {
    let root = tempfile::tempdir().unwrap();
    let plan = v2_plan(root.path());
    let mut json = serde_json::to_value(&plan).unwrap();
    json["unknown"] = true.into();
    assert!(serde_json::from_value::<ApplyPlanV2>(json).is_err());

    let mut escaped = plan.clone();
    escaped.progress_path = root.path().join("v0.2.0/../progress.json");
    assert!(matches!(
        validate_apply_plan_v2(&escaped, &Platform::new("linux", "x86_64")),
        Err(UpdateCoreError::Protocol(_))
    ));
}

#[test]
fn progress_and_result_are_bounded_and_strict() {
    let root = tempfile::tempdir().unwrap();
    let plan = v2_plan(root.path());
    let progress =
        ApplyProgressV1::new(plan.transaction_id, ApplyPhaseV1::Installing).with_message("copying");
    validate_apply_progress(&progress).unwrap();
    write_apply_progress(&plan.progress_path, &progress).unwrap();
    assert_eq!(read_apply_progress(&plan.progress_path).unwrap(), progress);

    let result = ApplyResultV2::failed(
        plan.transaction_id,
        "0.1.0",
        "0.2.0",
        ApplyFailureCode::InstallerFailed,
        RecoveryAction::ReattemptInstall,
        "installer exited with code 1",
    );
    validate_apply_result_v2(&result).unwrap();
    write_apply_result_v2(&plan.result_path, &result).unwrap();
    assert_eq!(read_apply_result_v2(&plan.result_path).unwrap(), result);

    let mut unknown = serde_json::to_value(&progress).unwrap();
    unknown["extra"] = true.into();
    assert!(parse_apply_progress(&serde_json::to_vec(&unknown).unwrap()).is_err());
    let mut oversized = serde_json::to_vec(&progress).unwrap();
    oversized.extend(std::iter::repeat_n(b'x', 64 * 1024));
    assert!(parse_apply_progress(&oversized).is_err());
    assert!(parse_apply_result_v2(b"{} ").is_err());
}

#[test]
fn feedback_and_phase_values_are_serializable() {
    let feedback = serde_json::to_string(&ApplyFeedbackModeV1::Agent).unwrap();
    assert_eq!(feedback, "\"agent\"");
    let phase = serde_json::to_string(&ApplyPhaseV1::WaitingForExit).unwrap();
    assert_eq!(phase, "\"waiting_for_exit\"");
}

#[cfg(unix)]
#[test]
fn v2_validator_rejects_a_symlinked_transaction_directory() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let transactions = root.path().join("v0.2.0/transactions");
    fs::create_dir_all(&transactions).unwrap();
    symlink(
        outside.path(),
        transactions.join(Uuid::from_u128(1).hyphenated().to_string()),
    )
    .unwrap();
    let plan = v2_plan(root.path());
    assert!(validate_apply_plan_v2(&plan, &Platform::new("linux", "x86_64")).is_err());
}
