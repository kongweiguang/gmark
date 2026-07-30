#![recursion_limit = "256"]
// @author kongweiguang

//! gmark - a block-based Markdown editor built with GPUI.
//!
//! Reads file paths from command-line arguments and opens one GPUI window per
//! file. With no arguments, a single empty window is created.

mod adapters;
mod app;
mod components;
mod document_host;
mod editor;
mod net;
mod platform;
mod source_tools;
mod spellcheck;
mod ui;

pub(crate) use adapters::{document_io, export, recovery, resource_io};
pub(crate) use app::bootstrap::cli;
pub(crate) use app::diagnostics::{crash_report, perf};
pub(crate) use app::{app_menu, config, preferences, updater};
pub(crate) use platform::accessibility;
#[cfg(target_os = "windows")]
pub(crate) use platform::single_instance;
#[cfg(target_os = "macos")]
pub(crate) use platform::url as file_url;
pub(crate) use platform::window as window_chrome;
pub(crate) use ui::{i18n, theme};

/// 启动 gmark 桌面应用。
///
/// 该门面保持二进制入口稳定；启动顺序、平台生命周期与窗口恢复仍由内部模块负责。
pub fn run() -> anyhow::Result<()> {
    app::bootstrap::run_app();
    Ok(())
}
