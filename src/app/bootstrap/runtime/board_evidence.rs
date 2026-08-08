// @author kongweiguang
// @quality-exempt optional board evidence harness: restore the platform capture backend before wiring into bootstrap.

//! Board evidence command-line routing and run-level orchestration.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicI32, Ordering},
    },
};

use gpui::{App, Application};

use super::{board_evidence_capture, board_evidence_output};

use self::{
    board_evidence_batch::BoardEvidenceBatch, board_evidence_open::BoardEvidenceWindowRequest,
};

#[derive(Debug)]
pub(super) enum BoardEvidenceRequest {
    Single {
        fixture: crate::board_host::ui::BoardEvidenceFixture,
        output: PathBuf,
    },
    All {
        output_dir: PathBuf,
    },
}

pub(super) fn run_board_evidence(request: BoardEvidenceRequest) -> anyhow::Result<()> {
    let exit_code = Arc::new(AtomicI32::new(0));
    let exit_code_for_app = exit_code.clone();
    let app = Application::new().with_assets(super::GmarkAssets);
    app.run(move |cx| {
        let preferences = crate::config::load_or_create_app_preferences().unwrap_or_else(|error| {
            eprintln!("failed to initialize app preferences: {error}");
            Default::default()
        });
        crate::i18n::I18nManager::init_with_language_id(cx, &preferences.default_language_id);
        crate::theme::ThemeManager::init_with_preference(
            cx,
            preferences.theme_appearance,
            preferences.theme_palette,
        );
        crate::config::EditorSettings::init_with_typography(
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
        crate::board_host::BoardRuntimeSettings::init(&(), cx);
        crate::net::install_http_client(cx);
        crate::updater::UpdateCoordinator::init(preferences.auto_check_updates, cx);
        super::init_editor(cx, &preferences.keybindings);
        super::init_app_menu(cx);
        super::install_system_theme_observer(cx);

        if let Err(error) = start_board_evidence_run(cx, request, exit_code_for_app.clone()) {
            eprintln!("Board evidence capture failed before window startup: {error:#}");
            exit_code_for_app.store(1, Ordering::SeqCst);
            cx.quit();
        }
    });
    if exit_code.load(Ordering::SeqCst) != 0 {
        Err(anyhow::anyhow!("Board evidence capture failed"))
    } else {
        Ok(())
    }
}

/// 将 Board evidence CLI 分支留在 evidence adapter，避免启动生命周期承担参数细节。
pub(super) fn parse_runtime_arguments(
    arguments: &[String],
) -> Option<(bool, Vec<PathBuf>, Option<BoardEvidenceRequest>)> {
    match crate::cli::parse(arguments) {
        crate::cli::CliCommand::Run {
            detach,
            input_paths,
        } => Some((detach, input_paths, None)),
        crate::cli::CliCommand::BoardEvidence { fixture_id, output } => {
            let Some(fixture) = crate::board_host::ui::BoardEvidenceFixture::from_id(&fixture_id)
            else {
                eprintln!("Unknown Board evidence fixture: {fixture_id}");
                std::process::exit(2);
            };
            if let Err(error) =
                board_evidence_output::validate_board_evidence_output_target(&output)
            {
                eprintln!("Invalid Board evidence output: {error}");
                std::process::exit(2);
            }
            Some((
                false,
                Vec::new(),
                Some(BoardEvidenceRequest::Single { fixture, output }),
            ))
        }
        crate::cli::CliCommand::BoardEvidenceAll { output_dir } => {
            if let Err(error) =
                board_evidence_output::validate_board_evidence_output_directory_target(&output_dir)
            {
                eprintln!("Invalid Board evidence output directory: {error}");
                std::process::exit(2);
            }
            Some((
                false,
                Vec::new(),
                Some(BoardEvidenceRequest::All { output_dir }),
            ))
        }
        crate::cli::CliCommand::Version => {
            println!("gmark {}", env!("CARGO_PKG_VERSION"));
            None
        }
        crate::cli::CliCommand::Help => {
            println!("{}", crate::cli::help_text(env!("CARGO_PKG_VERSION")));
            None
        }
        crate::cli::CliCommand::UnknownOption(option) => {
            eprintln!("Unknown option: {option}");
            std::process::exit(1);
        }
        crate::cli::CliCommand::InvalidArguments(error) => {
            eprintln!("Invalid command-line arguments: {error}");
            std::process::exit(2);
        }
    }
}

pub(super) fn start_board_evidence_run(
    cx: &mut App,
    request: BoardEvidenceRequest,
    exit_code: Arc<AtomicI32>,
) -> anyhow::Result<()> {
    match request {
        BoardEvidenceRequest::Single { fixture, output } => {
            board_evidence_open::open_board_evidence_window(
                cx,
                BoardEvidenceWindowRequest {
                    fixture,
                    output,
                    batch: None,
                },
                exit_code,
            )
        }
        BoardEvidenceRequest::All { output_dir } => {
            let (batch, fixture, output) =
                BoardEvidenceBatch::new(output_dir).map_err(anyhow::Error::msg)?;
            let batch_for_failure = batch.clone();
            match board_evidence_open::open_board_evidence_window(
                cx,
                BoardEvidenceWindowRequest {
                    fixture,
                    output,
                    batch: Some(batch),
                },
                exit_code,
            ) {
                Ok(()) => Ok(()),
                Err(error) => {
                    let error = board_evidence_batch::fail_board_evidence_batch(
                        &batch_for_failure,
                        format!("stage=open-first-window; {error:#}"),
                    );
                    Err(anyhow::Error::msg(error))
                }
            }
        }
    }
}

mod board_evidence_batch {
    // @author kongweiguang

    //! Board evidence batch state, cleanup, and fixture sequencing.

    use std::{
        collections::BTreeSet,
        fs,
        path::{Path, PathBuf},
        sync::{
            Arc, Mutex,
            atomic::{AtomicI32, Ordering},
        },
    };

    use gpui::{AnyWindowHandle, Context};
    use image::RgbaImage;

    use super::{
        board_evidence_open::{self, BoardEvidenceWindowRequest},
        board_evidence_output::{self, BoardEvidenceArtifact},
        board_evidence_window::BoardEvidenceWindow,
    };

    pub(super) struct BoardEvidenceBatch {
        output_dir: PathBuf,
        fixtures: Vec<crate::board_host::ui::BoardEvidenceFixture>,
        next_index: usize,
        artifacts: Vec<BoardEvidenceArtifact>,
        captured_images: Vec<(crate::board_host::ui::BoardEvidenceFixture, RgbaImage)>,
        created_outputs: Vec<PathBuf>,
        manifest_created: bool,
        created_output_dir: bool,
        terminal: bool,
    }

    pub(super) enum BoardEvidenceAdvance {
        Next {
            fixture: crate::board_host::ui::BoardEvidenceFixture,
            output: PathBuf,
        },
        Finished,
    }

    impl BoardEvidenceBatch {
        pub(super) fn new(
            output_dir: PathBuf,
        ) -> Result<
            (
                Arc<Mutex<Self>>,
                crate::board_host::ui::BoardEvidenceFixture,
                PathBuf,
            ),
            String,
        > {
            board_evidence_output::validate_board_evidence_output_directory_target(&output_dir)?;
            let created_output_dir =
                board_evidence_output::ensure_board_evidence_output_directory(&output_dir)?;
            let fixtures = crate::board_host::ui::BoardEvidenceFixture::ALL.to_vec();
            let manifest_path = output_dir.join("manifest.json");
            let preflight = (|| {
                board_evidence_output::validate_board_evidence_new_file_target(&manifest_path)?;
                for fixture in &fixtures {
                    board_evidence_output::validate_board_evidence_new_file_target(
                        &output_dir.join(fixture.output_file_name()),
                    )?;
                }
                Ok::<(), String>(())
            })();
            if let Err(error) = preflight {
                let cleanup = if created_output_dir {
                    board_evidence_output::remove_board_evidence_output_directory(&output_dir)
                } else {
                    Ok(())
                };
                return Err(match cleanup {
                    Ok(()) => error,
                    Err(cleanup_error) => format!("{error}; cleanup failed: {cleanup_error}"),
                });
            }
            let Some(fixture) = fixtures.first().copied() else {
                let cleanup = if created_output_dir {
                    board_evidence_output::remove_board_evidence_output_directory(&output_dir)
                } else {
                    Ok(())
                };
                return Err(match cleanup {
                    Ok(()) => "Board evidence fixture list is empty".to_owned(),
                    Err(cleanup_error) => {
                        format!(
                            "Board evidence fixture list is empty; cleanup failed: {cleanup_error}"
                        )
                    }
                });
            };
            let output = output_dir.join(fixture.output_file_name());
            let batch = Self {
                output_dir,
                fixtures,
                next_index: 1,
                artifacts: Vec::new(),
                captured_images: Vec::new(),
                created_outputs: Vec::new(),
                manifest_created: false,
                created_output_dir,
                terminal: false,
            };
            Ok((Arc::new(Mutex::new(batch)), fixture, output))
        }

        pub(super) fn record_success(
            &mut self,
            fixture: crate::board_host::ui::BoardEvidenceFixture,
            output: &Path,
            image: &RgbaImage,
            expected_width: u32,
            expected_height: u32,
            capture_backend: super::board_evidence_capture::CaptureBackend,
        ) -> Result<BoardEvidenceAdvance, String> {
            if self.terminal {
                return Err("Board evidence batch has already reached a terminal state".to_owned());
            }
            board_evidence_output::write_png_atomically(
                output,
                image,
                expected_width,
                expected_height,
            )?;
            self.created_outputs.push(output.to_owned());
            // 矩阵比较必须基于 PNG decoder 的回读，而不是只相信 native buffer；否则编码/落盘
            // 阶段发生变化时，manifest 可能把未实际发布的像素误记为 VERIFIED。
            let decoded =
                board_evidence_output::read_png_file(output, expected_width, expected_height)?;
            let artifact = board_evidence_output::board_evidence_artifact(
                fixture,
                output,
                &decoded,
                capture_backend.method(),
            )?;
            self.captured_images.push((fixture, decoded));
            self.artifacts.push(artifact);
            let Some(next_fixture) = self.fixtures.get(self.next_index).copied() else {
                // 每张图已在 native capture、PNG 回读与 artifact 生成阶段分别验证；最终
                // 只做矩阵完整性/差异门禁，避免 debug 证据任务重复扫描整组全分辨率像素。
                board_evidence_output::validate_prevalidated_board_evidence_matrix(
                    &self.captured_images,
                )?;
                let capture_backends = self
                    .artifacts
                    .iter()
                    .map(|artifact| artifact.capture_method)
                    .collect::<BTreeSet<_>>();
                let capture_backends = capture_backends.into_iter().collect::<Vec<_>>();
                let capture_method = if capture_backends.len() == 1 {
                    (*capture_backends[0]).to_owned()
                } else {
                    "mixed capture backends; inspect each fixture artifact capture_method"
                        .to_owned()
                };
                let manifest = board_evidence_output::board_evidence_manifest(
                    std::env::consts::OS,
                    capture_method,
                    capture_backends,
                    self.output_dir.to_string_lossy().into_owned(),
                    self.artifacts.clone(),
                )?;
                let manifest_path = self.output_dir.join("manifest.json");
                board_evidence_output::write_manifest_atomically(&manifest_path, &manifest)?;
                self.manifest_created = true;
                self.terminal = true;
                return Ok(BoardEvidenceAdvance::Finished);
            };
            self.next_index = self.next_index.saturating_add(1);
            Ok(BoardEvidenceAdvance::Next {
                fixture: next_fixture,
                output: self.output_dir.join(next_fixture.output_file_name()),
            })
        }

        pub(super) fn fail(&mut self, error: String) -> String {
            if self.terminal {
                return error;
            }
            self.terminal = true;
            match self.cleanup() {
                Ok(()) => error,
                Err(cleanup_error) => format!("{error}; cleanup failed: {cleanup_error}"),
            }
        }

        fn cleanup(&mut self) -> Result<(), String> {
            let mut errors = Vec::new();
            for path in &self.created_outputs {
                match fs::symlink_metadata(path) {
                    Ok(metadata) if metadata.file_type().is_file() => {
                        if let Err(error) = fs::remove_file(path) {
                            errors.push(format!("remove '{}': {error}", path.display()));
                        }
                    }
                    Ok(metadata) if metadata.file_type().is_symlink() => errors.push(format!(
                        "refused to remove symlink '{}'; output ownership changed",
                        path.display()
                    )),
                    Ok(_) => errors.push(format!(
                        "refused to remove non-file '{}'; output ownership changed",
                        path.display()
                    )),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => errors.push(format!("inspect '{}': {error}", path.display())),
                }
            }
            if self.manifest_created {
                let manifest_path = self.output_dir.join("manifest.json");
                match fs::symlink_metadata(&manifest_path) {
                    Ok(metadata) if metadata.file_type().is_file() => {
                        if let Err(error) = fs::remove_file(&manifest_path) {
                            errors.push(format!("remove '{}': {error}", manifest_path.display()));
                        }
                    }
                    Ok(metadata) if metadata.file_type().is_symlink() => errors.push(format!(
                        "refused to remove symlink '{}'; manifest ownership changed",
                        manifest_path.display()
                    )),
                    Ok(_) => errors.push(format!(
                        "refused to remove non-file '{}'; manifest ownership changed",
                        manifest_path.display()
                    )),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => errors.push(format!(
                        "inspect manifest '{}': {error}",
                        manifest_path.display()
                    )),
                }
            }
            if self.created_output_dir
                && let Err(error) =
                    board_evidence_output::remove_board_evidence_output_directory(&self.output_dir)
            {
                errors.push(error);
            }
            if errors.is_empty() {
                Ok(())
            } else {
                Err(errors.join("; "))
            }
        }
    }

    impl Drop for BoardEvidenceBatch {
        fn drop(&mut self) {
            if self.terminal {
                return;
            }
            if let Err(error) = self.cleanup() {
                eprintln!(
                    "Board evidence batch dropped before completion; cleanup failed: {error}"
                );
            }
        }
    }

    pub(super) fn fail_board_evidence_batch(
        batch: &Arc<Mutex<BoardEvidenceBatch>>,
        error: String,
    ) -> String {
        match batch.lock() {
            Ok(mut state) => state.fail(error),
            Err(_) => format!("{error}; cleanup failed: batch mutex is poisoned"),
        }
    }

    pub(super) fn schedule_next_board_evidence_window(
        cx: &mut Context<BoardEvidenceWindow>,
        current_window: AnyWindowHandle,
        batch: Arc<Mutex<BoardEvidenceBatch>>,
        fixture: crate::board_host::ui::BoardEvidenceFixture,
        output: PathBuf,
        exit_code: Arc<AtomicI32>,
    ) {
        let batch_for_failure = batch.clone();
        let exit_code_for_open = exit_code.clone();
        cx.spawn(async move |_this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;
            let result = cx.update(move |cx| {
                board_evidence_open::open_board_evidence_window(
                    cx,
                    BoardEvidenceWindowRequest {
                        fixture,
                        output,
                        batch: Some(batch),
                    },
                    exit_code_for_open,
                )
            });
            let error = match result {
                Ok(Ok(())) => {
                    // Keep the previous window alive until the next HWND is successfully created;
                    // otherwise GPUI can terminate the app when the last evidence window is removed.
                    let _ = current_window.update(cx, |_, window, _| window.remove_window());
                    return;
                }
                Ok(Err(error)) => format!("stage=open-next-window; {error:#}"),
                Err(error) => format!("stage=app-update-next-window; {error}"),
            };
            let error = fail_board_evidence_batch(&batch_for_failure, error);
            exit_code.store(1, Ordering::SeqCst);
            eprintln!("Board evidence batch failed: {error}");
            let _ = cx.update(|cx| cx.quit());
        })
        .detach();
    }
}

mod board_evidence_open {
    // @author kongweiguang

    //! Board evidence window preparation and GPUI window ownership.

    use std::{
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicI32, Ordering},
        },
    };

    use gpui::{App, AppContext, BorrowAppContext, Bounds, px, size};

    use super::{
        board_evidence_batch::{self, BoardEvidenceBatch},
        board_evidence_window::BoardEvidenceWindow,
    };

    pub(super) struct BoardEvidenceWindowRequest {
        pub(super) fixture: crate::board_host::ui::BoardEvidenceFixture,
        pub(super) output: PathBuf,
        pub(super) batch: Option<Arc<Mutex<BoardEvidenceBatch>>>,
    }

    pub(super) fn open_board_evidence_window(
        cx: &mut App,
        request: BoardEvidenceWindowRequest,
        exit_code: Arc<AtomicI32>,
    ) -> anyhow::Result<()> {
        let BoardEvidenceWindowRequest {
            fixture,
            output,
            batch,
        } = request;
        // Light/Dark evidence must drive the same global ThemeManager consumed by the real
        // BoardHost canvas. Overriding only the deterministic chrome view-model creates a mixed
        // frame (light chrome over dark canvas colors) and can hide text in an otherwise valid
        // capture.
        let evidence_appearance = if fixture == crate::board_host::ui::BoardEvidenceFixture::Light {
            crate::theme::ThemeAppearance::Light
        } else {
            crate::theme::ThemeAppearance::Dark
        };
        let platform = cx.window_appearance();
        cx.update_global::<crate::theme::ThemeManager, _>(|manager, _cx| {
            manager.set_theme_preference(
                evidence_appearance,
                crate::theme::ThemePalette::Xcode,
                platform,
            )
        });
        let prepared = crate::board_host::prepare_board_evidence(fixture)?;
        let crate::board_host::PreparedBoardEvidence {
            prepared,
            scenario,
            workspace,
        } = prepared;
        let window_size = scenario.window;
        let bounds = Bounds::centered(
            None,
            size(
                px(f32::from(window_size.width)),
                px(f32::from(window_size.height)),
            ),
            cx,
        );
        let options = crate::platform::window::gmark_window_options(
            format!(
                "gmark - Board evidence - {} ({})",
                fixture.id(),
                fixture.output_file_name()
            )
            .into(),
            bounds,
        );
        let completion = Arc::new(AtomicBool::new(false));
        let handle = cx
            .open_window(options, move |window, cx| {
                let batch_for_close = batch.clone();
                let exit_code_for_close = exit_code.clone();
                let completion_for_close = completion.clone();
                window.on_window_should_close(cx, move |_, cx| {
                    if completion_for_close.swap(true, Ordering::SeqCst) {
                        return true;
                    }
                    let error = if let Some(batch) = batch_for_close.as_ref() {
                        board_evidence_batch::fail_board_evidence_batch(
                            batch,
                            format!(
                                "stage=window-close; fixture '{}' closed before evidence completion",
                                fixture.id()
                            ),
                        )
                    } else {
                        format!(
                            "stage=window-close; fixture '{}' closed before evidence completion",
                            fixture.id()
                        )
                    };
                    exit_code_for_close.store(1, Ordering::SeqCst);
                    eprintln!("Board evidence capture failed: {error}");
                    cx.quit();
                    true
                });
                let host = cx.new(|cx| {
                    let mut host = crate::board_host::BoardHost::from_prepared(prepared, cx);
                    host.install_evidence_fixture(fixture, cx);
                    host
                });
                cx.new(|cx| {
                    BoardEvidenceWindow::new(
                        host, fixture, output, batch, completion, workspace, exit_code, window, cx,
                    )
                })
            })
            .map_err(|error| anyhow::anyhow!("failed to open Board evidence window: {error}"))?;
        handle
            .update(cx, |_, window, _| window.activate_window())
            .map_err(|error| {
                anyhow::anyhow!("failed to activate Board evidence window: {error}")
            })?;
        Ok(())
    }
}

mod board_evidence_window {
    // @author kongweiguang

    //! Board evidence window readiness, accessibility synchronization, and capture trigger.

    use std::{
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicI32, Ordering},
        },
    };

    use futures::StreamExt as _;
    use gpui::prelude::*;
    use gpui::{App, Context, Entity, IntoElement, Render, Task, Window, div};
    use tempfile::TempDir;

    use super::board_evidence_batch::BoardEvidenceBatch;

    const MIN_PRESENTED_FRAMES: u8 = 2;

    pub(super) struct BoardEvidenceWindow {
        host: Entity<crate::board_host::BoardHost>,
        bridge: Option<crate::accessibility::AccessibilityBridge>,
        _accessibility_wake_task: Option<Task<()>>,
        accessibility_revision: Option<u64>,
        fixture: crate::board_host::ui::BoardEvidenceFixture,
        output: PathBuf,
        batch: Option<Arc<Mutex<BoardEvidenceBatch>>>,
        completion: Arc<AtomicBool>,
        _workspace: TempDir,
        exit_code: Arc<AtomicI32>,
        presented_frames: u8,
        post_paint_scheduled: bool,
        wait_started_at: std::time::Instant,
        capture_attempted: bool,
        finished: bool,
        initial_error: Option<String>,
    }

    impl BoardEvidenceWindow {
        pub(super) fn new(
            host: Entity<crate::board_host::BoardHost>,
            fixture: crate::board_host::ui::BoardEvidenceFixture,
            output: PathBuf,
            batch: Option<Arc<Mutex<BoardEvidenceBatch>>>,
            completion: Arc<AtomicBool>,
            workspace: TempDir,
            exit_code: Arc<AtomicI32>,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) -> Self {
            let initial = evidence_accessibility_snapshot(&host, window, cx);
            let (bridge, wake) =
                match crate::accessibility::AccessibilityBridge::new(window, initial) {
                    Some((bridge, wake)) => (Some(bridge), Some(wake)),
                    None => (None, None),
                };
            let initial_error = bridge.is_none().then(|| {
                "AccessKit bridge could not be installed for the Board evidence window".to_owned()
            });
            let accessibility_wake_task = wake.map(|mut wake| {
                cx.spawn(async move |this, cx| {
                    while wake.next().await.is_some() {
                        let Ok(()) = this.update(cx, |_view, cx| cx.notify()) else {
                            break;
                        };
                    }
                })
            });
            Self {
                host,
                bridge,
                _accessibility_wake_task: accessibility_wake_task,
                accessibility_revision: None,
                fixture,
                output,
                batch,
                completion,
                _workspace: workspace,
                exit_code,
                presented_frames: 0,
                post_paint_scheduled: false,
                wait_started_at: std::time::Instant::now(),
                capture_attempted: false,
                finished: false,
                initial_error,
            }
        }

        fn sync_accessibility(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            let actions = self
                .bridge
                .as_ref()
                .map(crate::accessibility::AccessibilityBridge::take_actions)
                .unwrap_or_default();
            for request in actions {
                if matches!(
                    request.action,
                    accesskit::Action::Click
                        | accesskit::Action::Increment
                        | accesskit::Action::Decrement
                        | accesskit::Action::Expand
                        | accesskit::Action::Collapse
                ) {
                    let _ = self.host.update(cx, |host, cx| {
                        host.handle_platform_accessibility_action(request.target_node.0, window, cx)
                    });
                }
            }

            let revision = self.host.read(cx).accessibility_revision();
            if self.accessibility_revision == Some(revision) {
                if let Some(bridge) = self.bridge.as_mut() {
                    bridge.update_focus(window.is_window_active());
                }
                return;
            }
            if let Some(bridge) = self.bridge.as_mut() {
                bridge.update(evidence_accessibility_snapshot(&self.host, window, cx));
                bridge.update_focus(window.is_window_active());
            }
            self.accessibility_revision = Some(revision);
        }

        fn attempt_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.capture_attempted || self.finished {
                return;
            }
            // 该方法只由 post-frame callback 进入；截图还必须建立在 BoardHost 已完成文件恢复、
            // AccessKit 节点发布和像素前置校验之后，任一条件失败都走同一退出路径。
            let board = self.host.read(cx);
            if !board.evidence_ready() {
                if self.wait_started_at.elapsed() > std::time::Duration::from_secs(5) {
                    self.finish_failure(
                        "timed out waiting for the recovery journal to reach the BoardHost"
                            .to_owned(),
                        cx,
                    );
                } else {
                    // 该分支来自 post-frame callback，此时 GPUI 仍在派发 paint 消息；只失效实体
                    // 让事件循环安排下一帧，不能在 paint 内查询 `Window::current_view`。
                    cx.notify();
                }
                return;
            }

            let keys = board.evidence_accessibility_keys(window, cx);
            if let Err(error) =
                crate::board_host::ui::validate_expected_accessibility_keys(self.fixture, &keys)
            {
                self.finish_failure(error, cx);
                return;
            }
            let accessibility = board.platform_accessibility_snapshot(window, cx);
            if accessibility.nodes.is_empty()
                || !accessibility
                    .nodes
                    .iter()
                    .any(|node| node.id == accessibility.root_id)
                || !accessibility
                    .nodes
                    .iter()
                    .any(|node| node.id == accessibility.focus_id)
            {
                self.finish_failure(
                    "Board evidence AccessKit surface has no valid root/focus presence".to_owned(),
                    cx,
                );
                return;
            }
            self.capture_attempted = true;
            let captured = match super::board_evidence_capture::capture_window_rgba(window) {
                Ok(captured) => captured,
                Err(error) => {
                    self.finish_failure(error, cx);
                    return;
                }
            };
            eprintln!(
                "Board evidence fixture '{}' captured via {}",
                self.fixture.id(),
                captured.backend.method()
            );
            let viewport = window.viewport_size();
            let expected_width = (f64::from(viewport.width) * f64::from(window.scale_factor()))
                .round()
                .max(1.0) as u32;
            let expected_height = (f64::from(viewport.height) * f64::from(window.scale_factor()))
                .round()
                .max(1.0) as u32;
            if let Some(batch) = self.batch.clone() {
                let advance = match batch.lock() {
                    Ok(mut state) => state.record_success(
                        self.fixture,
                        &self.output,
                        &captured.image,
                        expected_width,
                        expected_height,
                        captured.backend,
                    ),
                    Err(_) => Err("stage=batch-state; batch mutex is poisoned".to_owned()),
                };
                match advance {
                    Ok(super::board_evidence_batch::BoardEvidenceAdvance::Next {
                        fixture,
                        output,
                    }) => {
                        self.completion.store(true, Ordering::SeqCst);
                        self.finished = true;
                        let current_window = window.window_handle();
                        super::board_evidence_batch::schedule_next_board_evidence_window(
                            cx,
                            current_window,
                            batch,
                            fixture,
                            output,
                            self.exit_code.clone(),
                        );
                    }
                    Ok(super::board_evidence_batch::BoardEvidenceAdvance::Finished) => {
                        self.completion.store(true, Ordering::SeqCst);
                        self.finished = true;
                        cx.quit();
                    }
                    Err(error) => self.finish_failure(error, cx),
                }
                return;
            }
            if let Err(error) = super::board_evidence_output::write_png_atomically(
                &self.output,
                &captured.image,
                expected_width,
                expected_height,
            ) {
                self.finish_failure(error, cx);
                return;
            }
            self.completion.store(true, Ordering::SeqCst);
            self.finished = true;
            cx.quit();
        }

        fn schedule_post_paint_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.finished || self.post_paint_scheduled {
                return;
            }
            self.post_paint_scheduled = true;
            // GPUI 0.2.2 drains this callback on the next frame request, after the current
            // draw/present/complete_frame sequence.  It is therefore the first safe point at which
            // a native HWND capture can observe the BoardHost pixels just returned below.
            cx.on_next_frame(window, |this, window, cx| {
                this.post_paint_scheduled = false;
                this.on_presented_frame(window, cx);
            });
            // `on_next_frame` callback 在 GPUI 的 paint 消息内执行；此处调用
            // `Window::request_animation_frame` 会在 invalidator 仍处于 paint 时查询当前 view
            // 并触发 panic。改为通知实体，让事件循环在本次 paint/present 返回后安排下一次绘制。
            cx.notify();
        }

        fn on_presented_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.finished || self.capture_attempted {
                return;
            }
            if let Some(error) = self.initial_error.take() {
                self.finish_failure(error, cx);
                return;
            }
            self.presented_frames = self.presented_frames.saturating_add(1);
            // 只有 post-frame callback 才能计数；render 调用次数不代表 GPUI 已经完成 paint/present。
            if self.presented_frames < MIN_PRESENTED_FRAMES {
                self.schedule_post_paint_capture(window, cx);
                return;
            }
            self.attempt_capture(window, cx);
        }

        fn finish_failure(&mut self, error: String, cx: &mut Context<Self>) {
            if self.finished {
                return;
            }
            // 失败状态只允许写一次：先发布非零结果，再退出 GPUI，防止普通编辑窗口继续启动。
            self.completion.store(true, Ordering::SeqCst);
            self.finished = true;
            self.exit_code.store(1, Ordering::SeqCst);
            let error = self
                .batch
                .as_ref()
                .map(|batch| {
                    super::board_evidence_batch::fail_board_evidence_batch(batch, error.clone())
                })
                .unwrap_or(error);
            eprintln!(
                "Board evidence fixture '{}' failed: {error}",
                self.fixture.id()
            );
            cx.quit();
        }
    }

    fn evidence_accessibility_snapshot(
        host: &Entity<crate::board_host::BoardHost>,
        window: &Window,
        cx: &App,
    ) -> crate::accessibility::EditorAccessibilitySnapshot {
        let board = host.read(cx);
        let dirty = board.is_dirty();
        let busy = board.is_busy();
        let strings = cx.global::<crate::i18n::I18nManager>().strings();
        crate::accessibility::EditorAccessibilitySnapshot {
            title: board.title(),
            save_label: strings.menu_save.clone(),
            dirty,
            status: if busy {
                "Board operation in progress".to_owned()
            } else if dirty {
                strings.large_document_text("modified").to_owned()
            } else {
                strings.large_document_text("saved").to_owned()
            },
            error: None,
            busy,
            search_visible: false,
            navigation_visible: false,
            caret: None,
            lines: Vec::new(),
            folds: Vec::new(),
            surface: Some(board.platform_accessibility_snapshot(window, cx)),
        }
    }

    impl Render for BoardEvidenceWindow {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.sync_accessibility(window, cx);
            if !self.finished {
                self.schedule_post_paint_capture(window, cx);
            }
            div().size_full().child(self.host.clone())
        }
    }
}
