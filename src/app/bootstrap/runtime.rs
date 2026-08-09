// @author kongweiguang

//! Platform lifecycle, startup restoration, and helper acknowledgement wiring.

use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

#[cfg(target_os = "macos")]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tempfile::NamedTempFile;
use uuid::Uuid;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use futures::StreamExt;
#[cfg(target_os = "macos")]
use futures::channel::mpsc;
#[cfg(target_os = "windows")]
use gpui::Global;
use gpui::{App, Application, BorrowAppContext};

#[cfg(target_os = "macos")]
use crate::file_url::parse_file_url;
#[cfg(target_os = "windows")]
use crate::single_instance;
use crate::{
    app_menu::{
        self, init as init_app_menu, open_editor_window, open_paged_recovery_window,
        open_recovered_editor_tabs_window, open_workspace_session_window,
    },
    components::init_with_keybindings as init_editor,
    config, crash_report, editor,
    i18n::I18nManager,
    net, recovery,
    theme::ThemeManager,
    ui::visual_preferences::VisualPreferencesManager,
    updater,
};

use super::assets::GmarkAssets;

const UPDATE_ACK_CAPABILITY_ENV: &str = "GMARK_UPDATE_ACK_CAPABILITY";
const ACKNOWLEDGEMENT_FILE_NAME: &str = "startup-ack";
const ACK_CAPABILITY_FILE_PREFIX: &str = "startup-ack-capability-";
const MAX_ACK_CAPABILITY_BYTES: usize = 128;

/// 每个编辑器窗口监听自身外观；只有 system 模式会真正更新全局主题。
fn install_system_theme_observer(cx: &mut App) {
    cx.observe_new::<editor::Editor>(|_, window, cx| {
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
            println!("gmark {}", env!("CARGO_PKG_VERSION"));
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
        net::install_http_client(cx);
        updater::UpdateCoordinator::init(preferences.auto_check_updates, cx);
        init_editor(cx, &preferences.keybindings);
        init_app_menu(cx);
        install_system_theme_observer(cx);
        if let Some(path) = update_acknowledgement.as_ref() {
            if let Err(error) = write_update_acknowledgement(
                path,
                &crate::updater::update_cache_root(),
                update_acknowledgement_capability.as_deref(),
                env!("CARGO_PKG_VERSION"),
            ) {
                eprintln!("failed to acknowledge applied update: {error}");
            }
        }

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
            if let Err(error) = open_editor_window(cx, String::new(), None) {
                eprintln!("failed to open editor window: {error}");
            }
        }
        app_menu::install_menus(cx);
        cx.refresh_windows();
    });
}

/// Writes only the acknowledgement tied to the helper's active update transaction.
/// New helpers supply a random capability, which must validate without fallback.
/// Its absence is accepted only for the immediately preceding helper protocol,
/// after the same fixed transaction and apply-plan checks have succeeded.
fn write_update_acknowledgement(
    requested_path: &Path,
    updates_root: &Path,
    capability: Option<&str>,
    current_version: &str,
) -> Result<(), String> {
    let capability = capability
        .map(|capability| {
            Uuid::parse_str(capability)
                .map_err(|_| {
                    "update acknowledgement has an invalid transaction capability".to_owned()
                })
                .map(|capability| capability.hyphenated().to_string())
        })
        .transpose()?;
    let transaction_dir = acknowledgement_transaction_dir(requested_path, updates_root)?;
    let plan = validate_active_acknowledgement_plan(&transaction_dir, current_version)?;
    if matches!(plan, ActiveAcknowledgementPlan::V2 { .. }) && capability.is_none() {
        return Err("update protocol v2 requires an acknowledgement capability".to_owned());
    }
    if let Some(capability) = capability.as_deref() {
        validate_acknowledgement_capability(&transaction_dir, capability, plan.transaction_id())?;
    }
    write_acknowledgement_exclusive(&transaction_dir, current_version)
}

fn acknowledgement_transaction_dir(
    requested_path: &Path,
    updates_root: &Path,
) -> Result<PathBuf, String> {
    if requested_path.file_name().and_then(|name| name.to_str()) != Some(ACKNOWLEDGEMENT_FILE_NAME)
    {
        return Err("update acknowledgement path has an invalid file name".to_owned());
    }
    let requested_parent = requested_path
        .parent()
        .ok_or_else(|| "update acknowledgement path has no transaction directory".to_owned())?;
    let canonical_root = fs::canonicalize(updates_root)
        .map_err(|error| format!("failed to resolve update cache root: {error}"))?;
    let root_metadata = fs::symlink_metadata(&canonical_root)
        .map_err(|error| format!("failed to inspect update cache root: {error}"))?;
    if !root_metadata.file_type().is_dir() || root_metadata.file_type().is_symlink() {
        return Err("update cache root is not a real directory".to_owned());
    }
    let transaction_dir = fs::canonicalize(requested_parent).map_err(|error| {
        format!("failed to resolve update acknowledgement transaction: {error}")
    })?;
    let transaction_metadata = fs::symlink_metadata(&transaction_dir).map_err(|error| {
        format!("failed to inspect update acknowledgement transaction: {error}")
    })?;
    if !transaction_metadata.file_type().is_dir() || transaction_metadata.file_type().is_symlink() {
        return Err("update acknowledgement transaction is not a real directory".to_owned());
    }
    let version_dir = if transaction_dir.parent() == Some(canonical_root.as_path()) {
        transaction_dir.as_path()
    } else {
        let transactions_dir = transaction_dir.parent().ok_or_else(|| {
            "update acknowledgement transaction has no transactions root".to_owned()
        })?;
        let version_dir = transactions_dir
            .parent()
            .ok_or_else(|| "update acknowledgement transaction has no version root".to_owned())?;
        if transactions_dir.file_name().and_then(|name| name.to_str())
            != Some(gmark_update_core::ApplyPlanV2::TRANSACTIONS_DIR_NAME)
            || version_dir.parent() != Some(canonical_root.as_path())
            || transaction_dir
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| Uuid::parse_str(name).ok())
                .is_none()
        {
            return Err(
                "update acknowledgement is outside the active update cache root".to_owned(),
            );
        }
        version_dir
    };
    let version = version_dir
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix('v'))
        .ok_or_else(|| "update acknowledgement transaction has an invalid name".to_owned())?;
    semver::Version::parse(version)
        .map_err(|_| "update acknowledgement transaction has an invalid version".to_owned())?;
    Ok(transaction_dir)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveAcknowledgementPlan {
    V1,
    V2 { transaction_id: Uuid },
}

impl ActiveAcknowledgementPlan {
    fn transaction_id(self) -> Option<Uuid> {
        match self {
            Self::V1 => None,
            Self::V2 { transaction_id } => Some(transaction_id),
        }
    }
}

fn validate_active_acknowledgement_plan(
    transaction_dir: &Path,
    current_version: &str,
) -> Result<ActiveAcknowledgementPlan, String> {
    let plan_path = transaction_dir.join("apply-plan.json");
    let plan_metadata = fs::symlink_metadata(&plan_path)
        .map_err(|error| format!("failed to inspect update acknowledgement plan: {error}"))?;
    if !plan_metadata.file_type().is_file() || plan_metadata.file_type().is_symlink() {
        return Err("update acknowledgement plan is not a regular file".to_owned());
    }
    if plan_metadata.len() > gmark_update_core::MAX_APPLY_PLAN_BYTES {
        return Err("update acknowledgement plan exceeds its size limit".to_owned());
    }
    let plan_bytes = fs::read(&plan_path)
        .map_err(|error| format!("failed to inspect update acknowledgement plan: {error}"))?;
    let schema_version = serde_json::from_slice::<serde_json::Value>(&plan_bytes)
        .ok()
        .and_then(|value| {
            value
                .get("schema_version")
                .and_then(serde_json::Value::as_u64)
        });
    if schema_version == Some(u64::from(gmark_update_core::ApplyPlanV2::SCHEMA_VERSION)) {
        let plan = gmark_update_core::read_apply_plan_v2(&plan_path)
            .map_err(|error| format!("failed to read update acknowledgement plan v2: {error}"))?;
        let declared_plan_path = plan
            .transaction_dir()
            .ok_or_else(|| {
                "update acknowledgement plan v2 has no transaction directory".to_owned()
            })?
            .join(gmark_update_core::ApplyPlanV2::PLAN_FILE_NAME);
        gmark_update_core::validate_apply_plan_v2_at_path(
            &plan,
            &declared_plan_path,
            &gmark_update_core::Platform::current(),
        )
        .map_err(|error| format!("failed to validate update acknowledgement plan v2: {error}"))?;
        // Windows canonicalization adds a verbatim path prefix. Validate the
        // plan's lexical fixed layout first, then bind that exact file back to
        // the canonical transaction opened above instead of comparing unlike
        // path spellings.
        let canonical_declared_plan = fs::canonicalize(&declared_plan_path).map_err(|error| {
            format!("failed to resolve declared update acknowledgement plan v2: {error}")
        })?;
        if canonical_declared_plan != plan_path {
            return Err(
                "update acknowledgement plan v2 does not resolve to the active transaction"
                    .to_owned(),
            );
        }
        let plan_transaction = plan
            .transaction_dir()
            .and_then(|path| fs::canonicalize(path).ok());
        if plan.target_version != current_version
            || plan_transaction.as_deref() != Some(transaction_dir)
            || !plan_path_in_transaction(
                &plan.acknowledgement_path,
                transaction_dir,
                ACKNOWLEDGEMENT_FILE_NAME,
            )
        {
            return Err(
                "update acknowledgement is not bound to the active v2 transaction".to_owned(),
            );
        }
        return Ok(ActiveAcknowledgementPlan::V2 {
            transaction_id: plan.transaction_id,
        });
    }

    let plan = gmark_update_core::read_apply_plan(&plan_path)
        .map_err(|error| format!("failed to read update acknowledgement plan: {error}"))?;
    if plan.target_version != current_version
        || !plan_path_in_transaction(&plan.artifact_path, transaction_dir, "artifact.ready")
        || !plan_path_in_transaction(
            &plan.signed_envelope_path,
            transaction_dir,
            "manifest.envelope.json",
        )
        || !plan_path_in_transaction(
            &plan.acknowledgement_path,
            transaction_dir,
            ACKNOWLEDGEMENT_FILE_NAME,
        )
        || !plan_path_in_transaction(&plan.cancellation_path, transaction_dir, "cancel-install")
    {
        return Err("update acknowledgement is not bound to the active transaction".to_owned());
    }
    Ok(ActiveAcknowledgementPlan::V1)
}

fn plan_path_in_transaction(path: &Path, transaction_dir: &Path, expected_name: &str) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(expected_name)
        && path
            .parent()
            .and_then(|parent| fs::canonicalize(parent).ok())
            .as_deref()
            == Some(transaction_dir)
}

fn validate_acknowledgement_capability(
    transaction_dir: &Path,
    capability: &str,
    transaction_id: Option<Uuid>,
) -> Result<(), String> {
    let path = transaction_dir.join(format!("{ACK_CAPABILITY_FILE_PREFIX}{capability}"));
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("failed to inspect update acknowledgement capability: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("update acknowledgement capability is not a regular file".to_owned());
    }
    let mut file = File::open(&path)
        .map_err(|error| format!("failed to read update acknowledgement capability: {error}"))?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(MAX_ACK_CAPABILITY_BYTES.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read update acknowledgement capability: {error}"))?;
    let expected = transaction_id.map_or_else(
        || format!("{capability}\n"),
        |transaction_id| format!("{}:{capability}\n", transaction_id.hyphenated()),
    );
    if bytes != expected.as_bytes() {
        let transaction = transaction_id
            .map(|value| value.hyphenated().to_string())
            .unwrap_or_else(|| "legacy".to_owned());
        return Err(format!(
            "update acknowledgement capability did not match transaction {transaction}"
        ));
    }
    Ok(())
}

fn write_acknowledgement_exclusive(
    transaction_dir: &Path,
    current_version: &str,
) -> Result<(), String> {
    let acknowledgement_path = transaction_dir.join(ACKNOWLEDGEMENT_FILE_NAME);
    match fs::symlink_metadata(&acknowledgement_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("update acknowledgement target is a symbolic link".to_owned());
        }
        Ok(_) => return Err("update acknowledgement already exists".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to inspect update acknowledgement target: {error}"
            ));
        }
    }
    let mut temporary = NamedTempFile::new_in(transaction_dir)
        .map_err(|error| format!("failed to create update acknowledgement: {error}"))?;
    temporary
        .write_all(
            gmark_update_core::StartupAcknowledgementV1::for_target_version(current_version)
                .marker_bytes()
                .as_slice(),
        )
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("failed to persist update acknowledgement: {error}"))?;
    set_private_acknowledgement_permissions(temporary.as_file())?;
    // persist_noclobber is the atomic no-overwrite commit; an attacker-created
    // final symlink is an existing destination and is never followed or truncated.
    temporary
        .persist_noclobber(&acknowledgement_path)
        .map_err(|error| {
            format!(
                "failed to commit update acknowledgement '{}': {}",
                acknowledgement_path.display(),
                error.error
            )
        })?;
    Ok(())
}

#[cfg(unix)]
fn set_private_acknowledgement_permissions(file: &File) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;

    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to secure update acknowledgement: {error}"))
}

#[cfg(not(unix))]
fn set_private_acknowledgement_permissions(_file: &File) -> Result<(), String> {
    Ok(())
}

/// Helper 参数不属于用户 CLI；必须在普通参数解析前消费，避免文件路径或未知参数分支误判。
fn take_update_acknowledgement(args: &mut Vec<String>) -> Option<PathBuf> {
    let index = args
        .iter()
        .position(|argument| argument == "--update-ack")?;
    if index + 1 >= args.len() {
        args.remove(index);
        return None;
    }
    let path = PathBuf::from(args.remove(index + 1));
    args.remove(index);
    Some(path)
}

#[cfg(test)]
#[path = "../../../tests/unit/app/bootstrap/runtime.rs"]
mod tests;
