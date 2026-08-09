// @author kongweiguang

//! Apply protocol v2 data model and compatibility aliases.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ApplyPlanV1;

mod io;

pub use io::*;

/// The feedback channel selected by the app when it hands a v2 transaction to
/// the helper.  `progress_file` is the normal production mode: the helper is
/// the only writer and the app/agent only reads the atomically replaced file.
/// `agent` is used when a separate UI process owns presentation, while `silent`
/// is useful for recovery transactions that must not open another window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyFeedbackModeV1 {
    #[serde(alias = "ProgressFile", alias = "progress-file", alias = "progress")]
    ProgressFile,
    #[serde(alias = "Agent", alias = "native")]
    Agent,
    #[serde(alias = "Silent")]
    Silent,
}

/// Short name for adapters that do not need the schema suffix.
pub type FeedbackModeV1 = ApplyFeedbackModeV1;
/// Compatibility alias used by early v2 integrations.
pub type FeedbackMode = ApplyFeedbackModeV1;

/// Monotonic lifecycle phases published by an update helper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyPhaseV1 {
    #[serde(alias = "WaitingForExit", alias = "waiting-for-exit")]
    WaitingForExit,
    #[serde(alias = "Preparing")]
    Preparing,
    #[serde(alias = "Installing")]
    Installing,
    #[serde(alias = "Relaunching")]
    Relaunching,
    #[serde(alias = "Confirming")]
    Confirming,
    #[serde(alias = "RollingBack", alias = "rolling-back")]
    RollingBack,
    #[serde(alias = "Succeeded")]
    Succeeded,
    #[serde(alias = "Failed")]
    Failed,
}

impl ApplyPhaseV1 {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

/// Stable machine-readable failure categories.  The helper must never encode
/// an unbounded platform error as this enum; the human detail belongs in the
/// bounded `message` field on [`ApplyResultV2`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyFailureCode {
    #[serde(
        alias = "ParentExitTimeout",
        alias = "parent-exit-timeout",
        alias = "timeout_waiting_for_exit"
    )]
    WaitingForExitTimeout,
    #[serde(alias = "HelperLaunchFailed", alias = "helper-launch-failed")]
    HelperLaunchFailed,
    #[serde(alias = "InstallerLaunchFailed", alias = "installer-launch-failed")]
    InstallerLaunchFailed,
    #[serde(alias = "InstallerFailed", alias = "installer-failed")]
    InstallerFailed,
    #[serde(alias = "RelaunchFailed", alias = "relaunch-failed")]
    RelaunchFailed,
    #[serde(
        alias = "StartupConfirmationTimeout",
        alias = "startup-confirmation-timeout",
        alias = "startup_timeout"
    )]
    StartupConfirmationTimeout,
    #[serde(alias = "RollbackFailed", alias = "rollback-failed")]
    RollbackFailed,
    #[serde(
        alias = "ArtifactVerificationFailed",
        alias = "artifact-verification-failed"
    )]
    ArtifactVerificationFailed,
    #[serde(alias = "InvalidPlan", alias = "invalid-plan")]
    InvalidPlan,
    #[serde(alias = "Cancelled", alias = "canceled")]
    Cancelled,
    #[serde(alias = "DiskSpaceInsufficient", alias = "disk-space-insufficient")]
    DiskSpaceInsufficient,
    #[serde(alias = "PathViolation", alias = "path-violation")]
    PathViolation,
}

impl ApplyFailureCode {
    /// Associated aliases keep older callers source-compatible while the wire
    /// value remains the canonical snake_case spelling.
    // 原因：旧调用方仍使用该驼峰常量名；当兼容窗口结束并迁移全部调用方后移除。
    #[allow(non_upper_case_globals)]
    pub const ParentExitTimeout: Self = Self::WaitingForExitTimeout;
    // 原因：旧调用方仍使用该驼峰常量名；当兼容窗口结束并迁移全部调用方后移除。
    #[allow(non_upper_case_globals)]
    pub const TimeoutWaitingForExit: Self = Self::WaitingForExitTimeout;
    // 原因：旧调用方仍使用该驼峰常量名；当兼容窗口结束并迁移全部调用方后移除。
    #[allow(non_upper_case_globals)]
    pub const InstallFailed: Self = Self::InstallerFailed;
    // 原因：旧调用方仍使用该驼峰常量名；当兼容窗口结束并迁移全部调用方后移除。
    #[allow(non_upper_case_globals)]
    pub const InstallationFailed: Self = Self::InstallerFailed;
    // 原因：旧调用方仍使用该驼峰常量名；当兼容窗口结束并迁移全部调用方后移除。
    #[allow(non_upper_case_globals)]
    pub const StartupTimeout: Self = Self::StartupConfirmationTimeout;
    // 原因：旧调用方仍使用该驼峰常量名；当兼容窗口结束并迁移全部调用方后移除。
    #[allow(non_upper_case_globals)]
    pub const StartupConfirmationFailed: Self = Self::StartupConfirmationTimeout;
    // 原因：旧调用方仍使用该驼峰常量名；当兼容窗口结束并迁移全部调用方后移除。
    #[allow(non_upper_case_globals)]
    pub const DiskSpace: Self = Self::DiskSpaceInsufficient;

    #[must_use]
    pub const fn recommended_recovery(self) -> RecoveryAction {
        match self {
            Self::ArtifactVerificationFailed => RecoveryAction::Redownload,
            Self::StartupConfirmationTimeout => RecoveryAction::Recheck,
            Self::InvalidPlan | Self::RollbackFailed | Self::Cancelled | Self::PathViolation => {
                RecoveryAction::Manual
            }
            Self::WaitingForExitTimeout
            | Self::HelperLaunchFailed
            | Self::InstallerLaunchFailed
            | Self::InstallerFailed
            | Self::RelaunchFailed
            | Self::DiskSpaceInsufficient => RecoveryAction::ReattemptInstall,
        }
    }
}

/// Recovery guidance displayed by the app after a failed transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    #[serde(
        alias = "ReattemptInstall",
        alias = "retry_install",
        alias = "retry-install"
    )]
    ReattemptInstall,
    #[serde(alias = "Redownload", alias = "re-download")]
    Redownload,
    #[serde(alias = "Recheck", alias = "re-check")]
    Recheck,
    #[serde(alias = "Manual", alias = "manual_install", alias = "manual-install")]
    Manual,
}

impl RecoveryAction {
    // 原因：旧调用方仍使用该驼峰常量名；当兼容窗口结束并迁移全部调用方后移除。
    #[allow(non_upper_case_globals)]
    pub const RetryInstall: Self = Self::ReattemptInstall;
    // 原因：旧调用方仍使用该驼峰常量名；当兼容窗口结束并迁移全部调用方后移除。
    #[allow(non_upper_case_globals)]
    pub const RetryDownload: Self = Self::Redownload;
}

/// A v2 plan extends the established v1 fields without changing their wire
/// names.  The extra protocol files are all transaction-local and their names
/// are fixed by [`validate_apply_plan_v2`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyPlanV2 {
    pub schema_version: u8,
    pub parent_pid: u32,
    pub current_version: String,
    pub target_version: String,
    pub artifact_path: PathBuf,
    pub artifact_url: String,
    pub artifact_size: u64,
    pub artifact_sha256: String,
    pub artifact_format: String,
    pub signed_envelope_path: PathBuf,
    /// Platform installation root verified by both the app and helper.
    pub expected_install_root: PathBuf,
    pub target_path: PathBuf,
    pub backup_path: PathBuf,
    pub relaunch_path: PathBuf,
    pub acknowledgement_path: PathBuf,
    pub cancellation_path: PathBuf,
    pub result_path: PathBuf,
    pub helper_log_path: PathBuf,
    pub transaction_id: uuid::Uuid,
    pub lifetime_lock_path: PathBuf,
    pub progress_path: PathBuf,
    pub installer_log_path: PathBuf,
    pub feedback_mode: ApplyFeedbackModeV1,
}

impl ApplyPlanV2 {
    pub const SCHEMA_VERSION: u8 = 2;
    pub const ARTIFACT_FILE_NAME: &'static str = "artifact.ready";
    pub const SIGNED_ENVELOPE_FILE_NAME: &'static str = "manifest.envelope.json";
    pub const PLAN_FILE_NAME: &'static str = "apply-plan.json";
    pub const ACKNOWLEDGEMENT_FILE_NAME: &'static str = "startup-ack";
    pub const CANCELLATION_FILE_NAME: &'static str = "cancel-install";
    pub const RESULT_FILE_NAME: &'static str = "result.json";
    pub const HELPER_LOG_FILE_NAME: &'static str = "helper.log";
    pub const LIFETIME_LOCK_FILE_NAME: &'static str = "lifetime.lock";
    pub const PROGRESS_FILE_NAME: &'static str = "progress.json";
    pub const INSTALLER_LOG_FILE_NAME: &'static str = "installer.log";
    pub const TRANSACTIONS_DIR_NAME: &'static str = "transactions";

    /// Builds an isolated v2 attempt beneath the downloaded version directory.
    ///
    /// Callers copy the already verified artifact and envelope into the
    /// returned paths before handoff. A UUID directory prevents two attempts
    /// of the same release from sharing mutable protocol files.
    #[must_use]
    pub fn from_v1(plan: &ApplyPlanV1, transaction_id: uuid::Uuid) -> Self {
        let version_dir = plan
            .artifact_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let transaction_dir = version_dir
            .join(Self::TRANSACTIONS_DIR_NAME)
            .join(transaction_id.hyphenated().to_string());
        let expected_install_root = match plan.artifact_format.as_str() {
            "windows-setup-exe" => plan
                .target_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| plan.target_path.clone()),
            _ => plan.target_path.clone(),
        };
        let backup_name = expected_install_root
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| format!("{name}.gmark-update-backup-{}", transaction_id.hyphenated()))
            .unwrap_or_else(|| format!("gmark-update-backup-{}", transaction_id.hyphenated()));
        let backup_path = expected_install_root
            .parent()
            .map(|parent| parent.join(&backup_name))
            .unwrap_or_else(|| PathBuf::from(backup_name));
        Self {
            schema_version: Self::SCHEMA_VERSION,
            parent_pid: plan.parent_pid,
            current_version: plan.current_version.clone(),
            target_version: plan.target_version.clone(),
            artifact_path: transaction_dir.join(Self::ARTIFACT_FILE_NAME),
            artifact_url: plan.artifact_url.clone(),
            artifact_size: plan.artifact_size,
            artifact_sha256: plan.artifact_sha256.clone(),
            artifact_format: plan.artifact_format.clone(),
            signed_envelope_path: transaction_dir.join(Self::SIGNED_ENVELOPE_FILE_NAME),
            expected_install_root,
            target_path: plan.target_path.clone(),
            backup_path,
            relaunch_path: plan.relaunch_path.clone(),
            acknowledgement_path: transaction_dir.join(Self::ACKNOWLEDGEMENT_FILE_NAME),
            cancellation_path: transaction_dir.join(Self::CANCELLATION_FILE_NAME),
            result_path: transaction_dir.join(Self::RESULT_FILE_NAME),
            helper_log_path: transaction_dir.join(Self::HELPER_LOG_FILE_NAME),
            transaction_id,
            lifetime_lock_path: transaction_dir.join(Self::LIFETIME_LOCK_FILE_NAME),
            progress_path: transaction_dir.join(Self::PROGRESS_FILE_NAME),
            installer_log_path: transaction_dir.join(Self::INSTALLER_LOG_FILE_NAME),
            feedback_mode: ApplyFeedbackModeV1::ProgressFile,
        }
    }

    #[must_use]
    pub fn transaction_dir(&self) -> Option<&Path> {
        self.artifact_path.parent()
    }

    fn projection_for_artifact(&self) -> ApplyPlanV1 {
        let transaction_dir = self.transaction_dir().unwrap_or_else(|| Path::new("."));
        let updates_root = transaction_dir.parent().unwrap_or(transaction_dir);
        let legacy_backup_path = self
            .target_path
            .parent()
            .zip(self.target_path.file_name().and_then(|name| name.to_str()))
            .map(|(parent, name)| parent.join(format!("{name}.gmark-update-backup")))
            .unwrap_or_else(|| self.backup_path.clone());
        ApplyPlanV1 {
            schema_version: ApplyPlanV1::SCHEMA_VERSION,
            parent_pid: self.parent_pid,
            current_version: self.current_version.clone(),
            target_version: self.target_version.clone(),
            artifact_path: self.artifact_path.clone(),
            artifact_url: self.artifact_url.clone(),
            artifact_size: self.artifact_size,
            artifact_sha256: self.artifact_sha256.clone(),
            artifact_format: self.artifact_format.clone(),
            signed_envelope_path: self.signed_envelope_path.clone(),
            target_path: self.target_path.clone(),
            backup_path: legacy_backup_path,
            relaunch_path: self.relaunch_path.clone(),
            acknowledgement_path: self.acknowledgement_path.clone(),
            cancellation_path: self.cancellation_path.clone(),
            result_path: updates_root.join("last-result.json"),
            helper_log_path: updates_root.join("last-helper.log"),
        }
    }
}

/// Atomic helper progress snapshot.  The transaction id prevents a stale file
/// from a previous update being displayed as current progress.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyProgressV1 {
    pub schema_version: u8,
    pub transaction_id: uuid::Uuid,
    pub phase: ApplyPhaseV1,
    pub message: String,
}

impl ApplyProgressV1 {
    pub const SCHEMA_VERSION: u8 = 1;

    #[must_use]
    pub fn new(transaction_id: uuid::Uuid, phase: ApplyPhaseV1) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            transaction_id,
            phase,
            message: String::new(),
        }
    }

    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

/// Structured result written by a v2 helper.  `failure_code` and
/// `recovery_action` are absent on success and mandatory on failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyResultV2 {
    pub schema_version: u8,
    pub transaction_id: uuid::Uuid,
    pub status: String,
    pub from_version: String,
    pub to_version: String,
    pub message: String,
    #[serde(default)]
    pub failure_code: Option<ApplyFailureCode>,
    #[serde(default)]
    pub recovery_action: Option<RecoveryAction>,
}

impl ApplyResultV2 {
    pub const SCHEMA_VERSION: u8 = 2;

    #[must_use]
    pub fn succeeded(
        transaction_id: uuid::Uuid,
        from_version: impl Into<String>,
        to_version: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            transaction_id,
            status: "succeeded".to_owned(),
            from_version: from_version.into(),
            to_version: to_version.into(),
            message: "update installed and acknowledged".to_owned(),
            failure_code: None,
            recovery_action: None,
        }
    }

    #[must_use]
    pub fn failed(
        transaction_id: uuid::Uuid,
        from_version: impl Into<String>,
        to_version: impl Into<String>,
        failure_code: ApplyFailureCode,
        recovery_action: RecoveryAction,
        message: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            transaction_id,
            status: "failed".to_owned(),
            from_version: from_version.into(),
            to_version: to_version.into(),
            message: message.into(),
            failure_code: Some(failure_code),
            recovery_action: Some(recovery_action),
        }
    }
}
