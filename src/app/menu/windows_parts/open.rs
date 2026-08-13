// @author kongweiguang

//! Editor window construction and document-service adapters.

use super::*;

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

// 原因：大文档窗口必须先通过文档服务取得共享 lease，避免重复读取正文或丢失单飞约束。
fn open_large_editor_window(
    cx: &mut App,
    path: PathBuf,
    probe: gmark_paged_document::OpenProbe,
    loading: gmark_document_core::LoadingPolicy,
    restored_bounds: Option<WindowBounds>,
) -> anyhow::Result<WindowHandle<Editor>> {
    let service = app_document_service(cx);
    let result = if probe.strategy == gmark_paged_document::OpenStrategy::Paged {
        service.open_paged(
            &path,
            probe.clone(),
            loading,
            |normalized, probe, _policy| {
                let source =
                    gmark_paged_document::FileSource::open(normalized).map_err(|error| {
                        anyhow::anyhow!("failed to open '{}': {error}", normalized.display())
                    })?;
                gmark_paged_document::prepare_utf8_source(source, probe.encoding.clone())
                    .map_err(|error| anyhow::anyhow!("failed to prepare source: {error}"))
            },
        )
    } else {
        service.open_document_host(
            &path,
            probe.clone(),
            loading,
            |normalized, probe, _policy| {
                let source =
                    gmark_paged_document::FileSource::open(normalized).map_err(|error| {
                        anyhow::anyhow!("failed to open '{}': {error}", normalized.display())
                    })?;
                gmark_paged_document::prepare_utf8_source(source, probe.encoding.clone())
                    .map_err(|error| anyhow::anyhow!("failed to prepare source: {error}"))
            },
        )
    };
    let shared = match result {
        Ok(shared) => shared,
        Err(error) => {
            service.clear_probe(&path, loading);
            return Err(anyhow::anyhow!("failed to open shared document: {error}"));
        }
    };
    open_shared_document_host_window(cx, shared, path, restored_bounds)
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
pub(crate) fn open_recovered_editor_window(
    cx: &mut App,
    recovered: crate::recovery::RecoveredDocument,
) -> anyhow::Result<WindowHandle<Editor>> {
    let document_id = recovery_document_id(&recovered.document_id)?;
    let recovered_path = recovered.file_path.clone();
    let source = ResidentMarkdownSource::from_recovered(
        recovered.source.as_str(),
        recovered_path.clone(),
        recovered.source_format.clone(),
    )
    .map_err(|error| anyhow::anyhow!("failed to prepare recovered Markdown: {error}"))?;
    let shared = app_document_service(cx)
        .open_recovery(document_id, source)
        .map_err(|error| anyhow::anyhow!("failed to register recovered Markdown: {error}"))?;
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

// 原因：大文件恢复先检查 journal 和 probe，再交给分页 Editor，避免不完整恢复进入普通文本路径。
pub(crate) fn open_paged_recovery_window(
    cx: &mut App,
    journal_path: PathBuf,
) -> anyhow::Result<(WindowHandle<Editor>, PathBuf)> {
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
    let loading = policy.unwrap_or_else(default_loading_policy);

    // Images and known binary containers retain their view-only/error-page
    // behavior.  They never enter the process document registry.
    if crate::document_io::is_image_path(path)
        || crate::document_io::is_known_unsupported_document(path)
    {
        let opened = match crate::document_io::open_document_with_policy(path, loading) {
            Ok(opened) => opened,
            Err(error) => {
                open_file_failure_window(cx, path.to_path_buf(), error.to_string())?;
                record_recent_file_and_refresh(path, cx);
                return Ok(());
            }
        };
        if matches!(opened, crate::document_io::OpenedDocument::Image) {
            open_image_preview_window(cx, path.to_path_buf(), None)?;
        }
        record_recent_file_and_refresh(path, cx);
        return Ok(());
    }

    // Probe/classification is also single-flight.  Only the Opening owner of
    // the service's registry slot performs source preparation and body IO.
    let service = app_document_service(cx);
    let probe = match service.probe_file(path, loading, |normalized, policy| {
        crate::document_io::probe_document_with_policy(normalized, policy)
    }) {
        Ok(probe) => probe,
        Err(error) => {
            open_file_failure_window(cx, path.to_path_buf(), error.to_string())?;
            record_recent_file_and_refresh(path, cx);
            return Ok(());
        }
    };

    if crate::document_io::is_markdown_path(path)
        && probe.strategy == gmark_paged_document::OpenStrategy::Resident
    {
        let limits = loading.effective_limits();
        match service.open_resident_file(path, loading, |normalized, _policy| {
            crate::document_io::read_resident_text_from_probe(normalized, &probe, limits)
                .map(|opened| ResidentMarkdownSource::from_opened(normalized, opened))
        }) {
            Ok(shared) => {
                open_shared_resident_editor_window(cx, shared, Some(path.to_path_buf()), None)?;
            }
            Err(error) => {
                service.clear_probe(path, loading);
                open_file_failure_window(cx, path.to_path_buf(), error.to_string())?;
            }
        }
    } else if crate::document_io::is_svg_path(path)
        && probe.strategy == gmark_paged_document::OpenStrategy::Resident
    {
        // SVG keeps its source-backed preview semantics.  It is deliberately
        // outside the shared text-host migration until the preview renderer
        // can consume a DocumentHost without changing the existing surface.
        match crate::document_io::read_resident_text_from_probe(
            path,
            &probe,
            loading.effective_limits(),
        ) {
            Ok(opened) => {
                let handle = open_decoded_editor_window(cx, opened, Some(path.to_path_buf()))?;
                let _ = handle.update(cx, |editor, _window, cx| {
                    editor.set_view_mode(crate::editor::ViewMode::Preview, cx);
                });
            }
            Err(error) => {
                service.clear_probe(path, loading);
                open_file_failure_window(cx, path.to_path_buf(), error.to_string())?;
            }
        }
    } else if let Err(error) =
        open_large_editor_window(cx, path.to_path_buf(), probe, loading, None)
    {
        service.clear_probe(path, loading);
        open_file_failure_window(cx, path.to_path_buf(), error.to_string())?;
    }
    record_recent_file_and_refresh(path, cx);
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
