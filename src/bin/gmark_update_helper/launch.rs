// @author kongweiguang

//! Updated-binary launch, feedback-agent handoff, and startup confirmation.

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use gmark_update_core::{ApplyFeedbackModeV1, ApplyPlanV2};

const AGENT_DELAY: Duration = Duration::from_millis(700);
pub const STARTUP_CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(30);
const ACK_LIMIT: usize = 1024;

pub fn launch_updated(plan: &ApplyPlanV2) -> Result<Child, String> {
    clear_acknowledgement(&plan.acknowledgement_path)?;
    let mut command = Command::new(&plan.relaunch_path);
    command
        .current_dir(transaction_dir(plan))
        .arg("--update-ack")
        .arg(&plan.acknowledgement_path);
    command
        .spawn()
        .map_err(|error| format!("failed to relaunch updated gmark: {error}"))
}

/// Schedules the read-only presentation process without delaying installation.
/// Fast successful transactions suppress the window; failures force it to
/// appear before the helper exits so the terminal error remains visible.
pub struct FeedbackAgent {
    launch: Option<Arc<FeedbackAgentLaunch>>,
    successful: bool,
}

struct FeedbackAgentLaunch {
    started: AtomicBool,
    cancelled: AtomicBool,
    agent: PathBuf,
    transaction_dir: PathBuf,
    progress_path: PathBuf,
}

impl FeedbackAgentLaunch {
    fn start_once(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire)
            || self
                .started
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return Ok(());
        }
        Command::new(&self.agent)
            .current_dir(&self.transaction_dir)
            .arg("--progress")
            .arg(&self.progress_path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("failed to launch update feedback agent: {error}"))
    }
}

impl FeedbackAgent {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            launch: None,
            successful: false,
        }
    }

    pub fn schedule(plan: &ApplyPlanV2) -> Result<Self, String> {
        if plan.feedback_mode != ApplyFeedbackModeV1::Agent {
            return Ok(Self::disabled());
        }
        let transaction_dir = transaction_dir(plan).to_path_buf();
        let agent_name = if cfg!(windows) {
            "gmark-update-agent.exe"
        } else {
            "gmark-update-agent"
        };
        let agent = transaction_dir.join(agent_name);
        let metadata = fs::symlink_metadata(&agent)
            .map_err(|error| format!("failed to inspect update feedback agent: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err("update feedback agent must be a regular non-link file".to_owned());
        }
        let launch = Arc::new(FeedbackAgentLaunch {
            started: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            agent,
            transaction_dir,
            progress_path: plan.progress_path.clone(),
        });
        let delayed = Arc::clone(&launch);
        thread::spawn(move || {
            thread::sleep(AGENT_DELAY);
            let _ = delayed.start_once();
        });
        Ok(Self {
            launch: Some(launch),
            successful: false,
        })
    }

    pub fn mark_successful(&mut self) {
        self.successful = true;
        if let Some(launch) = &self.launch {
            launch.cancelled.store(true, Ordering::Release);
        }
    }
}

impl Drop for FeedbackAgent {
    fn drop(&mut self) {
        if !self.successful
            && let Some(launch) = &self.launch
        {
            let _ = launch.start_once();
        }
    }
}

/// Waits for acknowledgement and always terminates and reaps a still-running
/// updated child before returning a launch-confirmation error.
pub fn confirm_startup(plan: &ApplyPlanV2, mut child: Child) -> Result<(), String> {
    let deadline = Instant::now() + STARTUP_CONFIRMATION_TIMEOUT;
    loop {
        match startup_acknowledged(plan) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) => {
                return Err(with_stop_error(error, stop_child(&mut child)));
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(with_stop_error(
                    format!("updated gmark exited before acknowledgement: {status}"),
                    stop_child(&mut child),
                ));
            }
            Ok(None) => {}
            Err(error) => {
                return Err(with_stop_error(
                    format!("failed to observe relaunched gmark: {error}"),
                    stop_child(&mut child),
                ));
            }
        }
        if Instant::now() >= deadline {
            return Err(with_stop_error(
                "timed out waiting for startup acknowledgement".to_owned(),
                stop_child(&mut child),
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub fn relaunch_previous_after_rollback(plan: &ApplyPlanV2) -> Result<(), String> {
    Command::new(&plan.relaunch_path)
        .current_dir(transaction_dir(plan))
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn with_stop_error(message: String, stop: Result<(), String>) -> String {
    match stop {
        Ok(()) => message,
        Err(error) => format!("{message}; failed to stop updated gmark: {error}"),
    }
}

pub fn stop_child(child: &mut Child) -> Result<(), String> {
    // `try_wait` may already have reaped a child. Treat that state as a
    // successful stop while still calling `wait` so every live child follows
    // the required kill-and-reap path before rollback begins.
    let kill_error = match child.try_wait() {
        Ok(Some(_)) => None,
        Ok(None) => child.kill().err(),
        Err(error) => Some(error),
    };
    let wait_error = child.wait().err();
    match (kill_error, wait_error) {
        (None, None) => Ok(()),
        (Some(kill), None) => Err(format!("kill failed: {kill}")),
        (None, Some(wait)) => Err(format!("wait failed: {wait}")),
        (Some(kill), Some(wait)) => Err(format!("kill failed: {kill}; wait failed: {wait}")),
    }
}

fn transaction_dir(plan: &ApplyPlanV2) -> &Path {
    plan.transaction_dir().unwrap_or_else(|| Path::new("."))
}

fn clear_acknowledgement(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            fs::remove_file(path)
                .map_err(|error| format!("failed to clear startup acknowledgement: {error}"))
        }
        Ok(_) => Err("startup acknowledgement is not a regular non-link file".to_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "failed to inspect startup acknowledgement: {error}"
        )),
    }
}

fn startup_acknowledged(plan: &ApplyPlanV2) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(&plan.acknowledgement_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "failed to inspect startup acknowledgement: {error}"
            ));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err("startup acknowledgement must be a regular non-link file".to_owned());
    }
    let bytes = fs::read(&plan.acknowledgement_path)
        .map_err(|error| format!("failed to read startup acknowledgement: {error}"))?;
    if bytes.len() > ACK_LIMIT {
        return Err("startup acknowledgement exceeds its size limit".to_owned());
    }
    let mut expected = plan.target_version.as_bytes().to_vec();
    expected.push(b'\n');
    if bytes == expected {
        Ok(true)
    } else {
        Err("startup acknowledgement does not match the update target".to_owned())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/bin/gmark_update_helper/launch.rs"]
mod tests;
