// @author kongweiguang

//! 稳定、匿名的本地安装标识。

use std::{fs::OpenOptions, io::Write as _, path::Path};

use anyhow::{Context as _, Result};

use crate::ConfigDirs;

/// 从系统配置目录读取或创建安装 ID。
pub fn load_or_create_installation_id() -> Result<uuid::Uuid> {
    load_or_create_installation_id_with_dirs(&ConfigDirs::from_system()?)
}

/// 从显式配置目录读取或创建安装 ID。
pub fn load_or_create_installation_id_with_dirs(dirs: &ConfigDirs) -> Result<uuid::Uuid> {
    let path = dirs.installation_id_file();
    match std::fs::read_to_string(&path) {
        Ok(value) => return parse_installation_id(&path, &value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read '{}'", path.display()));
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }
    let id = uuid::Uuid::new_v4();
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
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

fn parse_installation_id(path: &Path, value: &str) -> Result<uuid::Uuid> {
    uuid::Uuid::parse_str(value.trim())
        .with_context(|| format!("'{}' contains an invalid installation id", path.display()))
}
