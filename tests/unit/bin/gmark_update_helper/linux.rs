// @author kongweiguang

use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer as _, SigningKey};
use gmark_update_core::{
    ApplyFeedbackModeV1, ApplyPlanV2, Platform, StagedApplyArtifact,
    stage_and_verify_apply_plan_artifact_v2,
};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    os::unix::fs::{PermissionsExt as _, symlink},
    path::{Path, PathBuf},
};
use uuid::Uuid;

/// Returns the transaction-owned compatibility backup path required by V2.
///
/// Linux never touches this path, but a valid wire plan still has to preserve
/// its schema-level sibling and transaction ownership invariants.
fn compatible_backup_path(target: &Path, transaction_id: Uuid) -> PathBuf {
    let name = target
        .file_name()
        .expect("target file name")
        .to_string_lossy();
    target.with_file_name(format!("{name}.gmark-update-backup-{transaction_id}"))
}

/// Builds a complete V2 layout so adapter tests exercise the same guarded
/// staged-artifact boundary as the helper's production path.
fn plan(root: &Path, target: &Path, backup: &Path, transaction_id: Uuid) -> ApplyPlanV2 {
    let transaction = root
        .join("v1.0.1/transactions")
        .join(transaction_id.to_string());
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

/// Writes a signed manifest matching the fixture artifact's bytes and plan.
///
/// Using the real verification boundary keeps the replacement tests from
/// accidentally accepting an unverified or mutable source file.
fn prepare_verified_artifact(plan: &mut ApplyPlanV2, bytes: &[u8]) -> SigningKey {
    fs::write(&plan.artifact_path, bytes).expect("artifact");
    plan.artifact_size = bytes.len() as u64;
    plan.artifact_sha256 = format!("{:x}", Sha256::digest(bytes));
    let signing_key = SigningKey::from_bytes(&[31; 32]);
    let payload = serde_json::to_vec(&json!({
        "schema_version": 2,
        "channel": "stable",
        "version": plan.target_version,
        "published_at": "2026-07-22T12:00:00Z",
        "notes": "fixture",
        "paused": false,
        "rollout_percent": 100,
        "release_url": "https://github.com/kongweiguang/gmark/releases/tag/v1.0.1",
        "artifacts": {
            "fixture": {
                "url": plan.artifact_url,
                "size": plan.artifact_size,
                "sha256": plan.artifact_sha256,
                "format": plan.artifact_format,
                "system_trust": "not-applicable"
            }
        }
    }))
    .expect("manifest payload");
    let signature = signing_key.sign(&payload);
    fs::write(
        &plan.signed_envelope_path,
        serde_json::to_vec(&json!({
            "schema_version": 1,
            "algorithm": "Ed25519",
            "payload": BASE64.encode(&payload),
            "signature": BASE64.encode(signature.to_bytes())
        }))
        .expect("manifest envelope"),
    )
    .expect("signed manifest");
    signing_key
}

/// Opens a guarded staged artifact for the adapter without bypassing core
/// signature, hash, size, or platform validation.
fn stage_artifact(plan: &ApplyPlanV2, signing_key: &SigningKey) -> StagedApplyArtifact {
    stage_and_verify_apply_plan_artifact_v2(
        plan,
        &signing_key.verifying_key(),
        &Platform::current(),
        plan.transaction_dir().expect("transaction directory"),
    )
    .expect("verified staged artifact")
}

/// Installs into the target directory with a single atomic replacement.
#[test]
fn install_replaces_target_without_creating_legacy_backup() {
    let root = tempfile::tempdir().expect("temporary root");
    let target = root.path().join("gmark.AppImage");
    fs::write(&target, b"current").expect("target");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o754)).expect("target mode");
    let transaction_id = Uuid::new_v4();
    let backup = compatible_backup_path(&target, transaction_id);
    let mut transaction = plan(root.path(), &target, &backup, transaction_id);
    let signing_key = prepare_verified_artifact(&mut transaction, b"updated");
    let mut artifact = stage_artifact(&transaction, &signing_key);

    install(&transaction, &mut artifact).expect("atomic AppImage install");

    assert_eq!(fs::read(&target).expect("installed target"), b"updated");
    assert_eq!(
        fs::symlink_metadata(&target)
            .expect("installed metadata")
            .permissions()
            .mode()
            & 0o7777,
        0o754
    );
    assert!(!backup.exists(), "legacy backup must remain unused");
    assert!(
        fs::read_dir(root.path())
            .expect("target directory")
            .all(|entry| !entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".gmark-update-install-"))
    );
}

/// Keeps the existing target untouched when temporary creation cannot commit.
#[test]
fn failed_temporary_copy_preserves_existing_target() {
    let root = tempfile::tempdir().expect("temporary root");
    let target = root.path().join("gmark.AppImage");
    fs::write(&target, b"current").expect("target");
    let transaction_id = Uuid::new_v4();
    let backup = compatible_backup_path(&target, transaction_id);
    let mut transaction = plan(root.path(), &target, &backup, transaction_id);
    let signing_key = prepare_verified_artifact(&mut transaction, b"updated");
    let mut artifact = stage_artifact(&transaction, &signing_key);
    let error = copy_verified_artifact(&mut artifact, &target, 0o755, 7)
        .expect_err("create_new must reject an existing target");

    assert!(error.contains("temporary"));
    assert_eq!(fs::read(&target).expect("unchanged target"), b"current");
}

/// Preserves the target's exact Unix mode rather than applying a new default.
#[test]
fn copied_artifact_inherits_existing_permissions() {
    let root = tempfile::tempdir().expect("temporary root");
    let target = root.path().join("gmark.AppImage");
    fs::write(&target, b"current").expect("target");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o741)).expect("target mode");
    let transaction_id = Uuid::new_v4();
    let backup = compatible_backup_path(&target, transaction_id);
    let mut transaction = plan(root.path(), &target, &backup, transaction_id);
    let signing_key = prepare_verified_artifact(&mut transaction, b"updated");
    let mut artifact = stage_artifact(&transaction, &signing_key);
    let temporary = root.path().join(".gmark-test-install.tmp");

    copy_verified_artifact(&mut artifact, &temporary, 0o741, 7).expect("copy artifact");

    assert_eq!(
        fs::symlink_metadata(&temporary)
            .expect("temporary metadata")
            .permissions()
            .mode()
            & 0o7777,
        0o741
    );
}

/// Rejects a symlinked ancestor before a plan can escape its installation root.
#[test]
fn path_validation_rejects_symlinked_ancestor() {
    let root = tempfile::tempdir().expect("temporary root");
    let real = root.path().join("real");
    let link = root.path().join("link");
    fs::create_dir(&real).expect("real directory");
    symlink(&real, &link).expect("directory symlink");
    let escaped_target = link.join("gmark.AppImage");

    let error = validate_path_components(&escaped_target, "AppImage target")
        .expect_err("ancestor symlink must be rejected");
    assert!(error.contains("symlink"));
}

/// Rejects relative and parent-component paths before filesystem access.
#[test]
fn path_validation_rejects_relative_and_parent_escape() {
    assert!(validate_clean_absolute_path(Path::new("../gmark.AppImage"), "target").is_err());
    assert!(validate_clean_absolute_path(Path::new("/tmp/../gmark.AppImage"), "target").is_err());
}

/// Rejects a target that is outside the plan's expected installation root.
#[test]
fn install_root_binding_rejects_absolute_escape() {
    let root = tempfile::tempdir().expect("temporary root");
    let target = root.path().join("other/gmark.AppImage");
    let transaction_id = Uuid::new_v4();
    let backup = compatible_backup_path(&target, transaction_id);
    let mut transaction = plan(root.path(), &target, &backup, transaction_id);
    transaction.expected_install_root = root.path().join("expected");

    let error = validate_install_root(&transaction).expect_err("target must stay under root");
    assert!(error.contains("escapes"));
}

/// Rejects a symlink target itself instead of replacing a link destination.
#[test]
fn target_validation_rejects_symlink_without_touching_destination() {
    let root = tempfile::tempdir().expect("temporary root");
    let actual = root.path().join("actual.AppImage");
    let link = root.path().join("gmark.AppImage");
    fs::write(&actual, b"current").expect("actual target");
    symlink(&actual, &link).expect("target symlink");

    let error = validate_target(&link).expect_err("target symlink must be rejected");
    assert!(error.contains("non-symlink"));
    assert_eq!(fs::read(&actual).expect("destination bytes"), b"current");
}
