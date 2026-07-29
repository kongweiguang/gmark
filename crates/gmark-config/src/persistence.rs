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
    if path.file_name().is_none() {
        bail!("atomic write target has no file name");
    }
    let parent = path
        .parent()
        .filter(|candidate| !candidate.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary = temporary_path(parent);
    let permissions = match fs::metadata(path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect '{}'", path.display()));
        }
    };

    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("failed to create '{}'", temporary.display()))?;
        if let Some(permissions) = permissions {
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
