// @author kongweiguang

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

//! Out-of-process installer for a core-verified GMark update transaction.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use flate2::read::GzDecoder;
use gmark_update_core::{
    ApplyPlanV1, ApplyResultV1, HelperSignalV1, Platform, StagedApplyArtifact, clear_helper_signal,
    helper_signal_present, read_validated_apply_plan, stage_and_verify_apply_plan_artifact,
    validate_apply_plan_files, verifying_key_from_base64, write_apply_result,
};
#[cfg(test)]
use gmark_update_core::{read_apply_plan, verify_apply_plan_artifact};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
#[cfg(target_os = "macos")]
use tempfile::Builder;
use tempfile::NamedTempFile;

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
    match run(&plan_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(RunError::Untrusted(error)) => {
            eprintln!("gmark update failed: {error}");
            ExitCode::FAILURE
        }
        Err(RunError::Trusted {
            plan,
            error,
            relaunch_after_failure,
        }) => {
            eprintln!("gmark update failed: {error}");
            report_trusted_failure(plan.as_ref(), &error, relaunch_after_failure);
            ExitCode::FAILURE
        }
    }
}

enum RunError {
    Untrusted(String),
    Trusted {
        plan: Box<ApplyPlanV1>,
        error: String,
        relaunch_after_failure: bool,
    },
}

struct ApplyFailure {
    error: String,
    relaunch_after_failure: bool,
}

impl ApplyFailure {
    fn before_apply(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            relaunch_after_failure: false,
        }
    }

    fn after_apply(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            relaunch_after_failure: true,
        }
    }
}

fn run(plan_path: &Path) -> Result<(), RunError> {
    // SECURITY: the only pre-validation read is the bounded, explicitly supplied plan.
    let plan = read_validated_apply_plan(plan_path, &Platform::current())
        .map_err(|error| RunError::Untrusted(error.to_string()))?;
    validate_plan(&plan).map_err(|error| RunError::Untrusted(error.to_string()))?;
    run_validated_plan(plan_path, &plan).map_err(|failure| RunError::Trusted {
        plan: Box::new(plan),
        error: failure.error,
        relaunch_after_failure: failure.relaunch_after_failure,
    })
}

fn run_validated_plan(plan_path: &Path, plan: &ApplyPlanV1) -> Result<(), ApplyFailure> {
    // All paths below are derived only after `read_validated_apply_plan` accepted
    // the fixed transaction layout, supplied plan location, and on-disk files.
    reset_log(plan);
    append_log(plan, "loaded apply plan");
    append_log(plan, "validated apply plan");
    wait_for_parent_or_cancel(plan).map_err(ApplyFailure::before_apply)?;
    append_log(plan, "parent exited");
    let staging_directory = artifact_staging_directory(plan).map_err(ApplyFailure::before_apply)?;
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let artifact =
        stage_signed_artifact(plan, staging_directory).map_err(ApplyFailure::before_apply)?;
    #[cfg(target_os = "macos")]
    let mut artifact =
        stage_signed_artifact(plan, staging_directory).map_err(ApplyFailure::before_apply)?;
    append_log(plan, "staged verified signed manifest and artifact bytes");
    validate_plan(plan).map_err(ApplyFailure::before_apply)?;
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let apply_result = apply_update(plan, &artifact);
    #[cfg(target_os = "macos")]
    let apply_result = apply_update(plan, &mut artifact);
    if let Err(error) = apply_result {
        return Err(ApplyFailure::after_apply(rollback_after_failure(
            plan, error,
        )));
    }
    append_log(plan, "applied platform update");
    if let Err(error) = launch_and_confirm(plan) {
        return Err(ApplyFailure::after_apply(error));
    }
    append_log(plan, "received startup acknowledgement");
    if let Err(error) = clear_backup_path(&plan.backup_path) {
        append_log(
            plan,
            &format!("retained update backup after success: {error}"),
        );
    }
    if let Err(error) = fs::remove_file(plan_path)
        && error.kind() != io::ErrorKind::NotFound
    {
        append_log(
            plan,
            &format!("retained applied plan after success: {error}"),
        );
    }
    let result = write_result(
        plan,
        "succeeded",
        "update installed and acknowledged".to_owned(),
    );
    if result.is_ok() {
        append_log(plan, "completed update transaction");
    }
    result.map_err(ApplyFailure::before_apply)
}

fn reset_log(plan: &ApplyPlanV1) {
    let Some(parent) = plan.helper_log_path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(temporary) = NamedTempFile::new_in(parent) else {
        return;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        let _ = temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600));
    }
    let _ = temporary.persist(&plan.helper_log_path);
}

fn append_log(plan: &ApplyPlanV1, message: &str) {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    if let Ok(mut file) = open_log_for_append(&plan.helper_log_path) {
        let _ = writeln!(file, "{elapsed} {message}");
        let _ = file.flush();
    }
}

fn open_log_for_append(path: &Path) -> io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::unix::fs::OpenOptionsExt as _;

        const O_NOFOLLOW: i32 = 0x2_0000;
        options.custom_flags(O_NOFOLLOW);
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt as _;

        const O_NOFOLLOW: i32 = 0x100;
        options.custom_flags(O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;

        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(all(
        unix,
        not(any(target_os = "linux", target_os = "android", target_os = "macos"))
    ))]
    if let Ok(metadata) = fs::symlink_metadata(path)
        && metadata.file_type().is_symlink()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "helper log is a symlink",
        ));
    }
    options.open(path)
}

#[cfg(test)]
fn read_plan(path: &Path) -> Result<ApplyPlanV1, String> {
    read_apply_plan(path).map_err(|error| error.to_string())
}

fn validate_plan(plan: &ApplyPlanV1) -> Result<(), String> {
    validate_apply_plan_files(plan, &Platform::current()).map_err(|error| error.to_string())
}

fn report_trusted_failure(plan: &ApplyPlanV1, error: &str, relaunch_after_failure: bool) {
    append_log(plan, &format!("failed: {error}"));
    let _ = write_result(plan, "failed", error.to_owned());
    if relaunch_after_failure && should_relaunch_after_failure(plan) {
        let _ = Command::new(&plan.relaunch_path).spawn();
    }
}

fn should_relaunch_after_failure(plan: &ApplyPlanV1) -> bool {
    // A marker read failure is fail-closed: never launch a process when cancellation
    // cannot be safely ruled out.
    matches!(
        helper_signal_present(plan, HelperSignalV1::Cancellation),
        Ok(false)
    ) && validate_plan(plan).is_ok()
}

fn wait_for_parent_or_cancel(plan: &ApplyPlanV1) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5 * 60);
    let mut system = System::new();
    let pid = Pid::from_u32(plan.parent_pid);
    loop {
        if helper_signal_present(plan, HelperSignalV1::Cancellation)
            .map_err(|error| error.to_string())?
        {
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

fn stage_signed_artifact(
    plan: &ApplyPlanV1,
    staging_directory: &Path,
) -> Result<StagedApplyArtifact, String> {
    let key = embedded_verifying_key()?;
    stage_and_verify_apply_plan_artifact(plan, &key, &Platform::current(), staging_directory)
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
fn artifact_staging_directory(plan: &ApplyPlanV1) -> Result<&Path, String> {
    plan.target_path
        .parent()
        .ok_or_else(|| "AppImage has no parent directory".to_owned())
}

#[cfg(not(target_os = "linux"))]
fn artifact_staging_directory(plan: &ApplyPlanV1) -> Result<&Path, String> {
    plan.artifact_path
        .parent()
        .ok_or_else(|| "update artifact has no transaction directory".to_owned())
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

fn embedded_verifying_key() -> Result<ed25519_dalek::VerifyingKey, String> {
    let encoded = option_env!("GMARK_UPDATE_PUBLIC_KEY_BASE64")
        .ok_or_else(|| "update helper has no embedded verification key".to_owned())?;
    verifying_key_from_base64(encoded).map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn apply_update(plan: &ApplyPlanV1, artifact: &StagedApplyArtifact) -> Result<(), String> {
    clear_backup_path(&plan.backup_path)?;
    fs::copy(&plan.target_path, &plan.backup_path)
        .map_err(|error| format!("failed to back up current executable: {error}"))?;
    let status = Command::new(artifact.path())
        .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-"])
        .status()
        .map_err(|error| format!("failed to start Windows installer: {error}"))?;
    if !status.success() {
        return Err(format!("Windows installer exited with {status}"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn apply_update(plan: &ApplyPlanV1, artifact: &mut StagedApplyArtifact) -> Result<(), String> {
    let parent = plan
        .target_path
        .parent()
        .ok_or_else(|| "application bundle has no parent directory".to_owned())?;
    let staging = Builder::new()
        .prefix(".gmark-update-")
        .tempdir_in(parent)
        .map_err(|error| format!("failed to create staging directory: {error}"))?;
    artifact
        .rewind()
        .map_err(|error| format!("failed to rewind macOS updater archive: {error}"))?;
    let decoder = GzDecoder::new(artifact.as_file_mut());
    let mut archive = tar::Archive::new(decoder);
    for entry in archive
        .entries()
        .map_err(|error| format!("failed to read macOS updater archive: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("invalid updater archive entry: {error}"))?;
        if !entry
            .unpack_in(staging.path())
            .map_err(|error| format!("failed to unpack updater archive: {error}"))?
        {
            return Err("updater archive attempted to escape the staging directory".to_owned());
        }
    }
    let staged_app = staging.path().join("gmark.app");
    if !staged_app.join("Contents/MacOS/gmark").is_file() {
        return Err("macOS updater archive has no gmark application bundle".to_owned());
    }
    clear_backup_path(&plan.backup_path)?;
    if let Err(error) = fs::rename(&plan.target_path, &plan.backup_path) {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            install_macos_with_authorization(&plan.target_path, &plan.backup_path, &staged_app)?;
            return Ok(());
        }
        return Err(format!("failed to back up current application: {error}"));
    }
    if let Err(error) = fs::rename(&staged_app, &plan.target_path) {
        return Err(match fs::rename(&plan.backup_path, &plan.target_path) {
            Ok(()) => format!("failed to install new application bundle: {error}"),
            Err(restore_error) => format!(
                "failed to install new application bundle: {error}; failed to restore previous application: {restore_error}"
            ),
        });
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn install_macos_with_authorization(
    target: &Path,
    backup: &Path,
    staged: &Path,
) -> Result<(), String> {
    if !expected_real_directory_exists(target, "update target")? {
        return Err("update target disappeared before authorization".to_owned());
    }
    if expected_real_directory_exists(backup, "update backup")? {
        return Err("update backup reappeared before authorization".to_owned());
    }
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
fn apply_update(plan: &ApplyPlanV1, artifact: &StagedApplyArtifact) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;

    let mut permissions = fs::metadata(&plan.target_path)
        .map_err(|error| format!("failed to inspect current AppImage: {error}"))?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o700);
    artifact
        .as_file()
        .set_permissions(permissions)
        .map_err(|error| format!("failed to preserve AppImage permissions: {error}"))?;
    artifact
        .as_file()
        .sync_all()
        .map_err(|error| format!("failed to sync staged AppImage: {error}"))?;
    let staging = artifact.path().to_path_buf();
    clear_backup_path(&plan.backup_path)?;
    fs::rename(&plan.target_path, &plan.backup_path)
        .map_err(|error| format!("failed to back up current AppImage: {error}"))?;
    if let Err(error) = fs::rename(&staging, &plan.target_path) {
        return Err(match fs::rename(&plan.backup_path, &plan.target_path) {
            Ok(()) => format!("failed to install new AppImage: {error}"),
            Err(restore_error) => format!(
                "failed to install new AppImage: {error}; failed to restore previous AppImage: {restore_error}"
            ),
        });
    }
    Ok(())
}

fn launch_and_confirm(plan: &ApplyPlanV1) -> Result<(), String> {
    if let Err(error) = clear_helper_signal(plan, HelperSignalV1::Acknowledgement) {
        return Err(rollback_after_failure(
            plan,
            format!("failed to clear startup acknowledgement: {error}"),
        ));
    }
    if let Err(error) = validate_plan(plan) {
        return Err(rollback_after_failure(plan, error));
    }
    match helper_signal_present(plan, HelperSignalV1::Acknowledgement) {
        Ok(false) => {}
        Ok(true) => {
            return Err(rollback_after_failure(
                plan,
                "stale startup acknowledgement was recreated before relaunch",
            ));
        }
        Err(error) => {
            return Err(rollback_after_failure(
                plan,
                format!("invalid startup acknowledgement: {error}"),
            ));
        }
    }
    let mut child = match Command::new(&plan.relaunch_path)
        .arg("--update-ack")
        .arg(&plan.acknowledgement_path)
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return Err(rollback_after_failure(
                plan,
                format!("failed to relaunch updated gmark: {error}"),
            ));
        }
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match helper_signal_present(plan, HelperSignalV1::Acknowledgement) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) => {
                let kill_error = child.kill().err();
                let error = match kill_error {
                    Some(kill_error) => {
                        format!(
                            "invalid startup acknowledgement: {error}; failed to stop relaunched gmark: {kill_error}"
                        )
                    }
                    None => format!("invalid startup acknowledgement: {error}"),
                };
                return Err(rollback_after_failure(plan, error));
            }
        }
        let status = child.try_wait().map_err(|error| {
            rollback_after_failure(plan, format!("failed to observe relaunched gmark: {error}"))
        })?;
        if let Some(status) = status {
            return Err(rollback_after_failure(
                plan,
                format!("updated gmark exited before acknowledgement: {status}"),
            ));
        }
        if Instant::now() >= deadline {
            let kill_error = child.kill().err();
            let error = match kill_error {
                Some(kill_error) => format!(
                    "updated gmark did not acknowledge startup; failed to stop relaunched gmark: {kill_error}"
                ),
                None => "updated gmark did not acknowledge startup".to_owned(),
            };
            return Err(rollback_after_failure(plan, error));
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn rollback_after_failure(plan: &ApplyPlanV1, error: impl Into<String>) -> String {
    let error = error.into();
    match rollback(plan) {
        Ok(()) => error,
        Err(rollback_error) => format!("{error}; rollback failed: {rollback_error}"),
    }
}

fn rollback(plan: &ApplyPlanV1) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let expected_backup = expected_real_directory_exists(&plan.backup_path, "update backup")?;
    #[cfg(not(target_os = "macos"))]
    let expected_backup = expected_regular_file_exists(&plan.backup_path, "update backup")?;
    if expected_backup {
        rollback_paths(plan)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn clear_backup_path(path: &Path) -> Result<(), String> {
    remove_real_directory_if_exists(path, "update backup")
}

#[cfg(not(target_os = "macos"))]
fn clear_backup_path(path: &Path) -> Result<(), String> {
    remove_regular_file_if_exists(path, "update backup")
}

#[cfg(target_os = "macos")]
fn rollback_paths(plan: &ApplyPlanV1) -> Result<(), String> {
    let direct =
        remove_real_directory_if_exists(&plan.target_path, "update target").and_then(|()| {
            fs::rename(&plan.backup_path, &plan.target_path)
                .map_err(|error| format!("failed to restore update backup: {error}"))
        });
    match direct {
        Ok(()) => Ok(()),
        Err(direct_error) => {
            ensure_real_directory_or_missing(&plan.target_path, "update target")?;
            if !expected_real_directory_exists(&plan.backup_path, "update backup")? {
                return Err(format!(
                    "failed to restore update backup: {direct_error}; update backup disappeared"
                ));
            }
            rollback_macos_with_authorization(&plan.target_path, &plan.backup_path).map_err(
                |authorization_error| {
                    format!(
                        "failed to restore update backup: {direct_error}; authorization fallback failed: {authorization_error}"
                    )
                },
            )
        }
    }
}

#[cfg(target_os = "macos")]
fn rollback_macos_with_authorization(target: &Path, backup: &Path) -> Result<(), String> {
    ensure_real_directory_or_missing(target, "update target")?;
    if !expected_real_directory_exists(backup, "update backup")? {
        return Err("update backup disappeared before authorization".to_owned());
    }
    let script = r#"
on run argv
  set targetPath to quoted form of item 1 of argv
  set backupPath to quoted form of item 2 of argv
  do shell script "/bin/rm -rf " & targetPath & "; /bin/mv " & backupPath & " " & targetPath with administrator privileges
end run
"#;
    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg(target)
        .arg(backup)
        .status()
        .map_err(|error| format!("failed to request macOS rollback authorization: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("macOS rollback authorization was denied or failed".to_owned())
    }
}

#[cfg(not(target_os = "macos"))]
fn rollback_paths(plan: &ApplyPlanV1) -> Result<(), String> {
    remove_regular_file_if_exists(&plan.target_path, "update target")?;
    fs::rename(&plan.backup_path, &plan.target_path)
        .map_err(|error| format!("failed to restore update backup: {error}"))
}

fn expected_regular_file_exists(path: &Path, label: &str) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_real_regular_file(&metadata) => Ok(true),
        Ok(_) => Err(format!(
            "{label} is not an expected regular non-reparse file"
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("failed to inspect {label}: {error}")),
    }
}

fn remove_regular_file_if_exists(path: &Path, label: &str) -> Result<(), String> {
    if expected_regular_file_exists(path, label)? {
        fs::remove_file(path).map_err(|error| format!("failed to remove {label}: {error}"))?;
    }
    Ok(())
}

fn is_real_regular_file(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_file()
        && !metadata.file_type().is_symlink()
        && !is_reparse_point(metadata)
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

#[cfg(target_os = "macos")]
fn expected_real_directory_exists(path: &Path, label: &str) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_real_directory(&metadata) => Ok(true),
        Ok(_) => Err(format!(
            "{label} is not an expected real application directory"
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("failed to inspect {label}: {error}")),
    }
}

#[cfg(target_os = "macos")]
fn ensure_real_directory_or_missing(path: &Path, label: &str) -> Result<(), String> {
    expected_real_directory_exists(path, label).map(|_| ())
}

#[cfg(target_os = "macos")]
fn remove_real_directory_if_exists(path: &Path, label: &str) -> Result<(), String> {
    if expected_real_directory_exists(path, label)? {
        fs::remove_dir_all(path).map_err(|error| format!("failed to remove {label}: {error}"))?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn is_real_directory(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_dir() && !metadata.file_type().is_symlink()
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
