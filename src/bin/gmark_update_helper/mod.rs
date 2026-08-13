// @author kongweiguang

//! Transactional ApplyPlanV2 executor.
//!
//! The helper deliberately has no process-table dependency.  The application
//! owns `lifetime.lock` while it is alive; the helper takes that same advisory
//! lock and keeps the handle open through the terminal result.  Every public
//! protocol snapshot is validated by `gmark-update-core` before an atomic
//! replacement is committed.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write as _},
    path::Path,
    time::Instant,
};

use gmark_update_core::{
    ApplyFailureCode, ApplyPhaseV1, ApplyPlanV2, ApplyResultV2, Platform, RecoveryAction,
    StagedApplyArtifact, read_apply_result_v2, read_validated_apply_plan_v2,
    stage_and_verify_apply_plan_artifact_v2, validate_apply_plan_v2_files,
    verifying_key_from_base64, write_apply_result_for_plan,
};

mod launch;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod transaction;
#[cfg(target_os = "windows")]
mod windows;

pub use launch::{
    FeedbackAgent, STARTUP_CONFIRMATION_TIMEOUT, confirm_startup, launch_updated,
    relaunch_previous_after_rollback, stop_child,
};
pub use transaction::{
    LIFETIME_LOCK_TIMEOUT, LOCK_POLL, LifetimeLock, LockError, ProgressWriter,
    acquire_lifetime_lock, acquire_lifetime_lock_until,
    acquire_lifetime_lock_until_with_cancellation, cancellation_requested,
    wait_for_lifecycle_lock_until,
};

/// A bounded, human-readable helper failure retained until result persistence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V2Failure {
    pub code: ApplyFailureCode,
    pub recovery_action: RecoveryAction,
    pub message: String,
    /// True once an installed target may need restoration.
    pub rollback_required: bool,
}

impl V2Failure {
    #[must_use]
    pub fn new(
        code: ApplyFailureCode,
        recovery_action: RecoveryAction,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            recovery_action,
            message: bound_message(message.into()),
            rollback_required: false,
        }
    }

    #[must_use]
    pub fn after_install(mut self) -> Self {
        self.rollback_required = true;
        self
    }

    #[must_use]
    pub fn rollback_failed(mut self, detail: impl Into<String>) -> Self {
        self.code = ApplyFailureCode::RollbackFailed;
        self.recovery_action = RecoveryAction::Manual;
        self.rollback_required = false;
        self.message = bound_message(format!(
            "{}; rollback failed: {}",
            self.message,
            detail.into()
        ));
        self
    }
}

#[derive(Debug)]
pub enum V2RunError {
    Untrusted(String),
    Trusted {
        plan: Box<ApplyPlanV2>,
        failure: V2Failure,
    },
}

impl std::fmt::Display for V2Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// Executes only schema-2 plans.  A malformed/untrusted plan is returned
/// before any transaction-local file is touched.
pub fn run_v2(plan_path: &Path) -> Result<(), V2RunError> {
    let plan = read_validated_apply_plan_v2(plan_path, &Platform::current())
        .map_err(|error| V2RunError::Untrusted(error.to_string()))?;
    validate_apply_plan_v2_files(&plan, &Platform::current())
        .map_err(|error| V2RunError::Untrusted(error.to_string()))?;

    match execute_v2(plan_path, &plan) {
        Ok(()) => Ok(()),
        Err(failure) => Err(V2RunError::Trusted {
            plan: Box::new(plan),
            failure,
        }),
    }
}

/// Compatibility spelling for callers that used the helper's original `run`
/// entry point.  It now always enforces the schema-2 transaction contract.
pub fn run(plan_path: &Path) -> Result<(), V2RunError> {
    run_v2(plan_path)
}

pub fn run_validated_plan(plan_path: &Path, plan: &ApplyPlanV2) -> Result<(), V2Failure> {
    execute_v2(plan_path, plan)
}

fn execute_v2(plan_path: &Path, plan: &ApplyPlanV2) -> Result<(), V2Failure> {
    let mut progress = ProgressWriter::new(plan);
    progress
        .publish(
            ApplyPhaseV1::WaitingForExit,
            "Waiting for Gmark to release the update lock",
        )
        .map_err(|error| failure_io(ApplyFailureCode::HelperLaunchFailed, error))?;

    let lock = match acquire_lifetime_lock_until_with_cancellation(
        &plan.lifetime_lock_path,
        &plan.cancellation_path,
        Instant::now() + LIFETIME_LOCK_TIMEOUT,
        LOCK_POLL,
    ) {
        Ok(lock) => lock,
        Err(LockError::Timeout) => {
            let failure = V2Failure::new(
                ApplyFailureCode::WaitingForExitTimeout,
                RecoveryAction::ReattemptInstall,
                "timed out waiting for Gmark to release the update lock",
            );
            persist_failure(plan, &mut progress, failure.clone());
            return Err(failure);
        }
        Err(LockError::Cancelled) => {
            let failure = V2Failure::new(
                ApplyFailureCode::Cancelled,
                RecoveryAction::ReattemptInstall,
                "installation was cancelled while waiting for Gmark to release the update lock",
            );
            persist_failure(plan, &mut progress, failure.clone());
            return Err(failure);
        }
        Err(LockError::Path(error)) => {
            let failure = V2Failure::new(
                ApplyFailureCode::PathViolation,
                RecoveryAction::Recheck,
                error,
            );
            persist_failure(plan, &mut progress, failure.clone());
            return Err(failure);
        }
        Err(LockError::Io(error)) => {
            let failure = V2Failure::new(
                ApplyFailureCode::HelperLaunchFailed,
                RecoveryAction::Recheck,
                format!("failed to acquire update lock: {error}"),
            );
            persist_failure(plan, &mut progress, failure.clone());
            return Err(failure);
        }
    };

    // A terminal result belongs to this transaction forever.  Refusing a
    // second invocation prevents replaying an installer after a successful
    // update or after a deliberate failure.
    match fs::symlink_metadata(&plan.result_path) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            match read_apply_result_v2(&plan.result_path) {
                Ok(existing) if existing.transaction_id == plan.transaction_id => {
                    let failure = V2Failure::new(
                        ApplyFailureCode::InvalidPlan,
                        RecoveryAction::Recheck,
                        "update transaction already has a terminal result",
                    );
                    let _ = lock;
                    return Err(failure);
                }
                Ok(_) => {
                    let failure = V2Failure::new(
                        ApplyFailureCode::InvalidPlan,
                        RecoveryAction::Recheck,
                        "update transaction result belongs to another transaction",
                    );
                    let _ = lock;
                    return Err(failure);
                }
                Err(error) => {
                    let failure = V2Failure::new(
                        ApplyFailureCode::InvalidPlan,
                        RecoveryAction::Recheck,
                        format!("existing update result is invalid: {error}"),
                    );
                    let _ = lock;
                    return Err(failure);
                }
            }
        }
        Ok(_) => {
            let failure = V2Failure::new(
                ApplyFailureCode::PathViolation,
                RecoveryAction::Recheck,
                "update result is not a regular non-link file",
            );
            let _ = lock;
            return Err(failure);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            let failure = V2Failure::new(
                ApplyFailureCode::PathViolation,
                RecoveryAction::Recheck,
                format!("failed to inspect existing update result: {error}"),
            );
            let _ = lock;
            return Err(failure);
        }
    }

    let cancelled = match cancellation_requested(&plan.cancellation_path) {
        Ok(cancelled) => cancelled,
        Err(error) => {
            let failure = failure_io(ApplyFailureCode::PathViolation, error);
            persist_failure(plan, &mut progress, failure.clone());
            return Err(failure);
        }
    };
    if cancelled {
        let failure = V2Failure::new(
            ApplyFailureCode::Cancelled,
            RecoveryAction::ReattemptInstall,
            "installation was cancelled before preparation",
        );
        persist_failure(plan, &mut progress, failure.clone());
        return Err(failure);
    }

    if let Err(error) = progress.publish(
        ApplyPhaseV1::Preparing,
        "Verifying the signed update artifact",
    ) {
        let failure = failure_io(ApplyFailureCode::HelperLaunchFailed, error);
        persist_failure(plan, &mut progress, failure.clone());
        return Err(failure);
    }
    let staging_directory = match artifact_staging_directory(plan) {
        Ok(directory) => directory,
        Err(error) => {
            let failure = failure_io(ApplyFailureCode::PathViolation, error);
            persist_failure(plan, &mut progress, failure.clone());
            return Err(failure);
        }
    };
    let key = match embedded_verifying_key() {
        Ok(key) => key,
        Err(error) => {
            let failure = failure_io(ApplyFailureCode::ArtifactVerificationFailed, error);
            persist_failure(plan, &mut progress, failure.clone());
            return Err(failure);
        }
    };
    let mut artifact = match stage_and_verify_apply_plan_artifact_v2(
        plan,
        &key,
        &Platform::current(),
        staging_directory,
    ) {
        Ok(artifact) => artifact,
        Err(error) => {
            let text = error.to_string();
            let code = if text.to_ascii_lowercase().contains("space") {
                ApplyFailureCode::DiskSpaceInsufficient
            } else if text.to_ascii_lowercase().contains("path")
                || text.to_ascii_lowercase().contains("symlink")
                || text.to_ascii_lowercase().contains("reparse")
            {
                ApplyFailureCode::PathViolation
            } else {
                ApplyFailureCode::ArtifactVerificationFailed
            };
            let failure = failure_io(code, text);
            persist_failure(plan, &mut progress, failure.clone());
            return Err(failure);
        }
    };

    if let Err(error) = progress.publish(ApplyPhaseV1::Installing, "Installing the verified update")
    {
        let failure = failure_io(ApplyFailureCode::HelperLaunchFailed, error);
        persist_failure(plan, &mut progress, failure.clone());
        return Err(failure);
    }
    let cancelled = match cancellation_requested(&plan.cancellation_path) {
        Ok(cancelled) => cancelled,
        Err(error) => {
            let failure = failure_io(ApplyFailureCode::PathViolation, error);
            persist_failure(plan, &mut progress, failure.clone());
            return Err(failure);
        }
    };
    if cancelled {
        let failure = V2Failure::new(
            ApplyFailureCode::Cancelled,
            RecoveryAction::ReattemptInstall,
            "installation was cancelled before applying the update",
        );
        persist_failure(plan, &mut progress, failure.clone());
        return Err(failure);
    }

    let mut feedback = match FeedbackAgent::schedule(plan) {
        Ok(feedback) => feedback,
        Err(error) => {
            append_log(plan, &format!("feedback agent unavailable: {error}"));
            FeedbackAgent::disabled()
        }
    };
    let backup_preexisting = fs::symlink_metadata(&plan.backup_path).is_ok();
    if let Err(error) = install_platform(plan, &mut artifact) {
        let code = classify_install_failure(&error);
        let failure = V2Failure::new(
            code,
            if code == ApplyFailureCode::PathViolation {
                RecoveryAction::Manual
            } else {
                RecoveryAction::ReattemptInstall
            },
            error,
        );
        // A platform adapter rejects a pre-existing UUID backup before any
        // install side effect. Never treat that refusal as a signal to
        // restore an unrelated backup left by another transaction.
        let backup_created = !backup_preexisting && fs::symlink_metadata(&plan.backup_path).is_ok();
        if !backup_created {
            persist_failure(plan, &mut progress, failure.clone());
            return Err(failure);
        }
        let _ = progress.publish(
            ApplyPhaseV1::RollingBack,
            "Restoring the previous Gmark installation after installer failure",
        );
        let failure = match rollback_platform(plan) {
            Err(error) => failure.rollback_failed(error),
            Ok(()) => match relaunch_previous_after_rollback(plan) {
                Ok(()) => failure,
                Err(error) => failure.rollback_failed(format!(
                    "rollback succeeded but old Gmark could not be relaunched: {error}"
                )),
            },
        };
        persist_failure(plan, &mut progress, failure.clone());
        return Err(failure);
    }
    if let Err(error) = progress.publish(ApplyPhaseV1::Relaunching, "Launching the updated Gmark") {
        let failure = failure_io(ApplyFailureCode::HelperLaunchFailed, error);
        persist_failure(plan, &mut progress, failure.clone());
        return Err(failure);
    }
    let mut child = match launch_updated(plan) {
        Ok(child) => child,
        Err(error) => {
            let failure = recover_after_launch_failure(
                plan,
                &mut progress,
                V2Failure::new(
                    ApplyFailureCode::RelaunchFailed,
                    RecoveryAction::ReattemptInstall,
                    error,
                )
                .after_install(),
            );
            return Err(failure);
        }
    };

    if let Err(error) = progress.publish(
        ApplyPhaseV1::Confirming,
        "Waiting for the updated Gmark startup acknowledgement",
    ) {
        let stop = stop_child(&mut child);
        let message = match stop {
            Ok(()) => error,
            Err(stop_error) => format!("{error}; failed to stop updated gmark: {stop_error}"),
        };
        let failure = recover_after_launch_failure(
            plan,
            &mut progress,
            V2Failure::new(
                ApplyFailureCode::HelperLaunchFailed,
                RecoveryAction::Recheck,
                message,
            )
            .after_install(),
        );
        return Err(failure);
    }
    if let Err(error) = confirm_startup(plan, child) {
        let failure = recover_after_launch_failure(
            plan,
            &mut progress,
            V2Failure::new(
                if error.contains("timed out") {
                    ApplyFailureCode::StartupConfirmationTimeout
                } else {
                    ApplyFailureCode::RelaunchFailed
                },
                RecoveryAction::ReattemptInstall,
                error,
            )
            .after_install(),
        );
        return Err(failure);
    }

    // The result and terminal progress are durable before any best-effort
    // cleanup.  Keep the lock alive until both atomic writes have completed.
    progress
        .publish(ApplyPhaseV1::Succeeded, "Update installed and acknowledged")
        .map_err(|error| failure_io(ApplyFailureCode::HelperLaunchFailed, error))?;
    let result = ApplyResultV2::succeeded(
        plan.transaction_id,
        plan.current_version.clone(),
        plan.target_version.clone(),
    );
    write_apply_result_for_plan(plan, &result)
        .map_err(|error| failure_io(ApplyFailureCode::HelperLaunchFailed, error.to_string()))?;
    feedback.mark_successful();
    let _ = lock;
    let _ = cleanup_backup(plan);
    let _ = fs::remove_file(plan_path);
    Ok(())
}

fn persist_failure(plan: &ApplyPlanV2, progress: &mut ProgressWriter<'_>, failure: V2Failure) {
    let _ = progress.publish(ApplyPhaseV1::Failed, &failure.message);
    let result = ApplyResultV2::failed(
        plan.transaction_id,
        plan.current_version.clone(),
        plan.target_version.clone(),
        failure.code,
        failure.recovery_action,
        failure.message.clone(),
    );
    let _ = write_apply_result_for_plan(plan, &result);
    append_log(
        plan,
        &format!("failed ({:?}): {}", failure.code, failure.message),
    );
}

pub fn report_v2_failure(plan: &ApplyPlanV2, failure: &V2Failure) {
    append_log(
        plan,
        &format!("failed ({:?}): {}", failure.code, failure.message),
    );
    // The execution path persists its result before returning.  This write is
    // deliberately best-effort for failures that occurred before a lock was
    // acquired or while another invocation owns the transaction.
    if read_apply_result_v2(&plan.result_path).is_err() {
        let result = ApplyResultV2::failed(
            plan.transaction_id,
            plan.current_version.clone(),
            plan.target_version.clone(),
            failure.code,
            failure.recovery_action,
            failure.message.clone(),
        );
        let _ = write_apply_result_for_plan(plan, &result);
    }
}

fn recover_after_launch_failure(
    plan: &ApplyPlanV2,
    progress: &mut ProgressWriter<'_>,
    failure: V2Failure,
) -> V2Failure {
    let _ = progress.publish(
        ApplyPhaseV1::RollingBack,
        "Restoring the previous Gmark installation",
    );
    match rollback_platform(plan) {
        Ok(()) => {
            // Do not relaunch the old binary until the entire rollback has
            // completed.  A mixed installation must never be advertised as
            // recoverable.
            match relaunch_previous_after_rollback(plan) {
                Ok(()) => {
                    persist_failure(plan, progress, failure.clone());
                    failure
                }
                Err(error) => {
                    let failure = failure.rollback_failed(format!(
                        "rollback succeeded but old Gmark could not be relaunched: {error}"
                    ));
                    persist_failure(plan, progress, failure.clone());
                    failure
                }
            }
        }
        Err(error) => {
            let failure = failure.rollback_failed(error);
            persist_failure(plan, progress, failure.clone());
            failure
        }
    }
}

fn failure_io(code: ApplyFailureCode, message: impl Into<String>) -> V2Failure {
    V2Failure::new(code, RecoveryAction::Recheck, message)
}

fn classify_install_failure(message: &str) -> ApplyFailureCode {
    let lower = message.to_ascii_lowercase();
    if lower.contains("space") {
        ApplyFailureCode::DiskSpaceInsufficient
    } else if lower.contains("path")
        || lower.contains("symlink")
        || lower.contains("reparse")
        || lower.contains("backup already exists")
        || lower.contains("transaction-owned")
    {
        ApplyFailureCode::PathViolation
    } else if lower.contains("start") || lower.contains("launch") {
        ApplyFailureCode::InstallerLaunchFailed
    } else {
        ApplyFailureCode::InstallerFailed
    }
}

fn bound_message(message: String) -> String {
    if message.len() <= gmark_update_core::MAX_APPLY_MESSAGE_BYTES {
        message
    } else {
        let mut end = gmark_update_core::MAX_APPLY_MESSAGE_BYTES;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &message[..end.saturating_sub(3)])
    }
}

fn embedded_verifying_key() -> Result<ed25519_dalek::VerifyingKey, String> {
    let encoded = option_env!("GMARK_UPDATE_PUBLIC_KEY_BASE64")
        .ok_or_else(|| "update helper has no embedded verification key".to_owned())?;
    verifying_key_from_base64(encoded).map_err(|error| error.to_string())
}

fn artifact_staging_directory(plan: &ApplyPlanV2) -> Result<&Path, String> {
    plan.artifact_path
        .parent()
        .ok_or_else(|| "update artifact has no transaction directory".to_owned())
}

fn append_log(plan: &ApplyPlanV2, message: &str) {
    let Ok(parent) = plan.helper_log_path.parent().ok_or(()) else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        #[cfg(target_os = "linux")]
        options.custom_flags(0x2_0000); // O_NOFOLLOW
        #[cfg(target_os = "macos")]
        options.custom_flags(0x100); // O_NOFOLLOW
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let Ok(mut file) = options.open(&plan.helper_log_path) else {
        return;
    };
    let _ = writeln!(file, "{message}");
    let _ = file.flush();
    let _ = file.sync_all();
}

fn cleanup_backup(plan: &ApplyPlanV2) -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        if fs::symlink_metadata(&plan.backup_path)
            .map(|metadata| metadata.file_type().is_dir() && !metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            fs::remove_dir_all(&plan.backup_path)
                .map_err(|error| format!("failed to remove update backup: {error}"))?;
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        match fs::symlink_metadata(&plan.backup_path) {
            Ok(metadata)
                if metadata.file_type().is_file() && !metadata.file_type().is_symlink() =>
            {
                fs::remove_file(&plan.backup_path)
                    .map_err(|error| format!("failed to remove update backup: {error}"))?;
            }
            Ok(_) => return Err("update backup is not a regular file".to_owned()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("failed to inspect update backup: {error}")),
        }
        Ok(())
    }
}

fn install_platform(plan: &ApplyPlanV2, artifact: &mut StagedApplyArtifact) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::install(plan, artifact)
    }
    #[cfg(target_os = "macos")]
    {
        macos::install(plan, artifact)
    }
    #[cfg(target_os = "linux")]
    {
        linux::install(plan, artifact)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = (plan, artifact);
        Err("unsupported update platform".to_owned())
    }
}

fn rollback_platform(plan: &ApplyPlanV2) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::rollback(plan)
    }
    #[cfg(target_os = "macos")]
    {
        macos::rollback(plan)
    }
    #[cfg(target_os = "linux")]
    {
        linux::rollback(plan)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = plan;
        Err("unsupported update platform".to_owned())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/bin/gmark_update_helper/helper.rs"]
mod tests;
