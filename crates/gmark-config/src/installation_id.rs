// @author kongweiguang

//! 稳定、匿名的本地安装标识。

use std::{fs::OpenOptions, io::Write as _, path::Path};

use anyhow::{Context as _, Result};

use crate::AppDirs;

/// 从系统配置目录读取或创建安装 ID。
pub fn load_or_create_installation_id() -> Result<uuid::Uuid> {
    load_or_create_installation_id_with_dirs(&AppDirs::from_system()?)
}

/// 从显式配置目录读取或创建安装 ID。
pub fn load_or_create_installation_id_with_dirs(dirs: &AppDirs) -> Result<uuid::Uuid> {
    dirs.validate_state_root()?;
    let path = dirs.installation_id_file();
    match std::fs::read_to_string(&path) {
        Ok(value) => return parse_installation_id(&path, &value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read '{}'", path.display()));
        }
    }
    dirs.ensure_state_parent(&path)?;
    let id = uuid::Uuid::new_v4();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            set_private_file_permissions(&file, &path)?;
            if let Err(error) = writeln!(file, "{id}").and_then(|_| file.sync_all()) {
                drop(file);
                // 仅清理由本次 create_new 所有的半成品，不能替换并发实例的已有值。
                let _ = std::fs::remove_file(&path);
                return Err(error)
                    .with_context(|| format!("failed to persist '{}'", path.display()));
            }
            Ok(id)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let value = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read raced '{}'", path.display()))?;
            parse_installation_id(&path, &value)
        }
        Err(error) => Err(error).with_context(|| format!("failed to create '{}'", path.display())),
    }
}

fn set_private_file_permissions(file: &std::fs::File, path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = file
            .metadata()
            .with_context(|| format!("failed to inspect '{}'", path.display()))?
            .permissions();
        permissions.set_mode(0o600);
        file.set_permissions(permissions)
            .with_context(|| format!("failed to protect '{}'", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (file, path);
    }
    Ok(())
}

fn parse_installation_id(path: &Path, value: &str) -> Result<uuid::Uuid> {
    uuid::Uuid::parse_str(value.trim())
        .with_context(|| format!("'{}' contains an invalid installation id", path.display()))
}
