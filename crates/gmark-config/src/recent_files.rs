// @author kongweiguang

//! 最近文件历史及其兼容的文本格式。

use std::fs::File;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context as _, Result, bail};

use crate::{AppDirs, persistence::atomic_write_private};

/// 历史文件中最多保留的路径数。
pub const RECENT_FILES_LIMIT: usize = 20;

/// 历史文件属于低信任持久化输入，读取时固定上限以免坏文件放大内存和启动延迟。
pub const RECENT_FILES_MAX_BYTES: usize = 64 * 1024;

static RECENT_FILES_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn recent_files_write_lock() -> &'static Mutex<()> {
    RECENT_FILES_WRITE_LOCK.get_or_init(|| Mutex::new(()))
}

/// 从系统配置目录读取最近文件。
pub fn read_recent_files() -> Result<Vec<PathBuf>> {
    read_recent_files_with_dirs(&AppDirs::from_system()?)
}

/// 从显式配置目录读取最近文件。
pub fn read_recent_files_with_dirs(dirs: &AppDirs) -> Result<Vec<PathBuf>> {
    dirs.validate_state_root()?;
    let path = dirs.history_file();
    let text = match read_history_text(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read '{}'", path.display()));
        }
    };
    Ok(normalize_recent_files(text.lines().map(PathBuf::from)))
}

/// 将路径提升到系统配置目录中的历史首位。
pub fn record_recent_file(path: &Path) -> Result<Vec<PathBuf>> {
    record_recent_file_with_dirs(path, &AppDirs::from_system()?)
}

/// 将路径提升到显式配置目录中的历史首位。
pub fn record_recent_file_with_dirs(path: &Path, dirs: &AppDirs) -> Result<Vec<PathBuf>> {
    let _write_guard = recent_files_write_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("recent-file history lock is poisoned"))?;
    if path.to_string_lossy().trim().is_empty() {
        bail!("recent file path cannot be empty");
    }
    if !is_recordable_recent_file_path(path) {
        return read_recent_files_with_dirs(dirs);
    }
    let mut paths = read_recent_files_with_dirs(dirs)?;
    let path = path.to_path_buf();
    paths.retain(|existing| !same_recent_path(existing, &path));
    paths.insert(0, path);
    paths.truncate(RECENT_FILES_LIMIT);
    write_recent_files_with_dirs(&paths, dirs)?;
    Ok(paths)
}

/// 从系统配置目录中的历史移除路径。
pub fn remove_recent_file(path: &Path) -> Result<Vec<PathBuf>> {
    remove_recent_file_with_dirs(path, &AppDirs::from_system()?)
}

/// 从显式配置目录中的历史移除路径。
pub fn remove_recent_file_with_dirs(path: &Path, dirs: &AppDirs) -> Result<Vec<PathBuf>> {
    let _write_guard = recent_files_write_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("recent-file history lock is poisoned"))?;
    let mut paths = read_recent_files_with_dirs(dirs)?;
    paths.retain(|existing| !same_recent_path(existing, path));
    write_recent_files_with_dirs(&paths, dirs)?;
    Ok(paths)
}

/// 归一化历史中的空路径、测试临时文件、重复项和超限项。
#[must_use]
pub fn normalize_recent_files(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut normalized: Vec<PathBuf> = Vec::new();
    for path in paths {
        let text = path.to_string_lossy();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = PathBuf::from(trimmed);
        if !is_recordable_recent_file_path(&path)
            || normalized
                .iter()
                .any(|existing| same_recent_path(existing, &path))
        {
            continue;
        }
        normalized.push(path);
        if normalized.len() == RECENT_FILES_LIMIT {
            break;
        }
    }
    normalized
}

fn write_recent_files_with_dirs(paths: &[PathBuf], dirs: &AppDirs) -> Result<()> {
    let history_file = dirs.history_file();
    let normalized = normalize_recent_files(paths.iter().cloned());
    if normalized.is_empty() {
        match std::fs::remove_file(&history_file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to remove '{}'", history_file.display()));
            }
        }
        return Ok(());
    }
    dirs.ensure_state_parent(&history_file)?;
    let mut content = String::new();
    for path in normalized {
        content.push_str(&path.to_string_lossy());
        content.push('\n');
    }
    atomic_write_private(&history_file, content.as_bytes())
}

/// 先检查元数据再以 `take` 读取，防止并发替换或异常增长绕过历史文件上限。
fn read_history_text(path: &Path) -> Result<String, std::io::Error> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || metadata.len() > RECENT_FILES_MAX_BYTES as u64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "recent-file history exceeds the {} byte safety limit",
                RECENT_FILES_MAX_BYTES
            ),
        ));
    }
    let file = File::open(path)?;
    let mut bytes = Vec::with_capacity(RECENT_FILES_MAX_BYTES.min(metadata.len() as usize));
    file.take((RECENT_FILES_MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > RECENT_FILES_MAX_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "recent-file history exceeds the {} byte safety limit",
                RECENT_FILES_MAX_BYTES
            ),
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()))
}

fn is_recordable_recent_file_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    if text.trim().is_empty() {
        return false;
    }
    !(is_inside_system_temp_dir(path) && has_gmark_temp_fixture_name(path))
}

fn is_inside_system_temp_dir(path: &Path) -> bool {
    let temp_dir = std::env::temp_dir();
    if cfg!(windows) {
        let path_text = normalize_windows_path_text(path);
        let mut temp_text = normalize_windows_path_text(&temp_dir);
        if !temp_text.ends_with('\\') {
            temp_text.push('\\');
        }
        path_text.starts_with(&temp_text)
    } else {
        path.starts_with(temp_dir)
    }
}

fn normalize_windows_path_text(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn has_gmark_temp_fixture_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            let name = name.to_ascii_lowercase();
            name.starts_with("gmark-drop-") || name.starts_with("velotypre-drop-")
        })
        .unwrap_or(false)
}

fn same_recent_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}
