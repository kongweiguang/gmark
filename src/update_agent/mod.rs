// @author kongweiguang

//! Presentation process for an update helper transaction.
//!
//! The helper owns the transaction and writes the progress snapshot.  This
//! module deliberately has no cancellation or installer controls: closing the
//! feedback window only closes this process and leaves the helper running.

use std::{
    ffi::OsString,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicI32, Ordering},
    },
    time::Duration,
};

use gmark_update_core::{ApplyPhaseV1, ApplyProgressV1, UpdateCoreError, read_apply_progress_v1};
use gpui::prelude::*;
use gpui::{
    App, Application, AsyncApp, Bounds, Context, Render, SharedString, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, px, size,
};
use uuid::Uuid;

mod accessibility;
#[path = "../ui/theme/update_agent.rs"]
mod update_agent_theme;

use update_agent_theme::UpdateAgentPalette;

const PROGRESS_ARGUMENT: &str = "--progress";
const WINDOW_WIDTH: f32 = 380.0;
const WINDOW_HEIGHT: f32 = 230.0;
const WINDOW_MIN_WIDTH: f32 = 320.0;
const WINDOW_MIN_HEIGHT: f32 = 190.0;
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const SUCCESS_DISPLAY_DURATION: Duration = Duration::from_millis(900);

/// Input accepted by the standalone agent.  The path is the transaction's
/// fixed `progress.json`; no plan, artifact, or cancellation path is accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentArgs {
    pub progress_path: PathBuf,
}

/// Parses the intentionally narrow agent command line.
pub fn parse_args(args: &[OsString]) -> Result<Option<AgentArgs>, String> {
    match args {
        [flag] if flag == "--help" || flag == "-h" => Ok(None),
        [flag, path] if flag == PROGRESS_ARGUMENT => {
            let path = PathBuf::from(path);
            validate_progress_path(&path)?;
            Ok(Some(AgentArgs {
                progress_path: path,
            }))
        }
        _ => Err(format!(
            "usage: gmark-update-agent {PROGRESS_ARGUMENT} <transaction-progress.json>"
        )),
    }
}

/// Validates the path shape before any progress bytes are read.  Core remains
/// the authority for file opening, size limits, and JSON/schema validation.
pub fn validate_progress_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err("update progress path must be absolute".to_owned());
    }
    if path.file_name().and_then(|name| name.to_str())
        != Some(gmark_update_core::ApplyPlanV2::PROGRESS_FILE_NAME)
    {
        return Err("update progress path must name progress.json".to_owned());
    }
    transaction_id_from_progress_path(path)?;
    Ok(())
}

fn transaction_id_from_progress_path(path: &Path) -> Result<Uuid, String> {
    let transaction_dir = path
        .parent()
        .ok_or_else(|| "update progress path has no transaction directory".to_owned())?;
    let transactions_dir = transaction_dir
        .parent()
        .ok_or_else(|| "update progress path has no transactions root".to_owned())?;
    let version_dir = transactions_dir
        .parent()
        .ok_or_else(|| "update progress path has no version root".to_owned())?;
    if transactions_dir.file_name().and_then(|name| name.to_str())
        != Some(gmark_update_core::ApplyPlanV2::TRANSACTIONS_DIR_NAME)
        || !version_dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('v'))
    {
        return Err("update progress path is outside a versioned transaction root".to_owned());
    }
    transaction_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "update transaction id is not valid UTF-8".to_owned())
        .and_then(|value| {
            Uuid::parse_str(value).map_err(|_| "update transaction id is invalid".to_owned())
        })
}

/// The state visible to users.  There is deliberately no percentage: helper
/// progress is phase-based and the protocol does not publish byte totals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentState {
    Waiting { message: String },
    Preparing { message: String },
    Installing { message: String },
    Relaunching { message: String },
    Confirming { message: String },
    RollingBack { message: String },
    Succeeded { message: String },
    Failed { message: String },
}

impl AgentState {
    #[must_use]
    pub fn from_progress(progress: &ApplyProgressV1) -> Self {
        let message = progress.message.clone();
        match progress.phase {
            ApplyPhaseV1::WaitingForExit => Self::Waiting { message },
            ApplyPhaseV1::Preparing => Self::Preparing { message },
            ApplyPhaseV1::Installing => Self::Installing { message },
            ApplyPhaseV1::Relaunching => Self::Relaunching { message },
            ApplyPhaseV1::Confirming => Self::Confirming { message },
            ApplyPhaseV1::RollingBack => Self::RollingBack { message },
            ApplyPhaseV1::Succeeded => Self::Succeeded { message },
            ApplyPhaseV1::Failed => Self::Failed { message },
        }
    }

    #[must_use]
    pub fn waiting() -> Self {
        Self::Waiting {
            message: "Waiting for the update helper…".to_owned(),
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Waiting { .. } => "Waiting",
            Self::Preparing { .. } => "Preparing",
            Self::Installing { .. } => "Installing",
            Self::Relaunching { .. } => "Relaunching",
            Self::Confirming { .. } => "Confirming",
            Self::RollingBack { .. } => "Rolling back",
            Self::Succeeded { .. } => "Succeeded",
            Self::Failed { .. } => "Failed",
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Waiting { message }
            | Self::Preparing { message }
            | Self::Installing { message }
            | Self::Relaunching { message }
            | Self::Confirming { message }
            | Self::RollingBack { message }
            | Self::Succeeded { message }
            | Self::Failed { message } => message,
        }
    }

    #[must_use]
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded { .. })
    }

    #[must_use]
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// Result of one bounded progress-file poll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgressRead {
    Waiting,
    Ready(AgentState),
    Invalid(String),
}

impl ProgressRead {
    #[must_use]
    pub fn state(&self) -> AgentState {
        match self {
            Self::Waiting => AgentState::waiting(),
            Self::Ready(state) => state.clone(),
            Self::Invalid(message) => AgentState::Failed {
                message: message.clone(),
            },
        }
    }
}

/// Reads a snapshot through the core parser.  A missing file is normal while
/// the helper is starting; every other IO/protocol error remains visible.
pub fn read_progress(path: &Path) -> ProgressRead {
    match read_apply_progress_v1(path) {
        Ok(progress) => match transaction_id_from_progress_path(path) {
            Ok(transaction_id) if progress.transaction_id == transaction_id => {
                ProgressRead::Ready(AgentState::from_progress(&progress))
            }
            Ok(_) => ProgressRead::Invalid(
                "Update progress belongs to a different transaction.".to_owned(),
            ),
            Err(error) => ProgressRead::Invalid(error),
        },
        Err(error) if is_missing_progress(path, &error) => ProgressRead::Waiting,
        Err(error) => ProgressRead::Invalid(format_progress_error(error)),
    }
}

fn is_missing_progress(path: &Path, error: &UpdateCoreError) -> bool {
    if path.exists() {
        return false;
    }
    matches!(error, UpdateCoreError::Io(message) if message.contains("No such file") || message.contains("cannot find the path") || message.contains("系统找不到指定的文件"))
}

fn format_progress_error(error: UpdateCoreError) -> String {
    format!("Unable to read update progress: {error}")
}

/// Runs the UI process and returns a process exit code suitable for `main`.
pub fn run(args: AgentArgs) -> i32 {
    let progress_path = args.progress_path;
    let app = Application::new();
    let exit_code = Arc::new(AtomicI32::new(0));
    let exit_code_for_app = exit_code.clone();
    app.run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(SharedString::from("Gmark Update")),
                ..TitlebarOptions::default()
            }),
            window_background: WindowBackgroundAppearance::Opaque,
            window_min_size: Some(size(px(WINDOW_MIN_WIDTH), px(WINDOW_MIN_HEIGHT))),
            app_id: Some("com.kongweiguang.gmark.update-agent".to_owned()),
            ..WindowOptions::default()
        };
        let progress_path_for_window = progress_path.clone();
        let handle = cx.open_window(options, move |window, cx| {
            // The helper owns installation and cancellation. Closing this
            // feedback process is presentation-only and must never write a
            // cancellation marker or otherwise signal the helper.
            window.on_window_should_close(cx, |_window, _cx| true);
            cx.new(|cx| UpdateAgentWindow::new(progress_path_for_window, window, cx))
        });
        if let Err(error) = handle {
            eprintln!("failed to open update feedback window: {error}");
            exit_code_for_app.store(1, Ordering::SeqCst);
            cx.quit();
        }
    });
    exit_code.load(Ordering::SeqCst)
}

struct UpdateAgentWindow {
    progress_path: PathBuf,
    state: AgentState,
    observed_state: Option<AgentState>,
    accessibility: Option<accessibility::Bridge>,
    poll_task: Option<gpui::Task<()>>,
}

impl UpdateAgentWindow {
    fn new(progress_path: PathBuf, window: &Window, cx: &mut Context<Self>) -> Self {
        let mut window = Self {
            progress_path,
            state: AgentState::waiting(),
            observed_state: None,
            accessibility: accessibility::Bridge::new(
                window,
                accessibility::Snapshot {
                    phase: "Waiting".to_owned(),
                    message: "Waiting for the update helper…".to_owned(),
                    failure: false,
                },
            ),
            poll_task: None,
        };
        window.start_polling(cx);
        window
    }

    fn start_polling(&mut self, cx: &mut Context<Self>) {
        let path = self.progress_path.clone();
        self.poll_task = Some(cx.spawn(async move |this, cx: &mut AsyncApp| {
            loop {
                let read_path = path.clone();
                let snapshot = cx
                    .background_spawn(async move { read_progress(&read_path) })
                    .await;
                let Ok(keep_polling) = this.update(cx, |view, cx| {
                    view.apply_read(snapshot, cx);
                    !view.state.is_failure() && !view.state.is_success()
                }) else {
                    return;
                };
                if !keep_polling {
                    return;
                }
                cx.background_executor().timer(POLL_INTERVAL).await;
            }
        }));
    }

    fn apply_read(&mut self, read: ProgressRead, cx: &mut Context<Self>) {
        let state = read.state();
        if self.observed_state.as_ref() == Some(&state) {
            return;
        }
        self.observed_state = Some(state.clone());
        self.state = state;
        if let Some(bridge) = self.accessibility.as_mut() {
            bridge.update(accessibility::Snapshot {
                phase: self.state.label().to_owned(),
                message: self.state.message().to_owned(),
                failure: self.state.is_failure(),
            });
        }
        if self.state.is_success() {
            let entity = cx.entity().downgrade();
            cx.spawn(async move |_this, cx| {
                cx.background_executor()
                    .timer(SUCCESS_DISPLAY_DURATION)
                    .await;
                let _ = entity.update(cx, |_view, cx| cx.quit());
            })
            .detach();
        }
        cx.notify();
    }
}

impl Render for UpdateAgentWindow {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let failure = self.state.is_failure();
        let success = self.state.is_success();
        let message = self.state.message();
        let message = if message.is_empty() {
            if success {
                "The update completed successfully.".to_owned()
            } else if failure {
                "The update helper reported a failure.".to_owned()
            } else {
                "The update helper is working…".to_owned()
            }
        } else {
            message.to_owned()
        };
        let colors = UpdateAgentPalette::for_appearance(window.appearance());
        let accent = colors.status_accent(failure, success);
        div()
            .size_full()
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .bg(colors.background)
            .text_color(colors.primary_text)
            .child(
                div()
                    .text_size(px(20.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Gmark update"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().size(px(9.0)).rounded(px(5.0)).bg(accent))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(self.state.label()),
                    ),
            )
            .child(
                div()
                    .id("update-agent-message")
                    .flex_1()
                    .min_h(px(0.0))
                    .min_w(px(0.0))
                    .overflow_y_scroll()
                    .whitespace_normal()
                    .text_size(px(13.0))
                    .text_color(colors.secondary_text)
                    .child(message),
            )
    }
}

#[cfg(test)]
#[path = "../../tests/unit/update_agent/mod.rs"]
mod tests;
