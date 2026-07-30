// @author kongweiguang

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

//! Out-of-process installer for a core-verified GMark update transaction.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use flate2::read::GzDecoder;
use gmark_update_core::{
    ApplyPlanV1, ApplyResultV1, HelperSignalV1, Platform, clear_helper_signal,
    helper_signal_present, read_apply_plan, validate_apply_plan_files, verify_apply_plan_artifact,
    verifying_key_from_base64, write_apply_result,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::fs::File;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

fn main() -> ExitCode {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.len() != 3 || args[1] != "--apply-plan" {
        eprintln!("usage: gmark-update-helper --apply-plan <path>");
        return ExitCode::from(2);
    }
    let plan_path = PathBuf::from(&args[2]);
    match run(&plan_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gmark update failed: {error}");
            if let Ok(plan) = read_plan(&plan_path) {
                append_log(&plan, &format!("failed: {error}"));
                let _ = write_result(&plan, "failed", error);
                if !plan.cancellation_path.exists() && plan.relaunch_path.exists() {
                    let _ = Command::new(&plan.relaunch_path).spawn();
                }
            }
            ExitCode::FAILURE
        }
    }
}

fn run(plan_path: &Path) -> Result<(), String> {
    let plan = read_plan(plan_path)?;
    reset_log(&plan);
    append_log(&plan, "loaded apply plan");
    validate_plan(&plan)?;
    append_log(&plan, "validated apply plan");
    wait_for_parent_or_cancel(&plan)?;
    append_log(&plan, "parent exited");
    verify_signed_artifact(&plan)?;
    append_log(&plan, "verified signed manifest and artifact bytes");
    if let Err(error) = apply_update(&plan) {
        rollback(&plan);
        return Err(error);
    }
    append_log(&plan, "applied platform update");
    launch_and_confirm(&plan)?;
    append_log(&plan, "received startup acknowledgement");
    let _ = fs::remove_file(&plan.backup_path);
    let _ = fs::remove_dir_all(&plan.backup_path);
    let _ = fs::remove_file(plan_path);
    let result = write_result(
        &plan,
        "succeeded",
        "update installed and acknowledged".to_owned(),
    );
    if result.is_ok() {
        append_log(&plan, "completed update transaction");
    }
    result
}

fn reset_log(plan: &ApplyPlanV1) {
    if let Some(parent) = plan.helper_log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::remove_file(&plan.helper_log_path);
}

fn append_log(plan: &ApplyPlanV1, message: &str) {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&plan.helper_log_path)
    {
        let _ = writeln!(file, "{elapsed} {message}");
        let _ = file.flush();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = fs::set_permissions(&plan.helper_log_path, fs::Permissions::from_mode(0o600));
        }
    }
}

fn read_plan(path: &Path) -> Result<ApplyPlanV1, String> {
    read_apply_plan(path).map_err(|error| error.to_string())
}

fn validate_plan(plan: &ApplyPlanV1) -> Result<(), String> {
    validate_apply_plan_files(plan, &Platform::current()).map_err(|error| error.to_string())
}

fn wait_for_parent_or_cancel(plan: &ApplyPlanV1) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5 * 60);
    let mut system = System::new();
    let pid = Pid::from_u32(plan.parent_pid);
    loop {
        if plan.cancellation_path.exists() {
            return Err("installation was cancelled before the app exited".to_owned());
        }
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            ProcessRefreshKind::new(),
        );
        if system.process(pid).is_none() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for gmark to exit".to_owned());
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn verify_signed_artifact(plan: &ApplyPlanV1) -> Result<(), String> {
    let key = embedded_verifying_key()?;
    verify_signed_artifact_with_key(plan, &key)
}

fn verify_signed_artifact_with_key(
    plan: &ApplyPlanV1,
    key: &ed25519_dalek::VerifyingKey,
) -> Result<(), String> {
    verify_apply_plan_artifact(plan, key, &Platform::current())
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn embedded_verifying_key() -> Result<ed25519_dalek::VerifyingKey, String> {
    let encoded = option_env!("GMARK_UPDATE_PUBLIC_KEY_BASE64")
        .ok_or_else(|| "update helper has no embedded verification key".to_owned())?;
    verifying_key_from_base64(encoded).map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn apply_update(plan: &ApplyPlanV1) -> Result<(), String> {
    let _ = fs::remove_file(&plan.backup_path);
    fs::copy(&plan.target_path, &plan.backup_path)
        .map_err(|error| format!("failed to back up current executable: {error}"))?;
    let status = Command::new(&plan.artifact_path)
        .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-"])
        .status()
        .map_err(|error| format!("failed to start Windows installer: {error}"))?;
    if !status.success() {
        return Err(format!("Windows installer exited with {status}"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn apply_update(plan: &ApplyPlanV1) -> Result<(), String> {
    let parent = plan
        .target_path
        .parent()
        .ok_or_else(|| "application bundle has no parent directory".to_owned())?;
    let staging = parent.join(format!(".gmark-update-{}", plan.target_version));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir(&staging)
        .map_err(|error| format!("failed to create staging directory: {error}"))?;
    let decoder = GzDecoder::new(
        File::open(&plan.artifact_path)
            .map_err(|error| format!("failed to open macOS updater archive: {error}"))?,
    );
    let mut archive = tar::Archive::new(decoder);
    for entry in archive
        .entries()
        .map_err(|error| format!("failed to read macOS updater archive: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("invalid updater archive entry: {error}"))?;
        if !entry
            .unpack_in(&staging)
            .map_err(|error| format!("failed to unpack updater archive: {error}"))?
        {
            return Err("updater archive attempted to escape the staging directory".to_owned());
        }
    }
    let staged_app = staging.join("gmark.app");
    if !staged_app.join("Contents/MacOS/gmark").is_file() {
        return Err("macOS updater archive has no gmark application bundle".to_owned());
    }
    let _ = fs::remove_dir_all(&plan.backup_path);
    if let Err(error) = fs::rename(&plan.target_path, &plan.backup_path) {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            install_macos_with_authorization(&plan.target_path, &plan.backup_path, &staged_app)?;
            let _ = fs::remove_dir_all(staging);
            return Ok(());
        }
        return Err(format!("failed to back up current application: {error}"));
    }
    if let Err(error) = fs::rename(&staged_app, &plan.target_path) {
        let _ = fs::rename(&plan.backup_path, &plan.target_path);
        return Err(format!("failed to install new application bundle: {error}"));
    }
    let _ = fs::remove_dir_all(staging);
    Ok(())
}

#[cfg(target_os = "macos")]
fn install_macos_with_authorization(
    target: &Path,
    backup: &Path,
    staged: &Path,
) -> Result<(), String> {
    let script = r#"
on run argv
  set targetPath to quoted form of item 1 of argv
  set backupPath to quoted form of item 2 of argv
  set stagedPath to quoted form of item 3 of argv
  do shell script "/bin/rm -rf " & backupPath & "; /bin/mv " & targetPath & " " & backupPath & "; /bin/mv " & stagedPath & " " & targetPath with administrator privileges
end run
"#;
    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg(target)
        .arg(backup)
        .arg(staged)
        .status()
        .map_err(|error| format!("failed to request macOS update authorization: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("macOS update authorization was denied or failed".to_owned())
    }
}

#[cfg(target_os = "linux")]
fn apply_update(plan: &ApplyPlanV1) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;

    let parent = plan
        .target_path
        .parent()
        .ok_or_else(|| "AppImage has no parent directory".to_owned())?;
    let staging = parent.join(format!(".gmark-update-{}", plan.target_version));
    fs::copy(&plan.artifact_path, &staging)
        .map_err(|error| format!("failed to stage AppImage update: {error}"))?;
    let mut permissions = fs::metadata(&plan.target_path)
        .map_err(|error| format!("failed to inspect current AppImage: {error}"))?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o700);
    fs::set_permissions(&staging, permissions)
        .map_err(|error| format!("failed to preserve AppImage permissions: {error}"))?;
    File::open(&staging)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("failed to sync staged AppImage: {error}"))?;
    let _ = fs::remove_file(&plan.backup_path);
    fs::rename(&plan.target_path, &plan.backup_path)
        .map_err(|error| format!("failed to back up current AppImage: {error}"))?;
    if let Err(error) = fs::rename(&staging, &plan.target_path) {
        let _ = fs::rename(&plan.backup_path, &plan.target_path);
        return Err(format!("failed to install new AppImage: {error}"));
    }
    Ok(())
}

fn launch_and_confirm(plan: &ApplyPlanV1) -> Result<(), String> {
    let _ = clear_helper_signal(plan, HelperSignalV1::Acknowledgement);
    let mut child = Command::new(&plan.relaunch_path)
        .arg("--update-ack")
        .arg(&plan.acknowledgement_path)
        .spawn()
        .map_err(|error| format!("failed to relaunch updated gmark: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if helper_signal_present(plan, HelperSignalV1::Acknowledgement).unwrap_or(false) {
            return Ok(());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("failed to observe relaunched gmark: {error}"))?
        {
            rollback(plan);
            return Err(format!(
                "updated gmark exited before acknowledgement: {status}"
            ));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            rollback(plan);
            return Err("updated gmark did not acknowledge startup".to_owned());
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn rollback(plan: &ApplyPlanV1) {
    if plan.backup_path.exists() {
        rollback_paths(plan);
    }
}

#[cfg(target_os = "macos")]
fn rollback_paths(plan: &ApplyPlanV1) {
    let direct = fs::remove_dir_all(&plan.target_path)
        .and_then(|()| fs::rename(&plan.backup_path, &plan.target_path));
    if direct.is_err() {
        let script = r#"
on run argv
  set targetPath to quoted form of item 1 of argv
  set backupPath to quoted form of item 2 of argv
  do shell script "/bin/rm -rf " & targetPath & "; /bin/mv " & backupPath & " " & targetPath with administrator privileges
end run
"#;
        let _ = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .arg(&plan.target_path)
            .arg(&plan.backup_path)
            .status();
    }
}

#[cfg(not(target_os = "macos"))]
fn rollback_paths(plan: &ApplyPlanV1) {
    let _ = fs::remove_file(&plan.target_path);
    let _ = fs::remove_dir_all(&plan.target_path);
    let _ = fs::rename(&plan.backup_path, &plan.target_path);
}

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

#[cfg(test)]
#[path = "../../tests/unit/bin/gmark_update_helper.rs"]
mod tests;
