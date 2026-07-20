// @author kongweiguang

use super::{chromium_pdf_params, file_url_from_path, render_pdf, render_pdf_cancellable};
use crate::export::html::render_chromium_pdf_html_with_base_dir;
use crate::theme::Theme;
use std::sync::atomic::AtomicBool;

#[test]
fn chromium_pdf_html_uses_print_layout_and_preserves_resources() {
    let html = render_chromium_pdf_html_with_base_dir(
        "# Title\n\n```mermaid\nflowchart LR\nA --> B\n```\n\n$$\nx^2\n$$",
        &Theme::default_theme(),
        "Doc",
        None,
    );

    assert!(html.contains("@page"));
    assert!(html.contains("size: A4"));
    assert!(html.contains("margin: 15mm"));
    assert!(html.contains("class=\"vlt-document\""));
    assert!(html.contains("data:image/svg+xml;base64,"));
    assert!(html.contains("<svg"));
    assert!(!html.contains("width: min(100% - 48px, 920px);"));
}

#[test]
fn chromium_pdf_params_use_page_css_and_backgrounds() {
    let params = chromium_pdf_params();

    assert_eq!(params.print_background, Some(true));
    assert_eq!(params.prefer_css_page_size, Some(true));
    assert_eq!(params.margin_top, Some(0.0));
    assert_eq!(params.margin_bottom, Some(0.0));
    assert_eq!(params.margin_left, Some(0.0));
    assert_eq!(params.margin_right, Some(0.0));
}

#[test]
fn file_url_from_path_supports_local_paths() {
    let path = std::env::temp_dir().join("gmark pdf test.html");
    let url = file_url_from_path(&path).expect("file url");

    assert_eq!(url.scheme(), "file");
    assert!(url.as_str().contains("gmark%20pdf%20test.html"));
}

#[test]
fn render_pdf_reports_actionable_error_without_chromium() {
    match render_pdf("# Title\n\nBody", &Theme::default_theme(), "Doc", None) {
        Ok(pdf) => assert!(pdf.starts_with(b"%PDF")),
        Err(err) => {
            let message = err.to_string();
            assert!(
                message.contains("Chromium")
                    || message.contains("Chrome")
                    || message.contains("CHROME"),
                "unexpected PDF export error: {message}"
            );
        }
    }
}

#[test]
fn cancelled_pdf_export_returns_before_browser_launch() {
    let cancelled = AtomicBool::new(true);
    let error = render_pdf_cancellable("# Title", &Theme::default_theme(), "Doc", None, &cancelled)
        .unwrap_err();
    assert_eq!(error.to_string(), "export cancelled");
}
