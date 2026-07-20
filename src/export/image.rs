// @author kongweiguang

//! PNG generation through a local Chromium-compatible browser.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context as _, anyhow};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::page::ScreenshotParams;
use futures::StreamExt;
use uuid::Uuid;

use crate::export::html::render_html_with_base_dir;
use crate::theme::Theme;

const IMAGE_VIEWPORT_WIDTH: u32 = 1280;
const IMAGE_VIEWPORT_HEIGHT: u32 = 1600;
const IMAGE_TIMEOUT: Duration = Duration::from_secs(45);

#[cfg(test)]
pub(crate) fn render_png(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_path: Option<&Path>,
) -> anyhow::Result<Vec<u8>> {
    let cancelled = AtomicBool::new(false);
    render_png_cancellable(markdown, theme, title, base_path, &cancelled)
}

pub(crate) fn render_png_cancellable(
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
        .thread_name("gmark-image-export")
        .build()
        .context("failed to create image export runtime")?;
    runtime.block_on(async move {
        tokio::time::timeout(IMAGE_TIMEOUT, async {
            tokio::select! {
                result = render_png_async(markdown, theme, title, base_path) => result,
                () = wait_for_export_cancel(cancelled) => Err(anyhow!("export cancelled")),
            }
        })
        .await
        .map_err(|_| anyhow!("image export timed out while waiting for Chromium"))?
    })
}

async fn render_png_async(
    markdown: &str,
    theme: &Theme,
    title: &str,
    base_path: Option<&Path>,
) -> anyhow::Result<Vec<u8>> {
    let html = render_html_with_base_dir(markdown, theme, title, base_path);
    let temp = ImageTempFiles::create(&html)?;
    let config = BrowserConfig::builder()
        .new_headless_mode()
        .window_size(IMAGE_VIEWPORT_WIDTH, IMAGE_VIEWPORT_HEIGHT)
        .user_data_dir(temp.user_data_dir.clone())
        .build()
        .map_err(|error| anyhow!("failed to build Chromium browser config: {error}"))?;
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
        let file_url = url::Url::from_file_path(&temp.html_path).map_err(|_| {
            anyhow!(
                "failed to convert '{}' to a file URL",
                temp.html_path.display()
            )
        })?;
        let page = browser
            .new_page(file_url.as_str())
            .await
            .context("failed to open export HTML in Chromium")?;
        page.wait_for_navigation()
            .await
            .context("Chromium did not finish loading export HTML")?;
        // Headless Chromium may ignore the OS window size and default to 800px; the explicit
        // device metrics keep exported image wrapping deterministic across platforms.
        page.execute(SetDeviceMetricsOverrideParams::new(
            IMAGE_VIEWPORT_WIDTH,
            IMAGE_VIEWPORT_HEIGHT,
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

fn png_screenshot_params() -> ScreenshotParams {
    ScreenshotParams::builder()
        .format(CaptureScreenshotFormat::Png)
        .full_page(true)
        .build()
}

async fn wait_for_export_cancel(cancelled: &AtomicBool) {
    while !cancelled.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

struct ImageTempFiles {
    html_path: PathBuf,
    user_data_dir: PathBuf,
}

impl ImageTempFiles {
    fn create(html: &str) -> anyhow::Result<Self> {
        let id = Uuid::new_v4();
        let html_path = std::env::temp_dir().join(format!("gmark-image-export-{id}.html"));
        let user_data_dir = std::env::temp_dir().join(format!("gmark-image-profile-{id}"));
        fs::write(&html_path, html)
            .with_context(|| format!("failed to write temporary HTML '{}'", html_path.display()))?;
        fs::create_dir_all(&user_data_dir).with_context(|| {
            format!(
                "failed to create Chromium profile '{}'",
                user_data_dir.display()
            )
        })?;
        Ok(Self {
            html_path,
            user_data_dir,
        })
    }
}

impl Drop for ImageTempFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.html_path);
        let _ = fs::remove_dir_all(&self.user_data_dir);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/export/image.rs"]
mod tests;
