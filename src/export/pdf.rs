// @author kongweiguang

//! PDF generation through a local Chromium-compatible browser.
//!
//! The browser HTML export is the source of truth for visual PDF fidelity. This
//! module writes that HTML to a temporary file, opens it in headless Chromium,
//! and asks DevTools to print the page to PDF.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context as _, anyhow};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;
use futures::StreamExt;
use uuid::Uuid;

use crate::export::html::render_chromium_pdf_html_with_base_dir;
use crate::theme::Theme;

const CHROMIUM_VIEWPORT_WIDTH: u32 = 1280;
const CHROMIUM_VIEWPORT_HEIGHT: u32 = 1600;
const PDF_TIMEOUT: Duration = Duration::from_secs(45);

/// Renders themed PDF bytes from Markdown through the local Chromium print engine.
#[cfg(test)]
pub(crate) fn render_pdf(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_path: Option<&Path>,
) -> anyhow::Result<Vec<u8>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("gmark-pdf-export")
        .build()
        .context("failed to create PDF export runtime")?;

    runtime.block_on(async move {
        tokio::time::timeout(
            PDF_TIMEOUT,
            render_pdf_async(markdown, theme, title, base_path),
        )
        .await
        .map_err(|_| anyhow!("PDF export timed out while waiting for Chromium"))?
    })
}

pub(crate) async fn render_pdf_async(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_path: Option<&Path>,
) -> anyhow::Result<Vec<u8>> {
    let html = render_chromium_pdf_html_with_base_dir(markdown, theme, title, base_path);
    let temp = PdfTempFiles::create(&html)?;
    let result = render_pdf_from_html_file_async(temp.html_path.clone()).await;
    temp.cleanup();
    result
}

async fn render_pdf_from_html_file_async(html_path: PathBuf) -> anyhow::Result<Vec<u8>> {
    let user_data_dir = unique_temp_path("gmark-chromium-profile");
    fs::create_dir_all(&user_data_dir)
        .with_context(|| format!("failed to create '{}'", user_data_dir.display()))?;

    let config = BrowserConfig::builder()
        .new_headless_mode()
        .window_size(CHROMIUM_VIEWPORT_WIDTH, CHROMIUM_VIEWPORT_HEIGHT)
        .user_data_dir(user_data_dir.clone())
        .build()
        .map_err(|err| anyhow!("failed to build Chromium browser config: {err}"))?;

    let (mut browser, mut handler) = Browser::launch(config).await.map_err(|err| {
        anyhow!(
            "failed to launch Chromium for PDF export: {err}. Install Chrome, Chromium, or Edge, or set the CHROME environment variable to the browser executable path"
        )
    })?;

    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    let result = async {
        let file_url = file_url_from_path(&html_path)?;
        let page = browser
            .new_page(file_url.as_str())
            .await
            .context("failed to open export HTML in Chromium")?;
        page.wait_for_navigation()
            .await
            .context("Chromium did not finish loading export HTML")?;

        let params = chromium_pdf_params();
        page.pdf(params)
            .await
            .context("Chromium failed to print export HTML to PDF")
    }
    .await;

    let _ = browser.close().await;
    handler_task.abort();
    let _ = fs::remove_dir_all(&user_data_dir);

    result
}

fn chromium_pdf_params() -> PrintToPdfParams {
    PrintToPdfParams {
        print_background: Some(true),
        prefer_css_page_size: Some(true),
        paper_width: Some(8.27),
        paper_height: Some(11.69),
        margin_top: Some(0.0),
        margin_bottom: Some(0.0),
        margin_left: Some(0.0),
        margin_right: Some(0.0),
        ..Default::default()
    }
}

pub(crate) fn render_pdf_cancellable(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_path: Option<&Path>,
    cancelled: &AtomicBool,
) -> anyhow::Result<Vec<u8>> {
    if cancelled.load(Ordering::Acquire) {
        return Err(anyhow!("export cancelled"));
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("gmark-pdf-export")
        .build()
        .context("failed to create PDF export runtime")?;
    runtime.block_on(async move {
        tokio::time::timeout(PDF_TIMEOUT, async {
            tokio::select! {
                result = render_pdf_async(markdown, theme, title, base_path) => result,
                () = wait_for_export_cancel(cancelled) => Err(anyhow!("export cancelled")),
            }
        })
        .await
        .map_err(|_| anyhow!("PDF export timed out while waiting for Chromium"))?
    })
}

async fn wait_for_export_cancel(cancelled: &AtomicBool) {
    while !cancelled.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn file_url_from_path(path: &Path) -> anyhow::Result<url::Url> {
    url::Url::from_file_path(path)
        .map_err(|_| anyhow!("failed to convert '{}' to a file URL", path.display()))
}

fn unique_temp_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
}

struct PdfTempFiles {
    html_path: PathBuf,
}

impl PdfTempFiles {
    fn create(html: &str) -> anyhow::Result<Self> {
        let html_path = unique_temp_path("gmark-export").with_extension("html");
        fs::write(&html_path, html)
            .with_context(|| format!("failed to write temporary HTML '{}'", html_path.display()))?;
        Ok(Self { html_path })
    }

    fn cleanup(&self) {
        let _ = fs::remove_file(&self.html_path);
    }
}

impl Drop for PdfTempFiles {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/export/pdf.rs"]
mod tests;
