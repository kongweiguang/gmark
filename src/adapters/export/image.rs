// @author kongweiguang

//! Theme-to-domain adapter for Chromium PNG export.

use std::path::Path;

use crate::theme::Theme;

use super::theme::export_theme;

#[cfg(test)]
pub(crate) use gmark_export::png_screenshot_params;

#[cfg(test)]
pub(crate) fn render_png(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_path: Option<&Path>,
) -> anyhow::Result<Vec<u8>> {
    gmark_export::render_png(markdown, &export_theme(theme), title, base_path)
}

pub(crate) fn render_png_cancellable(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_path: Option<&Path>,
    cancelled: &std::sync::atomic::AtomicBool,
) -> anyhow::Result<Vec<u8>> {
    gmark_export::render_png_cancellable(
        markdown,
        &export_theme(theme),
        title,
        base_path,
        cancelled,
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/export/image.rs"]
mod tests;
