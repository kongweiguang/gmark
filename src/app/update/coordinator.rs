// @author kongweiguang

//! GPUI-facing update coordinator.

use super::*;

#[derive(Clone)]

pub(crate) struct UpdateCoordinator(Entity<UpdateService>);

impl Global for UpdateCoordinator {}

impl UpdateCoordinator {
    /// Publishes the updater without monitoring installation in the old
    /// process; a relaunched process may observe only its own terminal result.
    pub(crate) fn init(auto_check: bool, relaunched_transaction: Option<PathBuf>, cx: &mut App) {
        let updates_root = match update_cache_root() {
            Ok(root) => root,
            Err(error) => {
                let service = cx.new(|_| {
                    UpdateService::new_unavailable(
                        format!("software updater is unavailable: {error:#}"),
                        auto_check,
                    )
                });
                cx.set_global(Self(service));
                return;
            }
        };
        #[cfg(feature = "updater-e2e")]
        let e2e_failure = std::env::var_os("GMARK_UPDATER_E2E_FAILURE")
            .and_then(|value| value.into_string().ok())
            .filter(|value| !value.is_empty());
        #[cfg(feature = "updater-e2e")]
        let has_e2e_failure = e2e_failure.is_some();
        let service = cx.new(move |_| {
            let service = UpdateService::new(updates_root, auto_check);
            #[cfg(feature = "updater-e2e")]
            {
                let mut service = service;
                if let Some(message) = e2e_failure {
                    // 首帧前写入确定性失败，避免视觉验收依赖一次额外输入或后台调度时序。
                    service.state = UpdateState::Failed {
                        release: None,
                        message,
                        retryable: false,
                    };
                }
                service
            }
            #[cfg(not(feature = "updater-e2e"))]
            service
        });
        cx.set_global(Self(service.clone()));

        if let Some(transaction_dir) = relaunched_transaction {
            let result_service = service.clone();
            cx.spawn(async move |cx: &mut AsyncApp| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(200))
                        .await;
                    let found = result_service
                        .update(cx, |service, cx| {
                            service.refresh_relaunched_transaction(&transaction_dir, cx)
                        })
                        .unwrap_or(false);
                    if found {
                        break;
                    }
                }
            })
            .detach();
        }

        // 确定性失败场景必须保持到验收驱动完成操作；自动检查不能在十秒后把面板重置为 Idle。
        #[cfg(feature = "updater-e2e")]
        let automatic_check_allowed = !has_e2e_failure;
        #[cfg(not(feature = "updater-e2e"))]
        let automatic_check_allowed = true;
        if auto_check && automatic_check_allowed && service.read(cx).automatic_check_due() {
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

    /// Delegates approval to the normal quit flow so dirty windows can veto
    /// without creating a helper transaction or consuming the ready artifact.
    pub(crate) fn install_and_restart(cx: &mut App) {
        let Some(entity) = cx
            .try_global::<Self>()
            .map(|coordinator| coordinator.0.clone())
        else {
            return;
        };
        if !entity.update(cx, |service, _cx| {
            service.state.accepts(UpdateCommand::InstallAndRestart)
        }) {
            return;
        }
        // The quit coordinator performs the normal multi-window save/discard
        // flow first; only its approved handoff callback may create a plan.
        let _ = crate::app_menu::request_update_quit_application(cx);
    }

    /// Commits the V2 helper handoff after the quit coordinator has approved
    /// every editor window.  Keeping this narrow adapter here lets the quit
    /// lifecycle remain independent from the update service implementation;
    /// the service still owns plan validation and helper startup.
    pub(crate) fn handoff_install_after_quit_approval(cx: &mut App) -> bool {
        let Some(entity) = cx
            .try_global::<Self>()
            .map(|coordinator| coordinator.0.clone())
        else {
            return false;
        };
        entity.update(cx, |service, cx| service.prepare_install(cx))
    }

    /// 保留旧编辑器回调的兼容入口；退出意图本身由 QuitCoordinator 撤销，
    /// 因而这里不再维护或取消 updater 镜像事务。
    pub(crate) fn cancel_pending_install(cx: &mut App) {
        let _ = cx;
    }
}
