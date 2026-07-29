// @author kongweiguang

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

//! Out-of-process installer used only by signed gmark update transactions.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, VerifyingKey};
#[cfg(target_os = "macos")]
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const MAX_PLAN_BYTES: u64 = 64 * 1024;
const MAX_ENVELOPE_BYTES: usize = 128 * 1024;
const MAX_PAYLOAD_BYTES: usize = 96 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyPlanV1 {
    schema_version: u8,
    parent_pid: u32,
    current_version: String,
    target_version: String,
    artifact_path: PathBuf,
    artifact_url: String,
    artifact_size: u64,
    artifact_sha256: String,
    artifact_format: String,
    signed_envelope_path: PathBuf,
    target_path: PathBuf,
    backup_path: PathBuf,
    relaunch_path: PathBuf,
    acknowledgement_path: PathBuf,
    cancellation_path: PathBuf,
    result_path: PathBuf,
    helper_log_path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedEnvelope {
    schema_version: u8,
    algorithm: String,
    payload: String,
    signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u8,
    channel: String,
    version: String,
    published_at: String,
    notes: String,
    paused: bool,
    rollout_percent: u8,
    release_url: String,
    artifacts: BTreeMap<String, Artifact>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    url: String,
    size: u64,
    sha256: String,
    format: String,
    system_trust: String,
}

#[derive(Serialize)]
struct ApplyResult<'a> {
    schema_version: u8,
    status: &'a str,
    from_version: &'a str,
    to_version: &'a str,
    message: String,
}

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
    let length = fs::metadata(path)
        .map_err(|error| format!("failed to inspect apply plan: {error}"))?
        .len();
    if length == 0 || length > MAX_PLAN_BYTES {
        return Err("apply plan exceeds its size limit".to_owned());
    }
    let bytes = fs::read(path).map_err(|error| format!("failed to read apply plan: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid apply plan: {error}"))
}

fn validate_plan(plan: &ApplyPlanV1) -> Result<(), String> {
    if plan.schema_version != 1 {
        return Err("unsupported apply plan schema".to_owned());
    }
    if semver::Version::parse(&plan.target_version)
        .map_err(|error| format!("invalid target version: {error}"))?
        <= semver::Version::parse(&plan.current_version)
            .map_err(|error| format!("invalid current version: {error}"))?
    {
        return Err("target version must be newer than current version".to_owned());
    }
    if !plan.artifact_path.is_file() || !plan.signed_envelope_path.is_file() {
        return Err("verified update files are missing".to_owned());
    }
    let transaction_dir = plan
        .artifact_path
        .parent()
        .ok_or_else(|| "update artifact has no transaction directory".to_owned())?;
    let updates_root = transaction_dir
        .parent()
        .ok_or_else(|| "update transaction has no cache root".to_owned())?;
    if plan.artifact_path != transaction_dir.join("artifact.ready")
        || plan.signed_envelope_path != transaction_dir.join("manifest.envelope.json")
        || plan.acknowledgement_path != transaction_dir.join("startup-ack")
        || plan.cancellation_path != transaction_dir.join("cancel-install")
        || plan.result_path != updates_root.join("last-result.json")
        || plan.helper_log_path != updates_root.join("last-helper.log")
    {
        return Err("apply plan paths do not match the versioned update protocol".to_owned());
    }
    if plan.artifact_size == 0
        || plan.artifact_size > 512 * 1024 * 1024
        || plan.artifact_sha256.len() != 64
        || !plan
            .artifact_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("apply plan has invalid artifact bounds or digest".to_owned());
    }
    let artifact_url = url::Url::parse(&plan.artifact_url)
        .map_err(|error| format!("invalid artifact URL: {error}"))?;
    if artifact_url.scheme() != "https"
        || artifact_url.host_str() != Some("github.com")
        || !artifact_url
            .path()
            .starts_with("/kongweiguang/gmark/releases/download/")
    {
        return Err("apply plan artifact URL is not an official GitHub release URL".to_owned());
    }
    validate_platform_plan(plan)?;
    Ok(())
}

fn validate_platform_plan(plan: &ApplyPlanV1) -> Result<(), String> {
    let expected_format = if cfg!(target_os = "windows") {
        "windows-setup-exe"
    } else if cfg!(target_os = "macos") {
        "macos-app-tar-gz"
    } else if cfg!(target_os = "linux") {
        "linux-app-image"
    } else {
        return Err("this platform cannot apply gmark updates".to_owned());
    };
    if plan.artifact_format != expected_format {
        return Err(format!(
            "artifact format '{}' is invalid for this platform",
            plan.artifact_format
        ));
    }
    let target_parent = plan
        .target_path
        .parent()
        .ok_or_else(|| "update target has no parent directory".to_owned())?;
    let target_name = plan
        .target_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "update target has no valid file name".to_owned())?;
    if plan.backup_path != target_parent.join(format!("{target_name}.gmark-update-backup")) {
        return Err("backup path is not the required sibling of the update target".to_owned());
    }
    #[cfg(target_os = "windows")]
    if plan.relaunch_path != plan.target_path
        || !target_name.eq_ignore_ascii_case("gmark.exe")
        || !is_regular_non_symlink(&plan.target_path)
    {
        return Err("Windows update target is not the installed gmark executable".to_owned());
    }
    #[cfg(target_os = "macos")]
    if target_name != "gmark.app"
        || plan
            .target_path
            .extension()
            .and_then(|value| value.to_str())
            != Some("app")
        || plan.relaunch_path != plan.target_path.join("Contents/MacOS/gmark")
        || !plan.target_path.is_dir()
    {
        return Err("macOS update target is not a gmark application bundle".to_owned());
    }
    #[cfg(target_os = "linux")]
    if plan.relaunch_path != plan.target_path || !is_regular_non_symlink(&plan.target_path) {
        return Err("Linux update target is not a regular AppImage file".to_owned());
    }
    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn is_regular_non_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
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

fn verify_signed_artifact_with_key(plan: &ApplyPlanV1, key: &VerifyingKey) -> Result<(), String> {
    let envelope_bytes = fs::read(&plan.signed_envelope_path)
        .map_err(|error| format!("failed to read signed manifest: {error}"))?;
    if envelope_bytes.is_empty() || envelope_bytes.len() > MAX_ENVELOPE_BYTES {
        return Err("signed manifest envelope exceeds its size limit".to_owned());
    }
    let envelope: SignedEnvelope = serde_json::from_slice(&envelope_bytes)
        .map_err(|error| format!("invalid signed manifest envelope: {error}"))?;
    if envelope.schema_version != 1 || envelope.algorithm != "Ed25519" {
        return Err("unsupported signed manifest envelope".to_owned());
    }
    let payload = BASE64
        .decode(envelope.payload)
        .map_err(|error| format!("invalid manifest payload base64: {error}"))?;
    if payload.is_empty() || payload.len() > MAX_PAYLOAD_BYTES {
        return Err("signed manifest payload exceeds its size limit".to_owned());
    }
    let signature = BASE64
        .decode(envelope.signature)
        .map_err(|error| format!("invalid manifest signature base64: {error}"))?;
    let signature = Signature::from_slice(&signature)
        .map_err(|error| format!("invalid manifest signature: {error}"))?;
    key.verify_strict(&payload, &signature)
        .map_err(|_| "manifest signature verification failed".to_owned())?;
    let manifest: Manifest = serde_json::from_slice(&payload)
        .map_err(|error| format!("invalid signed manifest: {error}"))?;
    if manifest.schema_version != 2
        || manifest.channel != "stable"
        || manifest.version != plan.target_version
        || manifest.paused
        || manifest.rollout_percent > 100
        || !is_rfc3339_utc(&manifest.published_at)
        || manifest.notes.len() > 32 * 1024
        || !is_official_release_url(&manifest.release_url)
    {
        return Err("signed manifest does not match the apply plan".to_owned());
    }
    let artifact = manifest
        .artifacts
        .values()
        .find(|artifact| artifact.url == plan.artifact_url)
        .ok_or_else(|| "apply artifact is absent from signed manifest".to_owned())?;
    if artifact.size != plan.artifact_size
        || !artifact.sha256.eq_ignore_ascii_case(&plan.artifact_sha256)
        || artifact.format != plan.artifact_format
        || !system_trust_matches_platform(&artifact.system_trust)
    {
        return Err("signed artifact metadata does not match the apply plan".to_owned());
    }
    let metadata = fs::metadata(&plan.artifact_path)
        .map_err(|error| format!("failed to inspect update artifact: {error}"))?;
    if metadata.len() != plan.artifact_size {
        return Err("update artifact size changed after verification".to_owned());
    }
    let actual = sha256_file(&plan.artifact_path)?;
    if !actual.eq_ignore_ascii_case(&plan.artifact_sha256) {
        return Err("update artifact hash changed after verification".to_owned());
    }
    Ok(())
}

fn is_official_release_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("github.com")
            && url.path().starts_with("/kongweiguang/gmark/releases/")
    })
}

fn is_rfc3339_utc(value: &str) -> bool {
    value.ends_with('Z')
        && value.len() >= 20
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.as_bytes().get(10) == Some(&b'T')
        && value.as_bytes().get(13) == Some(&b':')
        && value.as_bytes().get(16) == Some(&b':')
}

fn system_trust_matches_platform(value: &str) -> bool {
    match std::env::consts::OS {
        "windows" => matches!(value, "unsigned" | "authenticode"),
        "macos" => matches!(value, "unsigned" | "developer-id-notarized"),
        "linux" => value == "not-applicable",
        _ => false,
    }
}

fn embedded_verifying_key() -> Result<VerifyingKey, String> {
    let encoded = option_env!("GMARK_UPDATE_PUBLIC_KEY_BASE64")
        .ok_or_else(|| "update helper has no embedded verification key".to_owned())?;
    let bytes = BASE64
        .decode(encoded)
        .map_err(|error| format!("invalid embedded verification key: {error}"))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| format!("verification key has {} bytes", bytes.len()))?;
    VerifyingKey::from_bytes(&bytes)
        .map_err(|error| format!("invalid embedded verification key: {error}"))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("failed to hash artifact: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to hash artifact: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
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
    let _ = fs::remove_file(&plan.acknowledgement_path);
    let mut child = Command::new(&plan.relaunch_path)
        .arg("--update-ack")
        .arg(&plan.acknowledgement_path)
        .spawn()
        .map_err(|error| format!("failed to relaunch updated gmark: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if plan.acknowledgement_path.is_file() {
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
    if !plan.backup_path.exists() {
        return;
    }
    rollback_paths(plan);
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
    if let Some(parent) = plan.result_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create result directory: {error}"))?;
    }
    let bytes = serde_json::to_vec_pretty(&ApplyResult {
        schema_version: 1,
        status,
        from_version: &plan.current_version,
        to_version: &plan.target_version,
        message,
    })
    .map_err(|error| format!("failed to serialize update result: {error}"))?;
    let parent = plan.result_path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("failed to create update result: {error}"))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("failed to persist update result: {error}"))?;
    temporary
        .persist(&plan.result_path)
        .map(|_| ())
        .map_err(|error| format!("failed to commit update result: {}", error.error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};
    use serde_json::json;

    fn fixture_plan(root: &Path) -> ApplyPlanV1 {
        let transaction = root.join("v0.2.0");
        fs::create_dir_all(&transaction).unwrap();
        let artifact = transaction.join("artifact.ready");
        let envelope = transaction.join("manifest.envelope.json");
        fs::write(&artifact, b"artifact").unwrap();
        fs::write(&envelope, b"manifest").unwrap();
        let target = if cfg!(target_os = "windows") {
            root.join("gmark.exe")
        } else if cfg!(target_os = "macos") {
            root.join("gmark.app")
        } else {
            root.join("gmark.AppImage")
        };
        if cfg!(target_os = "macos") {
            fs::create_dir_all(target.join("Contents/MacOS")).unwrap();
        } else {
            fs::write(&target, b"current").unwrap();
        }
        let target_name = target.file_name().unwrap().to_string_lossy();
        let backup = target.with_file_name(format!("{target_name}.gmark-update-backup"));
        let relaunch = if cfg!(target_os = "macos") {
            target.join("Contents/MacOS/gmark")
        } else {
            target.clone()
        };
        ApplyPlanV1 {
            schema_version: 1,
            parent_pid: u32::MAX,
            current_version: "0.1.0".into(),
            target_version: "0.2.0".into(),
            artifact_path: artifact,
            artifact_url: "https://github.com/kongweiguang/gmark/releases/download/v0.2.0/a".into(),
            artifact_size: 8,
            artifact_sha256: "00".repeat(32),
            artifact_format: if cfg!(target_os = "windows") {
                "windows-setup-exe"
            } else if cfg!(target_os = "macos") {
                "macos-app-tar-gz"
            } else {
                "linux-app-image"
            }
            .into(),
            signed_envelope_path: envelope,
            target_path: target,
            backup_path: backup,
            relaunch_path: relaunch,
            acknowledgement_path: transaction.join("startup-ack"),
            cancellation_path: transaction.join("cancel-install"),
            result_path: root.join("last-result.json"),
            helper_log_path: root.join("last-helper.log"),
        }
    }

    #[test]
    fn apply_plan_rejects_downgrades_and_missing_artifacts() {
        let root = tempfile::tempdir().unwrap();
        let mut plan = fixture_plan(root.path());
        assert!(validate_plan(&plan).is_ok());
        plan.target_version = "0.0.9".into();
        assert!(validate_plan(&plan).is_err());
        plan.target_version = "0.2.0".into();
        fs::remove_file(&plan.artifact_path).unwrap();
        assert!(validate_plan(&plan).is_err());
    }

    #[test]
    fn apply_plan_rejects_cross_platform_formats_and_unrelated_backups() {
        let root = tempfile::tempdir().unwrap();
        let mut plan = fixture_plan(root.path());
        plan.artifact_format = "not-this-platform".to_owned();
        assert!(validate_plan(&plan).is_err());
        plan = fixture_plan(root.path());
        plan.backup_path = root.path().join("unrelated-backup");
        assert!(validate_plan(&plan).is_err());
    }

    #[test]
    fn cancellation_marker_prevents_any_install_side_effect() {
        let root = tempfile::tempdir().unwrap();
        let plan = fixture_plan(root.path());
        let original_target = if plan.target_path.is_file() {
            Some(fs::read(&plan.target_path).unwrap())
        } else {
            None
        };
        fs::write(&plan.cancellation_path, b"cancelled").unwrap();
        assert!(
            wait_for_parent_or_cancel(&plan)
                .unwrap_err()
                .contains("cancelled")
        );
        assert!(plan.target_path.exists());
        assert!(!plan.backup_path.exists());
        if let Some(original_target) = original_target {
            assert_eq!(fs::read(&plan.target_path).unwrap(), original_target);
        }
    }

    #[test]
    fn sha256_file_matches_standard_vector() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("abc");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn apply_result_atomically_replaces_a_previous_result() {
        let root = tempfile::tempdir().unwrap();
        let plan = fixture_plan(root.path());
        write_result(&plan, "failed", "first".to_owned()).unwrap();
        write_result(&plan, "succeeded", "second".to_owned()).unwrap();
        let result: serde_json::Value =
            serde_json::from_slice(&fs::read(&plan.result_path).unwrap()).unwrap();
        assert_eq!(result["status"], "succeeded");
        assert_eq!(result["message"], "second");
    }

    #[test]
    fn helper_log_is_local_bounded_history_for_the_latest_transaction() {
        let root = tempfile::tempdir().unwrap();
        let plan = fixture_plan(root.path());
        reset_log(&plan);
        append_log(&plan, "verified artifact");
        append_log(&plan, "completed update");
        let log = fs::read_to_string(&plan.helper_log_path).unwrap();
        assert!(log.contains("verified artifact"));
        assert!(log.contains("completed update"));
    }

    #[test]
    fn oversized_apply_plan_is_rejected_before_deserialization() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("apply-plan.json");
        fs::write(&path, vec![b' '; MAX_PLAN_BYTES as usize + 1]).unwrap();
        assert!(read_plan(&path).unwrap_err().contains("size limit"));
    }

    #[test]
    fn helper_reverifies_manifest_signature_and_artifact_bytes() {
        let root = tempfile::tempdir().unwrap();
        let mut plan = fixture_plan(root.path());
        plan.artifact_sha256 = sha256_file(&plan.artifact_path).unwrap();
        let signing_key = SigningKey::from_bytes(&[31; 32]);
        let system_trust = if cfg!(target_os = "linux") {
            "not-applicable"
        } else {
            "unsigned"
        };
        let payload = serde_json::to_vec(&json!({
            "schema_version": 2,
            "channel": "stable",
            "version": plan.target_version,
            "published_at": "2026-07-22T12:00:00Z",
            "notes": "fixture",
            "paused": false,
            "rollout_percent": 100,
            "release_url": "https://github.com/kongweiguang/gmark/releases/tag/v0.2.0",
            "artifacts": {
                "fixture": {
                    "url": plan.artifact_url,
                    "size": plan.artifact_size,
                    "sha256": plan.artifact_sha256,
                    "format": plan.artifact_format,
                    "system_trust": system_trust
                }
            }
        }))
        .unwrap();
        let signature = signing_key.sign(&payload);
        fs::write(
            &plan.signed_envelope_path,
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "algorithm": "Ed25519",
                "payload": BASE64.encode(&payload),
                "signature": BASE64.encode(signature.to_bytes())
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(verify_signed_artifact_with_key(&plan, &signing_key.verifying_key()).is_ok());
        fs::write(&plan.artifact_path, b"tampered").unwrap();
        assert!(verify_signed_artifact_with_key(&plan, &signing_key.verifying_key()).is_err());
    }
}
