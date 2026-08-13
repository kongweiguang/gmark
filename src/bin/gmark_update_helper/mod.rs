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
    path::{Path, PathBuf},
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

pub use launch::{FeedbackAgent, STARTUP_CONFIRMATION_TIMEOUT, confirm_startup, launch_updated};
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstallCommitState {
    Uncommitted,
    CommittedOrUnknown,
}

/// 平台安装器必须显式标记错误是否发生在提交边界之后，避免 UI 建议重复执行可能已经生效的安装。
#[derive(Debug)]
struct PlatformInstallFailure {
    commit_state: InstallCommitState,
    message: String,
}

impl PlatformInstallFailure {
    /// 提交前失败可安全创建新事务重试，因为可见安装目标尚未改变。
    fn uncommitted(message: impl Into<String>) -> Self {
        Self {
            commit_state: InstallCommitState::Uncommitted,
            message: message.into(),
        }
    }

    /// 提交后或安装器状态不确定时必须转人工恢复，禁止假设旧版本仍完整存在。
    fn committed_or_unknown(message: impl Into<String>) -> Self {
        Self {
            commit_state: InstallCommitState::CommittedOrUnknown,
            message: message.into(),
        }
    }
}

impl From<String> for PlatformInstallFailure {
    fn from(message: String) -> Self {
        Self::uncommitted(message)
    }
}

impl V2Failure {
    /// Bounds helper diagnostics before they cross the result/progress
    /// protocol, keeping recovery guidance readable and serializable.
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
        }
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

/// Executes a plan that was already validated by the caller while retaining
/// the same lock, verification, and terminal-result guarantees as `run_v2`.
pub fn run_validated_plan(plan_path: &Path, plan: &ApplyPlanV2) -> Result<(), V2Failure> {
    execute_v2(plan_path, plan)
}

/// Owns the helper transaction from lifecycle-lock acquisition through the
/// durable result so a committed installation is never silently undone.
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

    if let Err(error) = claim_install_attempt(plan) {
        let failure = V2Failure::new(
            ApplyFailureCode::InvalidPlan,
            RecoveryAction::Recheck,
            error,
        );
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
    if let Err(error) = install_platform(plan, &mut artifact) {
        let code = classify_install_failure(&error.message);
        let failure = if error.commit_state == InstallCommitState::CommittedOrUnknown {
            post_install_failure(plan, code, error.message)
        } else {
            V2Failure::new(
                code,
                if code == ApplyFailureCode::PathViolation {
                    RecoveryAction::Manual
                } else {
                    RecoveryAction::ReattemptInstall
                },
                error.message,
            )
        };
        persist_failure(plan, &mut progress, failure.clone());
        return Err(failure);
    }
    if let Err(error) = progress.publish(ApplyPhaseV1::Relaunching, "Launching the updated Gmark") {
        let failure = post_install_failure(plan, ApplyFailureCode::HelperLaunchFailed, error);
        persist_failure(plan, &mut progress, failure.clone());
        return Err(failure);
    }
    let child = match launch_updated(plan) {
        Ok(child) => child,
        Err(error) => {
            let failure = post_install_failure(plan, ApplyFailureCode::RelaunchFailed, error);
            persist_failure(plan, &mut progress, failure.clone());
            return Err(failure);
        }
    };

    if let Err(error) = progress.publish(
        ApplyPhaseV1::Confirming,
        "Waiting for the updated Gmark startup acknowledgement",
    ) {
        let failure = post_install_failure(plan, ApplyFailureCode::HelperLaunchFailed, error);
        // Dropping the handle only detaches observation; it deliberately does
        // not terminate a process that may still finish starting normally.
        drop(child);
        persist_failure(plan, &mut progress, failure.clone());
        return Err(failure);
    }
    if let Err(error) = confirm_startup(plan, child) {
        let code = if error.contains("timed out") {
            ApplyFailureCode::StartupConfirmationTimeout
        } else {
            ApplyFailureCode::RelaunchFailed
        };
        let failure = post_install_failure(plan, code, error);
        persist_failure(plan, &mut progress, failure.clone());
        return Err(failure);
    }

    // 终态结果是权威事实，必须先于展示用途的 progress 落盘；progress 故障不能把成功安装改写成失败。
    let result = ApplyResultV2::succeeded(
        plan.transaction_id,
        plan.current_version.clone(),
        plan.target_version.clone(),
    );
    if let Err(error) = write_apply_result_for_plan(plan, &result) {
        let failure = post_install_failure(
            plan,
            ApplyFailureCode::HelperLaunchFailed,
            error.to_string(),
        );
        persist_failure(plan, &mut progress, failure.clone());
        return Err(failure);
    }
    if let Err(error) =
        progress.publish(ApplyPhaseV1::Succeeded, "Update installed and acknowledged")
    {
        append_log(
            plan,
            &format!(
                "terminal result persisted but success progress could not be updated: {error}"
            ),
        );
    }
    feedback.mark_successful();
    let _ = fs::remove_file(plan_path);
    Ok(())
}

/// Adds recovery details after commit because the new version must remain in
/// place and the user needs a copyable manual download and diagnostic path.
fn post_install_failure(
    plan: &ApplyPlanV2,
    code: ApplyFailureCode,
    message: impl Into<String>,
) -> V2Failure {
    V2Failure::new(
        code,
        RecoveryAction::Manual,
        format!(
            "{}; new version {} is installed and will not be rolled back automatically; manual action: open the installed Gmark or download the package; open helper log: {}; open installer log: {}; package: {}",
            message.into(),
            plan.target_version,
            plan.helper_log_path.display(),
            plan.installer_log_path.display(),
            plan.artifact_url,
        ),
    )
}

/// 单次执行声明在任何平台写入前使用 create_new 落盘；即使结果文件随后 I/O 失败，重复 helper 也不能再次运行安装器。
fn claim_install_attempt(plan: &ApplyPlanV2) -> Result<PathBuf, String> {
    let transaction_dir = plan
        .transaction_dir()
        .ok_or_else(|| "update plan has no transaction directory".to_owned())?;
    let path = transaction_dir.join("execution.claim");
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
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
    let mut file = options.open(&path).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            "update transaction execution was already claimed and cannot be replayed".to_owned()
        } else {
            format!("failed to claim update transaction execution: {error}")
        }
    })?;
    writeln!(file, "{}", plan.transaction_id)
        .map_err(|error| format!("failed to write update execution claim: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("failed to sync update execution claim: {error}"))?;
    Ok(path)
}

/// Persists a terminal failure even when progress output is unavailable, so
/// the next invocation cannot replay an already attempted transaction.
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

/// Records failures that happen before the main execution path can persist a
/// result, while never replacing an existing terminal transaction result.
pub fn report_v2_failure(plan: &ApplyPlanV2, failure: &V2Failure) {
    // The execution path persists its result before returning. This write is
    // deliberately best-effort for failures that occurred before a lock was
    // acquired or while another invocation owns the transaction.
    if read_apply_result_v2(&plan.result_path).is_err() {
        append_log(
            plan,
            &format!("failed ({:?}): {}", failure.code, failure.message),
        );
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

fn install_platform(
    plan: &ApplyPlanV2,
    artifact: &mut StagedApplyArtifact,
) -> Result<(), PlatformInstallFailure> {
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
        Err(PlatformInstallFailure::uncommitted(
            "unsupported update platform",
        ))
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/bin/gmark_update_helper/helper.rs"]
mod tests;
