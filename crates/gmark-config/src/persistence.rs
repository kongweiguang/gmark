// @author kongweiguang

//! 同目录临时文件的持久化辅助函数。

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};

/// 将内容写入目标所在文件系统后替换目标，避免暴露半写入的配置文件。
pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    atomic_write_with_mode(path, contents, None)
}

/// 将敏感状态文件原子写入并固定为 Unix `0600`。
pub(crate) fn atomic_write_private(path: &Path, contents: &[u8]) -> Result<()> {
    atomic_write_with_mode(path, contents, Some(0o600))
}

fn atomic_write_with_mode(path: &Path, contents: &[u8], mode: Option<u32>) -> Result<()> {
    if path.file_name().is_none() {
        bail!("atomic write target has no file name");
    }
    let parent = path
        .parent()
        .filter(|candidate| !candidate.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary = temporary_path(parent);
    let permissions = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || is_reparse_point(&metadata) => {
            bail!(
                "atomic write target '{}' must not be a symbolic link",
                path.display()
            );
        }
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect '{}'", path.display()));
        }
    };

    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        if mode.is_some() {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .with_context(|| format!("failed to create '{}'", temporary.display()))?;
        if let Some(mode) = mode {
            set_mode(&file, mode, path)?;
        } else if let Some(permissions) = permissions {
            file.set_permissions(permissions).with_context(|| {
                format!("failed to preserve permissions for '{}'", path.display())
            })?;
        }
        file.write_all(contents)
            .with_context(|| format!("failed to write '{}'", temporary.display()))?;
        file.flush()
            .with_context(|| format!("failed to flush '{}'", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync '{}'", temporary.display()))?;
        drop(file);

        fs::rename(&temporary, path)
            .with_context(|| format!("failed to atomically replace '{}'", path.display()))?;
        OpenOptions::new()
            .write(true)
            .open(path)
            .and_then(|file| file.sync_all())
            .with_context(|| format!("failed to sync '{}'", path.display()))?;
        sync_parent_directory(parent, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn set_mode(file: &fs::File, mode: u32, path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = file
            .metadata()
            .with_context(|| format!("failed to inspect '{}'", path.display()))?
            .permissions();
        permissions.set_mode(mode);
        file.set_permissions(permissions)
            .with_context(|| format!("failed to protect '{}'", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (file, mode, path);
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn temporary_path(parent: &Path) -> PathBuf {
    parent.join(format!(".gmark-config-{}.tmp", uuid::Uuid::new_v4()))
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path, path: &Path) -> Result<()> {
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("failed to sync parent directory for '{}'", path.display()))
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path, _path: &Path) -> Result<()> {
    Ok(())
}
