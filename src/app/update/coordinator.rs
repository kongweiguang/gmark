// @author kongweiguang

//! GPUI-facing update coordinator.

use super::*;

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

    /// 未保存文档处理完并关闭一个窗口后，继续检查剩余窗口，最终退出主进程让 helper 接管。
    pub(crate) fn continue_pending_install_quit(cx: &mut App) {
        if matches!(Self::try_state(cx), Some(UpdateState::Installing { .. })) {
            crate::app_menu::request_quit_application(cx);
        }
    }
}
