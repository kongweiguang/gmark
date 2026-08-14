// @author kongweiguang

//! Workspace-session restoration for editor windows.

use super::open::{
    app_document_service, default_loading_policy, open_editor_window, recovery_document_id,
    window_title,
};
use super::*;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use futures::future::{Either, select};

const SESSION_RESTORE_DEADLINE: Duration = Duration::from_secs(30);

#[derive(Clone, Default)]
struct SessionCancellation {
    cancelled: Arc<AtomicBool>,
}

impl SessionCancellation {
    /// 在每个会话 tab 的同步阶段之间检查取消，隔离超时后的迟到 lease。
    // 原因：UNC 读取不可抢占，但 registry 发布前仍必须有明确的取消边界。
    fn check(&self) -> anyhow::Result<()> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(anyhow::anyhow!("workspace session restore was cancelled"))
        } else {
            Ok(())
        }
    }

    /// 标记当前会话代次失效，后台 worker 返回后不会继续安装结果。
    // 原因：select 丢弃任务句柄不能中断已经运行的同步文件调用。
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

type PreparedWorkspaceTab = (
    crate::config::workspace_session::WorkspaceSessionTab,
    WorkspaceSessionRestoredOpen,
    PathBuf,
    Option<crate::recovery::RecoveredDocument>,
);

struct PreparedWorkspaceSession {
    session: crate::config::workspace_session::WorkspaceSession,
    opened: Vec<PreparedWorkspaceTab>,
}

/// 只在后台完成会话中的文件探测、恢复 journal 读取和共享 controller 构造，
/// 使窗口首帧不被单个慢盘或 UNC tab 拖住。
fn prepare_workspace_session(
    service: crate::app::document_service::DocumentService,
    session: crate::config::workspace_session::WorkspaceSession,
    loading: gmark_document_core::LoadingPolicy,
    cancellation: SessionCancellation,
) -> anyhow::Result<PreparedWorkspaceSession> {
    cancellation.check()?;
    let mut session_tabs = Vec::new();
    collect_workspace_session_tabs(&session.root, &session.panes, &mut session_tabs);
    let recovered = crate::config::AppDirs::from_system()
        .and_then(|dirs| {
            dirs.validate_state_root()?;
            crate::recovery::load_recovery_documents(&dirs.recovery_dir())
        })
        .unwrap_or_else(|error| {
            eprintln!("workspace recovery unavailable: {error:#}");
            Vec::new()
        });
    let mut opened = Vec::with_capacity(session_tabs.len());
    for tab in session_tabs {
        cancellation.check()?;
        // Clone only the small document reference before matching so failed
        // branches can move the complete tab into the readonly error entry.
        let document = tab.document.clone();
        let (shared, path, recovered_document) = match &document {
            crate::config::workspace_session::WorkspaceSessionDocumentRef::File(path) => {
                if crate::document_io::is_image_path(path) {
                    opened.push((
                        tab,
                        WorkspaceSessionRestoredOpen::Image { path: path.clone() },
                        path.clone(),
                        None,
                    ));
                    continue;
                }
                let probe_cancellation = cancellation.clone();
                let probe = match service.probe_file(path, loading, |normalized, policy| {
                    let probe = crate::document_io::probe_document_with_policy(normalized, policy)?;
                    probe_cancellation.check()?;
                    Ok::<_, anyhow::Error>(probe)
                }) {
                    Ok(probe) => probe,
                    Err(error) => {
                        eprintln!(
                            "failed to probe workspace tab '{}': {error}",
                            path.display()
                        );
                        opened.push((
                            tab,
                            WorkspaceSessionRestoredOpen::Error {
                                path: path.clone(),
                                message: error.to_string(),
                            },
                            path.clone(),
                            None,
                        ));
                        continue;
                    }
                };
                cancellation.check()?;
                if crate::document_io::is_markdown_path(path)
                    && probe.strategy == gmark_paged_document::OpenStrategy::Resident
                {
                    let limits = loading.effective_limits();
                    let resident_cancellation = cancellation.clone();
                    let shared = match service.open_resident_file(path, loading, |normalized, _| {
                        let opened = crate::document_io::read_resident_text_from_probe(
                            normalized, &probe, limits,
                        )?;
                        resident_cancellation.check()?;
                        Ok::<_, anyhow::Error>(ResidentMarkdownSource::from_opened(
                            normalized, opened,
                        ))
                    }) {
                        Ok(shared) => WorkspaceSessionRestoredOpen::Resident(shared),
                        Err(error) => {
                            service.clear_probe(path, loading);
                            eprintln!(
                                "failed to restore workspace tab '{}': {error}",
                                path.display()
                            );
                            opened.push((
                                tab,
                                WorkspaceSessionRestoredOpen::Error {
                                    path: path.clone(),
                                    message: error.to_string(),
                                },
                                path.clone(),
                                None,
                            ));
                            continue;
                        }
                    };
                    cancellation.check()?;
                    (shared, path.clone(), None)
                } else {
                    let host_cancellation = cancellation.clone();
                    let shared = match service.open_document_host(
                        path,
                        probe.clone(),
                        loading,
                        |normalized, probe, _| {
                            let source = gmark_paged_document::FileSource::open(normalized)
                                .map_err(|error| {
                                    anyhow::anyhow!(
                                        "failed to open '{}': {error}",
                                        normalized.display()
                                    )
                                })?;
                            let prepared = gmark_paged_document::prepare_utf8_source(
                                source,
                                probe.encoding.clone(),
                            )
                            .map_err(|error| {
                                anyhow::anyhow!("failed to prepare source: {error}")
                            })?;
                            host_cancellation.check()?;
                            Ok::<_, anyhow::Error>(prepared)
                        },
                    ) {
                        Ok(shared) => WorkspaceSessionRestoredOpen::Host(shared),
                        Err(error) => {
                            service.clear_probe(path, loading);
                            eprintln!(
                                "failed to restore workspace tab '{}': {error}",
                                path.display()
                            );
                            opened.push((
                                tab,
                                WorkspaceSessionRestoredOpen::Error {
                                    path: path.clone(),
                                    message: error.to_string(),
                                },
                                path.clone(),
                                None,
                            ));
                            continue;
                        }
                    };
                    cancellation.check()?;
                    (shared, path.clone(), None)
                }
            }
            crate::config::workspace_session::WorkspaceSessionDocumentRef::Recovery(id) => {
                let Some(recovered) = recovered.iter().find(|document| {
                    uuid::Uuid::parse_str(&document.document_id).ok() == Some(*id)
                }) else {
                    eprintln!("recovery document {id} is unavailable");
                    opened.push((
                        tab,
                        WorkspaceSessionRestoredOpen::Error {
                            path: PathBuf::new(),
                            message: format!("recovery document {id} is unavailable"),
                        },
                        PathBuf::new(),
                        None,
                    ));
                    continue;
                };
                let document_id = match recovery_document_id(&recovered.document_id) {
                    Ok(document_id) => document_id,
                    Err(error) => {
                        eprintln!("failed to parse recovery document {id}: {error}");
                        opened.push((
                            tab,
                            WorkspaceSessionRestoredOpen::Error {
                                path: recovered.file_path.clone().unwrap_or_default(),
                                message: error.to_string(),
                            },
                            recovered.file_path.clone().unwrap_or_default(),
                            None,
                        ));
                        continue;
                    }
                };
                let source = match ResidentMarkdownSource::from_recovered(
                    recovered.source.as_str(),
                    recovered.file_path.clone(),
                    recovered.source_format.clone(),
                ) {
                    Ok(source) => source,
                    Err(error) => {
                        eprintln!("failed to prepare recovery document {id}: {error}");
                        opened.push((
                            tab,
                            WorkspaceSessionRestoredOpen::Error {
                                path: recovered.file_path.clone().unwrap_or_default(),
                                message: error.to_string(),
                            },
                            recovered.file_path.clone().unwrap_or_default(),
                            None,
                        ));
                        continue;
                    }
                };
                let shared = match service.open_recovery(document_id, source) {
                    Ok(shared) => WorkspaceSessionRestoredOpen::Resident(shared),
                    Err(error) => {
                        eprintln!("failed to open recovery document {id}: {error}");
                        opened.push((
                            tab,
                            WorkspaceSessionRestoredOpen::Error {
                                path: recovered.file_path.clone().unwrap_or_default(),
                                message: error.to_string(),
                            },
                            recovered.file_path.clone().unwrap_or_default(),
                            None,
                        ));
                        continue;
                    }
                };
                cancellation.check()?;
                (
                    shared,
                    recovered.file_path.clone().unwrap_or_default(),
                    Some(recovered.clone()),
                )
            }
        };
        opened.push((tab, shared, path, recovered_document));
    }
    Ok(PreparedWorkspaceSession { session, opened })
}

/// 启动后台会话准备并立即返回，让调用方可以先提交一个可绘制窗口首帧。
// 原因：会话恢复是可取消的增量工作，不能把首帧建立绑定到任意一个文件的 I/O 完成。
pub(crate) fn open_workspace_session_window(
    cx: &mut App,
    session: crate::config::workspace_session::WorkspaceSession,
) -> bool {
    if !session.has_tabs() {
        return false;
    }
    let first_frame = match open_editor_window(cx, String::new(), None) {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("failed to open workspace session first frame: {error}");
            return false;
        }
    };
    let service = app_document_service(cx);
    let cancellation = SessionCancellation::default();
    cx.spawn(async move |cx| {
        let worker_cancellation = cancellation.clone();
        let prepared = match select(
            cx.background_spawn(async move {
                let loading = default_loading_policy();
                prepare_workspace_session(service, session, loading, worker_cancellation)
            }),
            cx.background_executor().timer(SESSION_RESTORE_DEADLINE),
        )
        .await
        {
            Either::Left((prepared, _timer)) => prepared,
            Either::Right((_elapsed, _preparation)) => {
                cancellation.cancel();
                Err(anyhow::anyhow!("timed out restoring workspace session"))
            }
        };
        let _ = cx.update(move |cx| match prepared {
            Ok(prepared) => {
                if install_prepared_workspace_session(cx, prepared) {
                    let _ = first_frame.update(cx, |_editor, window, _cx| window.remove_window());
                }
            }
            Err(error) => eprintln!("failed to prepare workspace session: {error:#}"),
        });
    })
    .detach();
    true
}

/// 在主线程只构造窗口和 pane 状态，把后台准备的 lease 原样交给 Editor。
// 原因：恢复结果必须在 GPUI 线程发布，避免跨线程触碰窗口树，同时不重复读取文档正文。
fn install_prepared_workspace_session(cx: &mut App, prepared: PreparedWorkspaceSession) -> bool {
    let PreparedWorkspaceSession { session, opened } = prepared;
    let window_bounds = session
        .window
        .as_ref()
        .map(|window| restored_window_bounds(window, cx));
    let mut opened = opened.into_iter();
    let Some((first_tab, first_shared, first_path, first_recovered)) = opened.next() else {
        return false;
    };
    let first_view_id = gmark_document_core::DocumentViewInstanceId::from_uuid(first_tab.id);
    let mut restored = Vec::with_capacity(1 + opened.len());
    // A readonly image/error cannot be mounted by the first shell editor. Keep
    // a duplicate enum in the canonical restore list so the pane model still
    // receives the tab while the shell itself falls back to an empty editor.
    let first_readonly = match &first_shared {
        WorkspaceSessionRestoredOpen::Image { path } => {
            Some(WorkspaceSessionRestoredOpen::Image { path: path.clone() })
        }
        WorkspaceSessionRestoredOpen::Error { path, message } => {
            Some(WorkspaceSessionRestoredOpen::Error {
                path: path.clone(),
                message: message.clone(),
            })
        }
        WorkspaceSessionRestoredOpen::Resident(_) | WorkspaceSessionRestoredOpen::Host(_) => None,
    };
    restored.push((first_tab, first_readonly));
    for (tab, shared, _, _) in opened {
        restored.push((tab, Some(shared)));
    }
    let title = window_title(Some(&first_path));
    let first_host_presentation =
        Editor::host_presentation_from_workspace_state(&restored[0].0.state);
    let options = window_bounds.map_or_else(
        || {
            gmark_window_options(
                title.clone(),
                Bounds::centered(None, size(px(1080.), px(720.)), cx),
            )
        },
        |bounds| gmark_window_options_with_bounds(title.clone(), bounds),
    );
    let first_path_for_window = first_path;
    let first_is_readonly = matches!(
        &first_shared,
        WorkspaceSessionRestoredOpen::Image { .. } | WorkspaceSessionRestoredOpen::Error { .. }
    );
    let handle = cx
        .open_window(options, move |window, cx| {
            let fallback_path = first_path_for_window.clone();
            let editor = cx.new(move |cx| {
                let result: anyhow::Result<Editor> = match first_shared {
                    WorkspaceSessionRestoredOpen::Resident(shared) => {
                        if let Some(recovered) = first_recovered {
                            Editor::from_shared_recovery_with_view_id(
                                cx,
                                shared,
                                recovered,
                                first_view_id,
                            )
                            .map_err(|error| anyhow::anyhow!(error.to_string()))
                        } else {
                            Editor::from_shared_resident_open_with_view_id(
                                cx,
                                shared,
                                Some(fallback_path.clone()),
                                first_view_id,
                            )
                            .map_err(|error| anyhow::anyhow!(error.to_string()))
                        }
                    }
                    WorkspaceSessionRestoredOpen::Host(shared) => {
                        Ok(Editor::from_shared_document_host_with_view_id(
                            cx,
                            fallback_path.clone(),
                            shared.probe.clone(),
                            shared.lease.handle(),
                            shared.lease,
                            first_view_id,
                            first_host_presentation,
                        ))
                    }
                    WorkspaceSessionRestoredOpen::Image { .. }
                    | WorkspaceSessionRestoredOpen::Error { .. } => Err(anyhow::anyhow!(
                        "readonly workspace tab uses the pane read-only canvas"
                    )),
                };
                match result {
                    Ok(editor) => editor,
                    Err(_) if first_is_readonly => {
                        // Read-only tabs are installed by the canonical pane
                        // restore below; keep the temporary shell neutral so
                        // it cannot surface a duplicate file-open failure.
                        Editor::from_markdown(cx, String::new(), Some(fallback_path.clone()))
                    }
                    Err(error) => {
                        let mut editor =
                            Editor::from_markdown(cx, String::new(), Some(fallback_path.clone()));
                        editor.install_initial_file_open_failure(
                            fallback_path.clone(),
                            error.to_string(),
                            cx,
                        );
                        editor
                    }
                }
            });
            editor.update(cx, |editor, cx| {
                editor.install_accessibility_bridge(window, cx)
            });
            editor
        })
        .map_err(|error| {
            eprintln!("failed to open workspace session window: {error}");
            error
        });
    let Ok(handle) = handle else {
        return false;
    };
    handle
        .update(cx, |editor, _window, cx| {
            editor
                .restore_canonical_workspace_session(session, restored, cx)
                .map_err(|error| {
                    eprintln!("failed to restore canonical workspace panes: {error}");
                    error
                })
        })
        .is_ok()
}

// 原因：按 pane 树顺序收集标签，保证恢复输入与用户上次看到的窗格顺序一致。
fn collect_workspace_session_tabs(
    node: &crate::config::workspace_session::WorkspaceSessionPaneNode,
    panes: &std::collections::BTreeMap<
        crate::config::workspace_session::WorkspaceSessionPaneId,
        crate::config::workspace_session::WorkspaceSessionPane,
    >,
    tabs: &mut Vec<crate::config::workspace_session::WorkspaceSessionTab>,
) {
    match node {
        crate::config::workspace_session::WorkspaceSessionPaneNode::Leaf(pane_id) => {
            if let Some(pane) = panes.get(pane_id) {
                tabs.extend(pane.tabs.iter().cloned());
            }
        }
        crate::config::workspace_session::WorkspaceSessionPaneNode::Split {
            first, second, ..
        } => {
            collect_workspace_session_tabs(first, panes, tabs);
            collect_workspace_session_tabs(second, panes, tabs);
        }
    }
}
