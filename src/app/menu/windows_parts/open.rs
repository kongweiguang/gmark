// @author kongweiguang

//! Editor window construction and document-service adapters.

use super::*;

#[path = "open_parts.rs"]
mod open_parts;

use open_parts::{
    FILE_OPEN_DEADLINE, OpenCancellation, PreparedFileOpen, install_prepared_file_open,
    prepare_file_open,
};

use futures::future::{Either, select};

// 原因：所有窗口统一生成标题，保证不同打开入口对同一路径使用一致的用户可见名称。
pub(super) fn window_title(file_path: Option<&Path>) -> SharedString {
    if let Some(path) = file_path {
        // OsStr::to_string_lossy returns Cow<str>; calling .to_string() on
        // it always allocates a fresh String, even for the valid-UTF-8 path
        // (the common case). Borrow the Cow directly into format! — its
        // Display impl writes the borrowed bytes straight into the output
        // String, no intermediate allocation.
        format!(
            "Gmark - {}",
            path.file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_else(|| path.to_string_lossy())
        )
        .into()
    } else {
        SharedString::new("Gmark")
    }
}

/// Opens an editor window for the given Markdown content and optional path.
// 原因：保留轻量 Markdown 入口并委托统一解码路径，避免启动和恢复窗口分叉。
pub(crate) fn open_editor_window(
    cx: &mut App,
    markdown: String,
    file_path: Option<PathBuf>,
) -> anyhow::Result<WindowHandle<Editor>> {
    open_decoded_editor_window(
        cx,
        crate::document_io::OpenedMarkdown {
            text: markdown,
            encoding: crate::document_io::DocumentEncoding::Utf8,
            text_encoding: gmark_document_core::TextEncoding::Utf8 { bom: false },
            file_identity: None,
            loading_limits: gmark_document_core::LoadingPolicy::default().effective_limits(),
        },
        file_path,
    )
}

// 原因：让已解码内容继续经过带边界处理的窗口构造，确保默认与恢复 bounds 语义一致。
pub(crate) fn open_decoded_editor_window(
    cx: &mut App,
    opened: crate::document_io::OpenedMarkdown,
    file_path: Option<PathBuf>,
) -> anyhow::Result<WindowHandle<Editor>> {
    open_decoded_editor_window_with_bounds(cx, opened, file_path, None)
}

// 原因：把服务 lease 交给 Editor 生命周期持有，保证窗口关闭前文档运行时不会被提前释放。
fn open_shared_document_host_window(
    cx: &mut App,
    shared: SharedDocumentHostOpen,
    path: PathBuf,
    restored_bounds: Option<WindowBounds>,
) -> anyhow::Result<WindowHandle<Editor>> {
    let structured_preview = shared.probe.strategy == gmark_paged_document::OpenStrategy::Resident
        && matches!(
            shared.probe.format,
            gmark_document_core::DocumentFormat::Json
                | gmark_document_core::DocumentFormat::Delimited { .. }
        );
    let SharedDocumentHostOpen { lease, probe, .. } = shared;
    let handle = lease.handle();
    let title = window_title(Some(&path));
    let options = restored_bounds.map_or_else(
        || {
            let bounds = Bounds::centered(None, size(px(1080.), px(720.)), cx);
            gmark_window_options(title.clone(), bounds)
        },
        |bounds| gmark_window_options_with_bounds(title.clone(), bounds),
    );
    let handle = cx
        .open_window(options, move |window, cx| {
            let editor =
                cx.new(move |cx| Editor::from_shared_document_host(cx, path, probe, handle, lease));
            if structured_preview {
                editor.update(cx, |editor, cx| {
                    editor.set_view_mode(crate::editor::ViewMode::Preview, cx);
                });
            }
            editor.update(cx, |editor, cx| {
                editor.install_accessibility_bridge(window, cx)
            });
            editor
        })
        .map_err(|error| anyhow::anyhow!("failed to create large-document window: {error}"))?;
    handle
        .update(cx, |editor, window, cx| {
            window.activate_window();
            editor.force_install_close_guard(cx, window);
        })
        .map_err(|error| anyhow::anyhow!("failed to initialize large-document window: {error}"))?;
    Ok(handle)
}

// 原因：集中处理窗口 bounds 和可访问性安装，避免不同编辑器入口产生布局差异。
fn open_decoded_editor_window_with_bounds(
    cx: &mut App,
    opened: crate::document_io::OpenedMarkdown,
    file_path: Option<PathBuf>,
    restored_bounds: Option<WindowBounds>,
) -> anyhow::Result<WindowHandle<Editor>> {
    let title = window_title(file_path.as_deref());
    let options = restored_bounds.map_or_else(
        || {
            let bounds = Bounds::centered(None, size(px(1080.), px(720.)), cx);
            gmark_window_options(title.clone(), bounds)
        },
        |bounds| gmark_window_options_with_bounds(title.clone(), bounds),
    );
    let handle = cx
        .open_window(options, move |window, cx| {
            let editor = cx.new(move |cx| Editor::from_opened_markdown(cx, opened, file_path));
            editor.update(cx, |editor, cx| {
                editor.install_accessibility_bridge(window, cx)
            });
            editor
        })
        .map_err(|error| anyhow::anyhow!("failed to open editor window: {error}"))?;

    if let Err(error) = handle.update(cx, |editor, window, cx| {
        window.activate_window();
        editor.force_install_close_guard(cx, window);
    }) {
        eprintln!("failed to initialize editor window: {error}");
    }

    Ok(handle)
}

/// Construct one Editor view over the service-owned Resident Controller.
///
/// The lease is moved into the constructor closure.  If adapter construction
/// fails after the registry has opened the document, keep the established file
/// failure surface rather than creating a second resident body for recovery.
// 原因：Resident 文档使用同一服务控制器，构造失败时回落既有错误面而不复制正文。
fn open_shared_resident_editor_window(
    cx: &mut App,
    shared: SharedResidentOpen,
    file_path: Option<PathBuf>,
    restored_bounds: Option<WindowBounds>,
) -> anyhow::Result<WindowHandle<Editor>> {
    let title = window_title(file_path.as_deref());
    let options = restored_bounds.map_or_else(
        || {
            let bounds = Bounds::centered(None, size(px(1080.), px(720.)), cx);
            gmark_window_options(title.clone(), bounds)
        },
        |bounds| gmark_window_options_with_bounds(title.clone(), bounds),
    );
    let handle = cx
        .open_window(options, move |window, cx| {
            let fallback_path = file_path.clone();
            let editor =
                cx.new(
                    move |cx| match Editor::from_shared_resident_open(cx, shared, file_path) {
                        Ok(editor) => editor,
                        Err(error) => {
                            let reason = error.to_string();
                            let mut editor =
                                Editor::from_markdown(cx, String::new(), fallback_path.clone());
                            editor.install_initial_file_open_failure(
                                fallback_path.unwrap_or_default(),
                                reason,
                                cx,
                            );
                            editor
                        }
                    },
                );
            editor.update(cx, |editor, cx| {
                editor.install_accessibility_bridge(window, cx)
            });
            editor
        })
        .map_err(|error| anyhow::anyhow!("failed to open editor window: {error}"))?;

    if let Err(error) = handle.update(cx, |editor, window, cx| {
        window.activate_window();
        editor.force_install_close_guard(cx, window);
    }) {
        eprintln!("failed to initialize editor window: {error}");
    }

    Ok(handle)
}

// 原因：文件失败仍打开标准编辑器错误面，确保调用方可以继续使用统一关闭与标题逻辑。
fn open_file_failure_window(
    cx: &mut App,
    path: PathBuf,
    reason: String,
) -> anyhow::Result<WindowHandle<Editor>> {
    let bounds = Bounds::centered(None, size(px(1080.), px(720.)), cx);
    let title = window_title(Some(&path));
    let handle = cx
        .open_window(gmark_window_options(title, bounds), move |window, cx| {
            let editor = cx.new(move |cx| {
                let mut editor = Editor::from_markdown(cx, String::new(), None);
                editor.install_initial_file_open_failure(path, reason, cx);
                editor
            });
            editor.update(cx, |editor, cx| {
                editor.install_accessibility_bridge(window, cx)
            });
            editor
        })
        .map_err(|error| anyhow::anyhow!("failed to open file error window: {error}"))?;
    if let Err(error) = handle.update(cx, |editor, window, cx| {
        window.activate_window();
        editor.force_install_close_guard(cx, window);
    }) {
        eprintln!("failed to initialize file error window: {error}");
    }
    Ok(handle)
}

// 原因：图片预览复用编辑器窗口外壳，保持只读预览与普通文件窗口一致的生命周期。
fn open_image_preview_window(
    cx: &mut App,
    path: PathBuf,
    restored_bounds: Option<WindowBounds>,
) -> anyhow::Result<WindowHandle<Editor>> {
    let title = window_title(Some(&path));
    let options = restored_bounds.map_or_else(
        || {
            let bounds = Bounds::centered(None, size(px(1080.), px(720.)), cx);
            gmark_window_options(title.clone(), bounds)
        },
        |bounds| gmark_window_options_with_bounds(title.clone(), bounds),
    );
    let handle = cx
        .open_window(options, move |window, cx| {
            let editor = cx.new(move |cx| {
                let mut editor = Editor::from_markdown(cx, String::new(), None);
                editor.install_initial_image_preview(path, cx);
                editor
            });
            editor.update(cx, |editor, cx| {
                editor.install_accessibility_bridge(window, cx)
            });
            editor
        })
        .map_err(|error| anyhow::anyhow!("failed to open image preview window: {error}"))?;
    if let Err(error) = handle.update(cx, |editor, window, cx| {
        window.activate_window();
        editor.force_install_close_guard(cx, window);
    }) {
        eprintln!("failed to initialize image preview window: {error}");
    }
    Ok(handle)
}

/// Opens an unfinished recovery session directly in the editor surface.
// 原因：恢复文档通过共享服务注册后再建窗口，避免恢复内容脱离运行时所有权。
pub(crate) struct PreparedRecoveredDocument {
    pub(crate) recovered: crate::recovery::RecoveredDocument,
    pub(crate) shared: SharedResidentOpen,
}

/// 在后台把恢复文本转换为共享 resident controller，保证恢复窗口首帧只
/// 接收已准备好的 lease，不会在 GPUI 线程解析 source-format 或触碰 journal。
pub(crate) fn prepare_recovered_document(
    service: DocumentService,
    recovered: crate::recovery::RecoveredDocument,
) -> anyhow::Result<PreparedRecoveredDocument> {
    let document_id = recovery_document_id(&recovered.document_id)?;
    let source = ResidentMarkdownSource::from_recovered(
        recovered.source.as_str(),
        recovered.file_path.clone(),
        recovered.source_format.clone(),
    )
    .map_err(|error| anyhow::anyhow!("failed to prepare recovered Markdown: {error}"))?;
    let shared = service
        .open_recovery(document_id, source)
        .map_err(|error| anyhow::anyhow!("failed to register recovered Markdown: {error}"))?;
    Ok(PreparedRecoveredDocument { recovered, shared })
}

/// 仅在 UI 线程创建恢复窗口；共享 controller 已由后台阶段完成构造。
// 原因：GPUI 的窗口和 Editor entity 具有线程亲和性，不能从文件 I/O worker 直接创建。
fn open_prepared_recovered_editor_window(
    cx: &mut App,
    prepared: PreparedRecoveredDocument,
) -> anyhow::Result<WindowHandle<Editor>> {
    let PreparedRecoveredDocument { recovered, shared } = prepared;
    let bounds = Bounds::centered(None, size(px(1080.), px(720.)), cx);
    let title = window_title(recovered.file_path.as_deref());
    let handle = cx
        .open_window(gmark_window_options(title, bounds), move |window, cx| {
            let fallback_path = recovered.file_path.clone();
            let editor =
                cx.new(
                    move |cx| match Editor::from_shared_recovery(cx, shared, recovered) {
                        Ok(editor) => editor,
                        Err(error) => {
                            let mut editor =
                                Editor::from_markdown(cx, String::new(), fallback_path.clone());
                            editor.install_initial_file_open_failure(
                                fallback_path.unwrap_or_default(),
                                error.to_string(),
                                cx,
                            );
                            editor
                        }
                    },
                );
            editor.update(cx, |editor, cx| {
                editor.install_accessibility_bridge(window, cx)
            });
            editor
        })
        .map_err(|error| anyhow::anyhow!("failed to open recovered editor window: {error}"))?;
    if let Err(error) = handle.update(cx, |editor, window, cx| {
        window.activate_window();
        window.set_window_edited(true);
        editor.force_install_close_guard(cx, window);
    }) {
        eprintln!("failed to initialize recovered editor window: {error}");
    }
    Ok(handle)
}

/// 消费后台准备的恢复文档并增量追加其余标签，避免恢复阶段重复打开正文。
// 原因：首个恢复文档负责建立窗口，其余文档只需保留快照，激活时再沿用既有 lazy tab 语义。
pub(crate) fn open_prepared_recovered_editor_tabs_window(
    cx: &mut App,
    mut prepared: Vec<PreparedRecoveredDocument>,
) -> Option<WindowHandle<Editor>> {
    if prepared.is_empty() {
        return None;
    }
    let first = prepared.remove(0);
    let additional = prepared
        .into_iter()
        .map(|prepared| prepared.recovered)
        .collect::<Vec<_>>();
    let handle = match open_prepared_recovered_editor_window(cx, first) {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("failed to open recovered editor: {error}");
            return None;
        }
    };
    if !additional.is_empty() {
        handle
            .update(cx, |editor, window, cx| {
                editor.append_recovered_tabs(additional, cx);
                window.set_window_edited(true);
            })
            .unwrap_or_else(|error| eprintln!("failed to append recovered tabs: {error}"));
    }
    Some(handle)
}

pub(crate) fn open_recovered_editor_window(
    cx: &mut App,
    recovered: crate::recovery::RecoveredDocument,
) -> anyhow::Result<WindowHandle<Editor>> {
    let prepared = prepare_recovered_document(app_document_service(cx), recovered)?;
    open_prepared_recovered_editor_window(cx, prepared)
}

// 原因：恢复记录的字符串标识必须在窗口入口统一解析，避免不同恢复分支接受不同格式。
pub(super) fn recovery_document_id(
    raw: &str,
) -> anyhow::Result<gmark_document_runtime::DocumentId> {
    let uuid = uuid::Uuid::parse_str(raw)
        .map_err(|error| anyhow::anyhow!("recovery document id '{raw}' is not a UUID: {error}"))?;
    Ok(gmark_document_runtime::DocumentId::from_uuid(uuid))
}

// 原因：把首个恢复文档作为宿主再追加其余标签，保留既有窗口和未保存状态语义。
pub(crate) fn open_recovered_editor_tabs_window(
    cx: &mut App,
    mut recovered: Vec<crate::recovery::RecoveredDocument>,
) -> Option<WindowHandle<Editor>> {
    if recovered.is_empty() {
        return None;
    }
    let additional = recovered.split_off(1);
    let first = recovered.pop()?;
    let handle = match open_recovered_editor_window(cx, first) {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("failed to open recovered editor: {error}");
            return None;
        }
    };
    if !additional.is_empty() {
        handle
            .update(cx, |editor, window, cx| {
                editor.append_recovered_tabs(additional, cx);
                window.set_window_edited(true);
            })
            .unwrap_or_else(|error| eprintln!("failed to append recovered tabs: {error}"));
    }
    Some(handle)
}

pub(crate) struct PreparedPagedRecovery {
    pub(crate) path: PathBuf,
    probe: gmark_paged_document::OpenProbe,
    source: gmark_paged_document::FileSource,
    journal_path: PathBuf,
}

/// 在后台检查大文件 recovery journal 并打开共享 source，避免恢复窗口首帧
/// 在 GPUI 线程访问 journal、metadata 或慢盘正文。
pub(crate) fn prepare_paged_recovery(
    journal_path: PathBuf,
) -> anyhow::Result<PreparedPagedRecovery> {
    let base = gmark_paged_document::inspect_paged_recovery_base(&journal_path)
        .map_err(|error| anyhow::anyhow!("failed to inspect large recovery: {error}"))?;
    let path = base.path;
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .map_err(|error| {
                anyhow::anyhow!(
                    "failed to probe recovered large file '{}': {error}",
                    path.display()
                )
            })?;
    let source = gmark_paged_document::FileSource::open(&path).map_err(|error| {
        anyhow::anyhow!(
            "failed to open recovered large file '{}': {error}",
            path.display()
        )
    })?;
    Ok(PreparedPagedRecovery {
        path,
        probe,
        source,
        journal_path,
    })
}

/// 仅在 UI 线程消费后台准备结果并创建分页恢复窗口。
// 原因：窗口和 Editor entity 需要 GPUI 线程，而所有输入资源都已在 worker 固定下来。
pub(crate) fn open_prepared_paged_recovery_window(
    cx: &mut App,
    prepared: PreparedPagedRecovery,
) -> anyhow::Result<(WindowHandle<Editor>, PathBuf)> {
    let PreparedPagedRecovery {
        path,
        probe,
        source,
        journal_path,
    } = prepared;
    let bounds = Bounds::centered(None, size(px(1080.), px(720.)), cx);
    let title = window_title(Some(&path));
    let restored_path = path.clone();
    let handle = cx
        .open_window(gmark_window_options(title, bounds), move |window, cx| {
            let editor = cx
                .new(move |cx| Editor::from_paged_recovery(cx, path, probe, source, journal_path));
            editor.update(cx, |editor, cx| {
                editor.install_accessibility_bridge(window, cx)
            });
            editor
        })
        .map_err(|error| anyhow::anyhow!("failed to open large recovery window: {error}"))?;
    handle
        .update(cx, |editor, window, cx| {
            window.activate_window();
            window.set_window_edited(true);
            editor.force_install_close_guard(cx, window);
        })
        .map_err(|error| anyhow::anyhow!("failed to initialize large recovery window: {error}"))?;
    Ok((handle, restored_path))
}

// 原因：保留手动恢复入口的同步返回契约；启动恢复改用 prepare/open_prepared 两阶段。
pub(crate) fn open_paged_recovery_window(
    cx: &mut App,
    journal_path: PathBuf,
) -> anyhow::Result<(WindowHandle<Editor>, PathBuf)> {
    let prepared = prepare_paged_recovery(journal_path)?;
    open_prepared_paged_recovery_window(cx, prepared)
}

// 原因：普通文件入口集中转发策略选择，保持菜单和启动调用方共用行为。
pub(crate) fn open_file_in_new_window(cx: &mut App, path: &Path) -> anyhow::Result<()> {
    open_file_in_new_window_with_policy(cx, path, None)
}

// 原因：安全源码入口只改变加载策略并复用普通窗口流程，避免安全模式复制打开逻辑。
pub(crate) fn open_file_in_safe_source_window(cx: &mut App, path: &Path) -> anyhow::Result<()> {
    open_file_in_new_window_with_policy(
        cx,
        path,
        Some(gmark_document_core::LoadingPolicy {
            force_safe_source: true,
            ..gmark_document_core::LoadingPolicy::default()
        }),
    )
}

#[cfg(test)]
// 原因：统一从测试默认值或用户偏好解析加载策略，确保所有窗口入口共享同一边界。
pub(super) fn default_loading_policy() -> gmark_document_core::LoadingPolicy {
    gmark_document_core::LoadingPolicy::default()
}

#[cfg(not(test))]
// 原因：统一从测试默认值或用户偏好解析加载策略，确保所有窗口入口共享同一边界。
pub(super) fn default_loading_policy() -> gmark_document_core::LoadingPolicy {
    crate::config::read_app_preferences()
        .map(|preferences| preferences.document_loading.policy())
        .unwrap_or_default()
}

// 原因：窗口入口懒初始化文档服务，以兼容尚未完成全局初始化的测试和早期调用。
pub(super) fn app_document_service(cx: &mut App) -> DocumentService {
    if let Some(service) = cx.try_global::<DocumentService>() {
        return service.clone();
    }
    let service = DocumentService::new();
    cx.set_global(service.clone());
    service
}

// 原因：按文件类型和探测策略选择预览、resident 或 host，保持每类文档原有窗口语义。
fn open_file_in_new_window_with_policy(
    cx: &mut App,
    path: &Path,
    policy: Option<gmark_document_core::LoadingPolicy>,
) -> anyhow::Result<()> {
    // 原因：慢盘、UNC 和 registry 单飞等待必须脱离 GPUI 主线程；任务被
    // 取消或超时后不再回写窗口，避免迟到结果覆盖下一代用户请求。
    let first_frame = open_editor_window(cx, String::new(), None)?;
    let service = app_document_service(cx);
    let path = path.to_path_buf();
    let cancellation = OpenCancellation::default();
    cx.spawn(async move |cx| {
        let worker_path = path.clone();
        let worker_cancellation = cancellation.clone();
        let operation = cx.background_spawn(async move {
            let loading = policy.unwrap_or_else(default_loading_policy);
            prepare_file_open(service, worker_path, loading, worker_cancellation)
        });
        let result = match select(
            operation,
            cx.background_executor().timer(FILE_OPEN_DEADLINE),
        )
        .await
        {
            Either::Left((result, _timer)) => result,
            Either::Right((_elapsed, _operation)) => {
                cancellation.cancel();
                Err(anyhow::anyhow!(
                    "timed out opening '{}'; the request was cancelled",
                    path.display()
                ))
            }
        };
        let _ = cx.update(move |cx| {
            let (replacement_installed, opened_successfully) = match result {
                Ok(prepared) => match install_prepared_file_open(cx, prepared) {
                    Ok(()) => (true, true),
                    Err(error) => (
                        open_file_failure_window(cx, path.clone(), error.to_string()).is_ok(),
                        false,
                    ),
                },
                Err(error) => (
                    open_file_failure_window(cx, path.clone(), error.to_string()).is_ok(),
                    false,
                ),
            };
            if replacement_installed {
                let _ = first_frame.update(cx, |_editor, window, _cx| window.remove_window());
            }
            if opened_successfully {
                crate::app_menu::record_recent_file_and_refresh(&path, cx);
            }
        });
    })
    .detach();
    Ok(())
}
// 原因：分离标签复用标准编辑器外壳，保持独立窗口的 dirty 和关闭守卫语义。
pub(crate) fn open_detached_tab_window(
    cx: &mut App,
    detached: crate::editor::DetachedTab,
) -> anyhow::Result<()> {
    let bounds = Bounds::centered(None, size(px(1080.), px(720.)), cx);
    let title = window_title(detached.file_path());
    let handle = cx
        .open_window(gmark_window_options(title, bounds), move |window, cx| {
            let editor = cx.new(move |cx| {
                let mut editor = Editor::from_markdown(cx, String::new(), None);
                editor.install_detached_tab(detached, cx);
                editor
            });
            editor.update(cx, |editor, cx| {
                editor.install_accessibility_bridge(window, cx)
            });
            editor
        })
        .map_err(|error| anyhow::anyhow!("failed to open detached tab window: {error}"))?;
    if let Err(error) = handle.update(cx, |editor, window, cx| {
        window.activate_window();
        window.set_window_edited(editor.is_document_dirty());
        editor.force_install_close_guard(cx, window);
    }) {
        eprintln!("failed to initialize detached tab window: {error}");
    }
    Ok(())
}
