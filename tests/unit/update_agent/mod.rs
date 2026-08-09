// @author kongweiguang

use std::{ffi::OsString, path::PathBuf};

use gmark_update_core::{ApplyPhaseV1, ApplyProgressV1};
use uuid::Uuid;

use super::{AgentState, ProgressRead, parse_args, read_progress, validate_progress_path};

fn progress(phase: ApplyPhaseV1, message: &str) -> ApplyProgressV1 {
    ApplyProgressV1::new(Uuid::new_v4(), phase).with_message(message)
}

#[test]
fn maps_every_protocol_phase_to_a_user_visible_state() {
    let phases = [
        (ApplyPhaseV1::WaitingForExit, "Waiting"),
        (ApplyPhaseV1::Preparing, "Preparing"),
        (ApplyPhaseV1::Installing, "Installing"),
        (ApplyPhaseV1::Relaunching, "Relaunching"),
        (ApplyPhaseV1::Confirming, "Confirming"),
        (ApplyPhaseV1::RollingBack, "Rolling back"),
        (ApplyPhaseV1::Succeeded, "Succeeded"),
        (ApplyPhaseV1::Failed, "Failed"),
    ];
    for (phase, label) in phases {
        assert_eq!(
            AgentState::from_progress(&progress(phase, "detail")).label(),
            label
        );
    }
}

#[test]
fn terminal_state_policy_only_auto_closes_success() {
    assert!(AgentState::from_progress(&progress(ApplyPhaseV1::Succeeded, "done")).is_success());
    assert!(!AgentState::from_progress(&progress(ApplyPhaseV1::Succeeded, "done")).is_failure());
    assert!(AgentState::from_progress(&progress(ApplyPhaseV1::Failed, "broken")).is_failure());
    assert!(!AgentState::from_progress(&progress(ApplyPhaseV1::Failed, "broken")).is_success());
}

#[test]
fn command_line_accepts_only_an_absolute_progress_path() {
    let transaction_id = Uuid::new_v4();
    let path = std::env::temp_dir()
        .join("gmark")
        .join("v0.2.0")
        .join(gmark_update_core::ApplyPlanV2::TRANSACTIONS_DIR_NAME)
        .join(transaction_id.hyphenated().to_string())
        .join("progress.json");
    let args = vec![OsString::from("--progress"), path.clone().into_os_string()];
    assert_eq!(parse_args(&args).unwrap().unwrap().progress_path, path);
    assert!(
        parse_args(&[
            OsString::from("--apply-plan"),
            path.clone().into_os_string()
        ])
        .is_err()
    );
    assert!(validate_progress_path(PathBuf::from("progress.json").as_path()).is_err());
    assert!(validate_progress_path(path.with_file_name("result.json").as_path()).is_err());
}

#[test]
fn strict_core_parser_rejects_unknown_progress_fields() {
    let root = tempfile::tempdir().unwrap();
    let transaction_id = Uuid::new_v4();
    let path = root
        .path()
        .join("v0.2.0")
        .join(gmark_update_core::ApplyPlanV2::TRANSACTIONS_DIR_NAME)
        .join(transaction_id.hyphenated().to_string())
        .join("progress.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        serde_json::json!({
            "schema_version": 1,
            "transaction_id": transaction_id,
            "phase": "failed",
            "message": "nope",
            "unexpected": true,
        })
        .to_string(),
    )
    .unwrap();
    assert!(matches!(read_progress(&path), ProgressRead::Invalid(_)));
}

#[test]
fn progress_transaction_must_match_its_directory() {
    let root = tempfile::tempdir().unwrap();
    let directory_id = Uuid::new_v4();
    let path = root
        .path()
        .join("v0.2.0")
        .join(gmark_update_core::ApplyPlanV2::TRANSACTIONS_DIR_NAME)
        .join(directory_id.hyphenated().to_string())
        .join("progress.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    gmark_update_core::write_apply_progress_v1(
        &path,
        &progress(ApplyPhaseV1::Installing, "installing"),
    )
    .unwrap();
    assert!(matches!(read_progress(&path), ProgressRead::Invalid(_)));
}
