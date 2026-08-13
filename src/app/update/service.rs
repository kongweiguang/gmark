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
    pub(super) retry_redownload: bool,
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
            retry_redownload: false,
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
            retry_redownload: false,
            auto_check_enabled,
            last_progress_refresh: Instant::now() - Duration::from_secs(1),
        }
    }

    /// Applies only the terminal result for the transaction that launched this
    /// process, avoiding the old process-side install monitor and unrelated
    /// cache scans while still making post-ack failures visible to the user.
    pub(super) fn refresh_relaunched_transaction(
        &mut self,
        transaction_dir: &std::path::Path,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = restored_transaction_state(transaction_dir) else {
            return false;
        };
        self.state = state;
        self.refresh(cx);
        true
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

    /// Reuses a still-valid verified cache entry after local handoff failure;
    /// only an explicit redownload discards the signed source payload.
    pub(super) fn retry(&mut self, cx: &mut Context<Self>) {
        if !self.state.accepts(UpdateCommand::Retry) {
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
            } else if let Some((cached_release, artifact_path)) =
                update_v2::restore_ready_release(&self.updates_root, env!("CARGO_PKG_VERSION"))
                && cached_release.version == release.version
            {
                // A helper-start failure must retry the verified bytes already
                // on disk; downloading again would turn a local recovery into
                // an unnecessary network operation.
                self.state = UpdateState::Ready {
                    release: cached_release,
                    artifact_path,
                };
                self.refresh(cx);
                return;
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

    /// Creates and launches exactly one helper after `QuitCoordinator` has
    /// approved every window, so a veto never leaves a lock or helper behind.
    pub(super) fn prepare_install(&mut self, cx: &mut Context<Self>) -> bool {
        if !matches!(self.state, UpdateState::Ready { .. }) {
            return false;
        }
        let payload = match self.state.clone() {
            UpdateState::Ready {
                release,
                artifact_path,
            } => Some((release, artifact_path)),
            _ => None,
        };
        let Some((release, artifact_path)) = payload else {
            return false;
        };
        match self.write_apply_plan(&release, &artifact_path) {
            Ok(prepared) => {
                let mut command = Command::new(&prepared.helper.path);
                command.arg("--apply-plan").arg(&prepared.plan_path);
                command.env(
                    UPDATE_ACK_CAPABILITY_ENV,
                    &prepared.acknowledgement_capability,
                );
                #[cfg(not(target_os = "windows"))]
                if let Some(agent) = &prepared.agent {
                    command.env("GMARK_UPDATE_AGENT_PATH", &agent.path);
                }
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
                        self.abort_prepared_install(&prepared);
                        self.state = UpdateState::Failed {
                            release: Some(release.clone()),
                            message: self.format_handoff_failure(
                                &release,
                                &message,
                                Some(&prepared.plan_v2),
                            ),
                            retryable: true,
                        };
                        self.refresh(cx);
                        return false;
                    }
                };
                #[cfg(not(target_os = "windows"))]
                let agent_guard = match prepared.agent.as_ref() {
                    Some(agent) => match verify_staged_helper_for_launch(agent) {
                        Ok(guard) => Some(guard),
                        Err(message) => {
                            drop(helper_guard);
                            self.abort_prepared_install(&prepared);
                            self.state = UpdateState::Failed {
                                release: Some(release.clone()),
                                message: self.format_handoff_failure(
                                    &release,
                                    &message,
                                    Some(&prepared.plan_v2),
                                ),
                                retryable: true,
                            };
                            self.refresh(cx);
                            return false;
                        }
                    },
                    None => {
                        drop(helper_guard);
                        self.abort_prepared_install(&prepared);
                        self.state = UpdateState::Failed {
                            release: Some(release.clone()),
                            message: self.format_handoff_failure(
                                &release,
                                "update feedback agent was not staged",
                                Some(&prepared.plan_v2),
                            ),
                            retryable: true,
                        };
                        self.refresh(cx);
                        return false;
                    }
                };
                match command.spawn() {
                    Ok(_) => {
                        drop(helper_guard);
                        #[cfg(not(target_os = "windows"))]
                        drop(agent_guard);
                        // The process-static lifecycle lock intentionally
                        // remains registered until process exit; the helper is
                        // now the only owner able to proceed after that exit.
                        true
                    }
                    Err(error) => {
                        drop(helper_guard);
                        #[cfg(not(target_os = "windows"))]
                        drop(agent_guard);
                        self.abort_prepared_install(&prepared);
                        self.state = UpdateState::Failed {
                            release: Some(release.clone()),
                            message: self.format_handoff_failure(
                                &release,
                                &format!("failed to start update helper: {error}"),
                                Some(&prepared.plan_v2),
                            ),
                            retryable: true,
                        };
                        self.refresh(cx);
                        false
                    }
                }
            }
            Err(message) => {
                self.state = UpdateState::Failed {
                    release: Some(release.clone()),
                    message: self.format_handoff_failure(&release, &message, None),
                    retryable: true,
                };
                self.refresh(cx);
                false
            }
        }
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

    fn abort_prepared_install(&mut self, prepared: &PreparedInstall) {
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

    /// Adds copyable recovery destinations to handoff failures because the
    /// helper may never get far enough to persist its own terminal result.
    fn format_handoff_failure(
        &self,
        release: &UpdateRelease,
        detail: &str,
        plan: Option<&gmark_update_core::ApplyPlanV2>,
    ) -> String {
        let manual_url = if release.artifact_url.is_empty() {
            "https://github.com/kongweiguang/gmark/releases"
        } else {
            release.artifact_url.as_str()
        };
        let (helper_log, installer_log) = plan
            .map(|plan| {
                (
                    plan.helper_log_path.display().to_string(),
                    plan.installer_log_path.display().to_string(),
                )
            })
            .unwrap_or_else(|| {
                (
                    "<transaction>/helper.log".to_owned(),
                    "<transaction>/installer.log".to_owned(),
                )
            });
        format!(
            "{detail}; manual download: {manual_url}; helper log: {helper_log}; installer log: {installer_log}"
        )
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
