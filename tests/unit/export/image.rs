// @author kongweiguang

use super::{png_screenshot_params, render_png, render_png_cancellable};
use crate::theme::Theme;
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use std::sync::atomic::AtomicBool;

#[test]
fn png_capture_uses_full_document_and_opaque_theme_background() {
    let params = png_screenshot_params();

    assert_eq!(params.full_page, Some(true));
    assert_eq!(params.omit_background, None);
    assert_eq!(params.cdp_params.format, Some(CaptureScreenshotFormat::Png));
}

#[test]
fn cancelled_png_export_returns_before_browser_launch() {
    let cancelled = AtomicBool::new(true);
    let error = render_png_cancellable("# Title", &Theme::default_theme(), "Doc", None, &cancelled)
        .unwrap_err();

    assert_eq!(error.to_string(), "export cancelled");
}

#[test]
fn render_png_returns_png_bytes_or_actionable_chromium_error() {
    match render_png("# Title\n\nBody", &Theme::default_theme(), "Doc", None) {
        Ok(png) => {
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
            assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 1280);
            assert!(u32::from_be_bytes(png[20..24].try_into().unwrap()) > 0);
        }
        Err(error) => {
            let message = error.to_string();
            assert!(
                message.contains("Chromium")
                    || message.contains("Chrome")
                    || message.contains("CHROME"),
                "unexpected image export error: {message}"
            );
        }
    }
}
