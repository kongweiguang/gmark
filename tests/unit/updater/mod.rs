// @author kongweiguang

use super::*;

#[test]
fn automatic_check_is_due_without_a_success_marker() {
    let root = tempfile::tempdir().unwrap();
    let service = UpdateService::new(root.path().to_path_buf(), true);
    assert!(service.automatic_check_due());
}

#[test]
fn recent_success_marker_suppresses_the_daily_check() {
    let root = tempfile::tempdir().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    std::fs::write(root.path().join("last-successful-check"), now.to_string()).unwrap();
    let service = UpdateService::new(root.path().to_path_buf(), true);
    assert!(!service.automatic_check_due());
}

#[test]
fn only_user_relevant_states_are_visible() {
    assert!(!UpdateState::Idle.is_visible());
    assert!(
        !UpdateState::Checking {
            origin: CheckOrigin::Automatic,
        }
        .is_visible()
    );
    assert!(
        UpdateState::Checking {
            origin: CheckOrigin::Manual,
        }
        .is_visible()
    );
}

#[test]
fn command_policy_rejects_duplicate_or_out_of_order_actions() {
    assert!(UpdateState::Idle.accepts(UpdateCommand::Check));
    assert!(!UpdateState::Idle.accepts(UpdateCommand::Download));
    assert!(
        !UpdateState::Checking {
            origin: CheckOrigin::Manual,
        }
        .accepts(UpdateCommand::Check)
    );

    let release = release_fixture();
    assert!(UpdateState::Available(release.clone()).accepts(UpdateCommand::Download));
    assert!(!UpdateState::Available(release.clone()).accepts(UpdateCommand::Resume));
    assert!(
        UpdateState::Paused {
            release: release.clone(),
            downloaded: 25,
            total: 100,
        }
        .accepts(UpdateCommand::Resume)
    );
    assert!(
        !UpdateState::Paused {
            release: release.clone(),
            downloaded: 25,
            total: 100,
        }
        .accepts(UpdateCommand::Download)
    );

    assert!(
        UpdateState::Failed {
            release: Some(release.clone()),
            message: "timeout".to_owned(),
            retryable: true,
        }
        .accepts(UpdateCommand::Retry)
    );
    assert!(
        !UpdateState::Failed {
            release: Some(release.clone()),
            message: "signature mismatch".to_owned(),
            retryable: false,
        }
        .accepts(UpdateCommand::Retry)
    );
    assert!(
        UpdateState::Ready {
            release,
            artifact_path: PathBuf::from("artifact.ready"),
        }
        .accepts(UpdateCommand::InstallAndRestart)
    );
}

#[test]
fn busy_states_cannot_be_dismissed_or_restarted() {
    let release = release_fixture();
    let downloading = UpdateState::Downloading {
        release: release.clone(),
        downloaded: 25,
        total: 100,
        bytes_per_second: 10,
    };
    assert!(downloading.accepts(UpdateCommand::Pause));
    assert!(!downloading.accepts(UpdateCommand::Dismiss));
    assert!(!downloading.accepts(UpdateCommand::InstallAndRestart));
    assert!(
        !UpdateState::Verifying {
            release: release.clone(),
        }
        .accepts(UpdateCommand::Dismiss)
    );
    assert!(!UpdateState::Installing { release }.accepts(UpdateCommand::Dismiss));
}

#[test]
fn pending_install_cancellation_restores_the_ready_payload() {
    let root = tempfile::tempdir().unwrap();
    let release = release_fixture();
    let artifact_path = root.path().join("artifact.ready");
    let cancellation_path = root.path().join("cancel-install");
    let mut service = UpdateService::new(root.path().to_path_buf(), true);
    service.pending_install = Some(PendingInstall {
        release: release.clone(),
        artifact_path: artifact_path.clone(),
        plan: ApplyPlanV1 {
            schema_version: ApplyPlanV1::SCHEMA_VERSION,
            parent_pid: 0,
            current_version: release.current_version.clone(),
            target_version: release.version.clone(),
            artifact_path: artifact_path.clone(),
            artifact_url: release.artifact_url.clone(),
            artifact_size: release.artifact_size,
            artifact_sha256: release.artifact_sha256.clone(),
            artifact_format: release.artifact_format.as_protocol_name().to_owned(),
            signed_envelope_path: root.path().join("manifest.envelope.json"),
            target_path: root.path().join("gmark.AppImage"),
            backup_path: root.path().join("gmark.AppImage.gmark-update-backup"),
            relaunch_path: root.path().join("gmark.AppImage"),
            acknowledgement_path: root.path().join("startup-ack"),
            cancellation_path: cancellation_path.clone(),
            result_path: root.path().join("last-result.json"),
            helper_log_path: root.path().join("last-helper.log"),
        },
    });
    service.state = UpdateState::Installing {
        release: release.clone(),
    };

    assert_eq!(service.restore_ready_after_cancel(), Ok(true));
    assert_eq!(service.restore_ready_after_cancel(), Ok(false));

    assert!(cancellation_path.is_file());
    assert_eq!(std::fs::read(&cancellation_path).unwrap(), b"cancelled\n");
    assert!(matches!(
        service.state,
        UpdateState::Ready {
            artifact_path: ref path,
            ..
        } if path == &artifact_path
    ));
}

#[test]
fn failed_pending_install_cancellation_does_not_restore_ready() {
    let root = tempfile::tempdir().unwrap();
    let release = release_fixture();
    let artifact_path = root.path().join("artifact.ready");
    let blocked_parent = root.path().join("not-a-directory");
    std::fs::write(&blocked_parent, b"not a directory").unwrap();
    let mut service = UpdateService::new(root.path().to_path_buf(), true);
    service.pending_install = Some(PendingInstall {
        release: release.clone(),
        artifact_path: artifact_path.clone(),
        plan: ApplyPlanV1 {
            schema_version: ApplyPlanV1::SCHEMA_VERSION,
            parent_pid: 0,
            current_version: release.current_version.clone(),
            target_version: release.version.clone(),
            artifact_path: artifact_path.clone(),
            artifact_url: release.artifact_url.clone(),
            artifact_size: release.artifact_size,
            artifact_sha256: release.artifact_sha256.clone(),
            artifact_format: release.artifact_format.as_protocol_name().to_owned(),
            signed_envelope_path: root.path().join("manifest.envelope.json"),
            target_path: root.path().join("gmark.AppImage"),
            backup_path: root.path().join("gmark.AppImage.gmark-update-backup"),
            relaunch_path: root.path().join("gmark.AppImage"),
            acknowledgement_path: root.path().join("startup-ack"),
            cancellation_path: blocked_parent.join("cancel-install"),
            result_path: root.path().join("last-result.json"),
            helper_log_path: root.path().join("last-helper.log"),
        },
    });
    service.state = UpdateState::Installing {
        release: release.clone(),
    };

    assert!(service.restore_ready_after_cancel().is_err());
    assert!(matches!(
        service.state,
        UpdateState::Failed {
            release: Some(_),
            retryable: false,
            ..
        }
    ));
    assert!(service.pending_install.is_some());
}

#[test]
fn apply_result_is_presented_once_without_deleting_diagnostics() {
    let root = tempfile::tempdir().unwrap();
    let result_path = root.path().join("last-result.json");
    std::fs::write(
        &result_path,
        br#"{"schema_version":1,"status":"succeeded","to_version":"1.1.0","message":"ok"}"#,
    )
    .unwrap();
    assert!(matches!(
        restored_startup_state(root.path()),
        Some(UpdateState::Succeeded { version, .. }) if version == "1.1.0"
    ));
    assert!(restored_startup_state(root.path()).is_none());
    assert!(result_path.is_file());
}

#[test]
fn unknown_legacy_apply_result_status_is_presented_as_failed() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("last-result.json"),
        br#"{"schema_version":1,"status":"interrupted","to_version":"1.1.0","message":"legacy result","extra":true}"#,
    )
    .unwrap();

    assert!(matches!(
        restored_startup_state(root.path()),
        Some(UpdateState::Failed {
            release: None,
            message,
            retryable: false,
        }) if message == "legacy result"
    ));
}

#[test]
fn oversized_cached_apply_result_is_ignored() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("last-result.json"),
        vec![b'x'; 64 * 1024 + 1],
    )
    .unwrap();

    assert!(restored_startup_state(root.path()).is_none());
}

#[test]
fn staged_helper_is_reverified_before_helper_launch() {
    let root = tempfile::tempdir().unwrap();
    let transaction = root.path().join("v1.1.0");
    std::fs::create_dir(&transaction).unwrap();
    let installed_helper = root.path().join("installed-helper");
    std::fs::write(&installed_helper, b"trusted helper").unwrap();

    let staged = stage_update_helper(&transaction, &installed_helper).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        std::fs::set_permissions(&staged.path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(&staged.path, b"replaced helper").unwrap();

    assert!(verify_staged_helper_for_launch(&staged).is_err());
}

#[cfg(windows)]
#[test]
fn staged_helper_launch_guard_keeps_windows_execution_compatible() {
    let root = tempfile::tempdir().unwrap();
    let transaction = root.path().join("v1.1.0");
    std::fs::create_dir(&transaction).unwrap();
    let system_root = std::env::var_os("SystemRoot").unwrap();
    let command_shell = PathBuf::from(system_root).join("System32/cmd.exe");
    let staged = stage_update_helper(&transaction, &command_shell).unwrap();
    let guard = verify_staged_helper_for_launch(&staged).unwrap();

    let status = std::process::Command::new(&staged.path)
        .args(["/C", "exit", "0"])
        .status()
        .unwrap();
    drop(guard);

    assert!(status.success());
}

#[cfg(windows)]
#[test]
fn staged_helper_launch_guard_blocks_mutation_until_drop() {
    let root = tempfile::tempdir().unwrap();
    let transaction = root.path().join("v1.1.0");
    std::fs::create_dir(&transaction).unwrap();
    let system_root = std::env::var_os("SystemRoot").unwrap();
    let command_shell = PathBuf::from(system_root).join("System32/cmd.exe");
    let staged = stage_update_helper(&transaction, &command_shell).unwrap();
    let guard = verify_staged_helper_for_launch(&staged).unwrap();
    let renamed = transaction.join("cleanup-helper.exe");

    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(&staged.path)
            .is_err()
    );
    assert!(std::fs::remove_file(&staged.path).is_err());
    assert!(std::fs::rename(&staged.path, &renamed).is_err());

    drop(guard);

    std::fs::rename(&staged.path, &renamed).unwrap();
    std::fs::remove_file(renamed).unwrap();
}

#[cfg(windows)]
#[test]
fn staged_helper_launch_guard_rejects_a_reparse_leaf() {
    let root = tempfile::tempdir().unwrap();
    let transaction = root.path().join("v1.1.0");
    std::fs::create_dir(&transaction).unwrap();
    let command_shell =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/cmd.exe");
    let staged = stage_update_helper(&transaction, &command_shell).unwrap();
    let original = transaction.join("verified-helper.exe");
    std::fs::rename(&staged.path, &original).unwrap();
    if let Err(error) = std::os::windows::fs::symlink_file(&original, &staged.path) {
        std::fs::rename(&original, &staged.path).unwrap();
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || error.raw_os_error() == Some(1314)
        {
            return;
        }
        panic!("failed to create staged-helper symlink: {error}");
    }

    assert!(verify_staged_helper_for_launch(&staged).is_err());
    std::fs::remove_file(&staged.path).unwrap();
    std::fs::rename(original, &staged.path).unwrap();
}

fn release_fixture() -> UpdateRelease {
    UpdateRelease {
        current_version: "1.0.0".to_owned(),
        version: "1.1.0".to_owned(),
        published_at: "2026-07-22T00:00:00Z".to_owned(),
        notes: "Release notes".to_owned(),
        release_url: "https://github.com/kongweiguang/gmark/releases/tag/v1.1.0".to_owned(),
        artifact_url: "https://github.com/kongweiguang/gmark/releases/download/v1.1.0/gmark.exe"
            .to_owned(),
        artifact_size: 100,
        artifact_sha256: "00".repeat(32),
        artifact_format: if cfg!(target_os = "windows") {
            update_v2::ArtifactFormat::WindowsSetupExe
        } else if cfg!(target_os = "macos") {
            update_v2::ArtifactFormat::MacosAppTarGz
        } else {
            update_v2::ArtifactFormat::LinuxAppImage
        },
        system_trust: update_v2::SystemTrust::Unsigned,
        signed_envelope: std::sync::Arc::from(&b"{}"[..]),
    }
}
