// @author kongweiguang

use super::*;

#[cfg(target_os = "macos")]
pub(crate) fn install_cli_tool(cx: &mut App) {
    use std::process::Command;

    let bin_link = "/usr/local/bin/gmark";
    let strings = cx.global::<I18nManager>().strings();

    let current_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            show_install_cli_error(cx, &format!("Failed to get executable path: {err}"));
            return;
        }
    };

    // Only allow from a portable .app bundle (e.g. drag-installed to /Applications)
    if !current_exe
        .to_string_lossy()
        .contains(".app/Contents/MacOS/")
    {
        show_install_cli_error(
            cx,
            "Command-line tool installation requires running from an .app bundle.\n\n\
             If the app was installed via the `.pkg` installer,\n\
             the CLI command is configured automatically.",
        );
        return;
    }

    let exe_path = applescript_string_literal(&current_exe.to_string_lossy());
    let link_path = applescript_string_literal(bin_link);
    let script = format!(
        r#"set exePath to {exe_path}
set linkPath to {link_path}
do shell script "rm -f " & quoted form of linkPath & linefeed & "ln -s " & quoted form of exePath & space & quoted form of linkPath with administrator privileges"#
    );

    match Command::new("osascript").arg("-e").arg(&script).output() {
        Ok(output) => {
            if output.status.success() {
                let title = "CLI Command Installed";
                let detail = format!(
                    "Successfully installed! You can now use 'gmark' from the terminal:\n\n\
                     \x1b[1mgmark README.md\x1b[0m\n\
                     \x1b[1mgmark file1.md file2.md\x1b[0m\n\n\
                     Location: {bin_link}\n\n\
                     Note: If you move or delete gmark.app,\n\
                     the 'gmark' command will stop working\n\
                     automatically (no cleanup needed)."
                );
                if let Some(window) = cx.active_window() {
                    let ok = strings.info_dialog_ok.clone();
                    let _ = window.update(cx, |_view, window, cx| {
                        let _ = window.prompt(
                            PromptLevel::Info,
                            &title,
                            Some(&detail),
                            &[ok.as_str()],
                            cx,
                        );
                    });
                }
            } else {
                // User pressed Cancel on the admin password dialog
                // or the link creation failed for another reason.
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let detail = if stderr.contains("User canceled") || stderr.contains("(-128)") {
                    "Installation cancelled.".to_string()
                } else {
                    format!("Installation failed: {stderr}")
                };
                show_install_cli_error(cx, &detail);
            }
        }
        Err(err) => {
            show_install_cli_error(cx, &format!("Failed to run installer: {err}"));
        }
    }
    // Refresh menus so the label changes between Install -> Uninstall.
    install_menus(cx);
}

#[cfg(target_os = "macos")]
pub(crate) fn uninstall_cli_tool(cx: &mut App) {
    use std::process::Command;

    let bin_link = "/usr/local/bin/gmark";
    let strings = cx.global::<I18nManager>().strings();

    if !is_cli_symlink_current_app() {
        show_install_cli_error(cx, "CLI command is not installed for this app.");
        return;
    }

    let link_path = applescript_string_literal(bin_link);
    let script = format!(
        r#"set linkPath to {link_path}
do shell script "rm -f " & quoted form of linkPath with administrator privileges"#
    );

    match Command::new("osascript").arg("-e").arg(&script).output() {
        Ok(output) => {
            if output.status.success() {
                let title = "CLI Command Uninstalled";
                let detail = "CLI command has been removed successfully.".to_string();
                if let Some(window) = cx.active_window() {
                    let ok = strings.info_dialog_ok.clone();
                    let _ = window.update(cx, |_view, window, cx| {
                        let _ = window.prompt(
                            PromptLevel::Info,
                            &title,
                            Some(&detail),
                            &[ok.as_str()],
                            cx,
                        );
                    });
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let detail = if stderr.contains("User canceled") || stderr.contains("(-128)") {
                    "Uninstall cancelled.".to_string()
                } else {
                    format!("Uninstall failed: {stderr}")
                };
                show_install_cli_error(cx, &detail);
            }
        }
        Err(err) => {
            show_install_cli_error(cx, &format!("Failed to run uninstaller: {err}"));
        }
    }
    // Refresh menus so the label changes between Install -> Uninstall.
    install_menus(cx);
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn install_cli_tool(cx: &mut App) {
    show_install_cli_error(
        cx,
        "Command-line tool installation is only available on macOS.",
    );
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn uninstall_cli_tool(cx: &mut App) {
    show_install_cli_error(
        cx,
        "Command-line tool uninstallation is only available on macOS.",
    );
}

fn show_install_cli_error(cx: &mut App, detail: &str) {
    let strings = cx.global::<I18nManager>().strings();
    let title = "Install Command-Line Tool Failed";

    if let Some(window) = cx.active_window() {
        let ok = strings.info_dialog_ok.clone();
        let _ = window.update(cx, |_view, window, cx| {
            let _ = window.prompt(
                PromptLevel::Critical,
                title,
                Some(detail),
                &[ok.as_str()],
                cx,
            );
        });
    } else {
        eprintln!("{title}: {detail}");
    }
}

pub(crate) fn record_recent_file_from_editor(path: &Path, cx: &mut App) {
    record_recent_file_and_refresh(path, cx);
}

pub(super) fn show_window_prompt(
    window: Option<AnyWindowHandle>,
    title: &str,
    detail: &str,
    cx: &mut App,
) {
    if let Some(window) = window {
        let ok = cx.global::<I18nManager>().strings().info_dialog_ok.clone();
        let _ = window.update(cx, |_view, window, cx| {
            let buttons = [ok.as_str()];
            let _ = window.prompt(PromptLevel::Critical, title, Some(detail), &buttons, cx);
        });
    } else {
        eprintln!("{title}: {detail}");
    }
}

fn with_active_editor<R>(
    cx: &mut App,
    update: impl FnOnce(&mut Editor, &mut Window, &mut Context<Editor>) -> R,
) -> Option<R> {
    let window = cx.active_window()?.downcast::<Editor>()?;
    window.update(cx, update).ok()
}

fn show_info_dialog_on_active_editor(cx: &mut App, kind: InfoDialogKind) {
    let _ = with_active_editor(cx, move |editor, _window, cx| {
        editor.show_info_dialog(kind, cx);
    });
}

fn request_update_check_on_active_editor(cx: &mut App) {
    let _ = with_active_editor(cx, |editor, window, cx| {
        editor.request_check_updates(window, cx);
    });
}

fn open_crash_reports(cx: &mut App) {
    match crate::crash_report::open_reports_directory() {
        Ok(()) => {}
        Err(error) => {
            let title = cx
                .global::<I18nManager>()
                .strings()
                .open_failed_title
                .clone();
            show_window_prompt(cx.active_window(), &title, &error.to_string(), cx);
        }
    }
}

pub(super) fn recent_files_for_menu() -> Vec<PathBuf> {
    match read_recent_files() {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("failed to read recent file history: {err}");
            Vec::new()
        }
    }
}

fn open_recent_file(cx: &mut App, path: PathBuf) {
    let error_window = cx.active_window();
    open_recent_file_with_error_window(cx, path, error_window);
}

fn open_recent_file_with_error_window(
    cx: &mut App,
    path: PathBuf,
    error_window: Option<AnyWindowHandle>,
) {
    if !path.is_file() {
        if let Err(err) = remove_recent_file(&path) {
            eprintln!("failed to remove missing recent file: {err}");
        }
        install_menus(cx);
        cx.refresh_windows();
        let strings = cx.global::<I18nManager>().strings().clone();
        let detail = strings
            .recent_file_missing_message_template
            .replace("{path}", &path.to_string_lossy());
        show_window_prompt(
            error_window,
            &strings.recent_file_missing_title,
            &detail,
            cx,
        );
        return;
    }

    if let Err(err) = open_file_in_new_window(cx, &path) {
        let title = cx
            .global::<I18nManager>()
            .strings()
            .open_failed_title
            .clone();
        show_window_prompt(error_window, &title, &err.to_string(), cx);
    }
}

fn is_editor_scoped_menu_action(action: &dyn Action) -> bool {
    is_editor_dispatch_action(action)
        || action.as_any().is::<SaveDocument>()
        || action.as_any().is::<SaveDocumentAs>()
        || action.as_any().is::<ExportHtml>()
        || action.as_any().is::<ExportImage>()
        || action.as_any().is::<ExportPdf>()
        || action.as_any().is::<NormalizeLineEndingsLf>()
        || action.as_any().is::<NormalizeLineEndingsCrLf>()
        || action.as_any().is::<NormalizeLineEndingsCr>()
        || action.as_any().is::<QuitApplication>()
        || action.as_any().is::<CloseWindow>()
        || action.as_any().is::<NewTab>()
        || action.as_any().is::<CloseTab>()
        || action.as_any().is::<ReopenClosedTab>()
        || action.as_any().is::<CheckForUpdates>()
        || action.as_any().is::<ShowAbout>()
        || action.as_any().is::<InstallCliTool>()
        || action.as_any().is::<UninstallCliTool>()
        || action.as_any().is::<ToggleWorkspace>()
        || action.as_any().is::<OpenFolder>()
        || action.as_any().is::<ToggleFocusMode>()
        || action.as_any().is::<ToggleTypewriterMode>()
}

/// 这些 action 必须沿窗口焦点路径分发，才能由当前 Block 或 Editor 处理。
/// 全局菜单与 Windows/Linux 自绘菜单共用同一判定，避免两条入口能力漂移。
fn is_editor_dispatch_action(action: &dyn Action) -> bool {
    action.as_any().is::<Undo>()
        || action.as_any().is::<Redo>()
        || action.as_any().is::<Cut>()
        || action.as_any().is::<Copy>()
        || action.as_any().is::<CopyAsMarkdown>()
        || action.as_any().is::<Paste>()
        || action.as_any().is::<PasteAsPlainText>()
        || action.as_any().is::<SelectAll>()
        || action.as_any().is::<FindInDocument>()
        || action.as_any().is::<ReplaceInDocument>()
        || action.as_any().is::<FindNext>()
        || action.as_any().is::<FindPrevious>()
        || action.as_any().is::<BoldSelection>()
        || action.as_any().is::<ItalicSelection>()
        || action.as_any().is::<StrikethroughSelection>()
        || action.as_any().is::<UnderlineSelection>()
        || action.as_any().is::<HighlightSelection>()
        || action.as_any().is::<SuperscriptSelection>()
        || action.as_any().is::<SubscriptSelection>()
        || action.as_any().is::<InlineMathSelection>()
        || action.as_any().is::<CodeSelection>()
        || action.as_any().is::<LinkSelection>()
}

pub(super) fn is_window_context_menu_action(action: &dyn Action) -> bool {
    action.as_any().is::<NewWindow>()
        || action.as_any().is::<OpenFile>()
        || action.as_any().is::<OpenSafeSource>()
        || action.as_any().is::<OpenPreferences>()
        || action.as_any().is::<OpenRecentFile>()
        || action.as_any().is::<NoRecentFiles>()
        || action.as_any().is::<AddLanguageConfig>()
        || action.as_any().is::<AddThemeConfig>()
        || action.as_any().is::<InstallCliTool>()
        || action.as_any().is::<UninstallCliTool>()
        || action.as_any().is::<OpenCrashReports>()
        || action.as_any().is::<OpenPrivacyPolicy>()
        || is_editor_scoped_menu_action(action)
}

fn current_window_candidates(cx: &mut App) -> Vec<AnyWindowHandle> {
    let mut candidates = Vec::new();
    let mut push_unique = |window: AnyWindowHandle| {
        if candidates
            .iter()
            .all(|candidate: &AnyWindowHandle| candidate.window_id() != window.window_id())
        {
            candidates.push(window);
        }
    };

    if let Some(window) = cx.active_window() {
        push_unique(window);
    }
    if let Some(windows) = cx.window_stack() {
        for window in windows {
            push_unique(window);
        }
    }
    for window in cx.windows() {
        push_unique(window);
    }

    candidates
}

fn request_close_editor_window(window: AnyWindowHandle, cx: &mut App) -> bool {
    let Some(window) = window.downcast::<Editor>() else {
        return false;
    };

    window
        .update(cx, |editor, window, cx| {
            editor.request_close_current_window(window, cx);
        })
        .is_ok()
}

fn request_close_current_editor_window(cx: &mut App) {
    let candidates = current_window_candidates(cx);
    if candidates.is_empty() {
        cx.quit();
        return;
    }

    for window in candidates {
        if request_close_editor_window(window, cx) {
            return;
        }
    }
}

pub(crate) fn request_quit_application(cx: &mut App) {
    let candidates = current_window_candidates(cx);
    if candidates.is_empty() {
        cx.quit();
        return;
    }

    for window in candidates {
        let Some(window) = window.downcast::<Editor>() else {
            continue;
        };

        let should_close = window
            .update(cx, |editor, window, cx| {
                editor.on_window_should_close_for_quit(window, cx)
            })
            .unwrap_or(false);
        if !should_close {
            return;
        }
    }

    cx.quit();
}

fn menu_block_kind(action: &dyn Action) -> Option<BlockKind> {
    if action.as_any().is::<SetHeading1>() {
        Some(BlockKind::Heading { level: 1 })
    } else if action.as_any().is::<SetHeading2>() {
        Some(BlockKind::Heading { level: 2 })
    } else if action.as_any().is::<SetHeading3>() {
        Some(BlockKind::Heading { level: 3 })
    } else if action.as_any().is::<SetHeading4>() {
        Some(BlockKind::Heading { level: 4 })
    } else if action.as_any().is::<SetHeading5>() {
        Some(BlockKind::Heading { level: 5 })
    } else if action.as_any().is::<SetHeading6>() {
        Some(BlockKind::Heading { level: 6 })
    } else if action.as_any().is::<SetParagraph>() {
        Some(BlockKind::Paragraph)
    } else if action.as_any().is::<SetBulletedList>() {
        Some(BlockKind::BulletedListItem)
    } else if action.as_any().is::<SetNumberedList>() {
        Some(BlockKind::NumberedListItem)
    } else if action.as_any().is::<SetTaskList>() {
        Some(BlockKind::TaskListItem { checked: false })
    } else if action.as_any().is::<SetQuote>() {
        Some(BlockKind::Quote)
    } else if action.as_any().is::<SetCodeBlock>() {
        Some(BlockKind::CodeBlock { language: None })
    } else {
        None
    }
}

/// Executes one of the app-menu actions against the current application state.
pub(crate) fn dispatch_menu_action(action: &dyn Action, cx: &mut App) {
    if action.as_any().is::<NewWindow>() {
        open_editor_window(cx, String::new(), None);
    } else if action.as_any().is::<NewTab>() {
        let _ = with_active_editor(cx, |editor, _window, cx| {
            editor.new_untitled_tab(cx);
        });
    } else if action.as_any().is::<OpenFile>() {
        prompt_and_open_files(cx);
    } else if action.as_any().is::<OpenSafeSource>() {
        prompt_and_open_safe_source(cx);
    } else if action.as_any().is::<OpenFolder>() {
        let _ = with_active_editor(cx, |editor, window, cx| {
            editor.on_open_folder_action(&OpenFolder, window, cx);
        });
    } else if action.as_any().is::<OpenPreferences>() {
        open_preferences_window(cx);
    } else if let Some(action) = action.as_any().downcast_ref::<OpenRecentFile>() {
        open_recent_file(cx, PathBuf::from(&action.path));
    } else if action.as_any().is::<NoRecentFiles>() {
    } else if action.as_any().is::<AddLanguageConfig>() {
        prompt_and_import_language_config(cx);
    } else if action.as_any().is::<AddThemeConfig>() {
        prompt_and_import_theme_config(cx);
    } else if is_editor_dispatch_action(action) {
        let action = action.boxed_clone();
        let _ = with_active_editor(cx, |_editor, window, cx| {
            window.dispatch_action(action, cx);
        });
    } else if let Some(kind) = menu_block_kind(action) {
        let _ = with_active_editor(cx, |editor, _window, cx| {
            editor.set_active_block_kind(kind, cx)
        });
    } else if action.as_any().is::<SaveDocument>() {
        let _ = with_active_editor(cx, |editor, window, cx| editor.save_document(window, cx));
    } else if action.as_any().is::<SaveDocumentAs>() {
        let _ = with_active_editor(cx, |editor, window, cx| editor.save_document_as(window, cx));
    } else if action.as_any().is::<ExportHtml>() {
        let _ = with_active_editor(cx, |editor, window, cx| {
            editor.export_document_via_prompt(ExportFormat::Html, window, cx)
        });
    } else if action.as_any().is::<ExportImage>() {
        let _ = with_active_editor(cx, |editor, window, cx| {
            editor.export_document_via_prompt(ExportFormat::Png, window, cx)
        });
    } else if action.as_any().is::<ExportPdf>() {
        let _ = with_active_editor(cx, |editor, window, cx| {
            editor.export_document_via_prompt(ExportFormat::Pdf, window, cx)
        });
    } else if action.as_any().is::<NormalizeLineEndingsLf>() {
        let _ = with_active_editor(cx, |editor, _window, cx| {
            editor.normalize_line_endings(gmark_document::LineEnding::Lf, cx);
        });
    } else if action.as_any().is::<NormalizeLineEndingsCrLf>() {
        let _ = with_active_editor(cx, |editor, _window, cx| {
            editor.normalize_line_endings(gmark_document::LineEnding::CrLf, cx);
        });
    } else if action.as_any().is::<NormalizeLineEndingsCr>() {
        let _ = with_active_editor(cx, |editor, _window, cx| {
            editor.normalize_line_endings(gmark_document::LineEnding::Cr, cx);
        });
    } else if let Some(action) = action.as_any().downcast_ref::<SelectTheme>() {
        match apply_configured_theme(cx, &action.theme_id) {
            Ok(changed) => {
                if changed {
                    install_menus(cx);
                    cx.refresh_windows();
                }
            }
            Err(err) => {
                let title = cx
                    .global::<I18nManager>()
                    .strings()
                    .preferences_save_failed_title
                    .clone();
                show_window_prompt(cx.active_window(), &title, &err.to_string(), cx);
            }
        }
    } else if let Some(action) = action.as_any().downcast_ref::<SelectLanguage>() {
        match apply_configured_language(cx, &action.language_id) {
            Ok(changed) => {
                if changed {
                    install_menus(cx);
                    cx.refresh_windows();
                }
            }
            Err(err) => {
                let title = cx
                    .global::<I18nManager>()
                    .strings()
                    .preferences_save_failed_title
                    .clone();
                show_window_prompt(cx.active_window(), &title, &err.to_string(), cx);
            }
        }
    } else if action.as_any().is::<CheckForUpdates>() {
        request_update_check_on_active_editor(cx);
    } else if action.as_any().is::<OpenCrashReports>() {
        open_crash_reports(cx);
    } else if action.as_any().is::<OpenPrivacyPolicy>() {
        cx.open_url(PRIVACY_POLICY_URL);
    } else if action.as_any().is::<ShowAbout>() {
        show_info_dialog_on_active_editor(cx, InfoDialogKind::About);
    } else if action.as_any().is::<InstallCliTool>() {
        install_cli_tool(cx);
    } else if action.as_any().is::<UninstallCliTool>() {
        uninstall_cli_tool(cx);
    } else if action.as_any().is::<ToggleWorkspace>() {
        let _ = with_active_editor(cx, |editor, window, cx| {
            editor.toggle_workspace_drawer(window, cx);
        });
    } else if action.as_any().is::<ToggleFocusMode>() {
        let _ = with_active_editor(cx, |editor, window, cx| {
            editor.on_toggle_focus_mode_action(&ToggleFocusMode, window, cx);
        });
    } else if action.as_any().is::<ToggleTypewriterMode>() {
        let _ = with_active_editor(cx, |editor, window, cx| {
            editor.on_toggle_typewriter_mode_action(&ToggleTypewriterMode, window, cx);
        });
    } else if action.as_any().is::<CloseTab>() {
        let _ = with_active_editor(cx, |editor, window, cx| {
            editor.on_close_tab_action(&CloseTab, window, cx);
        });
    } else if action.as_any().is::<ReopenClosedTab>() {
        let _ = with_active_editor(cx, |editor, window, cx| {
            editor.on_reopen_closed_tab_action(&ReopenClosedTab, window, cx);
        });
    } else if action.as_any().is::<QuitApplication>() {
        request_quit_application(cx);
    } else if action.as_any().is::<CloseWindow>() {
        request_close_current_editor_window(cx);
    }
}

/// Executes a menu action against a specific editor when the action is
/// editor-scoped, falling back to app-wide behavior for global actions.
pub(crate) fn dispatch_menu_action_for_editor(
    action: &dyn Action,
    target: &WeakEntity<Editor>,
    window: &mut Window,
    cx: &mut App,
) {
    if !is_window_context_menu_action(action) {
        let deferred_action = action.boxed_clone();
        cx.defer(move |cx| {
            dispatch_menu_action(deferred_action.as_ref(), cx);
        });
        return;
    }

    window.activate_window();
    let current_window = Some(window.window_handle());

    if action.as_any().is::<NewWindow>() {
        open_editor_window(cx, String::new(), None);
    } else if action.as_any().is::<NewTab>() {
        let _ = target.update(cx, |editor, cx| {
            editor.new_untitled_tab(cx);
        });
    } else if action.as_any().is::<OpenFile>() {
        prompt_and_open_files_with_error_window(cx, current_window);
    } else if action.as_any().is::<OpenSafeSource>() {
        prompt_and_open_safe_source_with_error_window(cx, current_window);
    } else if action.as_any().is::<OpenFolder>() {
        let _ = target.update(cx, |editor, cx| {
            editor.on_open_folder_action(&OpenFolder, window, cx);
        });
    } else if action.as_any().is::<OpenPreferences>() {
        open_preferences_window(cx);
    } else if let Some(action) = action.as_any().downcast_ref::<OpenRecentFile>() {
        open_recent_file_with_error_window(cx, PathBuf::from(&action.path), current_window);
    } else if action.as_any().is::<NoRecentFiles>() {
    } else if action.as_any().is::<AddLanguageConfig>() {
        prompt_and_import_language_config_with_error_window(cx, current_window);
    } else if action.as_any().is::<AddThemeConfig>() {
        prompt_and_import_theme_config_with_error_window(cx, current_window);
    } else if is_editor_dispatch_action(action) {
        window.dispatch_action(action.boxed_clone(), cx);
    } else if action.as_any().is::<SaveDocument>() {
        let _ = target.update(cx, |editor, cx| editor.request_save_document(cx));
    } else if action.as_any().is::<SaveDocumentAs>() {
        let _ = target.update(cx, |editor, cx| editor.request_save_document_as(cx));
    } else if action.as_any().is::<ExportHtml>() {
        let _ = target.update(cx, |editor, cx| {
            editor.export_document_via_prompt(ExportFormat::Html, window, cx);
        });
    } else if action.as_any().is::<ExportImage>() {
        let _ = target.update(cx, |editor, cx| {
            editor.export_document_via_prompt(ExportFormat::Png, window, cx);
        });
    } else if action.as_any().is::<ExportPdf>() {
        let _ = target.update(cx, |editor, cx| {
            editor.export_document_via_prompt(ExportFormat::Pdf, window, cx);
        });
    } else if action.as_any().is::<NormalizeLineEndingsLf>() {
        let _ = target.update(cx, |editor, cx| {
            editor.normalize_line_endings(gmark_document::LineEnding::Lf, cx);
        });
    } else if action.as_any().is::<NormalizeLineEndingsCrLf>() {
        let _ = target.update(cx, |editor, cx| {
            editor.normalize_line_endings(gmark_document::LineEnding::CrLf, cx);
        });
    } else if action.as_any().is::<NormalizeLineEndingsCr>() {
        let _ = target.update(cx, |editor, cx| {
            editor.normalize_line_endings(gmark_document::LineEnding::Cr, cx);
        });
    } else if action.as_any().is::<QuitApplication>() {
        request_quit_application(cx);
    } else if action.as_any().is::<CloseWindow>() {
        let _ = target.update(cx, |editor, cx| {
            editor.request_close_current_window(window, cx);
        });
    } else if action.as_any().is::<CloseTab>() {
        let _ = target.update(cx, |editor, cx| {
            editor.on_close_tab_action(&CloseTab, window, cx);
        });
    } else if action.as_any().is::<ReopenClosedTab>() {
        let _ = target.update(cx, |editor, cx| {
            editor.on_reopen_closed_tab_action(&ReopenClosedTab, window, cx);
        });
    } else if action.as_any().is::<CheckForUpdates>() {
        let _ = target.update(cx, |editor, cx| {
            editor.request_check_updates(window, cx);
        });
    } else if action.as_any().is::<OpenCrashReports>() {
        open_crash_reports(cx);
    } else if action.as_any().is::<OpenPrivacyPolicy>() {
        cx.open_url(PRIVACY_POLICY_URL);
    } else if action.as_any().is::<ShowAbout>() {
        let _ = target.update(cx, |editor, cx| {
            editor.show_info_dialog(InfoDialogKind::About, cx)
        });
    } else if action.as_any().is::<InstallCliTool>() {
        install_cli_tool(cx);
    } else if action.as_any().is::<UninstallCliTool>() {
        uninstall_cli_tool(cx);
    } else if action.as_any().is::<ToggleWorkspace>() {
        let _ = target.update(cx, |editor, cx| {
            editor.toggle_workspace_drawer(window, cx);
        });
    } else if action.as_any().is::<ToggleFocusMode>() {
        let _ = target.update(cx, |editor, cx| {
            editor.on_toggle_focus_mode_action(&ToggleFocusMode, window, cx);
        });
    } else if action.as_any().is::<ToggleTypewriterMode>() {
        let _ = target.update(cx, |editor, cx| {
            editor.on_toggle_typewriter_mode_action(&ToggleTypewriterMode, window, cx);
        });
    }
}
