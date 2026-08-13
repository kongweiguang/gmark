// @author kongweiguang

#![recursion_limit = "256"]
// Reason: platform, recovery, and pane extension points compile across feature targets; remove when every feature-specific path has dedicated coverage.
#![allow(dead_code)]
// Reason: shared imports support mutually exclusive feature targets; remove when each target owns its imports.
#![allow(unused_imports)]
// Reason: public constructors expose private implementation state to crate-local callers; remove when those callers use opaque adapters.
#![allow(private_interfaces)]
// Reason: typed errors preserve recovery details across the app boundary; remove when the public error contract is boxed without losing them.
#![allow(clippy::result_large_err)]
// Reason: callback and adapter signatures keep trait-object guarantees at the boundary; remove when named aliases replace the complex types.
#![allow(clippy::type_complexity)]
//! Gmark - a block-based Markdown editor built with GPUI.
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
