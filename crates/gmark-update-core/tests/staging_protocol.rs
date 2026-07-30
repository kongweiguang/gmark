// @author kongweiguang

use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};
#[cfg(windows)]
use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt as _, process::Command};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer as _, SigningKey};
use gmark_update_core::{
    ApplyPlanV1, ApplyResultV1, BoundedTransferOutcome, CancellationV1, HelperSignalV1,
    MAX_ENVELOPE_BYTES, PartialMetadata, Platform, StartupAcknowledgementV1, UpdateCoreError,
    clear_helper_signal, copy_and_verify_bounded, copy_bounded, helper_signal_present,
    read_apply_plan, read_apply_result, read_partial_metadata, read_validated_apply_plan,
    resume_request, stage_and_verify_apply_plan_artifact, startup_acknowledgement_matches,
    validate_apply_plan, validate_apply_plan_files, verify_apply_plan_artifact,
    verify_artifact_file, write_apply_plan, write_apply_result, write_helper_signal,
    write_partial_metadata,
};
use serde_json::json;
use sha2::{Digest, Sha256};

const ARTIFACT_URL: &str =
    "https://github.com/kongweiguang/gmark/releases/download/v0.2.0/gmark.AppImage";

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn fixture_plan(root: &Path) -> ApplyPlanV1 {
    let transaction = root.join("v0.2.0");
    let target = root.join("gmark.AppImage");
    let target_name = target.file_name().unwrap().to_string_lossy();
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
        backup_path: root.join(format!("{target_name}.gmark-update-backup")),
        relaunch_path: target,
        acknowledgement_path: transaction.join("startup-ack"),
        cancellation_path: transaction.join("cancel-install"),
        result_path: root.join("last-result.json"),
        helper_log_path: root.join("last-helper.log"),
    }
}

fn write_signed_v2_fixture(plan: &ApplyPlanV1, key: &SigningKey, system_trust: &str) {
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
    let signature = key.sign(&payload);
    let envelope = serde_json::to_vec(&json!({
        "schema_version": 1,
        "algorithm": "Ed25519",
        "payload": STANDARD.encode(payload),
        "signature": STANDARD.encode(signature.to_bytes())
    }))
    .unwrap();
    fs::write(&plan.signed_envelope_path, envelope).unwrap();
}

#[test]
fn detects_artifact_tampering_after_a_successful_hash_check() {
    let root = tempfile::tempdir().unwrap();
    let artifact = root.path().join("artifact.ready");
    let bytes = b"artifact!";
    fs::write(&artifact, bytes).unwrap();
    let hash = digest(bytes);
    verify_artifact_file(&artifact, bytes.len() as u64, &hash).unwrap();

    fs::write(&artifact, b"tampered!").unwrap();
    assert!(matches!(
        verify_artifact_file(&artifact, bytes.len() as u64, &hash),
        Err(UpdateCoreError::HashMismatch { .. })
    ));
}

#[test]
fn helper_reverification_binds_plan_manifest_and_artifact_bytes() {
    let root = tempfile::tempdir().unwrap();
    let mut plan = fixture_plan(root.path());
    let artifact = b"artifact!";
    plan.artifact_size = artifact.len() as u64;
    plan.artifact_sha256 = digest(artifact);
    fs::create_dir_all(plan.artifact_path.parent().unwrap()).unwrap();
    fs::write(&plan.artifact_path, artifact).unwrap();
    fs::write(&plan.target_path, b"appimage").unwrap();

    let key = SigningKey::from_bytes(&[31; 32]);
    write_signed_v2_fixture(&plan, &key, "not-applicable");

    verify_apply_plan_artifact(
        &plan,
        &key.verifying_key(),
        &Platform::new("linux", "x86_64"),
    )
    .unwrap();
    let staged = stage_and_verify_apply_plan_artifact(
        &plan,
        &key.verifying_key(),
        &Platform::new("linux", "x86_64"),
        root.path(),
    )
    .unwrap();
    fs::write(&plan.artifact_path, b"tampered!").unwrap();
    assert_eq!(fs::read(staged.path()).unwrap(), artifact);
    assert!(matches!(
        verify_apply_plan_artifact(
            &plan,
            &key.verifying_key(),
            &Platform::new("linux", "x86_64")
        ),
        Err(UpdateCoreError::HashMismatch { .. })
    ));
}

#[cfg(windows)]
#[test]
fn staged_windows_artifact_denies_mutation_until_drop_and_remains_launchable() {
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE: u32 = 0x0000_0004;

    let root = tempfile::tempdir().unwrap();
    let mut plan = fixture_plan(root.path());
    let target = root.path().join("gmark.exe");
    plan.target_path = target.clone();
    plan.backup_path = root.path().join("gmark.exe.gmark-update-backup");
    plan.relaunch_path = target;
    plan.artifact_url =
        "https://github.com/kongweiguang/gmark/releases/download/v0.2.0/gmark-setup.exe".to_owned();
    plan.artifact_format = "windows-setup-exe".to_owned();
    fs::create_dir_all(plan.artifact_path.parent().unwrap()).unwrap();
    fs::write(&plan.target_path, b"current gmark executable").unwrap();
    let command_processor = std::env::var_os("ComSpec").expect("Windows must provide ComSpec");
    fs::copy(command_processor, &plan.artifact_path).unwrap();
    let artifact = fs::read(&plan.artifact_path).unwrap();
    plan.artifact_size = artifact.len() as u64;
    plan.artifact_sha256 = digest(&artifact);
    let key = SigningKey::from_bytes(&[47; 32]);
    write_signed_v2_fixture(&plan, &key, "unsigned");

    let staged = stage_and_verify_apply_plan_artifact(
        &plan,
        &key.verifying_key(),
        &Platform::new("windows", "x86_64"),
        root.path(),
    )
    .unwrap();
    let staged_path = staged.path().to_path_buf();
    let mut writer = OpenOptions::new();
    writer
        .write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
    assert!(
        writer.open(&staged_path).is_err(),
        "the guarded staged artifact must deny a second writer"
    );
    assert!(
        fs::remove_file(&staged_path).is_err(),
        "the guarded staged artifact must deny deletion"
    );
    assert!(
        fs::rename(&staged_path, root.path().join("renamed-installer.exe")).is_err(),
        "the guarded staged artifact must deny rename"
    );
    let status = Command::new(&staged_path)
        .args(["/C", "exit", "0"])
        .status()
        .expect("CreateProcess must open the guarded staged executable");
    assert!(status.success());

    drop(staged);
    assert!(
        !staged_path.exists(),
        "cleanup must remove the staged artifact after its guard closes"
    );
}

#[test]
fn bounded_copy_and_resume_metadata_preserve_resume_contract() {
    let metadata = PartialMetadata {
        etag: Some("etag-1".to_owned()),
        last_modified: Some("ignored".to_owned()),
    };
    let request = resume_request(3, 6, &metadata).unwrap().unwrap();
    assert_eq!(request.offset, 3);
    assert_eq!(request.if_range.as_deref(), Some("etag-1"));

    let mut reader = Cursor::new(b"def".to_vec());
    let mut output = Vec::new();
    let result = copy_bounded(&mut reader, &mut output, 3, 6, || false, |_| {}).unwrap();
    assert_eq!(result, BoundedTransferOutcome::Complete { downloaded: 6 });
    assert_eq!(output, b"def");
}

#[test]
fn bounded_hash_copy_rejects_oversize_and_tampering() {
    let payload = b"verified legacy installer";
    let hash = digest(payload);
    let mut output = Vec::new();
    assert_eq!(
        copy_and_verify_bounded(&mut &payload[..], &mut output, 512, &hash).unwrap(),
        payload.len() as u64
    );
    assert_eq!(output, payload);

    assert!(matches!(
        copy_and_verify_bounded(&mut &payload[..], &mut Vec::new(), 4, &hash),
        Err(UpdateCoreError::TooLarge)
    ));
    assert!(matches!(
        copy_and_verify_bounded(&mut &payload[..], &mut Vec::new(), 512, &"00".repeat(32)),
        Err(UpdateCoreError::HashMismatch { .. })
    ));
}

#[test]
fn apply_plan_round_trip_rejects_unknown_fields_and_validates_paths() {
    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    let plan_path = root.path().join("v0.2.0/apply-plan.json");
    write_apply_plan(&plan_path, &plan).unwrap();
    assert_eq!(read_apply_plan(&plan_path).unwrap(), plan);
    validate_apply_plan(&plan, &Platform::new("linux", "x86_64")).unwrap();
    assert!(matches!(
        validate_apply_plan_files(&plan, &Platform::new("linux", "x86_64")),
        Err(UpdateCoreError::Protocol(message)) if message == "verified update files are missing"
    ));
    fs::create_dir_all(plan.artifact_path.parent().unwrap()).unwrap();
    fs::write(&plan.artifact_path, b"artifact!").unwrap();
    fs::write(&plan.signed_envelope_path, b"envelope").unwrap();
    fs::write(&plan.target_path, b"appimage").unwrap();
    validate_apply_plan_files(&plan, &Platform::new("linux", "x86_64")).unwrap();

    let mut value = serde_json::to_value(&plan).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("unknown".to_owned(), serde_json::json!(true));
    assert!(serde_json::from_value::<ApplyPlanV1>(value).is_err());
}

#[test]
fn validated_plan_is_bound_to_its_supplied_transaction_path() {
    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    let plan_path = root.path().join("v0.2.0/apply-plan.json");
    write_apply_plan(&plan_path, &plan).unwrap();
    assert_eq!(
        read_validated_apply_plan(&plan_path, &Platform::new("linux", "x86_64")).unwrap(),
        plan
    );

    let misplaced_plan = root.path().join("apply-plan.json");
    write_apply_plan(&misplaced_plan, &plan).unwrap();
    assert!(read_validated_apply_plan(&misplaced_plan, &Platform::new("linux", "x86_64")).is_err());

    let mut malicious = plan.clone();
    malicious.relaunch_path = root.path().join("attacker-relaunch");
    malicious.helper_log_path = root.path().join("attacker-helper.log");
    malicious.result_path = root.path().join("attacker-result.json");
    write_apply_plan(&plan_path, &malicious).unwrap();
    assert!(read_validated_apply_plan(&plan_path, &Platform::new("linux", "x86_64")).is_err());
    assert!(!malicious.helper_log_path.exists());
    assert!(!malicious.result_path.exists());
}

#[test]
fn signed_envelope_is_bounded_before_parsing() {
    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    fs::create_dir_all(plan.artifact_path.parent().unwrap()).unwrap();
    fs::write(&plan.artifact_path, b"artifact!").unwrap();
    fs::write(&plan.target_path, b"appimage").unwrap();
    fs::write(
        &plan.signed_envelope_path,
        vec![b' '; MAX_ENVELOPE_BYTES + 1],
    )
    .unwrap();

    let key = SigningKey::from_bytes(&[31; 32]);
    assert!(
        verify_apply_plan_artifact(
            &plan,
            &key.verifying_key(),
            &Platform::new("linux", "x86_64"),
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn apply_plan_is_persisted_with_private_permissions() {
    use std::os::unix::fs::PermissionsExt as _;

    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    let plan_path = root.path().join("v0.2.0/apply-plan.json");
    write_apply_plan(&plan_path, &plan).unwrap();

    assert_eq!(
        fs::metadata(plan_path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn helper_markers_and_result_json_remain_compatible() {
    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    write_helper_signal(&plan, HelperSignalV1::Acknowledgement).unwrap();
    assert!(helper_signal_present(&plan, HelperSignalV1::Acknowledgement).unwrap());
    assert_eq!(
        fs::read(&plan.acknowledgement_path).unwrap(),
        b"0.2.0\n",
        "the app acknowledges the launched target version"
    );
    assert_eq!(
        StartupAcknowledgementV1::for_target_version("0.2.0").marker_bytes(),
        b"0.2.0\n"
    );
    write_helper_signal(&plan, HelperSignalV1::Cancellation).unwrap();
    assert!(helper_signal_present(&plan, HelperSignalV1::Cancellation).unwrap());
    assert_eq!(
        fs::read(&plan.cancellation_path).unwrap(),
        CancellationV1::MARKER_BYTES
    );
    clear_helper_signal(&plan, HelperSignalV1::Cancellation).unwrap();
    assert!(!helper_signal_present(&plan, HelperSignalV1::Cancellation).unwrap());

    let result = ApplyResultV1::succeeded("0.1.0", "0.2.0");
    write_apply_result(&plan.result_path, &result).unwrap();
    assert_eq!(read_apply_result(&plan.result_path).unwrap(), result);
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(&plan.result_path).unwrap()).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["status"], "succeeded");
    assert_eq!(json["from_version"], "0.1.0");
    assert_eq!(json["to_version"], "0.2.0");

    fs::write(
        &plan.result_path,
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "status": "succeeded",
            "to_version": "0.2.0",
            "message": "legacy result"
        }))
        .unwrap(),
    )
    .unwrap();
    let legacy = read_apply_result(&plan.result_path).unwrap();
    assert!(legacy.from_version.is_empty());
}

#[test]
fn acknowledgement_requires_the_exact_expected_marker() {
    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    fs::create_dir_all(plan.acknowledgement_path.parent().unwrap()).unwrap();
    assert!(!startup_acknowledgement_matches(&plan).unwrap());

    for marker in [
        b"".as_slice(),
        b"0.2".as_slice(),
        b"0.1.0\n".as_slice(),
        b"0.2.0\nextra".as_slice(),
    ] {
        fs::write(&plan.acknowledgement_path, marker).unwrap();
        assert!(startup_acknowledgement_matches(&plan).is_err());
        clear_helper_signal(&plan, HelperSignalV1::Acknowledgement).unwrap();
    }

    write_helper_signal(&plan, HelperSignalV1::Acknowledgement).unwrap();
    assert!(startup_acknowledgement_matches(&plan).unwrap());
}

#[cfg(unix)]
#[test]
fn acknowledgement_rejects_a_symlink_marker() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    fs::create_dir_all(plan.acknowledgement_path.parent().unwrap()).unwrap();
    let marker = root.path().join("outside-marker");
    fs::write(&marker, b"0.2.0\n").unwrap();
    symlink(&marker, &plan.acknowledgement_path).unwrap();

    assert!(startup_acknowledgement_matches(&plan).is_err());
}

#[cfg(unix)]
#[test]
fn apply_plan_reader_rejects_a_symlink() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let plan = fixture_plan(root.path());
    let source = root.path().join("source-plan.json");
    let linked = root.path().join("v0.2.0/apply-plan.json");
    write_apply_plan(&source, &plan).unwrap();
    fs::create_dir_all(linked.parent().unwrap()).unwrap();
    symlink(&source, &linked).unwrap();

    assert!(read_apply_plan(&linked).is_err());
}

#[test]
fn apply_result_reader_accepts_legacy_extra_fields() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("last-result.json");
    fs::write(
        &path,
        br#"{"schema_version":1,"status":"failed","to_version":"0.2.0","message":"legacy result","legacy_extra":true}"#,
    )
    .unwrap();

    let result = read_apply_result(&path).unwrap();
    assert!(result.from_version.is_empty());
    assert_eq!(result.status, "failed");
    assert_eq!(result.to_version, "0.2.0");
    assert_eq!(result.message, "legacy result");
}

#[test]
fn apply_result_writer_rejects_unknown_statuses() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("last-result.json");
    let result = ApplyResultV1 {
        schema_version: ApplyResultV1::SCHEMA_VERSION,
        status: "interrupted".to_owned(),
        from_version: "0.1.0".to_owned(),
        to_version: "0.2.0".to_owned(),
        message: "legacy result".to_owned(),
    };

    assert!(matches!(
        write_apply_result(&path, &result),
        Err(UpdateCoreError::Protocol(message)) if message == "update result has an unsupported status"
    ));
    assert!(!path.exists());
}

#[test]
fn partial_metadata_round_trips_through_the_staging_sidecar() {
    let root = tempfile::tempdir().unwrap();
    let path: PathBuf = root.path().join("partial.json");
    let metadata = PartialMetadata {
        etag: None,
        last_modified: Some("Wed, 22 Jul 2026 12:00:00 GMT".to_owned()),
    };
    write_partial_metadata(&path, &metadata).unwrap();
    assert_eq!(read_partial_metadata(&path).unwrap(), metadata);
}
