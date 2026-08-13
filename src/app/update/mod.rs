// @author kongweiguang

//! Application-wide update coordinator.
//!
//! 多窗口共享一个权威状态与一个后台 worker；UI 只能发送命令，网络、缓存和状态转换
//! 都集中在这里，避免两个窗口同时下载或启动两次安装事务。

use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use futures::StreamExt as _;
use futures::channel::{mpsc, oneshot};
use gmark_update_core::{
    ApplyPlanV1, CancellationV1, HelperSignalV1, clear_helper_signal, parse_apply_result,
};
use gpui::{App, AppContext as _, AsyncApp, Context, Entity, Global, Task};

use crate::net::update_v2::{
    self, CheckOrigin, CheckOutcome, DownloadControl, DownloadEvent, UpdateRelease,
};

const AUTO_CHECK_DELAY: Duration = Duration::from_secs(10);
const AUTO_CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Resolve and provision the one cache root used by the update protocol.
///
/// The root is deliberately established before the coordinator is published:
/// a missing or unsafe directory must make updater state observable as failed,
/// rather than silently moving a transaction into a process-temporary path.
pub(crate) fn update_cache_root() -> Result<PathBuf> {
    let (root, app_dirs) = {
        #[cfg(feature = "updater-e2e")]
        {
            if let Some(path) = std::env::var_os("GMARK_UPDATER_E2E_UPDATE_ROOT") {
                let path = PathBuf::from(path);
                if path.as_os_str().is_empty() {
                    bail!("updater E2E update root must not be empty");
                }
                (path, None)
            } else {
                let dirs = gmark_config::AppDirs::from_system()
                    .context("failed to resolve application directories")?;
                (dirs.updates_dir(), Some(dirs))
            }
        }
        #[cfg(not(feature = "updater-e2e"))]
        {
            let dirs = gmark_config::AppDirs::from_system()
                .context("failed to resolve application directories")?;
            (dirs.updates_dir(), Some(dirs))
        }
    };

    if !root.is_absolute() {
        bail!(
            "updater update root must be an absolute path: {}",
            root.display()
        );
    }
    if root.parent().is_none_or(|parent| parent == root.as_path()) {
        bail!("updater update root must not be a filesystem root");
    }
    if root
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        bail!(
            "updater update root must be a normalized path: {}",
            root.display()
        );
    }

    if let Some(dirs) = app_dirs {
        let sentinel = root.join(".gmark-update-root");
        dirs.ensure_cache_parent(&sentinel).with_context(|| {
            format!(
                "failed to create updater update root '{}': cache",
                root.display()
            )
        })?;
        validate_real_directory(&root)?;
    } else {
        ensure_real_directory_tree(&root)?;
    }
    Ok(root)
}

fn ensure_real_directory_tree(root: &Path) -> Result<()> {
    let mut pending = Vec::new();
    let mut current = root.to_path_buf();
    let existing = loop {
        match fs::symlink_metadata(&current) {
            Ok(metadata) => break Some((current.clone(), metadata)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                pending.push(current.clone());
                let Some(parent) = current.parent() else {
                    break None;
                };
                if parent == current || parent.as_os_str().is_empty() {
                    break None;
                }
                current = parent.to_path_buf();
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect updater update root '{}': existing",
                        current.display()
                    )
                });
            }
        }
    };
    if let Some((existing_path, metadata)) = existing {
        validate_real_directory_metadata(&existing_path, &metadata)?;
    }
    for directory in pending.into_iter().rev() {
        match fs::DirBuilder::new().create(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to create updater update root '{}': directory",
                        directory.display()
                    )
                });
            }
        }
        let metadata = fs::symlink_metadata(&directory).with_context(|| {
            format!(
                "failed to inspect updater update root '{}'",
                directory.display()
            )
        })?;
        validate_real_directory_metadata(&directory, &metadata)?;
    }
    Ok(())
}

fn validate_real_directory(root: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(root).with_context(|| {
        format!(
            "failed to inspect updater update root '{}': final",
            root.display()
        )
    })?;
    validate_real_directory_metadata(root, &metadata)
}

fn validate_real_directory_metadata(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if metadata.file_type().is_symlink() {
        bail!(
            "updater update root contains a symbolic link: {}",
            path.display()
        );
    }
    if is_reparse_metadata(metadata) {
        bail!(
            "updater update root contains a reparse point: {}",
            path.display()
        );
    }
    if !metadata.is_dir() {
        bail!(
            "updater update root component is not a directory: {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_metadata(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_metadata(_metadata: &fs::Metadata) -> bool {
    false
}

mod coordinator;
mod install;
mod service;
mod state;

pub(crate) use coordinator::UpdateCoordinator;
use install::*;
use service::UpdateService;
use state::UpdateCommand;
pub(crate) use state::UpdateState;

pub(crate) fn resolve_current_update_target()
-> std::result::Result<install::CurrentUpdateTarget, String> {
    install::resolve_current_update_target()
}

#[cfg(test)]
#[path = "../../../tests/unit/updater/mod.rs"]
mod tests;
