// @author kongweiguang

//! Update check, download, and install orchestration service.

use super::install;
use super::*;

pub(crate) struct UpdateService {
    pub(super) state: UpdateState,
    pub(super) updates_root: PathBuf,
    pub(super) available: bool,
    pub(super) worker: Option<Task<()>>,
    pub(super) download_control: Option<DownloadControl>,
    pub(super) progress_started_at: Option<(Instant, u64)>,
    pub(super) pending_install: Option<PendingInstall>,
    pub(super) pending_install_v2: Option<InstallAttempt>,
    pub(super) retry_install: Option<RetryPayload>,
    pub(super) retry_redownload: bool,
    pub(super) install_monitor: Option<Task<()>>,
    pub(super) prepare_claimed: bool,
    pub(super) auto_check_enabled: bool,
    pub(super) last_progress_refresh: Instant,
}

impl UpdateService {
    pub(super) fn new(updates_root: PathBuf, auto_check_enabled: bool) -> Self {
        cleanup_update_cache(&updates_root);
        let state = restored_startup_state(&updates_root).unwrap_or_else(|| {
            update_v2::restore_ready_release(&updates_root, env!("CARGO_PKG_VERSION"))
                .map(|(release, artifact_path)| UpdateState::Ready {
                    release,
                    artifact_path,
                })
                .unwrap_or_default()
        });
        Self {
            state,
            updates_root,
            available: true,
            worker: None,
            download_control: None,
            progress_started_at: None,
            pending_install: None,
            pending_install_v2: None,
            retry_install: None,
            retry_redownload: false,
            install_monitor: None,
            prepare_claimed: false,
            auto_check_enabled,
            last_progress_refresh: Instant::now() - Duration::from_secs(1),
        }
    }

    pub(super) fn new_unavailable(message: String, auto_check_enabled: bool) -> Self {
        Self {
            state: UpdateState::Failed {
                release: None,
                message,
                retryable: false,
            },
            // No path is retained when the configured root could not be
            // resolved.  Keeping an empty sentinel avoids manufacturing a
            // second transaction root while all filesystem operations remain
            // guarded by `available`.
            updates_root: PathBuf::new(),
            available: false,
            worker: None,
            download_control: None,
            progress_started_at: None,
            pending_install: None,
            pending_install_v2: None,
            retry_install: None,
            retry_redownload: false,
            install_monitor: None,
            prepare_claimed: false,
            auto_check_enabled,
            last_progress_refresh: Instant::now() - Duration::from_secs(1),
        }
    }

    pub(super) fn automatic_check_due(&self) -> bool {
        if !self.available {
            return false;
        }
        let path = self.updates_root.join("last-successful-check");
        let Ok(value) = std::fs::read_to_string(path) else {
            return true;
        };
        let Ok(previous) = value.trim().parse::<u64>() else {
            return true;
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        now.saturating_sub(previous) >= AUTO_CHECK_INTERVAL.as_secs()
    }

    pub(super) fn refresh_apply_result(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.available {
            return false;
        }
        if let Some(state) = restored_startup_state(&self.updates_root)
            && !matches!(
                self.state,
                UpdateState::Downloading { .. } | UpdateState::Verifying { .. }
            )
        {
            self.pending_install = None;
            self.state = state;
            self.refresh(cx);
            return true;
        }
        false
    }

    pub(super) fn check(&mut self, origin: CheckOrigin, cx: &mut Context<Self>) {
        if !self.available {
            return;
        }
        if !self.state.accepts(UpdateCommand::Check) {
            return;
        }
        self.state = UpdateState::Checking { origin };
        self.refresh(cx);

        let (sender, receiver) = oneshot::channel();
        std::thread::spawn(move || {
            let result = update_v2::check_latest_version_v2(env!("CARGO_PKG_VERSION"));
            let _ = sender.send(result);
        });
        self.worker = Some(cx.spawn(async move |this, cx| {
            let result = receiver.await.unwrap_or_else(|_| {
                Err(update_v2::UpdateV2Error::Network(
                    "update check worker ended unexpectedly".to_owned(),
                ))
            });
            let _ = this.update(cx, |service, cx| {
                service.worker = None;
                match result {
                    Ok(CheckOutcome::Available(release)) => {
                        service.persist_successful_check();
                        service.state = UpdateState::Available(release);
                    }
                    Ok(CheckOutcome::UpToDate {
                        current_version,
                        latest_version,
                    }) => {
                        service.persist_successful_check();
                        service.state = if origin == CheckOrigin::Manual {
                            UpdateState::UpToDate {
                                current_version,
                                latest_version,
                            }
                        } else {
                            UpdateState::Idle
                        };
                    }
                    Err(error) => {
                        service.state = if origin == CheckOrigin::Manual {
                            UpdateState::Failed {
                                release: None,
                                message: error.to_string(),
                                retryable: matches!(error, update_v2::UpdateV2Error::Network(_)),
                            }
                        } else {
                            UpdateState::Idle
                        };
                    }
                }
                service.refresh(cx);
            });
        }));
    }

    pub(super) fn download(&mut self, cx: &mut Context<Self>) {
        if !self.state.accepts(UpdateCommand::Download) {
            return;
        }
        let Some(release) = self.state.release().cloned() else {
            return;
        };
        self.start_download(release, cx);
    }

    pub(super) fn start_download(&mut self, release: UpdateRelease, cx: &mut Context<Self>) {
        self.retry_redownload = false;
        let control = DownloadControl::default();
        self.download_control = Some(control.clone());
        self.progress_started_at = Some((Instant::now(), 0));
        self.state = UpdateState::Downloading {
            release: release.clone(),
            downloaded: 0,
            total: release.artifact_size,
            bytes_per_second: 0,
        };
        self.refresh(cx);

        let updates_root = self.updates_root.clone();
        let (sender, mut receiver) = mpsc::unbounded::<WorkerEvent>();
        std::thread::spawn(move || {
            for attempt in 0..3_u32 {
                let result =
                    update_v2::download_release(&release, &updates_root, &control, |event| {
                        let _ = sender.unbounded_send(WorkerEvent::Download(event));
                    });
                match result {
                    Ok(_) => break,
                    Err(error) if error.retryable() && attempt < 2 => {
                        std::thread::sleep(Duration::from_secs(1_u64 << attempt));
                    }
                    Err(error) => {
                        let _ = sender.unbounded_send(WorkerEvent::Failed {
                            message: error.to_string(),
                            retryable: error.retryable(),
                        });
                        break;
                    }
                }
            }
        });
        self.worker = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = receiver.next().await {
                let terminal = matches!(
                    event,
                    WorkerEvent::Download(DownloadEvent::Finished { .. })
                        | WorkerEvent::Download(DownloadEvent::Paused { .. })
                        | WorkerEvent::Failed { .. }
                );
                let _ = this.update(cx, |service, cx| {
                    service.apply_worker_event(event, cx);
                    if terminal {
                        service.worker = None;
                        service.download_control = None;
                    }
                });
                if terminal {
                    break;
                }
            }
        }));
    }

    pub(super) fn pause(&mut self, cx: &mut Context<Self>) {
        if self.state.accepts(UpdateCommand::Pause)
            && let Some(control) = &self.download_control
        {
            control.pause();
            self.refresh(cx);
        }
    }

    pub(super) fn resume(&mut self, cx: &mut Context<Self>) {
        if !self.state.accepts(UpdateCommand::Resume) {
            return;
        }
        let Some(release) = self.state.release().cloned() else {
            return;
        };
        self.start_download(release, cx);
    }

    pub(super) fn retry(&mut self, cx: &mut Context<Self>) {
        if !self.state.accepts(UpdateCommand::Retry) {
            return;
        }
        if let Some(payload) = self.retry_install.take() {
            // RetryInstall is explicitly an apply retry: the signed envelope
            // and verified artifact remain untouched, so no network download
            // is started.  A fresh prepare allocates a new transaction UUID.
            self.state = UpdateState::Ready {
                release: payload.release,
                artifact_path: payload.artifact_path,
            };
            self.refresh(cx);
            return;
        }
        let redownload = self.retry_redownload;
        self.retry_redownload = false;
        let release = match &self.state {
            UpdateState::Failed {
                release: Some(release),
                ..
            } => Some(release.clone()),
            UpdateState::Failed { release: None, .. } => None,
            _ => return,
        };
        if let Some(release) = release {
            if redownload {
                if let Err(error) = discard_verified_source(&self.updates_root, &release.version) {
                    self.state = UpdateState::Failed {
                        release: Some(release),
                        message: error,
                        retryable: false,
                    };
                    self.refresh(cx);
                    return;
                }
            }
            self.start_download(release, cx);
        } else {
            self.check(CheckOrigin::Manual, cx);
        }
    }

    pub(super) fn dismiss(&mut self, cx: &mut Context<Self>) {
        if !self.available {
            return;
        }
        if !self.state.accepts(UpdateCommand::Dismiss) {
            return;
        }
        self.state = UpdateState::Idle;
        self.refresh(cx);
    }

    /// Reserves the verified source payload while the quit coordinator asks
    /// every editor window for approval.  No transaction directory, lock, or
    /// helper exists until the handoff callback invokes `prepare_install`.
    pub(super) fn begin_awaiting_quit(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.state.accepts(UpdateCommand::InstallAndRestart)
            || self.prepare_claimed
            || self.pending_install_v2.is_some()
        {
            return false;
        }
        let UpdateState::Ready {
            release,
            artifact_path,
        } = self.state.clone()
        else {
            return false;
        };
        self.state = UpdateState::AwaitingQuit {
            release,
            artifact_path,
        };
        self.refresh(cx);
        true
    }

    pub(super) fn prepare_install(&mut self, cx: &mut Context<Self>) -> bool {
        if !matches!(self.state, UpdateState::AwaitingQuit { .. })
            || self.prepare_claimed
            || self.pending_install_v2.is_some()
        {
            return false;
        }
        let payload = match self.state.clone() {
            UpdateState::AwaitingQuit {
                release,
                artifact_path,
            } => Some((release, artifact_path)),
            _ => None,
        };
        let Some((release, artifact_path)) = payload else {
            return false;
        };
        self.prepare_claimed = true;
        match self.write_apply_plan(&release, &artifact_path) {
            Ok(prepared) => {
                let mut command = Command::new(&prepared.helper.path);
                command.arg("--apply-plan").arg(&prepared.plan_path);
                command.env(
                    UPDATE_ACK_CAPABILITY_ENV,
                    &prepared.acknowledgement_capability,
                );
                command.env("GMARK_UPDATE_AGENT_PATH", &prepared.agent.path);
                command.env(
                    "GMARK_UPDATE_TRANSACTION_ID",
                    prepared.plan_v2.transaction_id.hyphenated().to_string(),
                );
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt as _;

                    // helper 是纯后台事务进程；Windows Terminal 不应为它创建可见黑框。
                    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                    command.creation_flags(CREATE_NO_WINDOW);
                }
                let helper_guard = match verify_staged_helper_for_launch(&prepared.helper) {
                    Ok(guard) => guard,
                    Err(message) => {
                        self.abort_prepared_install(&prepared, false);
                        self.state = UpdateState::Failed {
                            release: Some(release),
                            message,
                            retryable: true,
                        };
                        self.prepare_claimed = false;
                        self.refresh(cx);
                        return false;
                    }
                };
                let agent_guard = match verify_staged_helper_for_launch(&prepared.agent) {
                    Ok(guard) => guard,
                    Err(message) => {
                        drop(helper_guard);
                        self.abort_prepared_install(&prepared, false);
                        self.state = UpdateState::Failed {
                            release: Some(release),
                            message,
                            retryable: true,
                        };
                        self.prepare_claimed = false;
                        self.refresh(cx);
                        return false;
                    }
                };
                match command.spawn() {
                    Ok(_) => {
                        drop(helper_guard);
                        drop(agent_guard);
                        let attempt = InstallAttempt {
                            release: release.clone(),
                            artifact_path: artifact_path.clone(),
                            plan_path: prepared.plan_path.clone(),
                            plan: prepared.plan_v2.clone(),
                            helper: prepared.helper,
                            agent: prepared.agent,
                            acknowledgement_capability: prepared.acknowledgement_capability,
                            started_at: Instant::now(),
                        };
                        self.pending_install = Some(PendingInstall {
                            release: release.clone(),
                            artifact_path: artifact_path.clone(),
                            plan: prepared.plan,
                        });
                        self.pending_install_v2 = Some(attempt.clone());
                        self.state = UpdateState::Installing { release };
                        self.start_install_monitor(attempt, cx);
                        self.refresh(cx);
                        true
                    }
                    Err(error) => {
                        drop(helper_guard);
                        drop(agent_guard);
                        let attempt = InstallAttempt {
                            release: release.clone(),
                            artifact_path: artifact_path.clone(),
                            plan_path: prepared.plan_path.clone(),
                            plan: prepared.plan_v2.clone(),
                            helper: prepared.helper,
                            agent: prepared.agent,
                            acknowledgement_capability: prepared.acknowledgement_capability,
                            started_at: Instant::now(),
                        };
                        self.abort_attempt(&attempt, false);
                        self.retry_install = Some(RetryPayload {
                            release: release.clone(),
                            artifact_path: artifact_path.clone(),
                        });
                        self.state = UpdateState::Failed {
                            release: Some(release),
                            message: format!("failed to start update helper: {error}"),
                            retryable: true,
                        };
                        self.prepare_claimed = false;
                        self.refresh(cx);
                        false
                    }
                }
            }
            Err(message) => {
                self.prepare_claimed = false;
                self.state = UpdateState::Failed {
                    release: Some(release),
                    message,
                    retryable: false,
                };
                self.refresh(cx);
                false
            }
        }
    }

    pub(super) fn cancel_pending_install(&mut self, cx: &mut Context<Self>) {
        match self.restore_ready_after_cancel() {
            Ok(true) | Err(_) => self.refresh(cx),
            Ok(false) => {}
        }
    }

    pub(super) fn restore_ready_after_cancel(&mut self) -> Result<bool, String> {
        if let UpdateState::AwaitingQuit {
            release,
            artifact_path,
        } = self.state.clone()
        {
            self.prepare_claimed = false;
            self.state = UpdateState::Ready {
                release,
                artifact_path,
            };
            return Ok(true);
        }
        if let Some(attempt) = self.pending_install_v2.clone() {
            write_cancellation_marker(&attempt.plan.cancellation_path)?;
            self.pending_install_v2 = None;
            self.pending_install = None;
            self.prepare_claimed = false;
            self.abort_attempt(&attempt, false);
            self.retry_install = None;
            self.state = UpdateState::Ready {
                release: attempt.release,
                artifact_path: attempt.artifact_path,
            };
            return Ok(true);
        }
        let Some(pending) = self.pending_install.as_ref() else {
            return Ok(false);
        };
        if let Err(error) = write_cancellation_marker(&pending.plan.cancellation_path) {
            let message = format!(
                "failed to cancel update installation; the helper may still be waiting: {error}"
            );
            self.state = UpdateState::Failed {
                release: Some(pending.release.clone()),
                message: message.clone(),
                retryable: false,
            };
            return Err(message);
        }
        let Some(pending) = self.pending_install.take() else {
            return Ok(false);
        };
        self.prepare_claimed = false;
        self.state = UpdateState::Ready {
            release: pending.release,
            artifact_path: pending.artifact_path,
        };
        Ok(true)
    }

    pub(super) fn write_apply_plan(
        &self,
        release: &UpdateRelease,
        artifact_path: &std::path::Path,
    ) -> Result<PreparedInstall, String> {
        if !self.available {
            return Err("update cache root is unavailable".to_owned());
        }
        install::prepare_apply_plan(&self.updates_root, release, artifact_path)
    }

    fn abort_prepared_install(&mut self, prepared: &PreparedInstall, _preserve_artifact: bool) {
        if let Some(transaction_dir) = prepared.plan_v2.transaction_dir() {
            cleanup_failed_prepare(
                prepared.plan_v2.transaction_id,
                transaction_dir,
                Some(&prepared.acknowledgement_capability),
            );
        } else {
            let _ = release_lifecycle_lock(prepared.plan_v2.transaction_id);
        }
    }

    fn abort_attempt(&mut self, attempt: &InstallAttempt, keep_terminal_transaction: bool) {
        let transaction_dir = attempt.plan.transaction_dir().map(PathBuf::from);
        let _ = release_lifecycle_lock(attempt.plan.transaction_id);
        if let Some(transaction_dir) = transaction_dir {
            release_transaction_claim(&transaction_dir);
            if keep_terminal_transaction {
                let _ = std::fs::remove_file(&attempt.helper.path);
                let _ = std::fs::remove_file(&attempt.agent.path);
                let _ = std::fs::remove_file(&attempt.plan_path);
                let _ = std::fs::remove_file(acknowledgement_capability_path(
                    &transaction_dir,
                    &attempt.acknowledgement_capability,
                ));
            } else {
                let _ = std::fs::remove_dir_all(transaction_dir);
            }
        } else if !keep_terminal_transaction {
            let _ = std::fs::remove_file(&attempt.plan_path);
        }
    }

    fn start_install_monitor(&mut self, attempt: InstallAttempt, cx: &mut Context<Self>) {
        let plan = attempt.plan.clone();
        self.install_monitor = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let result = read_v2_result(&plan);
                let progress = read_v2_progress(&plan);
                let waiting_for_exit = progress.as_ref().is_some_and(|progress| {
                    progress.phase == gmark_update_core::ApplyPhaseV1::WaitingForExit
                });
                let timed_out =
                    waiting_for_exit && helper_timeout_expired(attempt.started_at, Instant::now());
                let terminal = result.is_some();
                let should_stop = terminal || timed_out;
                let Ok(()) = this.update(cx, |service, cx| {
                    if let Some(progress) = progress {
                        service.apply_install_progress(progress, cx);
                    }
                    if let Some(result) = result {
                        service.finish_install_attempt(result, cx);
                    } else if timed_out {
                        service.timeout_install_attempt(cx);
                    }
                    if should_stop {
                        service.install_monitor = None;
                    }
                }) else {
                    return;
                };
                if should_stop {
                    break;
                }
            }
        }));
    }

    fn apply_install_progress(
        &mut self,
        _progress: gmark_update_core::ApplyProgressV1,
        cx: &mut Context<Self>,
    ) {
        // Progress is deliberately read independently from the download
        // worker.  The current state model has no phase field; notifying here
        // keeps the feedback surface live without conflating worker lifetimes.
        self.refresh(cx);
    }

    fn finish_install_attempt(
        &mut self,
        result: gmark_update_core::ApplyResultV2,
        cx: &mut Context<Self>,
    ) {
        let Some(attempt) = self.pending_install_v2.take() else {
            return;
        };
        self.pending_install = None;
        self.prepare_claimed = false;
        self.abort_attempt(&attempt, true);
        if result.status == "succeeded" {
            self.retry_install = None;
            self.state = UpdateState::Succeeded {
                version: result.to_version,
                message: result.message,
            };
        } else {
            let release = attempt.release.clone();
            let artifact_path = attempt.artifact_path.clone();
            self.retry_install = None;
            self.retry_redownload = false;
            let (retryable, state_release) = match result.recovery_action {
                Some(gmark_update_core::RecoveryAction::ReattemptInstall) => {
                    self.retry_install = Some(RetryPayload {
                        release: release.clone(),
                        artifact_path,
                    });
                    (true, Some(release))
                }
                Some(gmark_update_core::RecoveryAction::Redownload) => {
                    self.retry_redownload = true;
                    (true, Some(release))
                }
                Some(gmark_update_core::RecoveryAction::Recheck) => (true, None),
                Some(gmark_update_core::RecoveryAction::Manual) | None => (false, None),
            };
            self.state = UpdateState::Failed {
                release: state_release,
                message: format_v2_failure(&result),
                retryable,
            };
        }
        self.refresh(cx);
    }

    fn timeout_install_attempt(&mut self, cx: &mut Context<Self>) {
        let Some(attempt) = self.pending_install_v2.take() else {
            return;
        };
        let _ = write_cancellation_marker(&attempt.plan.cancellation_path);
        self.pending_install = None;
        self.prepare_claimed = false;
        self.abort_attempt(&attempt, false);
        self.retry_install = Some(RetryPayload {
            release: attempt.release.clone(),
            artifact_path: attempt.artifact_path.clone(),
        });
        self.state = UpdateState::Failed {
            release: Some(attempt.release),
            message: "update helper did not reach a terminal result within 30 seconds".to_owned(),
            retryable: true,
        };
        self.refresh(cx);
    }

    pub(super) fn apply_worker_event(&mut self, event: WorkerEvent, cx: &mut Context<Self>) {
        let is_progress = matches!(
            &event,
            WorkerEvent::Download(DownloadEvent::Progress { .. })
        );
        let Some(release) = self.state.release().cloned() else {
            return;
        };
        match event {
            WorkerEvent::Download(DownloadEvent::Started { downloaded, total })
            | WorkerEvent::Download(DownloadEvent::Progress { downloaded, total }) => {
                let (started, initial) = self
                    .progress_started_at
                    .get_or_insert((Instant::now(), downloaded));
                let elapsed = started.elapsed().as_secs_f64();
                let bytes_per_second = if elapsed >= 0.25 {
                    ((downloaded.saturating_sub(*initial)) as f64 / elapsed) as u64
                } else {
                    0
                };
                self.state = UpdateState::Downloading {
                    release,
                    downloaded,
                    total,
                    bytes_per_second,
                };
            }
            WorkerEvent::Download(DownloadEvent::Verifying) => {
                self.state = UpdateState::Verifying { release };
            }
            WorkerEvent::Download(DownloadEvent::Finished { path }) => {
                self.state = UpdateState::Ready {
                    release,
                    artifact_path: path,
                };
            }
            WorkerEvent::Download(DownloadEvent::Paused { downloaded, total }) => {
                self.state = UpdateState::Paused {
                    release,
                    downloaded,
                    total,
                };
            }
            WorkerEvent::Failed { message, retryable } => {
                self.state = UpdateState::Failed {
                    release: Some(release),
                    message,
                    retryable,
                };
            }
        }
        if !is_progress || self.last_progress_refresh.elapsed() >= Duration::from_millis(100) {
            self.last_progress_refresh = Instant::now();
            self.refresh(cx);
        }
    }

    pub(super) fn persist_successful_check(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        if std::fs::create_dir_all(&self.updates_root).is_ok() {
            let _ = std::fs::write(
                self.updates_root.join("last-successful-check"),
                format!("{now}\n"),
            );
        }
    }

    pub(super) fn refresh(&self, cx: &mut Context<Self>) {
        cx.notify();
        cx.refresh_windows();
    }
}
