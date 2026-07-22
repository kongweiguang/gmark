// @author kongweiguang

//! Native application menu, app-level actions, and window close routing.
//!
//! This module owns menu construction and the actions that operate on the
//! active editor window. The Quit action is routed to the current window so the
//! existing unsaved-changes dialog remains authoritative for that window.

use std::path::{Path, PathBuf};

use gpui::*;

use crate::components::{
    AddLanguageConfig, AddThemeConfig, BlockKind, BoldSelection, CheckForUpdates, CloseTab,
    CloseWindow, CodeSelection, CommandPalette, Copy, CopyAsMarkdown, Cut, EditingCommandHistory,
    ExportHtml, ExportImage, ExportPdf, FindInDocument, FindNext, FindPrevious, HighlightSelection,
    InlineMathSelection, InstallCliTool, ItalicSelection, LinkSelection, NewTab, NewWindow,
    NextTab, NoRecentFiles, NormalizeLineEndingsCr, NormalizeLineEndingsCrLf,
    NormalizeLineEndingsLf, OpenCrashReports, OpenFile, OpenFolder, OpenPreferences,
    OpenPrivacyPolicy, OpenRecentFile, OpenSafeSource, Paste, PasteAsPlainText, PreviousTab,
    QuickOpen, QuitApplication, Redo, ReopenClosedTab, ReplaceInDocument, SaveDocument,
    SaveDocumentAs, SelectAll, SelectLanguage, SelectTheme, SetBulletedList, SetCodeBlock,
    SetHeading1, SetHeading2, SetHeading3, SetHeading4, SetHeading5, SetHeading6, SetNumberedList,
    SetParagraph, SetQuote, SetTaskList, ShowAbout, StrikethroughSelection, SubscriptSelection,
    SuperscriptSelection, ToggleFocusMode, ToggleTypewriterMode, ToggleViewMode, ToggleWorkspace,
    UnderlineSelection, Undo, UninstallCliTool,
};
use crate::config::{
    apply_configured_language, apply_configured_theme, import_language_config_and_select,
    import_theme_config_and_select, open_preferences_window, read_recent_files, record_recent_file,
    remove_recent_file,
};
use crate::editor::{Editor, InfoDialogKind};
use crate::export::ExportFormat;
use crate::i18n::I18nManager;
use crate::theme::ThemeManager;
use crate::window_chrome::{
    gmark_window_options, gmark_window_options_with_bounds, restored_window_bounds,
};

const PRIVACY_POLICY_URL: &str = "https://github.com/kongweiguang/gmark/blob/main/PRIVACY.md";

/// 自绘菜单和命令面板只消费语义图标 ID；原生菜单仍完全交给平台绘制。
pub(crate) fn menu_action_icon(action: &dyn Action) -> Option<&'static str> {
    let action = action.as_any();
    if action.is::<NewTab>() || action.is::<NewWindow>() {
        Some("icon/ui/plus.svg")
    } else if action.is::<ReopenClosedTab>() || action.is::<CheckForUpdates>() {
        Some("icon/ui/refresh.svg")
    } else if action.is::<CloseTab>() || action.is::<CloseWindow>() {
        Some("icon/ui/close.svg")
    } else if action.is::<OpenFile>()
        || action.is::<OpenSafeSource>()
        || action.is::<OpenRecentFile>()
        || action.is::<NoRecentFiles>()
    {
        Some("icon/ui/files.svg")
    } else if action.is::<OpenFolder>() || action.is::<OpenCrashReports>() {
        Some("icon/workspace/folder.svg")
    } else if action.is::<OpenPreferences>() {
        Some("icon/ui/sliders.svg")
    } else if action.is::<SaveDocument>() || action.is::<SaveDocumentAs>() {
        Some("icon/ui/save.svg")
    } else if action.is::<QuitApplication>() {
        Some("icon/ui/power.svg")
    } else if action.is::<Undo>() {
        Some("icon/ui/undo.svg")
    } else if action.is::<Redo>() {
        Some("icon/ui/redo.svg")
    } else if action.is::<Cut>() {
        Some("icon/ui/scissors.svg")
    } else if action.is::<Copy>() || action.is::<CopyAsMarkdown>() {
        Some("icon/ui/copy.svg")
    } else if action.is::<Paste>() || action.is::<PasteAsPlainText>() {
        Some("icon/ui/clipboard.svg")
    } else if action.is::<SelectAll>() {
        Some("icon/ui/check.svg")
    } else if action.is::<FindInDocument>()
        || action.is::<ReplaceInDocument>()
        || action.is::<FindNext>()
        || action.is::<FindPrevious>()
    {
        Some("icon/ui/search.svg")
    } else if action.is::<QuickOpen>() {
        Some("icon/ui/files.svg")
    } else if action.is::<CommandPalette>() {
        Some("icon/ui/search.svg")
    } else if action.is::<PreviousTab>() {
        Some("icon/ui/arrow-left.svg")
    } else if action.is::<NextTab>() {
        Some("icon/ui/arrow-right.svg")
    } else if action.is::<ToggleViewMode>() {
        Some("icon/ui/live.svg")
    } else if action.is::<NormalizeLineEndingsLf>()
        || action.is::<NormalizeLineEndingsCrLf>()
        || action.is::<NormalizeLineEndingsCr>()
    {
        Some("icon/ui/source.svg")
    } else if action.is::<ExportHtml>() || action.is::<ExportImage>() || action.is::<ExportPdf>() {
        Some("icon/ui/file-output.svg")
    } else if action.is::<SelectLanguage>()
        || action.is::<AddLanguageConfig>()
        || action.is::<InstallCliTool>()
        || action.is::<UninstallCliTool>()
    {
        Some("icon/ui/keyboard.svg")
    } else if action.is::<SelectTheme>() || action.is::<AddThemeConfig>() {
        Some("icon/ui/palette.svg")
    } else if action.is::<ToggleWorkspace>() {
        Some("icon/ui/panel-left.svg")
    } else if action.is::<ToggleFocusMode>() || action.is::<ToggleTypewriterMode>() {
        Some("icon/ui/type.svg")
    } else if action.is::<OpenPrivacyPolicy>() {
        Some("icon/ui/shield.svg")
    } else if action.is::<ShowAbout>() {
        Some("icon/ui/info.svg")
    } else {
        None
    }
}

/// Global app-menu state for platform menu lifecycle hooks.
#[derive(Default)]
pub(crate) struct AppMenuState {
    window_closed_subscription: Option<Subscription>,
    /// Windows' native menu bridge may expose only the launcher menu back to GPUI.
    /// Keep the authoritative owned snapshot for the custom in-window renderer.
    pub(crate) in_window_menus: Vec<OwnedMenu>,
}

impl Global for AppMenuState {}

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
) -> WindowHandle<Editor> {
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
) -> WindowHandle<Editor> {
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
) -> WindowHandle<Editor> {
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
        .unwrap();

    handle
        .update(cx, |editor, window, cx| {
            window.activate_window();
            editor.force_install_close_guard(cx, window);
        })
        .expect("newly opened editor window should be updateable");

    handle
}

fn open_file_failure_window(cx: &mut App, path: PathBuf, reason: String) -> WindowHandle<Editor> {
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
        .expect("file failure window should open");
    handle
        .update(cx, |editor, window, cx| {
            window.activate_window();
            editor.force_install_close_guard(cx, window);
        })
        .expect("file failure window should be updateable");
    handle
}

/// Opens an unfinished recovery session directly in the editor surface.
pub(crate) fn open_recovered_editor_window(
    cx: &mut App,
    recovered: crate::recovery::RecoveredDocument,
) -> WindowHandle<Editor> {
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
        .unwrap();
    handle
        .update(cx, |editor, window, cx| {
            window.activate_window();
            window.set_window_edited(true);
            editor.force_install_close_guard(cx, window);
        })
        .expect("recovered editor window should be updateable");
    handle
}

pub(crate) fn open_recovered_editor_tabs_window(
    cx: &mut App,
    mut recovered: Vec<crate::recovery::RecoveredDocument>,
) -> Option<WindowHandle<Editor>> {
    if recovered.is_empty() {
        return None;
    }
    let additional = recovered.split_off(1);
    let first = recovered.pop().expect("non-empty recovery batch");
    let handle = open_recovered_editor_window(cx, first);
    if !additional.is_empty() {
        handle
            .update(cx, |editor, window, cx| {
                editor.append_recovered_tabs(additional, cx);
                window.set_window_edited(true);
            })
            .expect("recovery tab window should be updateable");
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
            open_file_failure_window(cx, path.to_path_buf(), error.to_string());
            record_recent_file_and_refresh(path, cx);
            return Ok(());
        }
    };
    match opened {
        crate::document_io::OpenedDocument::Resident(opened) => {
            let handle = open_decoded_editor_window(cx, opened, Some(path.to_path_buf()));
            if !crate::document_io::is_markdown_path(path) {
                let _ = handle.update(cx, |editor, _window, cx| {
                    editor.set_view_mode(crate::editor::ViewMode::Source, cx);
                });
            }
        }
        crate::document_io::OpenedDocument::ResidentFormat(probe)
        | crate::document_io::OpenedDocument::Paged(probe) => {
            open_large_editor_window(cx, path.to_path_buf(), probe, None)?;
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
            open_decoded_editor_window_with_bounds(
                cx,
                opened.clone(),
                Some(first.path.clone()),
                window_bounds,
            )
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
    };
    handle
        .update(cx, |editor, _window, cx| {
            editor.restore_tab_session(
                session.id,
                restored,
                active_index,
                session.workspace_root,
                session.workspace_panel_width,
                session.workspace_docked_open,
                session.split_pane_ratio,
                cx,
            );
        })
        .is_ok()
}

pub(crate) fn open_detached_tab_window(cx: &mut App, detached: crate::editor::DetachedTab) {
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
        .expect("detached tab window should open");
    handle
        .update(cx, |editor, window, cx| {
            window.activate_window();
            window.set_window_edited(editor.is_document_dirty());
            editor.force_install_close_guard(cx, window);
        })
        .expect("detached tab editor should be updateable");
}

fn record_recent_file_and_refresh(path: &Path, cx: &mut App) {
    if let Err(err) = record_recent_file(path) {
        eprintln!("failed to update recent file history: {err}");
        return;
    }
    install_menus(cx);
    cx.refresh_windows();
}

#[cfg(target_os = "macos")]
/// Check whether `/usr/local/bin/gmark` is correctly installed for this app.
///
/// Returns `true` only if the symlink exists **and** resolves (directly or via
/// one level of canonicalization) to the currently running executable.
fn is_cli_symlink_current_app() -> bool {
    let link = std::path::Path::new("/usr/local/bin/gmark");
    let Ok(target) = std::fs::read_link(link) else {
        return false; // does not exist or not a symlink
    };
    let resolved = if target.is_absolute() {
        // Canonicalize the target itself (may fail if dangling).
        std::fs::canonicalize(&target).unwrap_or(target)
    } else {
        // Relative — resolve from symlink's parent directory.
        link.parent()
            .unwrap_or(std::path::Path::new("/"))
            .join(&target)
            .canonicalize()
            .unwrap_or(target)
    };
    match std::env::current_exe() {
        Ok(exe) => resolved == exe,
        Err(_) => false,
    }
}

#[cfg(any(target_os = "macos", test))]
fn applescript_string_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}
#[path = "app_menu_parts/commands.rs"]
mod commands;
#[path = "app_menu_parts/menus.rs"]
mod menus;

#[cfg(test)]
use commands::is_window_context_menu_action;
pub(crate) use commands::{
    dispatch_menu_action, dispatch_menu_action_for_editor, install_cli_tool,
    record_recent_file_from_editor, request_quit_application, uninstall_cli_tool,
};
use commands::{recent_files_for_menu, show_window_prompt};
#[cfg(test)]
use menus::build_menus;
pub(crate) use menus::{init, install_menus};
use menus::{
    prompt_and_import_language_config, prompt_and_import_language_config_with_error_window,
    prompt_and_import_theme_config, prompt_and_import_theme_config_with_error_window,
    prompt_and_open_files, prompt_and_open_files_with_error_window, prompt_and_open_safe_source,
    prompt_and_open_safe_source_with_error_window,
};
#[cfg(test)]
#[path = "../tests/unit/app_menu.rs"]
mod tests;
