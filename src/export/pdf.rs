// @author kongweiguang

//! Theme-to-domain adapter for Chromium PDF export.

use std::path::Path;

use crate::theme::Theme;

use super::theme::export_theme;

#[cfg(test)]
pub(crate) use gmark_export::{chromium_pdf_params, file_url_from_path};

#[cfg(test)]
pub(crate) fn render_pdf(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_path: Option<&Path>,
) -> anyhow::Result<Vec<u8>> {
    gmark_export::render_pdf(markdown, &export_theme(theme), title, base_path)
}

pub(crate) fn render_pdf_cancellable(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_path: Option<&Path>,
    cancelled: &std::sync::atomic::AtomicBool,
) -> anyhow::Result<Vec<u8>> {
    gmark_export::render_pdf_cancellable(
        markdown,
        &export_theme(theme),
        title,
        base_path,
        cancelled,
    )
}

#[cfg(test)]
#[path = "../../tests/unit/export/pdf.rs"]
mod tests;
