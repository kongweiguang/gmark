// @author kongweiguang

//! Chromium-backed PDF and PNG rendering with bounded waits and RAII cleanup.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::{Context as _, anyhow};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::cdp::browser_protocol::page::{CaptureScreenshotFormat, PrintToPdfParams};
use chromiumoxide::page::ScreenshotParams;
use futures::StreamExt;
use uuid::Uuid;

use crate::ExportCancellation;
use crate::ExportTheme;
use crate::html::{render_chromium_pdf_html_with_base_dir, render_html_with_base_dir};

const VIEWPORT_WIDTH: u32 = 1280;
const VIEWPORT_HEIGHT: u32 = 1600;
const CHROMIUM_TIMEOUT: Duration = Duration::from_secs(45);

/// Renders a full-page PNG through a local Chromium-compatible browser.
pub fn render_png(
    markdown: &str,
    theme: &ExportTheme,
    title: &str,
    base_path: Option<&Path>,
) -> anyhow::Result<Vec<u8>> {
    render_png_cancellable(markdown, theme, title, base_path, &AtomicBool::new(false))
}

/// Renders a full-page PNG, returning an explicit cancellation or timeout error.
pub fn render_png_cancellable<C: ExportCancellation + ?Sized>(
    markdown: &str,
    theme: &ExportTheme,
    title: &str,
    base_path: Option<&Path>,
    cancelled: &C,
) -> anyhow::Result<Vec<u8>> {
    if cancelled.is_cancelled() {
        return Err(anyhow!("export cancelled"));
    }
    let runtime = export_runtime(
        "gmark-image-export",
        "failed to create image export runtime",
    )?;
    runtime.block_on(async {
        tokio::time::timeout(CHROMIUM_TIMEOUT, async {
            tokio::select! {
                result = render_png_async(markdown, theme, title, base_path) => result,
                () = wait_for_export_cancel(cancelled) => Err(anyhow!("export cancelled")),
            }
        })
        .await
        .map_err(|_| anyhow!("image export timed out while waiting for Chromium"))?
    })
}

/// Renders PDF bytes through Chromium's print pipeline.
pub fn render_pdf(
    markdown: &str,
    theme: &ExportTheme,
    title: &str,
    base_path: Option<&Path>,
) -> anyhow::Result<Vec<u8>> {
    render_pdf_cancellable(markdown, theme, title, base_path, &AtomicBool::new(false))
}

/// Renders PDF bytes, observing cancellation while Chromium loads and prints.
pub fn render_pdf_cancellable<C: ExportCancellation + ?Sized>(
    markdown: &str,
    theme: &ExportTheme,
    title: &str,
    base_path: Option<&Path>,
    cancelled: &C,
) -> anyhow::Result<Vec<u8>> {
    if cancelled.is_cancelled() {
        return Err(anyhow!("export cancelled"));
    }
    let runtime = export_runtime("gmark-pdf-export", "failed to create PDF export runtime")?;
    runtime.block_on(async {
        tokio::time::timeout(CHROMIUM_TIMEOUT, async {
            tokio::select! {
                result = render_pdf_async(markdown, theme, title, base_path) => result,
                () = wait_for_export_cancel(cancelled) => Err(anyhow!("export cancelled")),
            }
        })
        .await
        .map_err(|_| anyhow!("PDF export timed out while waiting for Chromium"))?
    })
}

/// Screenshot parameters used by the public PNG export.
pub fn png_screenshot_params() -> ScreenshotParams {
    ScreenshotParams::builder()
        .format(CaptureScreenshotFormat::Png)
        .full_page(true)
        .build()
}

/// Chromium print settings shared by all PDF exports.
pub fn chromium_pdf_params() -> PrintToPdfParams {
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

/// Produces a correctly encoded `file:` URL for a local temporary HTML file.
pub fn file_url_from_path(path: &Path) -> anyhow::Result<url::Url> {
    url::Url::from_file_path(path)
        .map_err(|_| anyhow!("failed to convert '{}' to a file URL", path.display()))
}

async fn render_png_async(
    markdown: &str,
    theme: &ExportTheme,
    title: &str,
    base_path: Option<&Path>,
) -> anyhow::Result<Vec<u8>> {
    let html = render_html_with_base_dir(markdown, theme, title, base_path);
    let temp = ChromiumTempFiles::create(
        "gmark-image-export",
        "gmark-image-profile",
        "failed to create Chromium profile",
        &html,
    )?;
    let config = browser_config(&temp)?;
    let (mut browser, mut handler) = Browser::launch(config).await.map_err(|error| {
        anyhow!(
            "failed to launch Chromium for image export: {error}. Install Chrome, Chromium, or Edge, or set the CHROME environment variable to the browser executable path"
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
        let file_url = file_url_from_path(&temp.html_path)?;
        let page = browser
            .new_page(file_url.as_str())
            .await
            .context("failed to open export HTML in Chromium")?;
        page.wait_for_navigation()
            .await
            .context("Chromium did not finish loading export HTML")?;
        page.execute(SetDeviceMetricsOverrideParams::new(
            VIEWPORT_WIDTH,
            VIEWPORT_HEIGHT,
            1.0,
            false,
        ))
        .await
        .context("Chromium failed to set the image export viewport")?;
        page.screenshot(png_screenshot_params())
            .await
            .context("Chromium failed to capture export HTML as PNG")
    }
    .await;
    let _ = browser.close().await;
    handler_task.abort();
    result
}

async fn render_pdf_async(
    markdown: &str,
    theme: &ExportTheme,
    title: &str,
    base_path: Option<&Path>,
) -> anyhow::Result<Vec<u8>> {
    let html = render_chromium_pdf_html_with_base_dir(markdown, theme, title, base_path);
    let temp = ChromiumTempFiles::create(
        "gmark-export",
        "gmark-chromium-profile",
        "failed to create",
        &html,
    )?;
    let config = browser_config(&temp)?;
    let (mut browser, mut handler) = Browser::launch(config).await.map_err(|error| {
        anyhow!(
            "failed to launch Chromium for PDF export: {error}. Install Chrome, Chromium, or Edge, or set the CHROME environment variable to the browser executable path"
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
        let file_url = file_url_from_path(&temp.html_path)?;
        let page = browser
            .new_page(file_url.as_str())
            .await
            .context("failed to open export HTML in Chromium")?;
        page.wait_for_navigation()
            .await
            .context("Chromium did not finish loading export HTML")?;
        page.pdf(chromium_pdf_params())
            .await
            .context("Chromium failed to print export HTML to PDF")
    }
    .await;
    let _ = browser.close().await;
    handler_task.abort();
    result
}

fn export_runtime(
    name: &str,
    error_context: &'static str,
) -> anyhow::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name(name)
        .build()
        .context(error_context)
}

fn browser_config(temp: &ChromiumTempFiles) -> anyhow::Result<BrowserConfig> {
    BrowserConfig::builder()
        .new_headless_mode()
        .window_size(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)
        .user_data_dir(temp.user_data_dir.clone())
        .build()
        .map_err(|error| anyhow!("failed to build Chromium browser config: {error}"))
}

async fn wait_for_export_cancel<C: ExportCancellation + ?Sized>(cancelled: &C) {
    while !cancelled.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

struct ChromiumTempFiles {
    html_path: PathBuf,
    user_data_dir: PathBuf,
}

impl ChromiumTempFiles {
    fn create(
        html_prefix: &str,
        profile_prefix: &str,
        profile_error_context: &'static str,
        html: &str,
    ) -> anyhow::Result<Self> {
        let id = Uuid::new_v4();
        let html_path = std::env::temp_dir().join(format!("{html_prefix}-{id}.html"));
        let user_data_dir = std::env::temp_dir().join(format!("{profile_prefix}-{id}"));
        fs::write(&html_path, html)
            .with_context(|| format!("failed to write temporary HTML '{}'", html_path.display()))?;
        if let Err(error) = fs::create_dir_all(&user_data_dir) {
            let _ = fs::remove_file(&html_path);
            return Err(error)
                .with_context(|| format!("{profile_error_context} '{}'", user_data_dir.display()));
        }
        Ok(Self {
            html_path,
            user_data_dir,
        })
    }
}

impl Drop for ChromiumTempFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.html_path);
        let _ = fs::remove_dir_all(&self.user_data_dir);
    }
}
