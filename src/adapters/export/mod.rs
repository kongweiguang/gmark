// @author kongweiguang

//! Main-package adapters for the GPUI-independent export engine.

mod html;
mod image;
mod pdf;
mod theme;

/// Export target selected from the app menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    /// Full HTML document with embedded theme CSS.
    Html,
    /// Full-document PNG rendered from the themed HTML document.
    Png,
    /// PDF bytes rendered from the themed HTML document.
    Pdf,
}

impl ExportFormat {
    /// File extension used for save-dialog defaults.
    pub(crate) const fn extension(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Png => "png",
            Self::Pdf => "pdf",
        }
    }
}

pub(crate) use html::{
    PreparedHtmlResources, count_local_resource_cards, prepare_html_resources_with_progress,
    render_clipboard_fragment_with_base_dir, render_html_with_base_dir,
};
#[cfg(test)]
pub(crate) use image::render_png;
pub(crate) use image::render_png_cancellable;
#[cfg(test)]
pub(crate) use pdf::render_pdf;
pub(crate) use pdf::render_pdf_cancellable;
