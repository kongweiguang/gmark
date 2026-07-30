// @author kongweiguang

use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer as _, SigningKey};
use gmark_update_core::{MAX_APPLY_PLAN_BYTES, write_apply_plan};
use serde_json::json;
use sha2::{Digest as _, Sha256};

fn fixture_plan(root: &Path) -> ApplyPlanV1 {
    let transaction = root.join("v0.2.0");
    fs::create_dir_all(&transaction).unwrap();
    let artifact = transaction.join("artifact.ready");
    let envelope = transaction.join("manifest.envelope.json");
    fs::write(&artifact, b"artifact").unwrap();
    fs::write(&envelope, b"manifest").unwrap();
    let target = if cfg!(target_os = "windows") {
        root.join("gmark.exe")
    } else if cfg!(target_os = "macos") {
        root.join("gmark.app")
    } else {
        root.join("gmark.AppImage")
    };
    if cfg!(target_os = "macos") {
        fs::create_dir_all(target.join("Contents/MacOS")).unwrap();
    } else {
        fs::write(&target, b"current").unwrap();
    }
    let target_name = target.file_name().unwrap().to_string_lossy();
    let backup = target.with_file_name(format!("{target_name}.gmark-update-backup"));
    let relaunch = if cfg!(target_os = "macos") {
        target.join("Contents/MacOS/gmark")
    } else {
        target.clone()
    };
    ApplyPlanV1 {
        schema_version: ApplyPlanV1::SCHEMA_VERSION,
        parent_pid: u32::MAX,
        current_version: "0.1.0".into(),
        target_version: "0.2.0".into(),
        artifact_path: artifact,
        artifact_url: "https://github.com/kongweiguang/gmark/releases/download/v0.2.0/a".into(),
        artifact_size: 8,
        artifact_sha256: "00".repeat(32),
        artifact_format: if cfg!(target_os = "windows") {
            "windows-setup-exe"
        } else if cfg!(target_os = "macos") {
            "macos-app-tar-gz"
        } else {
            "linux-app-image"
        }
        .into(),
        signed_envelope_path: envelope,
        target_path: target,
        backup_path: backup,
        relaunch_path: relaunch,
        acknowledgement_path: transaction.join("startup-ack"),
        cancellation_path: transaction.join("cancel-install"),
        result_path: root.join("last-result.json"),
        helper_log_path: root.join("last-helper.log"),
    }
}

#[test]
fn apply_plan_rejects_downgrades_and_missing_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let mut plan = fixture_plan(root.path());
    assert!(validate_plan(&plan).is_ok());
    plan.target_version = "0.0.9".into();
    assert!(validate_plan(&plan).is_err());
    plan.target_version = "0.2.0".into();
    fs::remove_file(&plan.artifact_path).unwrap();
    assert!(validate_plan(&plan).is_err());
}

#[test]
fn apply_plan_rejects_cross_platform_formats_and_unrelated_backups() {
    let root = tempfile::tempdir().unwrap();
    let mut plan = fixture_plan(root.path());
    plan.artifact_format = "not-this-platform".to_owned();
    assert!(validate_plan(&plan).is_err());
    plan = fixture_plan(root.path());
    plan.backup_path = root.path().join("unrelated-backup");
    assert!(validate_plan(&plan).is_err());
}

#[test]
fn cancellation_marker_prevents_any_install_side_effect() {
    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    let original_target = if plan.target_path.is_file() {
        Some(fs::read(&plan.target_path).unwrap())
    } else {
        None
    };
    fs::write(&plan.cancellation_path, b"cancelled").unwrap();
    assert!(
        wait_for_parent_or_cancel(&plan)
            .unwrap_err()
            .contains("cancelled")
    );
    assert!(plan.target_path.exists());
    assert!(!plan.backup_path.exists());
    if let Some(original_target) = original_target {
        assert_eq!(fs::read(&plan.target_path).unwrap(), original_target);
    }
}

#[test]
fn apply_result_atomically_replaces_a_previous_result() {
    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    write_result(&plan, "failed", "first".to_owned()).unwrap();
    write_result(&plan, "succeeded", "second".to_owned()).unwrap();
    let result: serde_json::Value =
        serde_json::from_slice(&fs::read(&plan.result_path).unwrap()).unwrap();
    assert_eq!(result["status"], "succeeded");
    assert_eq!(result["message"], "second");
}

#[test]
fn helper_log_is_local_bounded_history_for_the_latest_transaction() {
    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    reset_log(&plan);
    append_log(&plan, "verified artifact");
    append_log(&plan, "completed update");
    let log = fs::read_to_string(&plan.helper_log_path).unwrap();
    assert!(log.contains("verified artifact"));
    assert!(log.contains("completed update"));
}

#[test]
fn oversized_apply_plan_is_rejected_before_deserialization() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("apply-plan.json");
    fs::write(&path, vec![b' '; MAX_APPLY_PLAN_BYTES as usize + 1]).unwrap();
    assert!(read_plan(&path).unwrap_err().contains("size limit"));
}

#[test]
fn invalid_plan_never_touches_attacker_controlled_destinations() {
    let root = tempfile::tempdir().unwrap();
    let attacker = tempfile::tempdir().unwrap();
    let mut plan = fixture_plan(root.path());
    let plan_path = plan.artifact_path.parent().unwrap().join("apply-plan.json");
    let attacker_log = attacker.path().join("helper.log");
    let attacker_result = attacker.path().join("result.json");
    let attacker_relaunch = attacker.path().join("relaunch-target");
    fs::write(&attacker_relaunch, b"not an application").unwrap();
    plan.helper_log_path = attacker_log.clone();
    plan.result_path = attacker_result.clone();
    plan.relaunch_path = attacker_relaunch;
    write_apply_plan(&plan_path, &plan).unwrap();

    let args = vec![
        OsString::from("gmark-update-helper"),
        OsString::from("--apply-plan"),
        plan_path.clone().into_os_string(),
    ];
    assert_eq!(run_from_args(&args), ExitCode::FAILURE);
    assert!(!attacker_log.exists());
    assert!(!attacker_result.exists());
    assert!(plan_path.exists());
}

#[test]
fn helper_reverifies_manifest_signature_and_artifact_bytes() {
    let root = tempfile::tempdir().unwrap();
    let mut plan = fixture_plan(root.path());
    plan.artifact_sha256 = format!(
        "{:x}",
        Sha256::digest(fs::read(&plan.artifact_path).unwrap())
    );
    let signing_key = SigningKey::from_bytes(&[31; 32]);
    let system_trust = if cfg!(target_os = "linux") {
        "not-applicable"
    } else {
        "unsigned"
    };
    let payload = serde_json::to_vec(&json!({
        "schema_version": 2,
        "channel": "stable",
        "version": plan.target_version,
        "published_at": "2026-07-22T12:00:00Z",
        "notes": "fixture",
        "paused": false,
        "rollout_percent": 100,
        "release_url": "https://github.com/kongweiguang/gmark/releases/tag/v0.2.0",
        "artifacts": {
            "fixture": {
                "url": plan.artifact_url,
                "size": plan.artifact_size,
                "sha256": plan.artifact_sha256,
                "format": plan.artifact_format,
                "system_trust": system_trust
            }
        }
    }))
    .unwrap();
    let signature = signing_key.sign(&payload);
    fs::write(
        &plan.signed_envelope_path,
        serde_json::to_vec(&json!({
            "schema_version": 1,
            "algorithm": "Ed25519",
            "payload": BASE64.encode(&payload),
            "signature": BASE64.encode(signature.to_bytes())
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(verify_signed_artifact_with_key(&plan, &signing_key.verifying_key()).is_ok());
    fs::write(&plan.artifact_path, b"tampered").unwrap();
    assert!(verify_signed_artifact_with_key(&plan, &signing_key.verifying_key()).is_err());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn file_backup_cleanup_rejects_a_directory_without_removing_it() {
    let root = tempfile::tempdir().unwrap();
    let backup = root.path().join("gmark.gmark-update-backup");
    fs::create_dir_all(&backup).unwrap();
    fs::write(backup.join("must-not-be-recursively-removed"), b"keep").unwrap();

    assert!(clear_backup_path(&backup).is_err());
    assert!(backup.is_dir());
    assert!(backup.join("must-not-be-recursively-removed").is_file());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn rollback_restores_an_expected_regular_file_backup() {
    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    fs::write(&plan.target_path, b"new executable").unwrap();
    fs::write(&plan.backup_path, b"previous executable").unwrap();

    rollback(&plan).unwrap();

    assert_eq!(fs::read(&plan.target_path).unwrap(), b"previous executable");
    assert!(!plan.backup_path.exists());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn rollback_rejects_an_unexpected_directory_target() {
    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    fs::remove_file(&plan.target_path).unwrap();
    fs::create_dir(&plan.target_path).unwrap();
    fs::write(&plan.backup_path, b"previous executable").unwrap();

    assert!(rollback(&plan).is_err());
    assert!(plan.target_path.is_dir());
    assert_eq!(fs::read(&plan.backup_path).unwrap(), b"previous executable");
}
