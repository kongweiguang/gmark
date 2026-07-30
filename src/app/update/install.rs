// @author kongweiguang

//! Helper-launch plan construction and cache recovery.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

use sha2::{Digest as _, Sha256};
use tempfile::NamedTempFile;
use uuid::Uuid;

use super::*;

const MAX_CACHED_RESULT_BYTES: usize = 64 * 1024;
const MAX_DISPLAYED_RESULT_BYTES: usize = 128;
const MAX_STAGED_HELPER_BYTES: u64 = 128 * 1024 * 1024;

/// Inherited rather than passed on the command line so an acknowledgement path
/// alone is not a capability to write into the update cache.
pub(super) const UPDATE_ACK_CAPABILITY_ENV: &str = "GMARK_UPDATE_ACK_CAPABILITY";
const ACK_CAPABILITY_FILE_PREFIX: &str = "startup-ack-capability-";

pub(super) enum WorkerEvent {
    Download(DownloadEvent),
    Failed { message: String, retryable: bool },
}

pub(super) fn restored_startup_state(updates_root: &std::path::Path) -> Option<UpdateState> {
    let result_path = updates_root.join("last-result.json");
    let bytes =
        read_bounded_cache_file(&result_path, MAX_CACHED_RESULT_BYTES, "update result").ok()?;
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&bytes);
    let fingerprint = format!("{:08x}\n", hasher.finalize());
    let displayed_path = updates_root.join("last-result-displayed");
    let displayed = read_bounded_cache_file(
        &displayed_path,
        MAX_DISPLAYED_RESULT_BYTES,
        "displayed update result",
    )
    .ok()
    .and_then(|bytes| String::from_utf8(bytes).ok());
    if displayed.as_deref() == Some(fingerprint.as_str()) {
        return None;
    }
    let result = parse_apply_result(&bytes).ok()?;
    let _ = std::fs::write(displayed_path, fingerprint);
    Some(if result.status == "succeeded" {
        UpdateState::Succeeded {
            version: result.to_version,
            message: result.message,
        }
    } else {
        UpdateState::Failed {
            release: None,
            message: result.message,
            retryable: false,
        }
    })
}

/// Reads cache metadata through a fixed upper bound so a hostile or corrupted
/// cache file cannot choose the allocation size before parsing begins.
fn read_bounded_cache_file(path: &Path, max_bytes: usize, label: &str) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).map_err(|error| format!("failed to open {label}: {error}"))?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read {label}: {error}"))?;
    if bytes.is_empty() || bytes.len() > max_bytes {
        return Err(format!("{label} exceeds its size limit"));
    }
    Ok(bytes)
}

pub(super) fn cleanup_update_cache(updates_root: &std::path::Path) {
    const RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
    let Ok(entries) = std::fs::read_dir(updates_root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.starts_with('v') || semver::Version::parse(name.trim_start_matches('v')).is_err() {
            continue;
        }
        let stale = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= RETENTION);
        if stale {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

pub(super) struct PendingInstall {
    pub(super) release: UpdateRelease,
    pub(super) artifact_path: PathBuf,
    pub(super) plan: ApplyPlanV1,
}

pub(super) struct PreparedInstall {
    pub(super) plan_path: PathBuf,
    pub(super) helper: StagedHelper,
    pub(super) plan: ApplyPlanV1,
    pub(super) acknowledgement_capability: String,
}

pub(super) struct StagedHelper {
    pub(super) path: PathBuf,
    length: u64,
    digest: [u8; 32],
}

/// On Windows this keeps the verified image open without write/delete sharing
/// while CreateProcess resolves it; the running image remains OS-locked after launch.
pub(super) struct StagedHelperLaunchGuard {
    #[cfg(windows)]
    _directory: File,
    #[cfg(windows)]
    _file: File,
}

pub(super) fn acknowledgement_capability_path(transaction_dir: &Path, capability: &str) -> PathBuf {
    transaction_dir.join(format!("{ACK_CAPABILITY_FILE_PREFIX}{capability}"))
}

pub(super) fn create_acknowledgement_capability(transaction_dir: &Path) -> Result<String, String> {
    harden_transaction_directory(transaction_dir)?;
    let capability = Uuid::new_v4().hyphenated().to_string();
    let path = acknowledgement_capability_path(transaction_dir, &capability);
    let mut temporary = NamedTempFile::new_in(transaction_dir)
        .map_err(|error| format!("failed to create update acknowledgement capability: {error}"))?;
    temporary
        .write_all(format!("{capability}\n").as_bytes())
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

/// Commits cancellation only after its complete marker is durable. A pre-existing
/// marker is accepted only when it is the exact marker created by this protocol.
pub(super) fn write_cancellation_marker(path: &Path) -> Result<(), String> {
    if existing_cancellation_marker(path)? {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "update cancellation path has no parent directory".to_owned())?;
    let parent_metadata = fs::symlink_metadata(parent)
        .map_err(|error| format!("failed to inspect cancellation directory: {error}"))?;
    if !parent_metadata.file_type().is_dir() || parent_metadata.file_type().is_symlink() {
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
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("cancellation marker is not a regular file".to_owned());
    }
    let marker = read_bounded_cache_file(
        path,
        CancellationV1::MARKER_BYTES.len(),
        "cancellation marker",
    )?;
    Ok(marker == CancellationV1::MARKER_BYTES)
}

pub(super) fn stage_update_helper(
    transaction_dir: &Path,
    installed_helper: &Path,
) -> Result<StagedHelper, String> {
    harden_transaction_directory(transaction_dir)?;
    let helper_name = if cfg!(windows) {
        format!("gmark-update-helper-copy-{}.exe", Uuid::new_v4())
    } else {
        format!("gmark-update-helper-copy-{}", Uuid::new_v4())
    };
    let path = transaction_dir.join(helper_name);
    let (length, digest) = copy_helper_exclusive(installed_helper, &path)?;
    harden_staged_helper(&path)?;
    let helper = StagedHelper {
        path,
        length,
        digest,
    };
    // Verify the copy after permissions are finalized, then verify it again at launch.
    let _ = verify_staged_helper_for_launch(&helper)?;
    Ok(helper)
}

pub(super) fn verify_staged_helper_for_launch(
    helper: &StagedHelper,
) -> Result<StagedHelperLaunchGuard, String> {
    let metadata = fs::symlink_metadata(&helper.path)
        .map_err(|error| format!("failed to inspect staged helper: {error}"))?;
    if !is_real_regular_file(&metadata) {
        return Err("staged helper is not a regular file".to_owned());
    }
    if metadata.len() != helper.length {
        return Err("staged helper changed after verification".to_owned());
    }

    #[cfg(windows)]
    let directory = {
        use std::os::windows::fs::OpenOptionsExt as _;

        let transaction_dir = helper
            .path
            .parent()
            .ok_or_else(|| "staged helper has no transaction directory".to_owned())?;
        let directory_metadata = fs::symlink_metadata(transaction_dir)
            .map_err(|error| format!("failed to inspect staged helper directory: {error}"))?;
        if !is_real_directory(&directory_metadata) {
            return Err("staged helper directory is not a real directory".to_owned());
        }
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ)
            .open(transaction_dir)
            .map_err(|error| format!("failed to lock staged helper directory: {error}"))?;
        if !is_real_directory(
            &directory
                .metadata()
                .map_err(|error| format!("failed to verify staged helper directory: {error}"))?,
        ) {
            return Err("opened staged helper directory is not a real directory".to_owned());
        }
        directory
    };
    #[cfg(windows)]
    let mut file = {
        use std::os::windows::fs::OpenOptionsExt as _;

        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        // Security: open the leaf itself, reject every reparse point, and deny
        // write/delete sharing until CreateProcess has opened the verified image.
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&helper.path)
            .map_err(|error| format!("failed to lock staged helper for launch: {error}"))?;
        let opened = file
            .metadata()
            .map_err(|error| format!("failed to verify staged helper handle: {error}"))?;
        if !is_real_regular_file(&opened) || opened.len() != helper.length {
            return Err("opened staged helper is not the verified regular file".to_owned());
        }
        file
    };
    #[cfg(not(windows))]
    let mut file = File::open(&helper.path)
        .map_err(|error| format!("failed to open staged helper for launch: {error}"))?;

    if hash_file_exact(&mut file, helper.length, "staged helper")? != helper.digest {
        return Err("staged helper changed after verification".to_owned());
    }
    Ok(StagedHelperLaunchGuard {
        #[cfg(windows)]
        _directory: directory,
        #[cfg(windows)]
        _file: file,
    })
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

fn copy_helper_exclusive(source: &Path, destination: &Path) -> Result<(u64, [u8; 32]), String> {
    let mut source_file = File::open(source).map_err(|error| {
        format!(
            "failed to open installed update helper '{}': {error}",
            source.display()
        )
    })?;
    let source_metadata = source_file
        .metadata()
        .map_err(|error| format!("failed to inspect installed update helper: {error}"))?;
    let expected_length = source_metadata.len();
    if !source_metadata.is_file()
        || expected_length == 0
        || expected_length > MAX_STAGED_HELPER_BYTES
    {
        return Err("installed update helper is not a bounded regular file".to_owned());
    }
    let mut destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("failed to create staged update helper: {error}"))?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    let copy_result = (|| -> Result<(), String> {
        loop {
            let read = source_file
                .read(&mut buffer)
                .map_err(|error| format!("failed to read installed update helper: {error}"))?;
            if read == 0 {
                break;
            }
            total = total
                .checked_add(read as u64)
                .ok_or_else(|| "installed update helper is too large".to_owned())?;
            if total > expected_length || total > MAX_STAGED_HELPER_BYTES {
                return Err("installed update helper changed while staging".to_owned());
            }
            destination_file
                .write_all(&buffer[..read])
                .map_err(|error| format!("failed to stage update helper: {error}"))?;
            hasher.update(&buffer[..read]);
        }
        if total != expected_length {
            return Err("installed update helper changed while staging".to_owned());
        }
        destination_file
            .sync_all()
            .map_err(|error| format!("failed to persist staged update helper: {error}"))
    })();
    if let Err(error) = copy_result {
        drop(destination_file);
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    Ok((total, hasher.finalize().into()))
}

fn harden_staged_helper(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect staged helper: {error}"))?;
    if !is_real_regular_file(&metadata) {
        return Err("staged helper is not a regular file".to_owned());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        fs::set_permissions(path, fs::Permissions::from_mode(0o500))
            .map_err(|error| format!("failed to secure staged helper: {error}"))?;
    }
    Ok(())
}

fn is_real_regular_file(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_file()
        && !metadata.file_type().is_symlink()
        && !is_windows_reparse_point(metadata)
}

fn is_real_directory(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_dir()
        && !metadata.file_type().is_symlink()
        && !is_windows_reparse_point(metadata)
}

fn is_windows_reparse_point(metadata: &fs::Metadata) -> bool {
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

fn hash_file_exact(file: &mut File, expected_length: u64, label: &str) -> Result<[u8; 32], String> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to read {label}: {error}"))?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| format!("{label} is too large"))?;
        if total > expected_length || total > MAX_STAGED_HELPER_BYTES {
            return Err(format!("{label} changed after verification"));
        }
        hasher.update(&buffer[..read]);
    }
    if total != expected_length {
        return Err(format!("{label} changed after verification"));
    }
    Ok(hasher.finalize().into())
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

pub(super) fn installed_helper_path() -> Result<PathBuf, String> {
    let current = std::env::current_exe()
        .map_err(|error| format!("failed to locate current executable: {error}"))?;
    let parent = current
        .parent()
        .ok_or_else(|| "current executable has no parent directory".to_owned())?;
    let local = parent.join(if cfg!(windows) {
        "gmark-update-helper.exe"
    } else {
        "gmark-update-helper"
    });
    if local.is_file() {
        return Ok(local);
    }
    #[cfg(target_os = "macos")]
    {
        let bundled = parent.join("../Helpers/gmark-update-helper");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    #[cfg(target_os = "linux")]
    if let Some(app_dir) = std::env::var_os("APPDIR") {
        let bundled = PathBuf::from(app_dir).join("usr/lib/gmark/gmark-update-helper");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    Err("this installation does not include gmark-update-helper".to_owned())
}

pub(super) fn current_update_target() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        return std::env::current_exe()
            .map_err(|error| format!("failed to locate installed gmark: {error}"));
    }
    #[cfg(target_os = "macos")]
    {
        let executable = std::env::current_exe()
            .map_err(|error| format!("failed to locate installed gmark: {error}"))?;
        return executable
            .parent()
            .and_then(|path| path.parent())
            .and_then(|path| path.parent())
            .map(std::path::Path::to_path_buf)
            .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
            .ok_or_else(|| "gmark is not running from a macOS application bundle".to_owned());
    }
    #[cfg(target_os = "linux")]
    {
        let target = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .ok_or_else(|| {
                "automatic installation is available only for AppImage; use the package manager for DEB"
                    .to_owned()
            })?;
        let metadata = std::fs::symlink_metadata(&target)
            .map_err(|error| format!("failed to inspect the current AppImage: {error}"))?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err("the current AppImage path is not a regular file".to_owned());
        }
        if metadata.permissions().readonly() {
            return Err("the current AppImage is not writable; use the release page".to_owned());
        }
        return Ok(target);
    }
    // 原因: 目标平台分支在编译期返回，保留统一的未知平台错误出口会让其在已知目标上不可达；移除条件: 更新 helper 改为以平台 trait 提供统一的非条件返回路径。
    #[allow(unreachable_code)]
    Err("this platform cannot install gmark updates".to_owned())
}

pub(super) fn current_relaunch_path(target: &std::path::Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    return target.join("Contents/MacOS/gmark");
    #[cfg(not(target_os = "macos"))]
    target.to_path_buf()
}

pub(super) fn sibling_backup_path(target: &std::path::Path) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("gmark");
    target.with_file_name(format!("{name}.gmark-update-backup"))
}
