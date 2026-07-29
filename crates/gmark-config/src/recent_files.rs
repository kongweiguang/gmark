// @author kongweiguang

//! 最近文件历史及其兼容的文本格式。

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::{ConfigDirs, persistence::atomic_write};

/// 历史文件中最多保留的路径数。
pub const RECENT_FILES_LIMIT: usize = 20;

/// 从系统配置目录读取最近文件。
pub fn read_recent_files() -> Result<Vec<PathBuf>> {
    read_recent_files_with_dirs(&ConfigDirs::from_system()?)
}

/// 从显式配置目录读取最近文件。
pub fn read_recent_files_with_dirs(dirs: &ConfigDirs) -> Result<Vec<PathBuf>> {
    let path = dirs.history_file();
    let text = match std::fs::read_to_string(&path) {
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
    record_recent_file_with_dirs(path, &ConfigDirs::from_system()?)
}

/// 将路径提升到显式配置目录中的历史首位。
pub fn record_recent_file_with_dirs(path: &Path, dirs: &ConfigDirs) -> Result<Vec<PathBuf>> {
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
    remove_recent_file_with_dirs(path, &ConfigDirs::from_system()?)
}

/// 从显式配置目录中的历史移除路径。
pub fn remove_recent_file_with_dirs(path: &Path, dirs: &ConfigDirs) -> Result<Vec<PathBuf>> {
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

fn write_recent_files_with_dirs(paths: &[PathBuf], dirs: &ConfigDirs) -> Result<()> {
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
    if let Some(parent) = history_file.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }
    let mut content = String::new();
    for path in normalized {
        content.push_str(&path.to_string_lossy());
        content.push('\n');
    }
    atomic_write(&history_file, content.as_bytes())
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
