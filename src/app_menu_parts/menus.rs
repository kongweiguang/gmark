// @author kongweiguang

use super::*;

pub(super) fn build_menus(
    _theme_manager: &ThemeManager,
    i18n_manager: &I18nManager,
    recent_files: &[PathBuf],
) -> Vec<Menu> {
    let strings = i18n_manager.strings().clone();

    let recent_items = if recent_files.is_empty() {
        vec![MenuItem::action(
            strings.menu_no_recent_files.clone(),
            NoRecentFiles,
        )]
    } else {
        recent_files
            .iter()
            .map(|path| {
                // into_owned on a Cow<str> reuses the Cow::Owned variant
                // (no copy) when the OS string is valid UTF-8 — the common
                // case — and only allocates for the lossy fallback. The
                // previous .to_string_lossy().to_string() always allocated.
                let label = path.to_string_lossy().into_owned();
                MenuItem::action(label.clone(), OpenRecentFile { path: label })
            })
            .collect()
    };

    #[cfg(target_os = "macos")]
    let initial_menus = {
        // On macOS, the first menu is the app menu (macOS overrides its title
        // with the app name). File operations go in a separate "File" menu to
        // match standard macOS conventions.
        vec![
            Menu {
                name: "gmark".into(),
                items: vec![
                    MenuItem::action(strings.menu_preferences.clone(), OpenPreferences),
                    MenuItem::separator(),
                    MenuItem::action(strings.menu_quit.clone(), QuitApplication),
                ],
            },
            Menu {
                name: strings.menu_file.into(),
                items: vec![
                    MenuItem::action(strings.menu_new_tab.clone(), NewTab),
                    MenuItem::action(strings.menu_new_window.clone(), NewWindow),
                    MenuItem::action(strings.menu_reopen_closed_tab.clone(), ReopenClosedTab),
                    MenuItem::action(strings.menu_close_tab.clone(), CloseTab),
                    MenuItem::action(strings.menu_close_window.clone(), CloseWindow),
                    MenuItem::action(strings.menu_open_file.clone(), OpenFile),
                    MenuItem::action(strings.menu_open_folder.clone(), OpenFolder),
                    MenuItem::submenu(Menu {
                        name: strings.menu_open_recent_file.clone().into(),
                        items: recent_items,
                    }),
                    MenuItem::separator(),
                    MenuItem::action(strings.menu_save.clone(), SaveDocument),
                    MenuItem::action(strings.menu_save_as.clone(), SaveDocumentAs),
                    MenuItem::submenu(Menu {
                        name: strings.menu_export.clone().into(),
                        items: vec![
                            MenuItem::action(strings.menu_export_html.clone(), ExportHtml),
                            MenuItem::action(strings.menu_export_image.clone(), ExportImage),
                            MenuItem::action(strings.menu_export_pdf.clone(), ExportPdf),
                        ],
                    }),
                    MenuItem::submenu(Menu {
                        name: strings.menu_line_endings.clone().into(),
                        items: vec![
                            MenuItem::action(
                                strings.menu_line_ending_lf.clone(),
                                NormalizeLineEndingsLf,
                            ),
                            MenuItem::action(
                                strings.menu_line_ending_crlf.clone(),
                                NormalizeLineEndingsCrLf,
                            ),
                            MenuItem::action(
                                strings.menu_line_ending_cr.clone(),
                                NormalizeLineEndingsCr,
                            ),
                        ],
                    }),
                ],
            },
        ]
    };

    #[cfg(not(target_os = "macos"))]
    let initial_menus = {
        vec![
            // 客户端标题栏把这个应用菜单渲染为图标；名称只作平台菜单契约，不显示在界面上。
            Menu {
                name: "gmark".into(),
                items: vec![
                    MenuItem::action(strings.menu_preferences.clone(), OpenPreferences),
                    MenuItem::separator(),
                    MenuItem::action(strings.menu_check_updates.clone(), CheckForUpdates),
                    MenuItem::separator(),
                    MenuItem::action(strings.menu_about.clone(), ShowAbout),
                    MenuItem::action(strings.menu_quit.clone(), QuitApplication),
                ],
            },
            Menu {
                name: strings.menu_file.into(),
                items: vec![
                    MenuItem::action(strings.menu_new_tab.clone(), NewTab),
                    MenuItem::action(strings.menu_new_window.clone(), NewWindow),
                    MenuItem::action(strings.menu_reopen_closed_tab.clone(), ReopenClosedTab),
                    MenuItem::action(strings.menu_close_tab.clone(), CloseTab),
                    MenuItem::action(strings.menu_close_window.clone(), CloseWindow),
                    MenuItem::action(strings.menu_open_file.clone(), OpenFile),
                    MenuItem::action(strings.menu_open_folder.clone(), OpenFolder),
                    MenuItem::submenu(Menu {
                        name: strings.menu_open_recent_file.clone().into(),
                        items: recent_items,
                    }),
                    MenuItem::separator(),
                    MenuItem::action(strings.menu_save.clone(), SaveDocument),
                    MenuItem::action(strings.menu_save_as.clone(), SaveDocumentAs),
                    MenuItem::submenu(Menu {
                        name: strings.menu_export.clone().into(),
                        items: vec![
                            MenuItem::action(strings.menu_export_html.clone(), ExportHtml),
                            MenuItem::action(strings.menu_export_image.clone(), ExportImage),
                            MenuItem::action(strings.menu_export_pdf.clone(), ExportPdf),
                        ],
                    }),
                    MenuItem::submenu(Menu {
                        name: strings.menu_line_endings.clone().into(),
                        items: vec![
                            MenuItem::action(
                                strings.menu_line_ending_lf.clone(),
                                NormalizeLineEndingsLf,
                            ),
                            MenuItem::action(
                                strings.menu_line_ending_crlf.clone(),
                                NormalizeLineEndingsCrLf,
                            ),
                            MenuItem::action(
                                strings.menu_line_ending_cr.clone(),
                                NormalizeLineEndingsCr,
                            ),
                        ],
                    }),
                ],
            },
        ]
    };

    #[cfg(target_os = "macos")]
    let help_items = {
        // Show different menu item depending on whether CLI is already
        // installed pointing to the current app.  Only portable
        // installations (drag-installed .app bundles) need this —
        // pkg-installed apps manage the symlink via postinstall.
        let cli_installed = is_cli_symlink_current_app();
        let mut items = vec![
            MenuItem::action(strings.menu_check_updates.clone(), CheckForUpdates),
            MenuItem::separator(),
        ];
        if cli_installed {
            items.push(MenuItem::action(
                SharedString::new(strings.menu_uninstall_cli_tool.as_str()),
                UninstallCliTool,
            ));
        } else {
            items.push(MenuItem::action(
                SharedString::new(strings.menu_install_cli_tool.as_str()),
                InstallCliTool,
            ));
        }
        items.push(MenuItem::separator());
        items.push(MenuItem::action(
            strings.menu_open_crash_reports.clone(),
            OpenCrashReports,
        ));
        items.push(MenuItem::action(
            strings.menu_privacy_policy.clone(),
            OpenPrivacyPolicy,
        ));
        items.push(MenuItem::separator());
        items.push(MenuItem::action(strings.menu_about.clone(), ShowAbout));
        items
    };
    #[cfg(not(target_os = "macos"))]
    let help_items = vec![
        MenuItem::action(strings.menu_open_crash_reports.clone(), OpenCrashReports),
        MenuItem::action(strings.menu_privacy_policy.clone(), OpenPrivacyPolicy),
    ];

    let mut menus = initial_menus;
    menus.extend([
        Menu {
            name: strings.menu_edit.clone().into(),
            items: vec![
                MenuItem::action(strings.preferences_shortcut_undo.clone(), Undo),
                MenuItem::action(strings.preferences_shortcut_redo.clone(), Redo),
                MenuItem::separator(),
                MenuItem::action(strings.preferences_shortcut_cut.clone(), Cut),
                MenuItem::action(strings.preferences_shortcut_copy.clone(), Copy),
                MenuItem::action(
                    strings.preferences_shortcut_copy_as_markdown.clone(),
                    CopyAsMarkdown,
                ),
                MenuItem::action(strings.preferences_shortcut_paste.clone(), Paste),
                MenuItem::action(
                    strings.preferences_shortcut_paste_as_plain_text.clone(),
                    PasteAsPlainText,
                ),
                MenuItem::action(strings.preferences_shortcut_select_all.clone(), SelectAll),
                MenuItem::separator(),
                MenuItem::action(
                    strings.preferences_shortcut_find_in_document.clone(),
                    FindInDocument,
                ),
                MenuItem::action(
                    strings.preferences_shortcut_replace_in_document.clone(),
                    ReplaceInDocument,
                ),
                MenuItem::separator(),
                MenuItem::action(strings.preferences_shortcut_find_next.clone(), FindNext),
                MenuItem::action(
                    strings.preferences_shortcut_find_previous.clone(),
                    FindPrevious,
                ),
            ],
        },
        Menu {
            name: strings.menu_view.into(),
            items: vec![
                MenuItem::action(strings.menu_toggle_workspace.clone(), ToggleWorkspace),
                MenuItem::separator(),
                MenuItem::action(strings.menu_toggle_focus_mode.clone(), ToggleFocusMode),
                MenuItem::action(
                    strings.menu_toggle_typewriter_mode.clone(),
                    ToggleTypewriterMode,
                ),
            ],
        },
        Menu {
            name: strings.menu_help.into(),
            items: help_items,
        },
    ]);
    menus
}

pub(crate) fn install_menus(cx: &mut App) {
    // 测试和窗口级重装菜单可能早于应用启动钩子；菜单快照必须能独立初始化。
    if cx.try_global::<AppMenuState>().is_none() {
        cx.set_global(AppMenuState::default());
    }
    let recent_files = recent_files_for_menu();
    let owned = build_menus(
        cx.global::<ThemeManager>(),
        cx.global::<I18nManager>(),
        &recent_files,
    )
    .into_iter()
    .map(Menu::owned)
    .collect();
    let menus = build_menus(
        cx.global::<ThemeManager>(),
        cx.global::<I18nManager>(),
        &recent_files,
    );
    cx.global_mut::<AppMenuState>().in_window_menus = owned;
    cx.set_menus(menus);
}

pub(super) fn prompt_and_open_files(cx: &mut App) {
    let error_window = cx.active_window();
    prompt_and_open_files_with_error_window(cx, error_window);
}

pub(super) fn prompt_and_open_files_with_error_window(
    cx: &mut App,
    error_window: Option<AnyWindowHandle>,
) {
    let prompt_title = cx
        .global::<I18nManager>()
        .strings()
        .open_markdown_files_prompt
        .clone();
    let prompt = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: Some(prompt_title.into()),
    });

    cx.spawn(async move |cx| match prompt.await {
        Ok(Ok(Some(paths))) => {
            let _ = cx.update(move |cx| {
                for path in paths {
                    if let Err(err) = open_file_in_new_window(cx, &path) {
                        let title = cx
                            .global::<I18nManager>()
                            .strings()
                            .open_failed_title
                            .clone();
                        show_window_prompt(error_window, &title, &err.to_string(), cx);
                    }
                }
            });
        }
        Ok(Err(err)) => {
            let detail = err.to_string();
            let _ = cx.update(move |cx| {
                let title = cx
                    .global::<I18nManager>()
                    .strings()
                    .open_failed_title
                    .clone();
                show_window_prompt(error_window, &title, &detail, cx);
            });
        }
        Ok(Ok(None)) | Err(_) => {}
    })
    .detach();
}

pub(super) fn prompt_and_import_language_config(cx: &mut App) {
    let error_window = cx.active_window();
    prompt_and_import_language_config_with_error_window(cx, error_window);
}

pub(super) fn prompt_and_import_language_config_with_error_window(
    cx: &mut App,
    error_window: Option<AnyWindowHandle>,
) {
    let prompt_title = cx
        .global::<I18nManager>()
        .strings()
        .add_language_config_prompt
        .clone();
    let prompt = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some(prompt_title.into()),
    });

    cx.spawn(async move |cx| match prompt.await {
        Ok(Ok(Some(paths))) => {
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = cx.update(move |cx| {
                let result = import_language_config_and_select(cx, &path);
                match result {
                    Ok(_) => {
                        install_menus(cx);
                        cx.refresh_windows();
                    }
                    Err(err) => {
                        let title = cx
                            .global::<I18nManager>()
                            .strings()
                            .config_import_failed_title
                            .clone();
                        show_window_prompt(error_window, &title, &err.to_string(), cx);
                    }
                }
            });
        }
        Ok(Err(err)) => {
            let detail = err.to_string();
            let _ = cx.update(move |cx| {
                let title = cx
                    .global::<I18nManager>()
                    .strings()
                    .config_import_failed_title
                    .clone();
                show_window_prompt(error_window, &title, &detail, cx);
            });
        }
        Ok(Ok(None)) | Err(_) => {}
    })
    .detach();
}

pub(super) fn prompt_and_import_theme_config(cx: &mut App) {
    let error_window = cx.active_window();
    prompt_and_import_theme_config_with_error_window(cx, error_window);
}

pub(super) fn prompt_and_import_theme_config_with_error_window(
    cx: &mut App,
    error_window: Option<AnyWindowHandle>,
) {
    let prompt_title = cx
        .global::<I18nManager>()
        .strings()
        .add_theme_config_prompt
        .clone();
    let prompt = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some(prompt_title.into()),
    });

    cx.spawn(async move |cx| match prompt.await {
        Ok(Ok(Some(paths))) => {
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = cx.update(move |cx| {
                let result = import_theme_config_and_select(cx, &path);
                match result {
                    Ok(_) => {
                        install_menus(cx);
                        cx.refresh_windows();
                    }
                    Err(err) => {
                        let title = cx
                            .global::<I18nManager>()
                            .strings()
                            .config_import_failed_title
                            .clone();
                        show_window_prompt(error_window, &title, &err.to_string(), cx);
                    }
                }
            });
        }
        Ok(Err(err)) => {
            let detail = err.to_string();
            let _ = cx.update(move |cx| {
                let title = cx
                    .global::<I18nManager>()
                    .strings()
                    .config_import_failed_title
                    .clone();
                show_window_prompt(error_window, &title, &detail, cx);
            });
        }
        Ok(Ok(None)) | Err(_) => {}
    })
    .detach();
}

fn handle_window_closed(cx: &mut App) {
    if cx.windows().is_empty() {
        cx.quit();
    }
}

/// Installs menu state, action handlers, and the native menu bar.
pub(crate) fn init(cx: &mut App) {
    EditingCommandHistory::init(cx);
    cx.set_global(AppMenuState::default());
    let subscription = cx.on_window_closed(handle_window_closed);
    cx.global_mut::<AppMenuState>().window_closed_subscription = Some(subscription);

    cx.on_action(|_: &NewWindow, cx| {
        dispatch_menu_action(&NewWindow, cx);
    });
    cx.on_action(|_: &NewTab, cx| {
        dispatch_menu_action(&NewTab, cx);
    });
    cx.on_action(|_: &OpenFile, cx| {
        dispatch_menu_action(&OpenFile, cx);
    });
    cx.on_action(|_: &OpenFolder, cx| {
        dispatch_menu_action(&OpenFolder, cx);
    });
    cx.on_action(|_: &OpenPreferences, cx| {
        dispatch_menu_action(&OpenPreferences, cx);
    });
    cx.on_action(|action: &OpenRecentFile, cx| {
        dispatch_menu_action(action, cx);
    });
    cx.on_action(|_: &NoRecentFiles, cx| {
        dispatch_menu_action(&NoRecentFiles, cx);
    });
    cx.on_action(|_: &AddLanguageConfig, cx| {
        dispatch_menu_action(&AddLanguageConfig, cx);
    });
    cx.on_action(|_: &AddThemeConfig, cx| {
        dispatch_menu_action(&AddThemeConfig, cx);
    });
    cx.on_action(|_: &SaveDocument, cx| {
        dispatch_menu_action(&SaveDocument, cx);
    });
    cx.on_action(|_: &SaveDocumentAs, cx| {
        dispatch_menu_action(&SaveDocumentAs, cx);
    });
    cx.on_action(|_: &ExportHtml, cx| {
        dispatch_menu_action(&ExportHtml, cx);
    });
    cx.on_action(|_: &ExportImage, cx| {
        dispatch_menu_action(&ExportImage, cx);
    });
    cx.on_action(|_: &ExportPdf, cx| {
        dispatch_menu_action(&ExportPdf, cx);
    });
    cx.on_action(|_: &NormalizeLineEndingsLf, cx| {
        dispatch_menu_action(&NormalizeLineEndingsLf, cx);
    });
    cx.on_action(|_: &NormalizeLineEndingsCrLf, cx| {
        dispatch_menu_action(&NormalizeLineEndingsCrLf, cx);
    });
    cx.on_action(|_: &NormalizeLineEndingsCr, cx| {
        dispatch_menu_action(&NormalizeLineEndingsCr, cx);
    });
    cx.on_action(|action: &SelectTheme, cx| {
        dispatch_menu_action(action, cx);
    });
    cx.on_action(|action: &SelectLanguage, cx| {
        dispatch_menu_action(action, cx);
    });
    cx.on_action(|_: &CheckForUpdates, cx| {
        dispatch_menu_action(&CheckForUpdates, cx);
    });
    cx.on_action(|_: &OpenCrashReports, cx| {
        dispatch_menu_action(&OpenCrashReports, cx);
    });
    cx.on_action(|_: &OpenPrivacyPolicy, cx| {
        dispatch_menu_action(&OpenPrivacyPolicy, cx);
    });
    cx.on_action(|_: &ShowAbout, cx| {
        dispatch_menu_action(&ShowAbout, cx);
    });
    cx.on_action(|_: &ToggleWorkspace, cx| {
        dispatch_menu_action(&ToggleWorkspace, cx);
    });
    cx.on_action(|_: &ToggleFocusMode, cx| {
        dispatch_menu_action(&ToggleFocusMode, cx);
    });
    cx.on_action(|_: &ToggleTypewriterMode, cx| {
        dispatch_menu_action(&ToggleTypewriterMode, cx);
    });
    cx.on_action(|_: &BoldSelection, cx| {
        dispatch_menu_action(&BoldSelection, cx);
    });
    cx.on_action(|_: &ItalicSelection, cx| {
        dispatch_menu_action(&ItalicSelection, cx);
    });
    cx.on_action(|_: &StrikethroughSelection, cx| {
        dispatch_menu_action(&StrikethroughSelection, cx);
    });
    cx.on_action(|_: &UnderlineSelection, cx| {
        dispatch_menu_action(&UnderlineSelection, cx);
    });
    cx.on_action(|_: &CodeSelection, cx| {
        dispatch_menu_action(&CodeSelection, cx);
    });
    cx.on_action(|_: &LinkSelection, cx| {
        dispatch_menu_action(&LinkSelection, cx);
    });
    cx.on_action(|_: &SetHeading1, cx| dispatch_menu_action(&SetHeading1, cx));
    cx.on_action(|_: &SetHeading2, cx| dispatch_menu_action(&SetHeading2, cx));
    cx.on_action(|_: &SetHeading3, cx| dispatch_menu_action(&SetHeading3, cx));
    cx.on_action(|_: &SetParagraph, cx| dispatch_menu_action(&SetParagraph, cx));
    cx.on_action(|_: &SetBulletedList, cx| dispatch_menu_action(&SetBulletedList, cx));
    cx.on_action(|_: &SetNumberedList, cx| dispatch_menu_action(&SetNumberedList, cx));
    cx.on_action(|_: &SetTaskList, cx| dispatch_menu_action(&SetTaskList, cx));
    cx.on_action(|_: &SetQuote, cx| dispatch_menu_action(&SetQuote, cx));
    cx.on_action(|_: &SetCodeBlock, cx| dispatch_menu_action(&SetCodeBlock, cx));
    cx.on_action(|_: &QuitApplication, cx| {
        dispatch_menu_action(&QuitApplication, cx);
    });
    cx.on_action(|_: &CloseWindow, cx| {
        dispatch_menu_action(&CloseWindow, cx);
    });
    cx.on_action(|_: &CloseTab, cx| {
        dispatch_menu_action(&CloseTab, cx);
    });
    cx.on_action(|_: &ReopenClosedTab, cx| {
        dispatch_menu_action(&ReopenClosedTab, cx);
    });

    install_menus(cx);
    cx.activate(true);
}
