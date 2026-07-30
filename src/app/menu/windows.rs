// @author kongweiguang

//! Editor and recovery window construction.

use super::*;

fn window_title(file_path: Option<&Path>) -> SharedString {
    if let Some(path) = file_path {
        // OsStr::to_string_lossy returns Cow<str>; calling .to_string() on
        // it always allocates a fresh String, even for the valid-UTF-8 path
        // (the common case). Borrow the Cow directly into format! — its
        // Display impl writes the borrowed bytes straight into the output
        // String, no intermediate allocation.
        format!(
            "gmark - {}",
            path.file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_else(|| path.to_string_lossy())
        )
        .into()
    } else {
        SharedString::new("gmark")
    }
}

/// Opens an editor window for the given Markdown content and optional path.
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

pub(crate) fn open_decoded_editor_window(
    cx: &mut App,
    opened: crate::document_io::OpenedMarkdown,
    file_path: Option<PathBuf>,
) -> anyhow::Result<WindowHandle<Editor>> {
    open_decoded_editor_window_with_bounds(cx, opened, file_path, None)
}

fn open_large_editor_window(
    cx: &mut App,
    path: PathBuf,
    probe: gmark_paged_document::OpenProbe,
    restored_bounds: Option<WindowBounds>,
) -> anyhow::Result<WindowHandle<Editor>> {
    let source = gmark_paged_document::FileSource::open(&path)
        .map_err(|error| anyhow::anyhow!("failed to open '{}': {error}", path.display()))?;
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
            let editor = cx.new(move |cx| Editor::from_source_backed_file(cx, path, probe, source));
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
pub(crate) fn open_recovered_editor_window(
    cx: &mut App,
    recovered: crate::recovery::RecoveredDocument,
) -> anyhow::Result<WindowHandle<Editor>> {
    let bounds = Bounds::centered(None, size(px(1080.), px(720.)), cx);
    let title = window_title(recovered.file_path.as_deref());
    let handle = cx
        .open_window(gmark_window_options(title, bounds), move |window, cx| {
            let editor = cx.new(move |cx| Editor::from_recovered(cx, recovered));
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

pub(crate) fn open_file_in_new_window(cx: &mut App, path: &Path) -> anyhow::Result<()> {
    open_file_in_new_window_with_policy(cx, path, None)
}

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

fn open_file_in_new_window_with_policy(
    cx: &mut App,
    path: &Path,
    policy: Option<gmark_document_core::LoadingPolicy>,
) -> anyhow::Result<()> {
    let opened = match match policy {
        Some(policy) => crate::document_io::open_document_with_policy(path, policy),
        None => crate::document_io::open_document(path),
    } {
        Ok(opened) => opened,
        Err(error) => {
            open_file_failure_window(cx, path.to_path_buf(), error.to_string())?;
            record_recent_file_and_refresh(path, cx);
            return Ok(());
        }
    };
    match opened {
        crate::document_io::OpenedDocument::Resident(opened) => {
            let handle = open_decoded_editor_window(cx, opened, Some(path.to_path_buf()))?;
            if crate::document_io::is_svg_path(path) {
                let _ = handle.update(cx, |editor, _window, cx| {
                    editor.set_view_mode(crate::editor::ViewMode::Preview, cx);
                });
            } else if !crate::document_io::is_markdown_path(path) {
                let _ = handle.update(cx, |editor, _window, cx| {
                    editor.set_view_mode(crate::editor::ViewMode::Source, cx);
                });
            }
        }
        crate::document_io::OpenedDocument::ResidentFormat(probe)
        | crate::document_io::OpenedDocument::Paged(probe) => {
            open_large_editor_window(cx, path.to_path_buf(), probe, None)?;
        }
        crate::document_io::OpenedDocument::Image => {
            open_image_preview_window(cx, path.to_path_buf(), None)?;
        }
    }
    record_recent_file_and_refresh(path, cx);
    Ok(())
}

pub(crate) fn open_workspace_session_window(
    cx: &mut App,
    session: crate::config::workspace_session::WorkspaceSession,
) -> bool {
    let window_bounds = session
        .window
        .as_ref()
        .map(|window| restored_window_bounds(window, cx));
    let active_path = session
        .tabs
        .get(session.active_index)
        .map(|tab| tab.path.clone());
    let mut restored = Vec::new();
    for tab in session.tabs {
        match crate::document_io::open_document(&tab.path) {
            Ok(opened) => restored.push(crate::editor::RestoredTab {
                opened,
                path: tab.path,
                pinned: tab.pinned,
                view_mode: tab.view_mode,
                selection: tab.selection,
                scroll_x: tab.scroll_x,
                scroll_y: tab.scroll_y,
            }),
            Err(error) => {
                eprintln!(
                    "failed to restore workspace tab '{}': {error}",
                    tab.path.display()
                );
            }
        }
    }
    let Some(first) = restored.first() else {
        return false;
    };
    let active_index = active_path
        .as_ref()
        .and_then(|path| restored.iter().position(|tab| tab.path == *path))
        .unwrap_or(0);
    let handle = match &first.opened {
        crate::document_io::OpenedDocument::Resident(opened) => {
            match open_decoded_editor_window_with_bounds(
                cx,
                opened.clone(),
                Some(first.path.clone()),
                window_bounds,
            ) {
                Ok(handle) => handle,
                Err(error) => {
                    eprintln!("failed to restore workspace window: {error}");
                    return false;
                }
            }
        }
        crate::document_io::OpenedDocument::ResidentFormat(probe)
        | crate::document_io::OpenedDocument::Paged(probe) => {
            match open_large_editor_window(cx, first.path.clone(), probe.clone(), window_bounds) {
                Ok(handle) => handle,
                Err(error) => {
                    eprintln!(
                        "failed to restore large workspace tab '{}': {error}",
                        first.path.display()
                    );
                    return false;
                }
            }
        }
        crate::document_io::OpenedDocument::Image => {
            match open_image_preview_window(cx, first.path.clone(), window_bounds) {
                Ok(handle) => handle,
                Err(error) => {
                    eprintln!("failed to restore image workspace window: {error}");
                    return false;
                }
            }
        }
    };
    handle
        .update(cx, |editor, _window, cx| {
            editor.restore_tab_session_with_sidebars(
                session.id,
                restored,
                active_index,
                session.workspace_root,
                session.workspace_panel_width,
                session.workspace_docked_open,
                session.document_sidebar_width,
                session.document_sidebar_docked_open,
                session.split_pane_ratio,
                cx,
            );
        })
        .is_ok()
}

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
