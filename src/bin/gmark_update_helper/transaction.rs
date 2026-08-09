// @author kongweiguang

//! ApplyPlanV2 transaction state: progress snapshots, cancellation, and the
//! advisory lifecycle lock held from handoff through the terminal result.

use std::{
    fs::{self, File, OpenOptions},
    io,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use gmark_update_core::{
    ApplyPhaseV1, ApplyPlanV2, ApplyProgressV1, write_apply_progress_for_plan,
};

pub const LIFETIME_LOCK_TIMEOUT: Duration = Duration::from_secs(30);
pub const LOCK_POLL: Duration = Duration::from_millis(100);

/// Progress writer that accepts only the helper's monotonic lifecycle.
pub struct ProgressWriter<'a> {
    plan: &'a ApplyPlanV2,
    current: Option<ApplyPhaseV1>,
}

impl<'a> ProgressWriter<'a> {
    #[must_use]
    pub fn new(plan: &'a ApplyPlanV2) -> Self {
        Self {
            plan,
            current: None,
        }
    }

    pub fn publish(&mut self, phase: ApplyPhaseV1, message: &str) -> Result<(), String> {
        if let Some(previous) = self.current {
            if !legal_transition(previous, phase) {
                return Err(format!(
                    "invalid update progress transition from {:?} to {:?}",
                    previous, phase
                ));
            }
        } else if phase != ApplyPhaseV1::WaitingForExit {
            return Err("update progress must begin at waiting_for_exit".to_owned());
        }
        let snapshot = ApplyProgressV1::new(self.plan.transaction_id, phase).with_message(message);
        write_apply_progress_for_plan(self.plan, &snapshot).map_err(|error| error.to_string())?;
        self.current = Some(phase);
        Ok(())
    }

    #[must_use]
    pub fn phase(&self) -> Option<ApplyPhaseV1> {
        self.current
    }
}

#[must_use]
pub fn legal_transition(previous: ApplyPhaseV1, next: ApplyPhaseV1) -> bool {
    matches!(
        (previous, next),
        (ApplyPhaseV1::WaitingForExit, ApplyPhaseV1::Preparing)
            | (ApplyPhaseV1::Preparing, ApplyPhaseV1::Installing)
            | (ApplyPhaseV1::Installing, ApplyPhaseV1::Relaunching)
            | (ApplyPhaseV1::Relaunching, ApplyPhaseV1::Confirming)
            | (ApplyPhaseV1::Confirming, ApplyPhaseV1::Succeeded)
            | (ApplyPhaseV1::Confirming, ApplyPhaseV1::RollingBack)
            | (ApplyPhaseV1::Installing, ApplyPhaseV1::RollingBack)
            | (ApplyPhaseV1::Relaunching, ApplyPhaseV1::RollingBack)
            | (ApplyPhaseV1::RollingBack, ApplyPhaseV1::Failed)
            | (ApplyPhaseV1::WaitingForExit, ApplyPhaseV1::Failed)
            | (ApplyPhaseV1::Preparing, ApplyPhaseV1::Failed)
            | (ApplyPhaseV1::Installing, ApplyPhaseV1::Failed)
            | (ApplyPhaseV1::Relaunching, ApplyPhaseV1::Failed)
            | (ApplyPhaseV1::Confirming, ApplyPhaseV1::Failed)
    )
}

/// Error category for lifecycle-lock acquisition. WouldBlock is the only
/// retryable condition; other IO errors fail immediately.
#[derive(Debug, PartialEq, Eq)]
pub enum LockError {
    Timeout,
    Cancelled,
    Path(String),
    Io(String),
}

/// The file handle remains alive until this value is dropped, so the helper
/// keeps exclusive ownership through writing the terminal result.
pub struct LifetimeLock {
    file: File,
}

impl LifetimeLock {
    #[must_use]
    pub fn as_file(&self) -> &File {
        &self.file
    }
}

impl Drop for LifetimeLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub fn acquire_lifetime_lock(
    path: &Path,
    timeout: Duration,
    poll: Duration,
) -> Result<LifetimeLock, LockError> {
    acquire_lifetime_lock_until(path, Instant::now() + timeout, poll)
}

pub fn acquire_lifetime_lock_until(
    path: &Path,
    deadline: Instant,
    poll: Duration,
) -> Result<LifetimeLock, LockError> {
    acquire_lifetime_lock_until_internal(path, deadline, poll, None)
}

pub fn acquire_lifetime_lock_until_with_cancellation(
    path: &Path,
    cancellation_path: &Path,
    deadline: Instant,
    poll: Duration,
) -> Result<LifetimeLock, LockError> {
    acquire_lifetime_lock_until_internal(path, deadline, poll, Some(cancellation_path))
}

fn acquire_lifetime_lock_until_internal(
    path: &Path,
    deadline: Instant,
    poll: Duration,
    cancellation_path: Option<&Path>,
) -> Result<LifetimeLock, LockError> {
    if !path.is_absolute() {
        return Err(LockError::Path(
            "lifetime lock path must be absolute".to_owned(),
        ));
    }
    if path.file_name().and_then(|name| name.to_str()) != Some(ApplyPlanV2::LIFETIME_LOCK_FILE_NAME)
    {
        return Err(LockError::Path(
            "lifetime lock path has an unexpected name".to_owned(),
        ));
    }
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(LockError::Path(
                "lifetime lock must be a regular non-link file".to_owned(),
            ));
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| LockError::Path("lifetime lock has no parent directory".to_owned()))?;
    if fs::symlink_metadata(parent)
        .map(|metadata| metadata.file_type().is_symlink() || !metadata.file_type().is_dir())
        .unwrap_or(true)
    {
        return Err(LockError::Path(
            "lifetime lock parent is not a real directory".to_owned(),
        ));
    }

    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(0x2_0000); // O_NOFOLLOW
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(0x100); // O_NOFOLLOW
    }
    let file = options
        .open(path)
        .map_err(|error| LockError::Io(format!("failed to open lifetime lock: {error}")))?;
    loop {
        if let Some(cancellation_path) = cancellation_path {
            match cancellation_requested(cancellation_path) {
                Ok(true) => return Err(LockError::Cancelled),
                Ok(false) => {}
                Err(error) => return Err(LockError::Path(error)),
            }
        }
        match file.try_lock() {
            Ok(()) => return Ok(LifetimeLock { file }),
            Err(std::fs::TryLockError::WouldBlock) => {
                if Instant::now() >= deadline {
                    return Err(LockError::Timeout);
                }
                thread::sleep(poll.min(Duration::from_millis(250)));
            }
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(LockError::Io(format!(
                    "failed to lock update transaction: {error}"
                )));
            }
        }
    }
}

/// Descriptive alias used by lifecycle-lock tests and callers.
pub fn wait_for_lifecycle_lock_until(
    path: &Path,
    deadline: Instant,
    poll: Duration,
) -> Result<LifetimeLock, LockError> {
    acquire_lifetime_lock_until(path, deadline, poll)
}

pub fn cancellation_requested(path: &Path) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("failed to inspect cancellation marker: {error}")),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err("cancellation marker must be a regular non-link file".to_owned());
    }
    Ok(true)
}

#[cfg(test)]
#[path = "../../../tests/unit/bin/gmark_update_helper/transaction.rs"]
mod tests;
