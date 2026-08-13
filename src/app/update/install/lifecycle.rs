// @author kongweiguang

//! Transaction claims, process-lifetime locks, capabilities, and install roots.

use super::*;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use tempfile::NamedTempFile;
use uuid::Uuid;

const CLAIM_FILE_NAME: &str = "apply.claim";
const ACK_CAPABILITY_FILE_PREFIX: &str = "startup-ack-capability-";

static LIFETIME_LOCKS: OnceLock<Mutex<HashMap<Uuid, File>>> = OnceLock::new();
static TRANSACTION_CLAIMS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

pub(crate) fn create_transaction_directory(transaction_dir: &Path) -> Result<(), String> {
    let transactions_dir = transaction_dir
        .parent()
        .ok_or_else(|| "update transaction has no transactions directory".to_owned())?;
    let version_dir = transactions_dir
        .parent()
        .ok_or_else(|| "update transactions directory has no version root".to_owned())?;
    let version_metadata = fs::symlink_metadata(version_dir)
        .map_err(|error| format!("failed to inspect update version directory: {error}"))?;
    if !is_real_directory(&version_metadata) {
        return Err("update version directory is not a real directory".to_owned());
    }
    match fs::symlink_metadata(transactions_dir) {
        Ok(metadata) if is_real_directory(&metadata) => {}
        Ok(_) => return Err("update transactions path is not a real directory".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(transactions_dir).map_err(|error| {
                format!("failed to create update transactions directory: {error}")
            })?;
        }
        Err(error) => {
            return Err(format!(
                "failed to inspect update transactions directory: {error}"
            ));
        }
    }
    match fs::create_dir(transaction_dir) {
        Ok(()) => harden_transaction_directory(transaction_dir),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err("update transaction directory already exists".to_owned())
        }
        Err(error) => Err(format!(
            "failed to create update transaction directory: {error}"
        )),
    }
}

fn lifetime_locks() -> &'static Mutex<HashMap<Uuid, File>> {
    LIFETIME_LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn transaction_claims() -> &'static Mutex<HashSet<PathBuf>> {
    TRANSACTION_CLAIMS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Claims only a newly-created real transaction directory.  The marker is a
/// second process boundary: create-new rejects stale objects and symlinks.
pub(crate) fn claim_transaction(transaction_dir: &Path) -> Result<bool, String> {
    let metadata = fs::symlink_metadata(transaction_dir)
        .map_err(|error| format!("failed to inspect update transaction directory: {error}"))?;
    if !is_real_directory(&metadata) {
        return Err("update transaction directory is not a real directory".to_owned());
    }
    let mut claims = transaction_claims()
        .lock()
        .map_err(|_| "update transaction claim registry is poisoned".to_owned())?;
    if !claims.insert(transaction_dir.to_path_buf()) {
        return Ok(false);
    }
    let marker = transaction_dir.join(CLAIM_FILE_NAME);
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
    {
        Ok(mut file) => {
            let result = file
                .write_all(b"claimed\n")
                .and_then(|()| file.sync_all())
                .map_err(|error| format!("failed to persist update transaction claim: {error}"));
            if let Err(error) = result {
                claims.remove(transaction_dir);
                let _ = fs::remove_file(marker);
                return Err(error);
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            claims.remove(transaction_dir);
            Ok(false)
        }
        Err(error) => {
            claims.remove(transaction_dir);
            Err(format!("failed to claim update transaction: {error}"))
        }
    }
}

pub(crate) fn release_transaction_claim(transaction_dir: &Path) {
    if let Ok(mut claims) = transaction_claims().lock() {
        claims.remove(transaction_dir);
    }
    let _ = fs::remove_file(transaction_dir.join(CLAIM_FILE_NAME));
}

/// Holds the OS lock in a process-static map until the helper has a terminal
/// result, cancellation is committed, or the app process exits.
pub(crate) fn register_lifecycle_lock(plan: &gmark_update_core::ApplyPlanV2) -> Result<(), String> {
    let parent = plan
        .lifetime_lock_path
        .parent()
        .ok_or_else(|| "update lifetime lock path has no parent directory".to_owned())?;
    harden_transaction_directory(parent)?;
    let path_metadata = match fs::symlink_metadata(&plan.lifetime_lock_path) {
        Ok(metadata) => {
            if !is_real_regular_file(&metadata) {
                return Err("update lifetime lock is not a regular file".to_owned());
            }
            Some(metadata)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("failed to inspect update lifetime lock: {error}")),
    };
    let file = if path_metadata.is_some() {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&plan.lifetime_lock_path)
            .map_err(|error| format!("failed to open update lifetime lock: {error}"))?
    } else {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&plan.lifetime_lock_path)
            .map_err(|error| format!("failed to create update lifetime lock: {error}"))?
    };
    match file.try_lock() {
        Ok(()) => {
            let mut locks = lifetime_locks()
                .lock()
                .map_err(|_| "update lifetime lock registry is poisoned".to_owned())?;
            if locks.contains_key(&plan.transaction_id) {
                let _ = file.unlock();
                return Err("update transaction lifetime lock is already registered".to_owned());
            }
            locks.insert(plan.transaction_id, file);
            Ok(())
        }
        Err(std::fs::TryLockError::WouldBlock) => {
            Err("update transaction lifetime lock is already held".to_owned())
        }
        Err(std::fs::TryLockError::Error(error)) => {
            Err(format!("failed to acquire update lifetime lock: {error}"))
        }
    }
}

pub(crate) fn release_lifecycle_lock(transaction_id: Uuid) -> Result<(), String> {
    let mut locks = lifetime_locks()
        .lock()
        .map_err(|_| "update lifetime lock registry is poisoned".to_owned())?;
    let Some(file) = locks.remove(&transaction_id) else {
        return Ok(());
    };
    file.unlock()
        .map_err(|error| format!("failed to release update lifetime lock: {error}"))
}

// 原因：回归测试需要观察进程内锁登记状态；当测试改用公开生命周期快照后移除。
#[allow(dead_code)]
pub(crate) fn lifecycle_lock_is_registered(transaction_id: Uuid) -> bool {
    lifetime_locks()
        .lock()
        .ok()
        .is_some_and(|locks| locks.contains_key(&transaction_id))
}

pub(crate) fn acknowledgement_capability_path(transaction_dir: &Path, capability: &str) -> PathBuf {
    transaction_dir.join(format!("{ACK_CAPABILITY_FILE_PREFIX}{capability}"))
}

/// The capability is independent from the transaction UUID exposed in paths
/// and process arguments. Its body binds both values so a copied capability
/// file cannot acknowledge another transaction.
pub(crate) fn create_acknowledgement_capability(transaction_dir: &Path) -> Result<String, String> {
    harden_transaction_directory(transaction_dir)?;
    let transaction_id = transaction_dir
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| Uuid::parse_str(name).ok())
        .ok_or_else(|| "update transaction directory has no valid transaction id".to_owned())?;
    let capability = Uuid::new_v4().hyphenated().to_string();
    let path = acknowledgement_capability_path(transaction_dir, &capability);
    let mut temporary = NamedTempFile::new_in(transaction_dir)
        .map_err(|error| format!("failed to create update acknowledgement capability: {error}"))?;
    temporary
        .write_all(format!("{}:{capability}\n", transaction_id.hyphenated()).as_bytes())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("failed to persist update acknowledgement capability: {error}"))?;
    set_private_file_permissions(temporary.as_file())?;
    temporary.persist_noclobber(&path).map_err(|error| {
        format!(
            "failed to commit update acknowledgement capability '{}': {}",
            path.display(),
            error.error
        )
    })?;
    Ok(capability)
}

pub(crate) fn write_cancellation_marker(path: &Path) -> Result<(), String> {
    if existing_cancellation_marker(path)? {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "update cancellation path has no parent directory".to_owned())?;
    let parent_metadata = fs::symlink_metadata(parent)
        .map_err(|error| format!("failed to inspect cancellation directory: {error}"))?;
    if !is_real_directory(&parent_metadata) {
        return Err("update cancellation directory is not a real directory".to_owned());
    }
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| format!("failed to create cancellation marker: {error}"))?;
    temporary
        .write_all(CancellationV1::MARKER_BYTES)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("failed to persist cancellation marker: {error}"))?;
    set_private_file_permissions(temporary.as_file())?;
    match temporary.persist_noclobber(path) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            existing_cancellation_marker(path).and_then(|present| {
                present
                    .then_some(())
                    .ok_or_else(|| "cancellation marker has unexpected contents".to_owned())
            })
        }
        Err(error) => Err(format!(
            "failed to commit cancellation marker: {}",
            error.error
        )),
    }
}

fn existing_cancellation_marker(path: &Path) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("failed to inspect cancellation marker: {error}")),
    };
    if !is_real_regular_file(&metadata) {
        return Err("cancellation marker is not a regular file".to_owned());
    }
    let marker = read_bounded_cache_file(
        path,
        CancellationV1::MARKER_BYTES.len(),
        "cancellation marker",
    )?;
    Ok(marker == CancellationV1::MARKER_BYTES)
}

fn harden_transaction_directory(transaction_dir: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(transaction_dir)
        .map_err(|error| format!("failed to inspect update transaction directory: {error}"))?;
    if !is_real_directory(&metadata) {
        return Err("update transaction directory is not a real directory".to_owned());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(transaction_dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("failed to secure update transaction directory: {error}"))?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(file: &File) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to secure update acknowledgement capability: {error}"))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_file: &File) -> Result<(), String> {
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CurrentUpdateTarget {
    pub(crate) target_path: PathBuf,
    pub(crate) expected_install_root: PathBuf,
}

pub(crate) fn current_update_target() -> Result<CurrentUpdateTarget, String> {
    #[cfg(target_os = "windows")]
    {
        let executable = std::env::current_exe()
            .map_err(|error| format!("failed to locate installed gmark: {error}"))?;
        let expected_install_root = windows_install_location()?.ok_or_else(|| {
            "Gmark is not registered by the fixed Inno installation; portable mode cannot self-update"
                .to_owned()
        })?;
        let parent = executable
            .parent()
            .ok_or_else(|| "installed gmark executable has no parent directory".to_owned())?;
        let root_metadata = fs::symlink_metadata(&expected_install_root)
            .map_err(|error| format!("failed to inspect registered install root: {error}"))?;
        if !is_real_directory(&root_metadata) {
            return Err("registered install root is not a real directory".to_owned());
        }
        let canonical_parent = fs::canonicalize(parent)
            .map_err(|error| format!("failed to resolve installed gmark directory: {error}"))?;
        let canonical_root = fs::canonicalize(&expected_install_root)
            .map_err(|error| format!("failed to resolve registered install root: {error}"))?;
        if canonical_parent != canonical_root {
            return Err(
                "running gmark executable is outside the registered install root".to_owned(),
            );
        }
        return Ok(CurrentUpdateTarget {
            target_path: executable,
            expected_install_root,
        });
    }
    #[cfg(target_os = "macos")]
    {
        let executable = std::env::current_exe()
            .map_err(|error| format!("failed to locate installed gmark: {error}"))?;
        let bundle = executable
            .parent()
            .and_then(|path| path.parent())
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
            .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
            .ok_or_else(|| "Gmark is not running from a macOS application bundle".to_owned())?;
        let bundle_metadata = fs::symlink_metadata(&bundle)
            .map_err(|error| format!("failed to inspect macOS application bundle: {error}"))?;
        if !is_real_directory(&bundle_metadata) {
            return Err("macOS application bundle is not a real directory".to_owned());
        }
        return Ok(CurrentUpdateTarget {
            target_path: bundle.clone(),
            expected_install_root: bundle,
        });
    }
    #[cfg(target_os = "linux")]
    {
        let target = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .ok_or_else(|| {
                "automatic installation is available only for AppImage; use the package manager for DEB"
                    .to_owned()
            })?;
        let metadata = fs::symlink_metadata(&target)
            .map_err(|error| format!("failed to inspect the current AppImage: {error}"))?;
        if !is_real_regular_file(&metadata) {
            return Err("the current AppImage path is not a regular file".to_owned());
        }
        if metadata.permissions().readonly() {
            return Err("the current AppImage is not writable; use the release page".to_owned());
        }
        return Ok(CurrentUpdateTarget {
            target_path: target.clone(),
            expected_install_root: target,
        });
    }
    // 原因：三平台 cfg 分支在目标平台都会提前返回；当改为每平台独立实现模块后移除。
    #[allow(unreachable_code)]
    Err("this platform cannot install gmark updates".to_owned())
}

#[cfg(target_os = "windows")]
fn windows_install_location() -> Result<Option<PathBuf>, String> {
    const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{7E04F75C-109D-4C5E-9E7B-BDE8F91FD0E1}_is1";
    let output = std::process::Command::new("reg.exe")
        .args(["query", UNINSTALL_KEY, "/v", "InstallLocation"])
        .output()
        .map_err(|error| format!("failed to query installed gmark location: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let value = text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        if fields.next() != Some("InstallLocation") {
            return None;
        }
        let mut values = fields.collect::<Vec<_>>();
        if values
            .first()
            .is_some_and(|value| matches!(*value, "REG_SZ" | "REG_EXPAND_SZ"))
        {
            values.remove(0);
        }
        let value = values.join(" ");
        (!value.is_empty()).then_some(value)
    });
    Ok(value.map(PathBuf::from))
}

pub(crate) fn current_relaunch_path(target: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    return target.join("Contents/MacOS/gmark");
    #[cfg(not(target_os = "macos"))]
    target.to_path_buf()
}

pub(crate) fn sibling_backup_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("gmark");
    target.with_file_name(format!("{name}.gmark-update-backup"))
}

// 原因：V1 回滚测试仍校验旧备份命名；当 V1 兼容测试退役后移除。
#[allow(dead_code)]
pub(crate) fn transaction_backup_path(target: &Path, transaction_id: Uuid) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("gmark");
    target.with_file_name(format!(
        "{name}.gmark-update-backup-{}",
        transaction_id.hyphenated()
    ))
}
