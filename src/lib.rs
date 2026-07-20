// @author kongweiguang

//! gmark - a block-based Markdown editor built with GPUI.
//!
//! Reads file paths from command-line arguments and opens one GPUI window per
//! file. With no arguments, a single empty window is created.

use std::borrow::Cow;
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[cfg(any(target_os = "macos", target_os = "windows"))]
use futures::StreamExt;
#[cfg(target_os = "macos")]
use futures::channel::mpsc;
use gpui::*;

mod accessibility;
mod app_identity;
mod app_menu;
mod cli;
mod components;
mod config;
mod crash_report;
mod document_io;
mod editor;
mod export;
#[cfg(any(target_os = "macos", test))]
mod file_url;
mod i18n;
mod large_file;
mod net;
mod perf;
mod preferences;
mod recovery;
#[cfg(target_os = "windows")]
mod single_instance;
mod spellcheck;
mod theme;
mod ui;
mod window_chrome;

use app_menu::{
    init as init_app_menu, open_editor_window, open_large_recovery_window,
    open_recovered_editor_tabs_window, open_workspace_session_window,
};
use components::init_with_keybindings as init_editor;
#[cfg(target_os = "macos")]
use file_url::parse_file_url;
use i18n::I18nManager;
use theme::ThemeManager;

/// 每个编辑器窗口监听自身外观；只有 `system` 模式会真正更新全局主题。
fn install_system_theme_observer(cx: &mut App) {
    cx.observe_new::<editor::Editor>(|_, window, cx| {
        let Some(window) = window else {
            return;
        };
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

struct GmarkAssets;

#[cfg(target_os = "windows")]
struct SingleInstanceState {
    _guard: single_instance::InstanceGuard,
}

#[cfg(target_os = "windows")]
impl Global for SingleInstanceState {}

fn absolute_input_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(&path))
        .unwrap_or(path)
}

#[cfg(target_os = "windows")]
fn handle_instance_message(cx: &mut App, message: single_instance::InstanceMessage) {
    if message.paths.is_empty() {
        let window = cx.active_window().or_else(|| cx.windows().last().copied());
        if let Some(window) = window {
            let _ = window.update(cx, |_view, window, _cx| window.activate_window());
        } else {
            open_editor_window(cx, String::new(), None);
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

    open_editor_window(cx, String::new(), None);
}

impl AssetSource for GmarkAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        match path {
            "icon/gmark-icon.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/gmark-icon.svg"
            )))),
            "icon/workspace/folder.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/workspace/folder.svg"
            )))),
            "icon/workspace/markdown.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/workspace/markdown.svg"
            )))),
            "icon/titlebar/chrome-close.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/titlebar/chrome-close.svg"
            )))),
            "icon/editor/tab-pin.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/editor/tab-pin.svg"
            )))),
            "icon/ui/files.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/files.svg"
            )))),
            "icon/ui/file.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/file.svg"
            )))),
            "icon/ui/outline.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/outline.svg"
            )))),
            "icon/ui/search.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/search.svg"
            )))),
            "icon/ui/panel-left.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/panel-left.svg"
            )))),
            "icon/ui/live.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/live.svg"
            )))),
            "icon/ui/source.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/source.svg"
            )))),
            "icon/ui/split.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/split.svg"
            )))),
            "icon/ui/preview.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/preview.svg"
            )))),
            "icon/ui/close.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/close.svg"
            )))),
            "icon/ui/chevron-right.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/chevron-right.svg"
            )))),
            "icon/ui/chevron-down.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/chevron-down.svg"
            )))),
            "icon/ui/more-horizontal.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/more-horizontal.svg"
            )))),
            "icon/ui/case-sensitive.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/case-sensitive.svg"
            )))),
            "icon/ui/whole-word.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/whole-word.svg"
            )))),
            "icon/ui/regex.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/regex.svg"
            )))),
            "icon/ui/chevron-up.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/chevron-up.svg"
            )))),
            "icon/ui/copy.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/copy.svg"
            )))),
            "icon/ui/check.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/check.svg"
            )))),
            "icon/ui/code.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/code.svg"
            )))),
            "icon/ui/link.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/link.svg"
            )))),
            "icon/ui/palette.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/palette.svg"
            )))),
            "icon/ui/image.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/image.svg"
            )))),
            "icon/ui/keyboard.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/keyboard.svg"
            )))),
            "icon/ui/panel-bottom.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/panel-bottom.svg"
            )))),
            "icon/ui/plus.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/plus.svg"
            )))),
            "icon/ui/minus.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/minus.svg"
            )))),
            "icon/ui/type.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/type.svg"
            )))),
            "icon/ui/sun.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/sun.svg"
            )))),
            "icon/ui/moon.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/moon.svg"
            )))),
            "icon/ui/monitor.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/monitor.svg"
            )))),
            "icon/ui/save.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/save.svg"
            )))),
            "icon/ui/sliders.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/sliders.svg"
            )))),
            "icon/ui/undo.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/undo.svg"
            )))),
            "icon/ui/redo.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/redo.svg"
            )))),
            "icon/ui/scissors.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/scissors.svg"
            )))),
            "icon/ui/clipboard.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/clipboard.svg"
            )))),
            "icon/ui/power.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/power.svg"
            )))),
            "icon/ui/file-output.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/file-output.svg"
            )))),
            "icon/ui/refresh.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/refresh.svg"
            )))),
            "icon/ui/shield.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/shield.svg"
            )))),
            "icon/ui/info.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/info.svg"
            )))),
            "icon/ui/lightbulb.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/lightbulb.svg"
            )))),
            "icon/ui/triangle-alert.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/triangle-alert.svg"
            )))),
            "icon/ui/shield-alert.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/shield-alert.svg"
            )))),
            "icon/ui/heading-1.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/heading-1.svg"
            )))),
            "icon/ui/heading-2.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/heading-2.svg"
            )))),
            "icon/ui/heading-3.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/heading-3.svg"
            )))),
            "icon/ui/list.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/list.svg"
            )))),
            "icon/ui/list-ordered.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/list-ordered.svg"
            )))),
            "icon/ui/list-checks.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/list-checks.svg"
            )))),
            "icon/ui/quote.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/quote.svg"
            )))),
            "icon/ui/sigma.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/sigma.svg"
            )))),
            "icon/ui/corner-up-left.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/corner-up-left.svg"
            )))),
            "icon/ui/align-left.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/align-left.svg"
            )))),
            "icon/ui/align-center.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/align-center.svg"
            )))),
            "icon/ui/align-right.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/align-right.svg"
            )))),
            "icon/ui/arrow-left.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/arrow-left.svg"
            )))),
            "icon/ui/arrow-right.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/arrow-right.svg"
            )))),
            "icon/ui/arrow-up.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/arrow-up.svg"
            )))),
            "icon/ui/arrow-down.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/arrow-down.svg"
            )))),
            "icon/ui/trash.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/trash.svg"
            )))),
            "icon/ui/table.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/ui/table.svg"
            )))),
            "icon/titlebar/chrome-minimize.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/titlebar/chrome-minimize.svg"
            )))),
            "icon/titlebar/chrome-maximize.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/titlebar/chrome-maximize.svg"
            )))),
            "icon/titlebar/chrome-restore.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icon/titlebar/chrome-restore.svg"
            )))),
            _ => Ok(None),
        }
    }

    fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

/// 启动 gmark 桌面应用。
///
/// 该门面保持二进制入口稳定；启动顺序、平台生命周期与窗口恢复仍由内部模块负责。
pub fn run() -> anyhow::Result<()> {
    run_app();
    Ok(())
}

fn run_app() {
    if let Err(error) = crash_report::install() {
        eprintln!("failed to initialize local crash reporting: {error:#}");
    }
    let args: Vec<String> = std::env::args().collect();
    let (detach, input_paths) = match cli::parse(&args[1..]) {
        cli::CliCommand::Run {
            detach,
            input_paths,
        } => (detach, input_paths),
        cli::CliCommand::Version => {
            println!("gmark {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        cli::CliCommand::Help => {
            println!("{}", cli::help_text(env!("CARGO_PKG_VERSION")));
            return;
        }
        cli::CliCommand::UnknownOption(option) => {
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
        ThemeManager::init_with_theme_id(cx, &preferences.default_theme_id);
        config::EditorSettings::init_with_typography(
            cx,
            preferences.show_table_headers,
            preferences.auto_save,
            preferences.spell_check,
            preferences.editor_font_size,
            preferences.editor_line_height_percent,
            preferences.editor_content_width,
            &preferences.editor_font_family,
            preferences.workspace_sidebar_position,
            preferences.show_tab_bar_actions,
        );
        net::install_http_client(cx);
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

        let recovery_dir = config::GmarkConfigDirs::from_system()
            .map(|dirs| dirs.recovery_dir())
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
            match gmark_large_document::list_large_recovery_journals(recovery_dir) {
                Ok(journals) => {
                    for journal in journals {
                        match gmark_large_document::large_recovery_has_edits(&journal) {
                            Ok(false) => {
                                if let Err(error) = std::fs::remove_file(&journal) {
                                    eprintln!(
                                        "failed to remove empty large recovery '{}': {error}",
                                        journal.display()
                                    );
                                }
                            }
                            Ok(true) => match open_large_recovery_window(cx, journal.clone()) {
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
            if !opened_recovery {
                open_startup_window(cx, preferences.startup_open);
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
            open_editor_window(cx, String::new(), None);
        }
        app_menu::install_menus(cx);
        cx.refresh_windows();
    });
}

#[cfg(test)]
#[path = "../tests/unit/app_assets.rs"]
mod asset_tests;
