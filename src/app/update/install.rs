// @author kongweiguang

//! Helper-launch plan construction and cache recovery.

use super::*;

pub(super) enum WorkerEvent {
    Download(DownloadEvent),
    Failed { message: String, retryable: bool },
}

pub(super) fn restored_startup_state(updates_root: &std::path::Path) -> Option<UpdateState> {
    let result_path = updates_root.join("last-result.json");
    let bytes = std::fs::read(&result_path).ok()?;
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&bytes);
    let fingerprint = format!("{:08x}\n", hasher.finalize());
    let displayed_path = updates_root.join("last-result-displayed");
    if std::fs::read_to_string(&displayed_path).ok().as_deref() == Some(fingerprint.as_str()) {
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

#[cfg(unix)]
pub(super) fn set_executable(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut permissions = std::fs::metadata(path)
        .map_err(|error| format!("failed to inspect staged helper: {error}"))?
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(path, permissions)
        .map_err(|error| format!("failed to secure staged helper: {error}"))
}

#[cfg(not(unix))]
pub(super) fn set_executable(_path: &std::path::Path) -> Result<(), String> {
    Ok(())
}
