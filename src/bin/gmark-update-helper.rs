// @author kongweiguang

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

//! Out-of-process installer for a core-verified ApplyPlanV2 transaction.

use std::{ffi::OsString, path::PathBuf, process::ExitCode};

#[path = "gmark_update_helper/mod.rs"]
mod gmark_update_helper;
pub use gmark_update_helper::*;

fn main() -> ExitCode {
    let args = std::env::args_os().collect::<Vec<_>>();
    run_from_args(&args)
}

fn run_from_args(args: &[OsString]) -> ExitCode {
    if args.len() != 3 || args[1] != "--apply-plan" {
        eprintln!("usage: gmark-update-helper --apply-plan <path>");
        return ExitCode::from(2);
    }
    let plan_path = PathBuf::from(&args[2]);
    match run_v2(&plan_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(V2RunError::Untrusted(error)) => {
            eprintln!("Gmark update failed:{error}");
            ExitCode::FAILURE
        }
        Err(V2RunError::Trusted { plan, failure }) => {
            eprintln!("Gmark update failed:{}", failure.message);
            report_v2_failure(plan.as_ref(), &failure);
            ExitCode::FAILURE
        }
    }
}

// These helpers only support the legacy protocol fixtures that still live in
// the unit-test module. They are deliberately not reachable from the binary
// entry point: all production reads, writes, installation, and recovery use
// ApplyPlanV2 in `gmark_update_helper`.
#[cfg(test)]
use std::{
    fs::{self, OpenOptions},
    io::{self, Write as _},
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

#[cfg(test)]
use gmark_update_core::{
    ApplyPlanV1, ApplyResultV1, Platform, read_apply_plan, validate_apply_plan_files,
    verify_apply_plan_artifact, write_apply_result,
};

#[cfg(test)]
fn read_plan(path: &Path) -> Result<ApplyPlanV1, String> {
    read_apply_plan(path).map_err(|error| error.to_string())
}

#[cfg(test)]
fn validate_plan(plan: &ApplyPlanV1) -> Result<(), String> {
    validate_apply_plan_files(plan, &Platform::current()).map_err(|error| error.to_string())
}

#[cfg(test)]
fn verify_signed_artifact_with_key(
    plan: &ApplyPlanV1,
    key: &ed25519_dalek::VerifyingKey,
) -> Result<(), String> {
    verify_apply_plan_artifact(plan, key, &Platform::current())
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
fn wait_for_parent_or_cancel(plan: &ApplyPlanV1) -> Result<(), String> {
    wait_for_parent_or_cancel_until(
        plan,
        Instant::now() + Duration::from_secs(5 * 60),
        Duration::from_millis(200),
    )
}

#[cfg(test)]
fn wait_for_parent_or_cancel_until(
    plan: &ApplyPlanV1,
    _deadline: Instant,
    _poll_interval: Duration,
) -> Result<(), String> {
    match fs::symlink_metadata(&plan.cancellation_path) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            Err("installation was cancelled before the app exited".to_owned())
        }
        Ok(_) => Err("cancellation marker must be a regular non-link file".to_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to inspect cancellation marker: {error}")),
    }
}

#[cfg(test)]
fn reset_log(plan: &ApplyPlanV1) {
    let Some(parent) = plan.helper_log_path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let temporary = parent.join(format!(
        ".gmark-test-log-{}-{}",
        std::process::id(),
        plan.target_version.replace('.', "-")
    ));
    if let Ok(file) = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
    {
        let _ = file.sync_all();
        let _ = fs::rename(&temporary, &plan.helper_log_path);
    }
}

#[cfg(test)]
fn append_log(plan: &ApplyPlanV1, message: &str) {
    let Some(parent) = plan.helper_log_path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&plan.helper_log_path)
    else {
        return;
    };
    let _ = writeln!(file, "{message}");
    let _ = file.flush();
}

#[cfg(test)]
fn write_result(plan: &ApplyPlanV1, status: &str, message: String) -> Result<(), String> {
    if !matches!(status, "succeeded" | "failed") {
        return Err("unsupported update result status".to_owned());
    }
    let result = ApplyResultV1 {
        schema_version: ApplyResultV1::SCHEMA_VERSION,
        status: status.to_owned(),
        from_version: plan.current_version.clone(),
        to_version: plan.target_version.clone(),
        message,
    };
    write_apply_result(&plan.result_path, &result).map_err(|error| error.to_string())
}

#[cfg(all(test, not(target_os = "macos")))]
fn clear_backup_path(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            fs::remove_file(path)
                .map_err(|error| format!("failed to remove update backup: {error}"))
        }
        Ok(_) => Err("update backup is not a regular file".to_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to inspect update backup: {error}")),
    }
}

#[cfg(all(test, not(target_os = "macos")))]
fn rollback(plan: &ApplyPlanV1) -> Result<(), String> {
    let target_metadata = fs::symlink_metadata(&plan.target_path)
        .map_err(|error| format!("failed to inspect update target: {error}"))?;
    if !target_metadata.file_type().is_file() || target_metadata.file_type().is_symlink() {
        return Err("update target is not a regular non-link file".to_owned());
    }
    let backup_metadata = fs::symlink_metadata(&plan.backup_path)
        .map_err(|error| format!("failed to inspect update backup: {error}"))?;
    if !backup_metadata.file_type().is_file() || backup_metadata.file_type().is_symlink() {
        return Err("update backup is not a regular non-link file".to_owned());
    }
    fs::remove_file(&plan.target_path)
        .map_err(|error| format!("failed to remove failed update target: {error}"))?;
    fs::rename(&plan.backup_path, &plan.target_path)
        .map_err(|error| format!("failed to restore update backup: {error}"))
}

#[cfg(test)]
#[path = "../../tests/unit/bin/gmark_update_helper.rs"]
mod tests;
