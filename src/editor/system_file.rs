// @author kongweiguang

//! Narrow platform adapter for opening local paths outside Gmark.

use std::path::Path;
use std::process::Command;

use anyhow::Context as _;

pub(super) fn open_with_system(path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer.exe");
        command.arg(path);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg("--").arg(path);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };
    command
        .spawn()
        .with_context(|| format!("failed to open '{}' with the system", path.display()))?;
    Ok(())
}

pub(super) fn reveal_in_file_manager(path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer.exe");
        if path.is_dir() {
            command.arg(path);
        } else {
            command.arg("/select,").arg(path);
        }
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg("-R").arg(path);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        });
        command
    };
    command
        .spawn()
        .with_context(|| format!("failed to reveal '{}'", path.display()))?;
    Ok(())
}
