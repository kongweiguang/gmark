// @author kongweiguang

//! Application-wide update coordinator.
//!
//! 多窗口共享一个权威状态与一个后台 worker；UI 只能发送命令，网络、缓存和状态转换
//! 都集中在这里，避免两个窗口同时下载或启动两次安装事务。

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::StreamExt as _;
use futures::channel::{mpsc, oneshot};
use gpui::{App, AppContext as _, AsyncApp, Context, Entity, Global, Task};
use serde::Deserialize;

use crate::net::update_v2::{
    self, CheckOrigin, CheckOutcome, DownloadControl, DownloadEvent, UpdateRelease,
};

mod protocol;

const AUTO_CHECK_DELAY: Duration = Duration::from_secs(10);
const AUTO_CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug, Default)]
pub(crate) enum UpdateState {
    #[default]
    Idle,
    Checking {
        origin: CheckOrigin,
    },
    UpToDate {
        current_version: String,
        latest_version: String,
    },
    Available(UpdateRelease),
    Downloading {
        release: UpdateRelease,
        downloaded: u64,
        total: u64,
        bytes_per_second: u64,
    },
    Paused {
        release: UpdateRelease,
        downloaded: u64,
        total: u64,
    },
    Verifying {
        release: UpdateRelease,
    },
    Ready {
        release: UpdateRelease,
        artifact_path: PathBuf,
    },
    Installing {
        release: UpdateRelease,
    },
    Succeeded {
        version: String,
        message: String,
    },
    Failed {
        release: Option<UpdateRelease>,
        message: String,
        retryable: bool,
    },
}

impl UpdateState {
    pub(crate) fn is_visible(&self) -> bool {
        !matches!(self, Self::Idle)
            && !matches!(
                self,
                Self::Checking {
                    origin: CheckOrigin::Automatic
                }
            )
    }

    pub(crate) fn release(&self) -> Option<&UpdateRelease> {
        match self {
            Self::Available(release)
            | Self::Downloading { release, .. }
            | Self::Paused { release, .. }
            | Self::Verifying { release }
            | Self::Ready { release, .. }
            | Self::Installing { release } => Some(release),
            Self::Failed { release, .. } => release.as_ref(),
            Self::Idle | Self::Checking { .. } | Self::UpToDate { .. } | Self::Succeeded { .. } => {
                None
            }
        }
    }

    /// 命令准入是状态机的纯决策层；UI 与后台事件都不能绕过同一组幂等边界。
    fn accepts(&self, command: UpdateCommand) -> bool {
        match command {
            UpdateCommand::Check => !matches!(
                self,
                Self::Checking { .. }
                    | Self::Downloading { .. }
                    | Self::Verifying { .. }
                    | Self::Installing { .. }
            ),
            UpdateCommand::Download => matches!(self, Self::Available(_)),
            UpdateCommand::Pause => matches!(self, Self::Downloading { .. }),
            UpdateCommand::Resume => matches!(self, Self::Paused { .. }),
            UpdateCommand::Retry => matches!(
                self,
                Self::Failed {
                    retryable: true,
                    ..
                }
            ),
            UpdateCommand::InstallAndRestart => matches!(self, Self::Ready { .. }),
            UpdateCommand::Dismiss => !matches!(
                self,
                Self::Downloading { .. } | Self::Verifying { .. } | Self::Installing { .. }
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateCommand {
    Check,
    Download,
    Pause,
    Resume,
    Retry,
    InstallAndRestart,
    Dismiss,
}

#[derive(Clone)]
pub(crate) struct UpdateCoordinator(Entity<UpdateService>);

impl Global for UpdateCoordinator {}

impl UpdateCoordinator {
    pub(crate) fn init(auto_check: bool, cx: &mut App) {
        let updates_root = crate::config::GmarkConfigDirs::from_system()
            .map(|dirs| dirs.updates_dir())
            .unwrap_or_else(|_| std::env::temp_dir().join("gmark-updates"));
        let service = cx.new(|_| UpdateService::new(updates_root, auto_check));
        cx.set_global(Self(service.clone()));

        let result_service = service.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            // 新进程先写启动握手，helper 随后提交结果；短轮询覆盖两者的调度竞态。
            for _ in 0..15 {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let found = result_service
                    .update(cx, |service, cx| service.refresh_apply_result(cx))
                    .unwrap_or(false);
                if found {
                    break;
                }
            }
        })
        .detach();

        if auto_check && service.read(cx).automatic_check_due() {
            cx.spawn(async move |cx: &mut AsyncApp| {
                cx.background_executor().timer(AUTO_CHECK_DELAY).await;
                let _ = service.update(cx, |service, cx| {
                    if service.auto_check_enabled {
                        service.check(CheckOrigin::Automatic, cx)
                    }
                });
            })
            .detach();
        }
    }

    pub(crate) fn entity(cx: &App) -> Entity<UpdateService> {
        cx.global::<Self>().0.clone()
    }

    pub(crate) fn state(cx: &App) -> UpdateState {
        Self::entity(cx).read(cx).state.clone()
    }

    pub(crate) fn try_state(cx: &App) -> Option<UpdateState> {
        let entity = cx
            .try_global::<Self>()
            .map(|coordinator| coordinator.0.clone())?;
        Some(entity.read(cx).state.clone())
    }

    pub(crate) fn accessibility_status(cx: &App) -> Option<String> {
        match Self::try_state(cx)? {
            UpdateState::Downloading {
                downloaded, total, ..
            } if total > 0 => Some(format!(
                "Downloading software update: {} percent",
                downloaded.saturating_mul(100) / total
            )),
            UpdateState::Verifying { .. } => Some("Verifying software update".to_owned()),
            UpdateState::Installing { .. } => Some("Installing software update".to_owned()),
            _ => None,
        }
    }

    pub(crate) fn can_self_install() -> bool {
        #[cfg(target_os = "linux")]
        {
            let Some(path) = std::env::var_os("APPIMAGE").map(PathBuf::from) else {
                return false;
            };
            return std::fs::symlink_metadata(path)
                .map(|metadata| {
                    metadata.file_type().is_file()
                        && !metadata.file_type().is_symlink()
                        && !metadata.permissions().readonly()
                })
                .unwrap_or(false);
        }
        #[cfg(not(target_os = "linux"))]
        true
    }

    pub(crate) fn check(origin: CheckOrigin, cx: &mut App) {
        let entity = Self::entity(cx);
        entity.update(cx, |service, cx| service.check(origin, cx));
    }

    pub(crate) fn download(cx: &mut App) {
        let entity = Self::entity(cx);
        entity.update(cx, |service, cx| service.download(cx));
    }

    pub(crate) fn pause(cx: &mut App) {
        let entity = Self::entity(cx);
        entity.update(cx, |service, cx| service.pause(cx));
    }

    pub(crate) fn resume(cx: &mut App) {
        let entity = Self::entity(cx);
        entity.update(cx, |service, cx| service.resume(cx));
    }

    pub(crate) fn retry(cx: &mut App) {
        let entity = Self::entity(cx);
        entity.update(cx, |service, cx| service.retry(cx));
    }

    pub(crate) fn dismiss(cx: &mut App) {
        let entity = Self::entity(cx);
        entity.update(cx, |service, cx| service.dismiss(cx));
    }

    pub(crate) fn set_auto_check(enabled: bool, cx: &mut App) {
        let Some(entity) = cx
            .try_global::<Self>()
            .map(|coordinator| coordinator.0.clone())
        else {
            return;
        };
        entity.update(cx, |service, _cx| service.auto_check_enabled = enabled);
    }

    pub(crate) fn install_and_restart(cx: &mut App) {
        let entity = Self::entity(cx);
        let prepared = entity.update(cx, |service, cx| service.prepare_install(cx));
        if prepared {
            crate::app_menu::request_quit_application(cx);
        }
    }

    /// 未保存对话框取消退出时，必须先撤销尚未发生副作用的 helper 事务。
    pub(crate) fn cancel_pending_install(cx: &mut App) {
        let Some(entity) = cx
            .try_global::<Self>()
            .map(|coordinator| coordinator.0.clone())
        else {
            return;
        };
        entity.update(cx, |service, cx| service.cancel_pending_install(cx));
    }
}

pub(crate) struct UpdateService {
    state: UpdateState,
    updates_root: PathBuf,
    worker: Option<Task<()>>,
    download_control: Option<DownloadControl>,
    progress_started_at: Option<(Instant, u64)>,
    pending_install: Option<PendingInstall>,
    auto_check_enabled: bool,
    last_progress_refresh: Instant,
}

impl UpdateService {
    fn new(updates_root: PathBuf, auto_check_enabled: bool) -> Self {
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

    fn automatic_check_due(&self) -> bool {
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

    fn refresh_apply_result(&mut self, cx: &mut Context<Self>) -> bool {
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

    fn check(&mut self, origin: CheckOrigin, cx: &mut Context<Self>) {
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

    fn download(&mut self, cx: &mut Context<Self>) {
        if !self.state.accepts(UpdateCommand::Download) {
            return;
        }
        let Some(release) = self.state.release().cloned() else {
            return;
        };
        self.start_download(release, cx);
    }

    fn start_download(&mut self, release: UpdateRelease, cx: &mut Context<Self>) {
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

    fn pause(&mut self, cx: &mut Context<Self>) {
        if self.state.accepts(UpdateCommand::Pause)
            && let Some(control) = &self.download_control
        {
            control.pause();
            self.refresh(cx);
        }
    }

    fn resume(&mut self, cx: &mut Context<Self>) {
        if !self.state.accepts(UpdateCommand::Resume) {
            return;
        }
        let Some(release) = self.state.release().cloned() else {
            return;
        };
        self.start_download(release, cx);
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
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

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if !self.state.accepts(UpdateCommand::Dismiss) {
            return;
        }
        self.state = UpdateState::Idle;
        self.refresh(cx);
    }

    fn prepare_install(&mut self, cx: &mut Context<Self>) -> bool {
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
            Ok((plan_path, helper_path, cancellation_path)) => {
                match Command::new(&helper_path)
                    .arg("--apply-plan")
                    .arg(&plan_path)
                    .spawn()
                {
                    Ok(_) => {
                        self.pending_install = Some(PendingInstall {
                            release: release.clone(),
                            artifact_path,
                            cancellation_path,
                        });
                        self.state = UpdateState::Installing { release };
                        self.refresh(cx);
                        true
                    }
                    Err(error) => {
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

    fn cancel_pending_install(&mut self, cx: &mut Context<Self>) {
        if self.restore_ready_after_cancel() {
            self.refresh(cx);
        }
    }

    fn restore_ready_after_cancel(&mut self) -> bool {
        let Some(pending) = self.pending_install.take() else {
            return false;
        };
        let _ = std::fs::write(&pending.cancellation_path, b"cancelled\n");
        self.state = UpdateState::Ready {
            release: pending.release,
            artifact_path: pending.artifact_path,
        };
        true
    }

    fn write_apply_plan(
        &self,
        release: &UpdateRelease,
        artifact_path: &std::path::Path,
    ) -> Result<(PathBuf, PathBuf, PathBuf), String> {
        use protocol::ApplyPlanV1;

        let transaction_dir = artifact_path
            .parent()
            .ok_or_else(|| "verified update has no transaction directory".to_owned())?;
        let installed_helper = installed_helper_path()?;
        let helper_name = if cfg!(windows) {
            "gmark-update-helper-copy.exe"
        } else {
            "gmark-update-helper-copy"
        };
        let helper_path = transaction_dir.join(helper_name);
        std::fs::copy(&installed_helper, &helper_path).map_err(|error| {
            format!(
                "failed to stage update helper '{}': {error}",
                installed_helper.display()
            )
        })?;
        set_executable(&helper_path)?;

        let target_path = current_update_target()?;
        let relaunch_path = current_relaunch_path(&target_path);
        let backup_path = sibling_backup_path(&target_path);
        let envelope_path = transaction_dir.join("manifest.envelope.json");
        if !envelope_path.is_file() {
            return Err("verified update manifest is missing from the cache".to_owned());
        }
        let plan_path = transaction_dir.join("apply-plan.json");
        let cancellation_path = transaction_dir.join("cancel-install");
        let acknowledgement_path = transaction_dir.join("startup-ack");
        let result_path = self.updates_root.join("last-result.json");
        let helper_log_path = self.updates_root.join("last-helper.log");
        let displayed_result_path = self.updates_root.join("last-result-displayed");
        for stale in [&result_path, &displayed_result_path] {
            let _ = std::fs::remove_file(stale);
        }
        for path in [&cancellation_path, &acknowledgement_path] {
            let _ = std::fs::remove_file(path);
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
            artifact_format: artifact_format_name(&release.artifact_format).to_owned(),
            signed_envelope_path: envelope_path,
            target_path,
            backup_path,
            relaunch_path,
            acknowledgement_path,
            cancellation_path: cancellation_path.clone(),
            result_path,
            helper_log_path,
        };
        let bytes = serde_json::to_vec_pretty(&plan)
            .map_err(|error| format!("failed to serialize update apply plan: {error}"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(transaction_dir)
            .map_err(|error| format!("failed to create update apply plan: {error}"))?;
        use std::io::Write as _;
        temporary
            .write_all(&bytes)
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|error| format!("failed to write update apply plan: {error}"))?;
        set_private(temporary.path())?;
        temporary
            .persist(&plan_path)
            .map_err(|error| format!("failed to commit update apply plan: {}", error.error))?;
        Ok((plan_path, helper_path, cancellation_path))
    }

    fn apply_worker_event(&mut self, event: WorkerEvent, cx: &mut Context<Self>) {
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

    fn persist_successful_check(&self) {
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

    fn refresh(&self, cx: &mut Context<Self>) {
        cx.notify();
        cx.refresh_windows();
    }
}

enum WorkerEvent {
    Download(DownloadEvent),
    Failed { message: String, retryable: bool },
}

#[derive(Deserialize)]
struct LastApplyResult {
    schema_version: u8,
    status: String,
    to_version: String,
    message: String,
}

fn restored_startup_state(updates_root: &std::path::Path) -> Option<UpdateState> {
    let bytes = std::fs::read(updates_root.join("last-result.json")).ok()?;
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&bytes);
    let fingerprint = format!("{:08x}\n", hasher.finalize());
    let displayed_path = updates_root.join("last-result-displayed");
    if std::fs::read_to_string(&displayed_path).ok().as_deref() == Some(fingerprint.as_str()) {
        return None;
    }
    let result: LastApplyResult = serde_json::from_slice(&bytes).ok()?;
    if result.schema_version != 1 {
        return None;
    }
    let _ = std::fs::write(displayed_path, fingerprint);
    Some(if result.status == "succeeded" {
        UpdateState::Succeeded {
            version: result.to_version,
            message: result.message,
        }
    } else {
        UpdateState::Failed {
            release: None,
            message: result.message,
            retryable: false,
        }
    })
}

fn cleanup_update_cache(updates_root: &std::path::Path) {
    const RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
    let Ok(entries) = std::fs::read_dir(updates_root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.starts_with('v') || semver::Version::parse(name.trim_start_matches('v')).is_err() {
            continue;
        }
        let stale = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= RETENTION);
        if stale {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

struct PendingInstall {
    release: UpdateRelease,
    artifact_path: PathBuf,
    cancellation_path: PathBuf,
}

fn artifact_format_name(format: &update_v2::ArtifactFormat) -> &'static str {
    match format {
        update_v2::ArtifactFormat::WindowsSetupExe => "windows-setup-exe",
        update_v2::ArtifactFormat::MacosAppTarGz => "macos-app-tar-gz",
        update_v2::ArtifactFormat::LinuxAppImage => "linux-app-image",
    }
}

fn installed_helper_path() -> Result<PathBuf, String> {
    let current = std::env::current_exe()
        .map_err(|error| format!("failed to locate current executable: {error}"))?;
    let parent = current
        .parent()
        .ok_or_else(|| "current executable has no parent directory".to_owned())?;
    let local = parent.join(if cfg!(windows) {
        "gmark-update-helper.exe"
    } else {
        "gmark-update-helper"
    });
    if local.is_file() {
        return Ok(local);
    }
    #[cfg(target_os = "macos")]
    {
        let bundled = parent.join("../Helpers/gmark-update-helper");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    #[cfg(target_os = "linux")]
    if let Some(app_dir) = std::env::var_os("APPDIR") {
        let bundled = PathBuf::from(app_dir).join("usr/lib/gmark/gmark-update-helper");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    Err("this installation does not include gmark-update-helper".to_owned())
}

fn current_update_target() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        return std::env::current_exe()
            .map_err(|error| format!("failed to locate installed gmark: {error}"));
    }
    #[cfg(target_os = "macos")]
    {
        let executable = std::env::current_exe()
            .map_err(|error| format!("failed to locate installed gmark: {error}"))?;
        return executable
            .parent()
            .and_then(|path| path.parent())
            .and_then(|path| path.parent())
            .map(std::path::Path::to_path_buf)
            .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
            .ok_or_else(|| "gmark is not running from a macOS application bundle".to_owned());
    }
    #[cfg(target_os = "linux")]
    {
        let target = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .ok_or_else(|| {
                "automatic installation is available only for AppImage; use the package manager for DEB"
                    .to_owned()
            })?;
        let metadata = std::fs::symlink_metadata(&target)
            .map_err(|error| format!("failed to inspect the current AppImage: {error}"))?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err("the current AppImage path is not a regular file".to_owned());
        }
        if metadata.permissions().readonly() {
            return Err("the current AppImage is not writable; use the release page".to_owned());
        }
        return Ok(target);
    }
    #[allow(unreachable_code)]
    Err("this platform cannot install gmark updates".to_owned())
}

fn current_relaunch_path(target: &std::path::Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    return target.join("Contents/MacOS/gmark");
    #[cfg(not(target_os = "macos"))]
    target.to_path_buf()
}

fn sibling_backup_path(target: &std::path::Path) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("gmark");
    target.with_file_name(format!("{name}.gmark-update-backup"))
}

#[cfg(unix)]
fn set_executable(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut permissions = std::fs::metadata(path)
        .map_err(|error| format!("failed to inspect staged helper: {error}"))?
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(path, permissions)
        .map_err(|error| format!("failed to secure staged helper: {error}"))
}

#[cfg(not(unix))]
fn set_executable(_path: &std::path::Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn set_private(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut permissions = std::fs::metadata(path)
        .map_err(|error| format!("failed to inspect update plan: {error}"))?
        .permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(path, permissions)
        .map_err(|error| format!("failed to secure update plan: {error}"))
}

#[cfg(not(unix))]
fn set_private(_path: &std::path::Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/updater/mod.rs"]
mod tests;
