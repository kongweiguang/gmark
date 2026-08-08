// @author kongweiguang

//! GPUI-independent HTML, PDF, and PNG document export.
//!
//! The crate deliberately accepts serializable-looking theme values and a tiny
//! cancellation trait. Application shells own UI, theme conversion, dialogs,
//! and atomic destination writes; this crate owns only rendering and temporary
//! export resources.

#![forbid(unsafe_code)]

mod chromium;
mod clipboard;
mod html;
mod images;
mod markup;
mod math;
mod resources;
mod theme;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub use chromium::{
    chromium_pdf_params, file_url_from_path, png_screenshot_params, render_pdf,
    render_pdf_cancellable, render_png, render_png_cancellable,
};
pub use clipboard::{ClipboardFragment, ClipboardSelection, export_clipboard_fragment};
pub use html::{
    contains_tibetan_text, render_chromium_pdf_html_with_base_dir, render_html,
    render_html_fragment_with_base_dir, render_html_with_base_dir,
};
pub use resources::{
    PreparedHtmlResources, copy_export_asset_cancellable, count_local_resource_cards,
    prepare_html_resources, prepare_html_resources_with_progress,
};
pub use theme::{
    ExportColor, ExportColorScheme, ExportFontWeight, ExportTheme, ExportThemeColors,
    ExportThemeDimensions, ExportThemeTypography,
};

/// Small cancellation boundary shared by synchronous resource work and
/// Chromium-backed rendering. Integrators may use [`AtomicBool`] directly.
pub trait ExportCancellation {
    /// Returns whether the caller requested that export stop as soon as safely possible.
    fn is_cancelled(&self) -> bool;
}

impl ExportCancellation for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Acquire)
    }
}

impl<T: ExportCancellation + ?Sized> ExportCancellation for &T {
    fn is_cancelled(&self) -> bool {
        (**self).is_cancelled()
    }
}

/// Cloneable cancellation handle for integrations that do not already keep an
/// [`AtomicBool`].
#[derive(Clone, Default)]
pub struct ExportCancellationHandle {
    cancelled: Arc<AtomicBool>,
}

impl ExportCancellationHandle {
    /// Requests cancellation of associated export work.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Clears a previous cancellation request so the handle can be reused.
    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::Release);
    }
}

impl ExportCancellation for ExportCancellationHandle {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}
