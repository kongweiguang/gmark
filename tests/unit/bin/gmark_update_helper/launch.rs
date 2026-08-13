// @author kongweiguang

use super::*;
use gmark_update_core::ApplyFeedbackModeV1;
use uuid::Uuid;

fn plan(root: &Path) -> ApplyPlanV2 {
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
        expected_install_root: root.join("gmark.AppImage"),
        target_path: root.join("gmark.AppImage"),
        backup_path: root.join("gmark.AppImage.gmark-update-backup-id"),
        relaunch_path: root.join("gmark.AppImage"),
        acknowledgement_path: transaction.join("startup-ack"),
        cancellation_path: transaction.join("cancel-install"),
        result_path: transaction.join("result.json"),
        helper_log_path: transaction.join("helper.log"),
        transaction_id: Uuid::new_v4(),
        lifetime_lock_path: transaction.join("lifetime.lock"),
        progress_path: transaction.join("progress.json"),
        installer_log_path: transaction.join("installer.log"),
        feedback_mode: ApplyFeedbackModeV1::ProgressFile,
    }
}

fn child_that_waits() -> Child {
    // Keep the fixture short-lived because confirmation failures intentionally
    // leave the updated process running for the user to inspect.
    #[cfg(unix)]
    {
        return Command::new("sh")
            .args(["-c", "sleep 0.5"])
            .spawn()
            .expect("sleep child");
    }
    #[cfg(windows)]
    {
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Milliseconds 500",
            ])
            .spawn()
            .expect("sleep child")
    }
}

#[test]
fn confirmation_timeout_is_bounded() {
    assert_eq!(STARTUP_CONFIRMATION_TIMEOUT.as_secs(), 30);
    #[cfg(not(target_os = "windows"))]
    assert_eq!(AGENT_DELAY.as_millis(), 700);
}

#[test]
fn confirmation_failure_leaves_updated_child_running() {
    let root = tempfile::tempdir().expect("temporary root");
    let mut plan = plan(root.path());
    plan.feedback_mode = ApplyFeedbackModeV1::ProgressFile;
    fs::write(&plan.acknowledgement_path, b"wrong\n").expect("stale acknowledgement");
    let child = child_that_waits();
    let error = confirm_startup(&plan, child).expect_err("wrong acknowledgement");
    assert!(error.contains("does not match"));
}

#[cfg(target_os = "windows")]
#[test]
fn windows_feedback_agent_mode_does_not_require_an_agent_binary() {
    let root = tempfile::tempdir().expect("temporary root");
    let mut plan = plan(root.path());
    plan.feedback_mode = ApplyFeedbackModeV1::Agent;

    // Inno Setup owns Windows progress presentation, so this path must not
    // inspect or launch a second feedback executable.
    let mut feedback = FeedbackAgent::schedule(&plan).expect("Windows feedback is disabled");
    feedback.mark_successful();
}

#[cfg(unix)]
#[test]
fn unix_feedback_keeps_the_minimal_agent_contract() {
    // Unix/macOS owns the lightweight progress surface, even when the
    // compatibility feedback enum still carries its older value.
    let root = tempfile::tempdir().expect("temporary root");
    let mut plan = plan(root.path());
    plan.feedback_mode = ApplyFeedbackModeV1::ProgressFile;
    let agent = plan
        .artifact_path
        .parent()
        .expect("transaction directory")
        .join("gmark-update-agent");
    fs::write(&agent, b"#!/bin/sh\nexit 0\n").expect("feedback agent fixture");

    let mut feedback = FeedbackAgent::schedule(&plan).expect("Unix feedback agent");
    assert!(feedback.launch.is_some());
    feedback.mark_successful();
}
