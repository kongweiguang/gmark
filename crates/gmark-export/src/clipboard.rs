// @author kongweiguang

//! Safe rich clipboard payloads derived from the export pipeline.

use std::path::Path;

use gmark_markdown::parse_markdown;

use crate::{ExportTheme, render_html_fragment_with_base_dir};

/// MIME payloads written by a platform clipboard adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardFragment {
    /// Markdown/plain fallback; this remains the source truth for paste.
    pub markdown: String,
    /// Safe HTML fragment for rich applications.
    pub html: String,
    /// Rendered semantic text used when a platform cannot accept HTML.
    pub plain_text: String,
}

/// Selection shape needed by the two supported clipboard paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClipboardSelection {
    /// A normal contiguous Markdown selection.
    Markdown { markdown: String },
    /// A rectangular native-table selection.
    Table { markdown: String, tsv: String },
}

impl ClipboardSelection {
    fn markdown(&self) -> &str {
        match self {
            Self::Markdown { markdown } | Self::Table { markdown, .. } => markdown,
        }
    }

    fn plain_text(&self, projected: String) -> String {
        match self {
            Self::Markdown { .. } => projected,
            Self::Table { tsv, .. } => tsv.clone(),
        }
    }
}

/// Creates a safe HTML + Markdown clipboard payload from one already selected
/// semantic fragment. The source is never reparsed by a second Markdown
/// implementation; Gmark's value model supplies the fallback text.
pub fn export_clipboard_fragment(
    selection: ClipboardSelection,
    theme: &ExportTheme,
    base_dir: Option<&Path>,
) -> ClipboardFragment {
    let markdown = selection.markdown().to_owned();
    let projection = parse_markdown(&markdown).visible_text_projection();
    // `render_html_fragment_with_base_dir` is the same sanitized rewrite
    // pipeline used by standalone HTML and Chromium/PDF export. Do not run
    // the Markdown HTML sanitizer over the completed fragment a second time:
    // that policy intentionally rejects generated `data:image/*` and SVG
    // payloads, which would make otherwise safe math and Mermaid output
    // disappear from rich clipboard consumers.
    let html = render_html_fragment_with_base_dir(&markdown, theme, base_dir);
    ClipboardFragment {
        markdown,
        plain_text: selection.plain_text(projection.text),
        html,
    }
}
