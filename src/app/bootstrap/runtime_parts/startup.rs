// @author kongweiguang

//! Startup lifecycle and platform-specific launch routing.

use super::*;

/// 每个编辑器窗口监听全局更新状态和自身外观，避免后台进度变化停留在旧画面。
fn install_system_theme_observer(cx: &mut App) {
    cx.observe_new::<editor::Editor>(|_, window, cx| {
        let update_service = updater::UpdateCoordinator::entity(cx);
        cx.observe(&update_service, |_, _, cx| cx.notify()).detach();
        let Some(window) = window else {
            return;
        };
        cx.update_global::<VisualPreferencesManager, _>(|manager, _cx| {
            manager.refresh_system();
        });
        cx.observe_window_appearance(window, |_, window, cx| {
            let changed = cx.update_global::<ThemeManager, _>(|manager, _cx| {
                manager.update_system_appearance(window.appearance())
            });
            if changed {
                app_menu::install_menus(cx);
                cx.refresh_windows();
            }
        })
        .detach();
    })
    .detach();
}

#[cfg(target_os = "windows")]
struct SingleInstanceState {
    _guard: single_instance::InstanceGuard,
}

#[cfg(target_os = "windows")]
impl Global for SingleInstanceState {}

// 原因：统一把 CLI 相对路径转换为进程工作目录下的绝对路径，避免平台窗口打开分支各自解释路径。
fn absolute_input_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(&path))
        .unwrap_or(path)
}

#[cfg(target_os = "windows")]
// 原因：单实例转发必须复用当前窗口打开入口，才能保持第二次启动与首次启动一致的编辑语义。
fn handle_instance_message(cx: &mut App, message: single_instance::InstanceMessage) {
    if message.paths.is_empty() {
        let window = cx.active_window().or_else(|| cx.windows().last().copied());
        if let Some(window) = window {
            let _ = window.update(cx, |_view, window, _cx| window.activate_window());
        } else if let Err(error) = open_editor_window(cx, String::new(), None) {
            eprintln!("failed to open editor window: {error}");
        }
        return;
    }
    for path in message.paths {
        if let Err(error) = app_menu::open_file_in_new_window(cx, &path) {
            eprintln!(
                "failed to open forwarded file '{}': {error}",
                path.display()
            );
        }
    }
}

// 原因：启动恢复需要沿用会话、最近文件和空编辑器的既有优先级，保证无输入时始终有可用窗口。
fn open_startup_window(cx: &mut App, startup_open: config::StartupOpenPreference) {
    if startup_open == config::StartupOpenPreference::LastOpenedFile {
        match config::workspace_session::read_workspace_sessions() {
            Ok(sessions) => {
                let mut opened = false;
                for session in sessions {
                    opened |= open_workspace_session_window(cx, session);
                }
                if opened {
                    return;
                }
            }
            Err(error) => eprintln!("failed to restore workspace session: {error}"),
        }
    }
    if startup_open == config::StartupOpenPreference::LastOpenedFile
        && let Some(path) = config::first_existing_recent_markdown_file()
    {
        if let Err(err) = app_menu::open_file_in_new_window(cx, &path) {
            eprintln!(
                "failed to read last opened file '{}': {err}",
                path.display()
            );
        } else {
            return;
        }
    }

    if let Err(error) = open_editor_window(cx, String::new(), None) {
        eprintln!("failed to open editor window: {error}");
    }
}

/// 只有启动确认已经安全落盘后才绑定终态监听，避免无效 capability 让新进程永久轮询不存在的事务结果。
fn acknowledge_relaunched_update(
    acknowledgement: &Path,
    capability: Option<&str>,
) -> Option<PathBuf> {
    let updates_root = match crate::updater::update_cache_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("failed to resolve update cache root for startup acknowledgement: {error:#}");
            return None;
        }
    };
    if let Err(error) = write_update_acknowledgement(
        acknowledgement,
        &updates_root,
        capability,
        env!("CARGO_PKG_VERSION"),
    ) {
        eprintln!("failed to acknowledge applied update: {error}");
        return None;
    }
    acknowledgement.parent().map(Path::to_path_buf)
}

// 原因：集中保留平台生命周期编排，拆分模块只改变代码归属而不改变初始化顺序。
pub(crate) fn run_app() {
    if let Err(error) = crash_report::install() {
        eprintln!("failed to initialize local crash reporting: {error:#}");
    }
    let mut args: Vec<String> = std::env::args().collect();
    let update_acknowledgement = take_update_acknowledgement(&mut args);
    let update_acknowledgement_capability = std::env::var_os(UPDATE_ACK_CAPABILITY_ENV)
        .map(|capability| capability.into_string().unwrap_or_default());
    let (detach, input_paths) = match crate::cli::parse(&args[1..]) {
        crate::cli::CliCommand::Run {
            detach,
            input_paths,
        } => (detach, input_paths),
        crate::cli::CliCommand::Version => {
            println!("Gmark {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        crate::cli::CliCommand::Help => {
            println!("{}", crate::cli::help_text(env!("CARGO_PKG_VERSION")));
            return;
        }
        crate::cli::CliCommand::UnknownOption(option) => {
            eprintln!("Unknown option: {option}");
            std::process::exit(1);
        }
    };
    let input_paths = input_paths
        .into_iter()
        .map(absolute_input_path)
        .collect::<Vec<_>>();

    #[cfg(target_os = "windows")]
    let (single_instance_guard, mut single_instance_rx) =
        match single_instance::acquire(&input_paths) {
            Ok(single_instance::InstanceLaunch::Primary { guard, receiver }) => (guard, receiver),
            Ok(single_instance::InstanceLaunch::Forwarded) => return,
            Err(error) => {
                eprintln!("failed to establish gmark single-instance IPC: {error:#}");
                std::process::exit(1);
            }
        };

    #[cfg(not(target_os = "macos"))]
    let _ = detach;

    // On macOS, detach from terminal if requested
    // TODO: Other platforms may also need to be adapted
    #[cfg(target_os = "macos")]
    if detach {
        use std::process::Command;

        // Re-launch the application in the background without the --detach flag
        let exe_path = std::env::current_exe().expect("Failed to get executable path");
        let non_detach_args: Vec<String> = args
            .iter()
            .filter(|arg| *arg != "--detach" && *arg != "-d")
            .cloned()
            .collect();

        Command::new(exe_path)
            .args(&non_detach_args[1..])
            .spawn()
            .expect("Failed to detach process");

        return;
    }

    #[cfg(target_os = "macos")]
    let (open_file_tx, mut open_file_rx) = mpsc::unbounded::<PathBuf>();
    #[cfg(target_os = "macos")]
    let open_file_requested = Arc::new(AtomicBool::new(false));

    let app = Application::new().with_assets(GmarkAssets);

    #[cfg(target_os = "macos")]
    {
        let open_file_requested_for_callback = open_file_requested.clone();
        app.on_open_urls(move |urls| {
            for url in urls {
                let Some(path) = parse_file_url(&url) else {
                    continue;
                };
                open_file_requested_for_callback.store(true, Ordering::SeqCst);
                let _ = open_file_tx.unbounded_send(path);
            }
        });
    }

    app.run(move |cx: &mut App| {
        #[cfg(target_os = "windows")]
        cx.set_global(SingleInstanceState {
            _guard: single_instance_guard,
        });
        let preferences = config::load_or_create_app_preferences().unwrap_or_else(|err| {
            eprintln!("failed to initialize app preferences: {err}");
            Default::default()
        });
        I18nManager::init_with_language_id(cx, &preferences.default_language_id);
        ThemeManager::init_with_preference(
            cx,
            preferences.theme_appearance,
            preferences.theme_palette,
        );
        // Render-only Markdown state is process-scoped: duplicate windows
        // share the latest fold/column snapshot and one bounded asset budget,
        // while the GPUI globals are dropped wholesale on application exit.
        cx.set_global(crate::editor::markdown_view_state::SharedMarkdownViewState::default());
        cx.set_global(crate::editor::render_asset_manager::SharedRenderAssetManager::default());
        VisualPreferencesManager::init(cx, preferences.visual_accessibility);
        config::EditorSettings::init_with_typography(
            cx,
            preferences.show_table_headers,
            preferences.auto_save,
            preferences.spell_check,
            preferences.editor_font_size,
            preferences.editor_line_height_percent,
            preferences.editor_content_width,
            &preferences.editor_font_family,
            preferences.show_tab_bar_actions,
        );
        DocumentService::init(cx);
        net::install_http_client(cx);
        let relaunched_update_transaction = update_acknowledgement.as_deref().and_then(|path| {
            acknowledge_relaunched_update(path, update_acknowledgement_capability.as_deref())
        });
        updater::UpdateCoordinator::init(
            preferences.auto_check_updates,
            relaunched_update_transaction,
            cx,
        );
        init_editor(cx, &preferences.keybindings);
        init_app_menu(cx);
        install_system_theme_observer(cx);

        #[cfg(target_os = "windows")]
        cx.spawn(async move |cx| {
            while let Some(message) = single_instance_rx.next().await {
                let _ = cx.update(move |cx| handle_instance_message(cx, message));
            }
        })
        .detach();

        let recovery_dir = config::AppDirs::from_system()
            .and_then(|dirs| {
                dirs.validate_state_root()?;
                Ok(dirs.recovery_dir())
            })
            .map_err(|error| eprintln!("recovery state unavailable: {error:#}"))
            .ok();
        let recovered_documents = recovery_dir
            .as_deref()
            .map(recovery::load_recovery_documents)
            .transpose()
            .unwrap_or_else(|error| {
                eprintln!("failed to scan recovery sessions: {error}");
                Some(Vec::new())
            })
            .unwrap_or_default();
        let mut opened_recovery = !recovered_documents.is_empty();
        let mut recovered_paths = recovered_documents
            .iter()
            .filter_map(|document| document.file_path.clone())
            .collect::<Vec<_>>();
        if let Some(recovery_dir) = &recovery_dir {
            match gmark_paged_document::list_paged_recovery_journals(recovery_dir) {
                Ok(journals) => {
                    for journal in journals {
                        match gmark_paged_document::paged_recovery_has_edits(&journal) {
                            Ok(false) => {
                                if let Err(error) = std::fs::remove_file(&journal) {
                                    eprintln!(
                                        "failed to remove empty large recovery '{}': {error}",
                                        journal.display()
                                    );
                                }
                            }
                            Ok(true) => match open_paged_recovery_window(cx, journal.clone()) {
                                Ok((_window, path)) => {
                                    opened_recovery = true;
                                    recovered_paths.push(path);
                                }
                                Err(error) => eprintln!(
                                    "failed to open large recovery '{}': {error}",
                                    journal.display()
                                ),
                            },
                            Err(error) => eprintln!(
                                "failed to inspect large recovery '{}': {error}",
                                journal.display()
                            ),
                        }
                    }
                }
                Err(error) => eprintln!("failed to scan large recovery sessions: {error}"),
            }
        }
        if let Err(error) =
            config::workspace_session::remove_paths_from_workspace_sessions(&recovered_paths)
        {
            eprintln!("failed to detach recovery paths from workspace sessions: {error}");
        }
        open_recovered_editor_tabs_window(cx, recovered_documents);
        if opened_recovery
            && input_paths.is_empty()
            && preferences.startup_open == config::StartupOpenPreference::LastOpenedFile
        {
            match config::workspace_session::read_workspace_sessions() {
                Ok(sessions) => {
                    for session in sessions {
                        if let Some(session) = session.without_paths(&recovered_paths) {
                            open_workspace_session_window(cx, session);
                        }
                    }
                }
                Err(error) => eprintln!("failed to restore clean workspace sessions: {error}"),
            }
        }

        #[cfg(target_os = "macos")]
        cx.spawn(async move |cx| {
            while let Some(path) = open_file_rx.next().await {
                let _ = cx.update(move |cx| {
                    if let Err(err) = app_menu::open_file_in_new_window(cx, &path) {
                        eprintln!("failed to open '{}': {err}", path.display());
                    }
                });
            }
        })
        .detach();

        if input_paths.is_empty() {
            #[cfg(target_os = "macos")]
            {
                let startup_open = preferences.startup_open;
                let open_file_requested = open_file_requested.clone();
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(150))
                        .await;
                    if !opened_recovery && !open_file_requested.load(Ordering::SeqCst) {
                        let _ = cx.update(move |cx| open_startup_window(cx, startup_open));
                    }
                })
                .detach();
            }

            #[cfg(not(target_os = "macos"))]
            {
                #[cfg(feature = "updater-e2e")]
                let e2e_failure_visible = std::env::var_os("GMARK_UPDATER_E2E_FAILURE").is_some();
                #[cfg(not(feature = "updater-e2e"))]
                let e2e_failure_visible = false;
                if !opened_recovery || e2e_failure_visible {
                    open_startup_window(cx, preferences.startup_open);
                }
            }

            return;
        }

        let mut opened_input = false;
        for path in &input_paths {
            match app_menu::open_file_in_new_window(cx, path) {
                Ok(()) => opened_input = true,
                Err(err) => eprintln!(
                    "failed to read '{}': {err}; file was not opened.",
                    path.display()
                ),
            }
        }
        if !opened_input && !opened_recovery {
            if let Err(error) = open_editor_window(cx, String::new(), None) {
                eprintln!("failed to open editor window: {error}");
            }
        }
        app_menu::install_menus(cx);
        cx.refresh_windows();
    });
}
