// @author kongweiguang

//! Background preparation helpers for the editor-file opening pipeline.

use super::*;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use futures::future::{Either, select};

pub(super) const FILE_OPEN_DEADLINE: Duration = Duration::from_secs(30);

#[derive(Clone, Default)]
pub(super) struct OpenCancellation {
    cancelled: Arc<AtomicBool>,
}

impl OpenCancellation {
    /// 只在同步阶段之间检查取消，保证超时后的结果不会继续进入 registry 发布点。
    // 原因：阻塞文件系统调用本身不可抢占，但调用边界仍可隔离迟到 owner。
    fn check(&self) -> anyhow::Result<()> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(anyhow::anyhow!("file open request was cancelled"))
        } else {
            Ok(())
        }
    }

    /// 标记当前准备代次失效；后台同步调用返回后会在下一个边界退出。
    // 原因：外层 select 丢弃 future 不足以停止已经进入 worker 的同步 I/O。
    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// 后台阶段产出的最小窗口输入；它只携带共享 lease 或已解码内容，避免
/// GPUI 回调重新探测、读取文件，保证首帧线程不触碰慢盘和 UNC 路径。
pub(super) enum PreparedFileOpen {
    Image {
        path: PathBuf,
    },
    Svg {
        path: PathBuf,
        opened: crate::document_io::OpenedMarkdown,
    },
    Resident {
        path: PathBuf,
        shared: SharedResidentOpen,
    },
    Host {
        path: PathBuf,
        shared: SharedDocumentHostOpen,
    },
}

/// 完成一次文件打开的所有磁盘、探测、解码和控制器构造；调用者必须把
/// 该函数放到后台 executor，才能让 GPUI 主线程只负责安装窗口。
pub(super) fn prepare_file_open(
    service: DocumentService,
    path: PathBuf,
    loading: gmark_document_core::LoadingPolicy,
    cancellation: OpenCancellation,
) -> anyhow::Result<PreparedFileOpen> {
    cancellation.check()?;
    if crate::document_io::is_image_path(&path) {
        crate::document_io::open_document_with_policy(&path, loading)
            .map_err(|error| anyhow::anyhow!("failed to open '{}': {error}", path.display()))?;
        cancellation.check()?;
        return Ok(PreparedFileOpen::Image { path });
    }
    if crate::document_io::is_known_unsupported_document(&path) {
        if let Err(error) = crate::document_io::open_document_with_policy(&path, loading) {
            return Err(anyhow::anyhow!(
                "failed to open '{}': {error}",
                path.display()
            ));
        }
        cancellation.check()?;
        return Err(anyhow::anyhow!("unsupported file type"));
    }

    let probe_cancellation = cancellation.clone();
    let probe = service
        .probe_file(&path, loading, |normalized, policy| {
            let probe = crate::document_io::probe_document_with_policy(normalized, policy)?;
            probe_cancellation.check()?;
            Ok::<_, anyhow::Error>(probe)
        })
        .map_err(|error| anyhow::anyhow!("failed to inspect '{}': {error}", path.display()))?;
    cancellation.check()?;

    if crate::document_io::is_markdown_path(&path)
        && probe.strategy == gmark_paged_document::OpenStrategy::Resident
    {
        let limits = loading.effective_limits();
        let resident_cancellation = cancellation.clone();
        let shared = service
            .open_resident_file(&path, loading, |normalized, _policy| {
                let opened =
                    crate::document_io::read_resident_text_from_probe(normalized, &probe, limits)?;
                resident_cancellation.check()?;
                Ok::<_, anyhow::Error>(ResidentMarkdownSource::from_opened(normalized, opened))
            })
            .map_err(|error| {
                service.clear_probe(&path, loading);
                anyhow::anyhow!("failed to open '{}': {error}", path.display())
            })?;
        cancellation.check()?;
        return Ok(PreparedFileOpen::Resident { path, shared });
    }

    if crate::document_io::is_svg_path(&path)
        && probe.strategy == gmark_paged_document::OpenStrategy::Resident
    {
        let opened = crate::document_io::read_resident_text_from_probe(
            &path,
            &probe,
            loading.effective_limits(),
        )
        .map_err(|error| {
            service.clear_probe(&path, loading);
            anyhow::anyhow!("failed to read '{}': {error}", path.display())
        })?;
        cancellation.check()?;
        return Ok(PreparedFileOpen::Svg { path, opened });
    }

    let result = if probe.strategy == gmark_paged_document::OpenStrategy::Paged {
        let paged_cancellation = cancellation.clone();
        service.open_paged(
            &path,
            probe.clone(),
            loading,
            |normalized, probe, _policy| {
                let source =
                    gmark_paged_document::FileSource::open(normalized).map_err(|error| {
                        anyhow::anyhow!("failed to open '{}': {error}", normalized.display())
                    })?;
                let prepared =
                    gmark_paged_document::prepare_utf8_source(source, probe.encoding.clone())
                        .map_err(|error| anyhow::anyhow!("failed to prepare source: {error}"))?;
                paged_cancellation.check()?;
                Ok::<_, anyhow::Error>(prepared)
            },
        )
    } else {
        let host_cancellation = cancellation.clone();
        service.open_document_host(
            &path,
            probe.clone(),
            loading,
            |normalized, probe, _policy| {
                let source =
                    gmark_paged_document::FileSource::open(normalized).map_err(|error| {
                        anyhow::anyhow!("failed to open '{}': {error}", normalized.display())
                    })?;
                let prepared =
                    gmark_paged_document::prepare_utf8_source(source, probe.encoding.clone())
                        .map_err(|error| anyhow::anyhow!("failed to prepare source: {error}"))?;
                host_cancellation.check()?;
                Ok::<_, anyhow::Error>(prepared)
            },
        )
    };
    cancellation.check()?;
    result
        .map(|shared| PreparedFileOpen::Host {
            path: path.clone(),
            shared,
        })
        .map_err(|error| {
            service.clear_probe(&path, loading);
            anyhow::anyhow!("failed to open '{}': {error}", path.display())
        })
}

/// 把后台准备结果安装到 GPUI；这里不再做任何路径探测或正文读取，迟到
/// 结果只会到达仍存活的任务，从而不会覆盖后续打开请求的窗口状态。
pub(super) fn install_prepared_file_open(
    cx: &mut App,
    prepared: PreparedFileOpen,
) -> anyhow::Result<()> {
    match prepared {
        PreparedFileOpen::Image { path } => {
            super::open_image_preview_window(cx, path, None).map(|_| ())
        }
        PreparedFileOpen::Svg { path, opened } => {
            let handle = super::open_decoded_editor_window(cx, opened, Some(path))?;
            let _ = handle.update(cx, |editor, _window, cx| {
                editor.set_view_mode(crate::editor::ViewMode::Preview, cx);
            });
            Ok(())
        }
        PreparedFileOpen::Resident { path, shared } => {
            super::open_shared_resident_editor_window(cx, shared, Some(path), None).map(|_| ())
        }
        PreparedFileOpen::Host { path, shared } => {
            super::open_shared_document_host_window(cx, shared, path, None).map(|_| ())
        }
    }
}
