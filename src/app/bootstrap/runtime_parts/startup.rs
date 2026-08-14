// @author kongweiguang

//! Startup lifecycle and platform-specific launch routing.

use super::*;

use std::time::Duration;

use futures::future::{Either, select};
use gpui::{AppContext as _, WindowHandle};

const STARTUP_RESTORE_DEADLINE: Duration = Duration::from_secs(30);

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

struct PreparedStartupState {
    recovered: Vec<app_menu::PreparedRecoveredDocument>,
    paged_recovery: Vec<app_menu::PreparedPagedRecovery>,
    sessions: Vec<config::workspace_session::WorkspaceSession>,
    recent_file: Option<PathBuf>,
}

/// 读取启动所需的历史路径时复用配置层的固定上限，避免损坏的历史文件把恢复
/// worker 变成无界内存读取；历史文件本身仍由普通文件打开任务负责真正的 probe/read。
// 原因：启动状态属于低信任持久化输入，大小边界必须由唯一的历史适配器统一执行。
fn first_existing_recent_markdown_file_bounded() -> Option<PathBuf> {
    config::read_recent_files()
        .ok()?
        .into_iter()
        .find(|path| path.is_file())
}

/// 在 GPUI 建立前读取并归一化偏好，保持首个 UI 回调只做内存初始化和窗口工作。
// 原因：偏好加载包含同步读写和兼容迁移，不能让配置盘延迟占用 GPUI 主线程。
fn load_preferences_before_gpui() -> crate::preferences::AppPreferences {
    match std::thread::spawn(config::load_or_create_app_preferences).join() {
        Ok(Ok(preferences)) => preferences,
        Ok(Err(error)) => {
            eprintln!("failed to initialize local preferences: {error}");
            Default::default()
        }
        Err(_) => {
            eprintln!("preference loader thread terminated unexpectedly");
            Default::default()
        }
    }
}

/// 在后台完成 recovery journal、恢复文本、工作区会话和最近文件的读取，
/// 让 GPUI 线程只接收已经准备好的 lease；失败分支保留可继续操作的空窗口。
// 原因：这些输入可能位于 UNC 或慢盘，任何一个同步读取都不应阻塞应用首帧。
fn prepare_startup_state(
    service: DocumentService,
    startup_open: config::StartupOpenPreference,
    restore_sessions: bool,
) -> PreparedStartupState {
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
    let recovered_paths = recovered_documents
        .iter()
        .filter_map(|document| document.file_path.clone())
        .collect::<Vec<_>>();
    let mut recovered = Vec::new();
    for document in recovered_documents {
        match prepare_recovered_document(service.clone(), document) {
            Ok(document) => recovered.push(document),
            Err(error) => eprintln!("failed to prepare recovery document: {error:#}"),
        }
    }

    let mut paged_recovery = Vec::new();
    let mut recovered_paths = recovered_paths;
    if let Some(recovery_dir) = &recovery_dir {
        match gmark_paged_document::list_paged_recovery_journals(recovery_dir) {
            Ok(journals) => {
                for journal in journals {
                    match gmark_paged_document::paged_recovery_has_edits(&journal) {
                        Ok(false) => {
                            if let Err(error) = fs::remove_file(&journal) {
                                eprintln!(
                                    "failed to remove empty large recovery '{}': {error}",
                                    journal.display()
                                );
                            }
                        }
                        Ok(true) => match prepare_paged_recovery(journal.clone()) {
                            Ok(prepared) => {
                                recovered_paths.push(prepared.path.clone());
                                paged_recovery.push(prepared);
                            }
                            Err(error) => eprintln!(
                                "failed to prepare large recovery '{}': {error}",
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
    let sessions =
        if restore_sessions && startup_open == config::StartupOpenPreference::LastOpenedFile {
            match config::workspace_session::read_workspace_sessions() {
                Ok(sessions) => sessions
                    .into_iter()
                    .filter_map(|session| session.without_paths(&recovered_paths))
                    .collect(),
                Err(error) => {
                    eprintln!("failed to restore clean workspace sessions: {error}");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
    let recent_file = if restore_sessions
        && startup_open == config::StartupOpenPreference::LastOpenedFile
        && recovered.is_empty()
        && paged_recovery.is_empty()
        && sessions.is_empty()
    {
        first_existing_recent_markdown_file_bounded()
    } else {
        None
    };
    PreparedStartupState {
        recovered,
        paged_recovery,
        sessions,
        recent_file,
    }
}

/// 只在 GPUI 线程安装后台准备结果，并先安排 session/recent 的增量窗口。
// 原因：恢复过程可迟到或超时，安装阶段必须是代次任务的唯一回写点，不能等待 worker。
fn install_prepared_startup_state(
    cx: &mut App,
    prepared: PreparedStartupState,
    startup_open: config::StartupOpenPreference,
    restore_sessions: bool,
    first_frame: Option<WindowHandle<editor::Editor>>,
) {
    let mut first_frame = first_frame;
    let mut opened_recovery = false;
    for recovery in prepared.paged_recovery {
        match open_prepared_paged_recovery_window(cx, recovery) {
            Ok((_window, _path)) => opened_recovery = true,
            Err(error) => eprintln!("failed to install large recovery window: {error:#}"),
        }
    }
    if app_menu::open_prepared_recovered_editor_tabs_window(cx, prepared.recovered).is_some() {
        opened_recovery = true;
    }

    if opened_recovery {
        remove_startup_first_frame(cx, first_frame.take());
        return;
    }
    if !restore_sessions || startup_open != config::StartupOpenPreference::LastOpenedFile {
        return;
    }
    let mut opened_session = false;
    for session in prepared.sessions {
        opened_session |= open_workspace_session_window(cx, session);
    }
    if opened_session {
        remove_startup_first_frame(cx, first_frame.take());
        return;
    }
    if let Some(path) = prepared.recent_file {
        if let Err(error) = app_menu::open_file_in_new_window(cx, &path) {
            eprintln!("failed to open recent file '{}': {error}", path.display());
        } else {
            remove_startup_first_frame(cx, first_frame.take());
        }
    }
}

/// 恢复已经创建了替代窗口后才移除占位首帧，失败或超时仍保留可交互窗口。
// 原因：先关首帧再开替代窗口会在慢盘恢复期间造成零窗口或焦点丢失。
fn remove_startup_first_frame(cx: &mut App, first_frame: Option<WindowHandle<editor::Editor>>) {
    if let Some(first_frame) = first_frame {
        let _ = first_frame.update(cx, |_editor, window, _cx| window.remove_window());
    }
}

/// 立即创建首帧，再以后台任务恢复文件、journal 和 workspace session；超过
/// deadline 的代次只留下首帧，不再把迟到 lease 安装到窗口树。
// 原因：用户可先看到可交互窗口，慢盘恢复不会阻塞 GPUI，也不会无限等待 Condvar。
fn schedule_startup_restore(
    cx: &mut App,
    startup_open: config::StartupOpenPreference,
    restore_sessions: bool,
) {
    let first_frame = if restore_sessions {
        match open_editor_window(cx, String::new(), None) {
            Ok(handle) => Some(handle),
            Err(error) => {
                eprintln!("failed to open startup first frame: {error}");
                None
            }
        }
    } else {
        None
    };
    let service = cx
        .try_global::<DocumentService>()
        .cloned()
        .unwrap_or_else(|| {
            let service = DocumentService::new();
            cx.set_global(service.clone());
            service
        });
    cx.spawn(async move |cx| {
        let prepared = match select(
            cx.background_spawn(async move {
                prepare_startup_state(service, startup_open, restore_sessions)
            }),
            cx.background_executor().timer(STARTUP_RESTORE_DEADLINE),
        )
        .await
        {
            Either::Left((prepared, _timer)) => prepared,
            Either::Right((_elapsed, _preparation)) => {
                eprintln!("timed out preparing startup recovery; keeping first frame");
                return;
            }
        };
        let _ = cx.update(move |cx| {
            install_prepared_startup_state(
                cx,
                prepared,
                startup_open,
                restore_sessions,
                first_frame,
            );
        });
    })
    .detach();
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

    let preferences = load_preferences_before_gpui();
    let app = Application::new().with_assets(GmarkAssets);

    #[cfg(target_os = "macos")]
    {
        app.on_open_urls(move |urls| {
            for url in urls {
                let Some(path) = parse_file_url(&url) else {
                    continue;
                };
                let _ = open_file_tx.unbounded_send(path);
            }
        });
    }

    app.run(move |cx: &mut App| {
        #[cfg(target_os = "windows")]
        cx.set_global(SingleInstanceState {
            _guard: single_instance_guard,
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
            while let Some(request) = single_instance_rx.next().await {
                let message = request.message.clone();
                let accepted = cx
                    .update(move |cx| handle_instance_message(cx, message))
                    .is_ok();
                request.respond(accepted);
            }
        })
        .detach();

        schedule_startup_restore(cx, preferences.startup_open, input_paths.is_empty());

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
        if !opened_input {
            if let Err(error) = open_editor_window(cx, String::new(), None) {
                eprintln!("failed to open editor window: {error}");
            }
        }
        app_menu::install_menus(cx);
        cx.refresh_windows();
    });
}
