// @author kongweiguang

//! Update check, download, and install orchestration service.

use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

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
    /// Monotonically separates callbacks from superseded restore, download, and staging work.
    pub(super) generation: u64,
    /// Metadata needed for the post-approval wait handoff; no active manager is retained here.
    pub(super) staged_install: Option<super::velopack::StagedInstall>,
    /// Keeps a verified source available after a staging failure or explicit cancellation.
    pub(super) ready_source: Option<(UpdateRelease, PathBuf)>,
    /// Allows UI cancellation to invalidate a worker even when Velopack is inside a copy call.
    pub(super) staging_cancel: Option<Arc<AtomicBool>>,
}

impl UpdateService {
    /// Keeps construction O(1) so the first window can render before cache cleanup or package hashing.
    pub(super) fn new(updates_root: PathBuf, auto_check_enabled: bool) -> Self {
        Self {
            // 恢复会读取并哈希缓存包，必须由 coordinator 在实体发布后放到后台线程。
            state: UpdateState::Restoring,
            updates_root,
            available: true,
            worker: None,
            download_control: None,
            progress_started_at: None,
            retry_redownload: false,
            auto_check_enabled,
            last_progress_refresh: Instant::now() - Duration::from_secs(1),
            generation: 0,
            staged_install: None,
            ready_source: None,
            staging_cancel: None,
        }
    }

    /// Preserves an explicit unavailable state without creating a fallback cache or worker.
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
            generation: 0,
            staged_install: None,
            ready_source: None,
            staging_cancel: None,
        }
    }

    /// Restores verified cache state and performs retention cleanup only after the first app frame.
    pub(super) fn start_restore(&mut self, cx: &mut Context<Self>) {
        if !self.available || !matches!(self.state, UpdateState::Restoring) || self.worker.is_some()
        {
            return;
        }
        let updates_root = self.updates_root.clone();
        let generation = self.generation;
        let (sender, receiver) = oneshot::channel();
        std::thread::spawn(move || {
            cleanup_update_cache(&updates_root);
            let restored =
                update_v2::restore_ready_release(&updates_root, env!("CARGO_PKG_VERSION"));
            let _ = sender.send(restored);
        });
        self.worker = Some(cx.spawn(async move |this, cx| {
            let restored = receiver.await.unwrap_or(None);
            let _ = this.update(cx, |service, cx| {
                if service.generation != generation
                    || !matches!(service.state, UpdateState::Restoring)
                {
                    return;
                }
                service.worker = None;
                if let Some((release, artifact_path)) = restored {
                    service.ready_source = Some((release.clone(), artifact_path.clone()));
                    service.state = UpdateState::Ready {
                        release,
                        artifact_path,
                    };
                } else {
                    service.state = UpdateState::Idle;
                }
                service.refresh(cx);
                // The coordinator's initial timer may have fired while restoration was still
                // busy; retry the due automatic check here so a large cache cannot suppress the
                // daily update check forever.
                if service.auto_check_enabled
                    && matches!(service.state, UpdateState::Idle)
                    && service.automatic_check_due()
                {
                    service.check(CheckOrigin::Automatic, cx);
                }
            });
        }));
    }

    /// Invalidates callbacks from older workers so cancellation and retries cannot resurrect stale state.
    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
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
        self.worker.take();
        self.next_generation();
        self.staging_cancel = None;
        self.download_control = None;
        self.ready_source = None;
        self.staged_install = None;
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

    /// Runs network checks outside GPUI and applies a result only to the generation that requested it.
    pub(super) fn check(&mut self, origin: CheckOrigin, cx: &mut Context<Self>) {
        if !self.available {
            return;
        }
        if !self.state.accepts(UpdateCommand::Check) {
            return;
        }
        self.worker.take();
        let generation = self.next_generation();
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
                if service.generation != generation
                    || !matches!(service.state, UpdateState::Checking { .. })
                {
                    return;
                }
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

    /// Keeps the existing resumable download worker off the UI thread and isolates each retry generation.
    pub(super) fn start_download(&mut self, release: UpdateRelease, cx: &mut Context<Self>) {
        self.worker.take();
        let generation = self.next_generation();
        self.retry_redownload = false;
        self.staged_install = None;
        self.ready_source = None;
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
                let stale = this
                    .update(cx, |service, cx| {
                        if service.generation != generation {
                            return true;
                        }
                        service.apply_worker_event(event, cx);
                        if terminal {
                            service.worker = None;
                            service.download_control = None;
                        }
                        false
                    })
                    .unwrap_or(true);
                if stale {
                    break;
                }
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
            if let Some(staged) = self.staged_install.take()
                && staged.release.version == release.version
            {
                self.ready_source = Some((staged.release.clone(), staged.artifact_path.clone()));
                self.state = UpdateState::Staged {
                    release: staged.release,
                    artifact_path: staged.artifact_path,
                };
                self.refresh(cx);
                return;
            }
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
            } else if let Some((cached_release, artifact_path)) = self.ready_source.take()
                && cached_release.version == release.version
            {
                // A staging failure retries the already verified source; hashing it again here
                // would reintroduce the UI stall that startup restoration was moved to a worker to avoid.
                self.start_staging(cached_release, artifact_path, cx);
                return;
            }
            self.start_download(release, cx);
        } else {
            self.check(CheckOrigin::Manual, cx);
        }
    }

    /// Hides a completed or failed update while dropping staged Velopack metadata from the UI state.
    pub(super) fn dismiss(&mut self, cx: &mut Context<Self>) {
        if !self.available {
            return;
        }
        if !self.state.accepts(UpdateCommand::Dismiss) {
            return;
        }
        self.state = UpdateState::Idle;
        self.staged_install = None;
        self.ready_source = None;
        self.refresh(cx);
    }

    /// Starts local Velopack staging and defers the quit approval until the package copy succeeds.
    pub(super) fn stage_install(&mut self, cx: &mut Context<Self>) {
        let payload = match self.state.clone() {
            UpdateState::Ready {
                release,
                artifact_path,
            } => Some((release, artifact_path)),
            UpdateState::Staged { .. } => {
                // A second click must only re-enter the quit flow; staging is already complete.
                let _ = crate::app_menu::request_update_quit_application(cx);
                None
            }
            _ => None,
        };
        let Some((release, artifact_path)) = payload else {
            return;
        };
        self.start_staging(release, artifact_path, cx);
    }

    /// Invalidates a staging generation and restores the verified source as a retryable Ready state.
    pub(super) fn cancel_staging(&mut self, cx: &mut Context<Self>) {
        let UpdateState::StagingInstall {
            release,
            artifact_path,
        } = self.state.clone()
        else {
            return;
        };
        if let Some(cancel) = &self.staging_cancel {
            cancel.store(true, Ordering::Release);
        }
        self.worker.take();
        self.staging_cancel = None;
        self.download_control = None;
        self.next_generation();
        self.ready_source = Some((release.clone(), artifact_path.clone()));
        self.staged_install = None;
        self.state = UpdateState::Ready {
            release,
            artifact_path,
        };
        self.refresh(cx);
    }

    /// Runs the Velopack package copy on a worker and asks the normal quit coordinator to approve
    /// all windows only after the local package cache is ready.
    fn start_staging(
        &mut self,
        release: UpdateRelease,
        artifact_path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.worker.take();
        let generation = self.next_generation();
        let cancel = Arc::new(AtomicBool::new(false));
        self.staging_cancel = Some(cancel.clone());
        self.download_control = None;
        self.ready_source = Some((release.clone(), artifact_path.clone()));
        self.staged_install = None;
        self.state = UpdateState::StagingInstall {
            release: release.clone(),
            artifact_path: artifact_path.clone(),
        };
        self.refresh(cx);

        let (sender, mut receiver) = mpsc::unbounded::<WorkerEvent>();
        std::thread::spawn(move || {
            if cancel.load(Ordering::Acquire) {
                return;
            }
            match super::velopack::stage_install(&release, &artifact_path) {
                Ok(staged) if !cancel.load(Ordering::Acquire) => {
                    let _ = sender.unbounded_send(WorkerEvent::Staged(Box::new(staged)));
                }
                Ok(_) => {}
                Err(message) if !cancel.load(Ordering::Acquire) => {
                    let _ = sender.unbounded_send(WorkerEvent::StageFailed { release, message });
                }
                Err(_) => {}
            }
        });
        self.worker = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = receiver.next().await {
                let stop = this
                    .update(cx, |service, cx| {
                        if service.generation != generation {
                            return true;
                        }
                        service.worker = None;
                        service.staging_cancel = None;
                        match event {
                            WorkerEvent::Staged(staged) => {
                                service.ready_source =
                                    Some((staged.release.clone(), staged.artifact_path.clone()));
                                service.state = UpdateState::Staged {
                                    release: staged.release.clone(),
                                    artifact_path: staged.artifact_path.clone(),
                                };
                                service.staged_install = Some(*staged);
                                service.refresh(cx);
                                // This is the second quit request: the first possible approval
                                // cannot be reused because staging may have taken user-visible time.
                                let _ = crate::app_menu::request_update_quit_application(cx);
                            }
                            WorkerEvent::StageFailed { release, message } => {
                                service.state = UpdateState::Failed {
                                    release: Some(release),
                                    message: format!("{message}；可重试本地安装准备"),
                                    retryable: true,
                                };
                                service.refresh(cx);
                            }
                            _ => {}
                        }
                        false
                    })
                    .unwrap_or(true);
                if stop {
                    break;
                }
            }
            let _ = this.update(cx, |service, cx| {
                if service.generation == generation {
                    service.worker = None;
                    service.staging_cancel = None;
                    let release = match &service.state {
                        UpdateState::StagingInstall { release, .. } => Some(release.clone()),
                        _ => None,
                    };
                    if let Some(release) = release {
                        service.state = UpdateState::Failed {
                            release: Some(release),
                            message: "安装准备工作线程意外结束；可重试本地安装准备".to_owned(),
                            retryable: true,
                        };
                        service.refresh(cx);
                    }
                }
            });
        }));
    }

    /// Starts only Velopack's exit waiter after the latest quit evaluation has approved every
    /// window; a handoff error remains retryable and never asks the application to quit.
    pub(super) fn handoff_install_after_quit_approval(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(staged) = self.staged_install.clone() else {
            return false;
        };
        if !matches!(self.state, UpdateState::Staged { .. }) {
            return false;
        }
        match super::velopack::handoff_staged_install(&staged) {
            Ok(()) => true,
            Err(message) => {
                self.state = UpdateState::Failed {
                    release: Some(staged.release.clone()),
                    message: format!("{message}；可重试退出安装"),
                    retryable: true,
                };
                self.refresh(cx);
                false
            }
        }
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
            WorkerEvent::Staged(_) | WorkerEvent::StageFailed { .. } => return,
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
