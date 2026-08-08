// @author kongweiguang

//! Theme-to-domain adapter for HTML export.

use std::path::Path;

use crate::theme::Theme;

use super::theme::export_theme;

#[cfg(test)]
pub(crate) use gmark_export::contains_tibetan_text;
pub(crate) use gmark_export::{
    PreparedHtmlResources, count_local_resource_cards, prepare_html_resources_with_progress,
};
#[cfg(test)]
pub(crate) use gmark_export::{copy_export_asset_cancellable, prepare_html_resources};

#[cfg(test)]
pub(crate) fn render_html(markdown: &str, theme: &Theme, title: &str) -> String {
    gmark_export::render_html(markdown, &export_theme(theme), title)
}

pub(crate) fn render_html_with_base_dir(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_dir: Option<&Path>,
) -> String {
    gmark_export::render_html_with_base_dir(markdown, &export_theme(theme), title, base_dir)
}

/// Renders a safe body fragment for the system rich clipboard adapter.
pub(crate) fn render_clipboard_fragment_with_base_dir(
    markdown: &str,
    theme: &Theme,
    base_dir: Option<&Path>,
) -> String {
    gmark_export::render_html_fragment_with_base_dir(markdown, &export_theme(theme), base_dir)
}

#[cfg(test)]
pub(crate) fn render_chromium_pdf_html_with_base_dir(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_dir: Option<&Path>,
) -> String {
    gmark_export::render_chromium_pdf_html_with_base_dir(
        markdown,
        &export_theme(theme),
        title,
        base_dir,
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/export/html.rs"]
mod tests;
