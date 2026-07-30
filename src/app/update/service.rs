// @author kongweiguang

//! Update check, download, and install orchestration service.

use super::*;

pub(crate) struct UpdateService {
    pub(super) state: UpdateState,
    pub(super) updates_root: PathBuf,
    pub(super) worker: Option<Task<()>>,
    pub(super) download_control: Option<DownloadControl>,
    pub(super) progress_started_at: Option<(Instant, u64)>,
    pub(super) pending_install: Option<PendingInstall>,
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
            worker: None,
            download_control: None,
            progress_started_at: None,
            pending_install: None,
            auto_check_enabled,
            last_progress_refresh: Instant::now() - Duration::from_secs(1),
        }
    }

    pub(super) fn automatic_check_due(&self) -> bool {
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
        let release = match &self.state {
            UpdateState::Failed {
                release: Some(release),
                ..
            } => Some(release.clone()),
            UpdateState::Failed { release: None, .. } => None,
            _ => return,
        };
        if let Some(release) = release {
            self.start_download(release, cx);
        } else {
            self.check(CheckOrigin::Manual, cx);
        }
    }

    pub(super) fn dismiss(&mut self, cx: &mut Context<Self>) {
        if !self.state.accepts(UpdateCommand::Dismiss) {
            return;
        }
        self.state = UpdateState::Idle;
        self.refresh(cx);
    }

    pub(super) fn prepare_install(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.state.accepts(UpdateCommand::InstallAndRestart) {
            return false;
        }
        let UpdateState::Ready {
            release,
            artifact_path,
        } = self.state.clone()
        else {
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
                        self.state = UpdateState::Failed {
                            release: Some(release),
                            message,
                            retryable: false,
                        };
                        self.refresh(cx);
                        return false;
                    }
                };
                match command.spawn() {
                    Ok(_) => {
                        drop(helper_guard);
                        self.pending_install = Some(PendingInstall {
                            release: release.clone(),
                            artifact_path,
                            plan: prepared.plan,
                        });
                        self.state = UpdateState::Installing { release };
                        self.refresh(cx);
                        true
                    }
                    Err(error) => {
                        drop(helper_guard);
                        self.state = UpdateState::Failed {
                            release: Some(release),
                            message: format!("failed to start update helper: {error}"),
                            retryable: true,
                        };
                        self.refresh(cx);
                        false
                    }
                }
            }
            Err(message) => {
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
        let pending = self
            .pending_install
            .take()
            .expect("pending install was checked above");
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
        let transaction_dir = artifact_path
            .parent()
            .ok_or_else(|| "verified update has no transaction directory".to_owned())?;
        if transaction_dir.parent() != Some(self.updates_root.as_path()) {
            return Err("verified update is outside the configured transaction root".to_owned());
        }
        let installed_helper = installed_helper_path()?;

        let target_path = current_update_target()?;
        let relaunch_path = current_relaunch_path(&target_path);
        let backup_path = sibling_backup_path(&target_path);
        let envelope_path = transaction_dir.join("manifest.envelope.json");
        if !envelope_path.is_file() {
            return Err("verified update manifest is missing from the cache".to_owned());
        }
        let plan_path = transaction_dir.join("apply-plan.json");
        let result_path = self.updates_root.join("last-result.json");
        let helper_log_path = self.updates_root.join("last-helper.log");
        let displayed_result_path = self.updates_root.join("last-result-displayed");
        for stale in [&result_path, &displayed_result_path] {
            let _ = std::fs::remove_file(stale);
        }
        let plan = ApplyPlanV1 {
            schema_version: ApplyPlanV1::SCHEMA_VERSION,
            parent_pid: std::process::id(),
            current_version: release.current_version.clone(),
            target_version: release.version.clone(),
            artifact_path: artifact_path.to_path_buf(),
            artifact_url: release.artifact_url.clone(),
            artifact_size: release.artifact_size,
            artifact_sha256: release.artifact_sha256.clone(),
            artifact_format: release.artifact_format.as_protocol_name().to_owned(),
            signed_envelope_path: envelope_path,
            target_path,
            backup_path,
            relaunch_path,
            acknowledgement_path: transaction_dir.join("startup-ack"),
            cancellation_path: transaction_dir.join("cancel-install"),
            result_path,
            helper_log_path,
        };
        for signal in [
            HelperSignalV1::Cancellation,
            HelperSignalV1::Acknowledgement,
        ] {
            clear_helper_signal(&plan, signal)
                .map_err(|error| format!("failed to clear stale update helper signal: {error}"))?;
        }
        let acknowledgement_capability = create_acknowledgement_capability(transaction_dir)?;
        write_apply_plan(&plan_path, &plan)
            .map_err(|error| format!("failed to write update apply plan: {error}"))?;
        let helper = stage_update_helper(transaction_dir, &installed_helper)?;
        Ok(PreparedInstall {
            plan_path,
            helper,
            plan,
            acknowledgement_capability,
        })
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
