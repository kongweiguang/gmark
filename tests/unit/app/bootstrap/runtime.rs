// @author kongweiguang

use std::path::{Path, PathBuf};

use gmark_update_core::{ApplyPlanV1, ApplyPlanV2, write_apply_plan, write_apply_plan_v2};

use super::{take_update_acknowledgement, write_update_acknowledgement};

const TARGET_VERSION: &str = "1.1.0";
const ACK_CAPABILITY: &str = "7c8b7d95-01a9-4ae0-b1f7-5b7454e99739";

#[test]
fn internal_update_ack_is_removed_before_user_cli_parsing() {
    let mut args = vec![
        "gmark".to_owned(),
        "--update-ack".to_owned(),
        "C:/temp/update-ack".to_owned(),
        "note.md".to_owned(),
    ];
    assert_eq!(
        take_update_acknowledgement(&mut args),
        Some(PathBuf::from("C:/temp/update-ack"))
    );
    assert_eq!(args, ["gmark", "note.md"]);
}

#[test]
fn normal_helper_relaunch_acknowledgement_remains_compatible() {
    let root = tempfile::tempdir().unwrap();
    let acknowledgement_path = active_transaction(root.path());

    write_update_acknowledgement(
        &acknowledgement_path,
        root.path(),
        Some(ACK_CAPABILITY),
        TARGET_VERSION,
    )
    .unwrap();

    assert_eq!(
        std::fs::read(acknowledgement_path).unwrap(),
        format!("{TARGET_VERSION}\n").as_bytes()
    );
}

#[test]
fn v2_acknowledgement_is_bound_to_transaction_id_and_nested_root() {
    let root = tempfile::tempdir().unwrap();
    let acknowledgement_path = v2_active_transaction(root.path());
    let transaction = acknowledgement_path.parent().unwrap();
    let transaction_id = transaction
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap();
    let capability_path = transaction.join(format!("startup-ack-capability-{ACK_CAPABILITY}"));
    assert_eq!(
        std::fs::read(&capability_path).unwrap(),
        format!("{transaction_id}:{ACK_CAPABILITY}\n").as_bytes()
    );

    write_update_acknowledgement(
        &acknowledgement_path,
        root.path(),
        Some(ACK_CAPABILITY),
        TARGET_VERSION,
    )
    .unwrap();

    assert_eq!(
        std::fs::read(acknowledgement_path).unwrap(),
        format!("{TARGET_VERSION}\n").as_bytes()
    );
}

#[test]
fn v2_acknowledgement_rejects_missing_capability() {
    let root = tempfile::tempdir().unwrap();
    let acknowledgement_path = v2_active_transaction(root.path());
    let error =
        write_update_acknowledgement(&acknowledgement_path, root.path(), None, TARGET_VERSION)
            .unwrap_err();
    assert!(error.contains("requires an acknowledgement capability"));
    assert!(!acknowledgement_path.exists());
}

#[test]
fn legacy_helper_without_capability_or_sidecar_acknowledges_active_transaction() {
    let root = tempfile::tempdir().unwrap();
    let acknowledgement_path = legacy_active_transaction(root.path());
    let transaction = acknowledgement_path.parent().unwrap();

    assert!(
        !transaction
            .join(format!("startup-ack-capability-{ACK_CAPABILITY}"))
            .exists()
    );
    write_update_acknowledgement(&acknowledgement_path, root.path(), None, TARGET_VERSION).unwrap();

    assert_eq!(
        std::fs::read(acknowledgement_path).unwrap(),
        format!("{TARGET_VERSION}\n").as_bytes()
    );
}

#[test]
fn arbitrary_update_ack_path_is_rejected_without_creating_a_file() {
    let updates_root = tempfile::tempdir().unwrap();
    let outside_root = tempfile::tempdir().unwrap();
    let arbitrary_path = outside_root.path().join("startup-ack");

    let error = write_update_acknowledgement(
        &arbitrary_path,
        updates_root.path(),
        Some(ACK_CAPABILITY),
        TARGET_VERSION,
    )
    .unwrap_err();

    assert!(error.contains("outside"));
    assert!(!arbitrary_path.exists());
}

#[test]
fn out_of_root_acknowledgement_is_rejected_without_truncating_an_existing_file() {
    let updates_root = tempfile::tempdir().unwrap();
    let outside_root = tempfile::tempdir().unwrap();
    let arbitrary_path = outside_root.path().join("startup-ack");
    std::fs::write(&arbitrary_path, b"keep this acknowledgement").unwrap();

    assert!(
        write_update_acknowledgement(
            &arbitrary_path,
            updates_root.path(),
            Some(ACK_CAPABILITY),
            TARGET_VERSION,
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(arbitrary_path).unwrap(),
        b"keep this acknowledgement"
    );
}

#[test]
fn legacy_helper_acknowledgement_rejects_an_arbitrary_leaf() {
    let root = tempfile::tempdir().unwrap();
    let acknowledgement_path = legacy_active_transaction(root.path());
    let arbitrary_path = acknowledgement_path.with_file_name("not-startup-ack");

    assert!(
        write_update_acknowledgement(&arbitrary_path, root.path(), None, TARGET_VERSION).is_err()
    );
    assert!(!arbitrary_path.exists());
}

#[test]
fn legacy_helper_acknowledgement_rejects_an_out_of_root_path() {
    let updates_root = tempfile::tempdir().unwrap();
    let _transaction = legacy_active_transaction(updates_root.path());
    let outside_root = tempfile::tempdir().unwrap();
    let acknowledgement_path = outside_root.path().join("startup-ack");

    assert!(
        write_update_acknowledgement(
            &acknowledgement_path,
            updates_root.path(),
            None,
            TARGET_VERSION,
        )
        .is_err()
    );
    assert!(!acknowledgement_path.exists());
}

#[test]
fn present_mismatched_acknowledgement_capability_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let acknowledgement_path = active_transaction(root.path());

    assert!(
        write_update_acknowledgement(
            &acknowledgement_path,
            root.path(),
            Some("77e5ecbb-e38d-4954-b963-2d58c9480d01"),
            TARGET_VERSION,
        )
        .is_err()
    );
    assert!(!acknowledgement_path.exists());
}

#[test]
fn symlinked_acknowledgement_target_is_rejected_without_truncating_the_victim() {
    let root = tempfile::tempdir().unwrap();
    let acknowledgement_path = active_transaction(root.path());
    let victim = root.path().join("victim");
    std::fs::write(&victim, b"preserve this file").unwrap();
    if let Err(error) = create_file_symlink(&victim, &acknowledgement_path) {
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || error.raw_os_error() == Some(1314)
        {
            return;
        }
        panic!("failed to create acknowledgement symlink: {error}");
    }

    assert!(
        write_update_acknowledgement(
            &acknowledgement_path,
            root.path(),
            Some(ACK_CAPABILITY),
            TARGET_VERSION,
        )
        .is_err()
    );
    assert_eq!(std::fs::read(victim).unwrap(), b"preserve this file");
}

#[test]
fn legacy_helper_acknowledgement_rejects_a_symlinked_target() {
    let root = tempfile::tempdir().unwrap();
    let acknowledgement_path = legacy_active_transaction(root.path());
    let victim = root.path().join("victim");
    std::fs::write(&victim, b"preserve this file").unwrap();
    if let Err(error) = create_file_symlink(&victim, &acknowledgement_path) {
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || error.raw_os_error() == Some(1314)
        {
            return;
        }
        panic!("failed to create acknowledgement symlink: {error}");
    }

    assert!(
        write_update_acknowledgement(&acknowledgement_path, root.path(), None, TARGET_VERSION,)
            .is_err()
    );
    assert_eq!(std::fs::read(victim).unwrap(), b"preserve this file");
}

fn active_transaction(updates_root: &Path) -> PathBuf {
    transaction(updates_root, true)
}

fn legacy_active_transaction(updates_root: &Path) -> PathBuf {
    transaction(updates_root, false)
}

fn v2_active_transaction(updates_root: &Path) -> PathBuf {
    let version_dir = updates_root.join(format!("v{TARGET_VERSION}"));
    let legacy_transaction = version_dir.clone();
    let install_root = updates_root.join("installed-gmark");
    std::fs::create_dir_all(&install_root).unwrap();
    let target = if cfg!(target_os = "windows") {
        install_root.join("gmark.exe")
    } else if cfg!(target_os = "macos") {
        install_root.join("gmark.app")
    } else {
        install_root.join("gmark.AppImage")
    };
    if cfg!(target_os = "macos") {
        std::fs::create_dir_all(&target).unwrap();
    } else {
        std::fs::write(&target, b"installed").unwrap();
    }
    let format = if cfg!(target_os = "windows") {
        "windows-setup-exe"
    } else if cfg!(target_os = "macos") {
        "macos-app-tar-gz"
    } else {
        "linux-app-image"
    };
    let artifact_url = if cfg!(target_os = "windows") {
        "https://github.com/kongweiguang/gmark/releases/download/v1.1.0/gmark-setup.exe"
    } else if cfg!(target_os = "macos") {
        "https://github.com/kongweiguang/gmark/releases/download/v1.1.0/gmark.app.tar.gz"
    } else {
        "https://github.com/kongweiguang/gmark/releases/download/v1.1.0/gmark.AppImage"
    };
    let relaunch = if cfg!(target_os = "macos") {
        target.join("Contents/MacOS/gmark")
    } else {
        target.clone()
    };
    let v1 = ApplyPlanV1 {
        schema_version: ApplyPlanV1::SCHEMA_VERSION,
        parent_pid: 0,
        current_version: "1.0.0".to_owned(),
        target_version: TARGET_VERSION.to_owned(),
        artifact_path: legacy_transaction.join("artifact.ready"),
        artifact_url: artifact_url.to_owned(),
        artifact_size: 8,
        artifact_sha256: "00".repeat(32),
        artifact_format: format.to_owned(),
        signed_envelope_path: legacy_transaction.join("manifest.envelope.json"),
        target_path: target.clone(),
        backup_path: target.with_extension("legacy-backup"),
        relaunch_path: relaunch,
        acknowledgement_path: legacy_transaction.join("startup-ack"),
        cancellation_path: legacy_transaction.join("cancel-install"),
        result_path: updates_root.join("last-result.json"),
        helper_log_path: updates_root.join("last-helper.log"),
    };
    let transaction_id = uuid::Uuid::parse_str("e13b1f02-c736-45df-9c19-52f24e6adace").unwrap();
    let plan = ApplyPlanV2::from_v1(&v1, transaction_id);
    let transaction = plan.transaction_dir().unwrap();
    std::fs::create_dir_all(transaction).unwrap();
    std::fs::write(&plan.artifact_path, b"artifact").unwrap();
    std::fs::write(&plan.signed_envelope_path, b"envelope").unwrap();
    std::fs::write(
        transaction.join(format!("startup-ack-capability-{ACK_CAPABILITY}")),
        format!("{}:{ACK_CAPABILITY}\n", transaction_id.hyphenated()),
    )
    .unwrap();
    write_apply_plan_v2(transaction.join(ApplyPlanV2::PLAN_FILE_NAME), &plan).unwrap();
    plan.acknowledgement_path
}

fn transaction(updates_root: &Path, include_capability: bool) -> PathBuf {
    let transaction = updates_root.join(format!("v{TARGET_VERSION}"));
    std::fs::create_dir_all(&transaction).unwrap();
    std::fs::write(transaction.join("artifact.ready"), b"artifact").unwrap();
    std::fs::write(transaction.join("manifest.envelope.json"), b"envelope").unwrap();
    if include_capability {
        std::fs::write(
            transaction.join(format!("startup-ack-capability-{ACK_CAPABILITY}")),
            format!("{ACK_CAPABILITY}\n"),
        )
        .unwrap();
    }
    let acknowledgement_path = transaction.join("startup-ack");
    write_apply_plan(
        transaction.join("apply-plan.json"),
        &ApplyPlanV1 {
            schema_version: ApplyPlanV1::SCHEMA_VERSION,
            parent_pid: 0,
            current_version: "1.0.0".to_owned(),
            target_version: TARGET_VERSION.to_owned(),
            artifact_path: transaction.join("artifact.ready"),
            artifact_url: "https://example.invalid/artifact".to_owned(),
            artifact_size: 8,
            artifact_sha256: "00".repeat(32),
            artifact_format: "linux-appimage".to_owned(),
            signed_envelope_path: transaction.join("manifest.envelope.json"),
            target_path: transaction.join("gmark"),
            backup_path: transaction.join("gmark.gmark-update-backup"),
            relaunch_path: transaction.join("gmark"),
            acknowledgement_path: acknowledgement_path.clone(),
            cancellation_path: transaction.join("cancel-install"),
            result_path: updates_root.join("last-result.json"),
            helper_log_path: updates_root.join("last-helper.log"),
        },
    )
    .unwrap();
    acknowledgement_path
}

#[cfg(unix)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}
