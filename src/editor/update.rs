// @author kongweiguang

//! Editor presentation and commands for the application-wide updater.

use gmark_update_core::SystemTrust;
use gpui::*;

use super::Editor;
use crate::i18n::I18nManager;
use crate::net::update_v2::CheckOrigin;
use crate::theme::Theme;
use crate::updater::{UpdateCoordinator, UpdateState};

type UpdateClickHandler = fn(&mut Editor, &ClickEvent, &mut Window, &mut Context<Editor>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdateAccessibilityAction {
    pub(crate) id: &'static str,
    pub(crate) label: String,
}

impl Editor {
    pub(crate) fn request_check_updates(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        UpdateCoordinator::check(CheckOrigin::Manual, cx);
    }

    fn on_update_download(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        UpdateCoordinator::download(cx);
    }

    fn on_update_pause(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        UpdateCoordinator::pause(cx);
    }

    fn on_update_retry(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        UpdateCoordinator::retry(cx);
    }

    fn on_update_resume(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        UpdateCoordinator::resume(cx);
    }

    fn on_update_install(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        UpdateCoordinator::install_and_restart(cx);
    }

    fn on_update_dismiss(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        UpdateCoordinator::dismiss(cx);
    }

    /// 复制完整诊断后再关闭弹层，让用户能把长路径和 helper 错误交给支持人员而不依赖文本选择能力。
    fn on_update_copy_and_dismiss(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_update_failure_and_dismiss(cx);
    }

    /// 键盘与鼠标共用同一复制路径，避免两种入口对是否关闭弹层产生不同语义。
    fn copy_update_failure_and_dismiss(&mut self, cx: &mut Context<Self>) {
        if let UpdateState::Failed { message, .. } = UpdateCoordinator::state(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(message));
        }
        UpdateCoordinator::dismiss(cx);
    }

    fn on_update_open_manual_download(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.open_url(manual_update_url(&UpdateCoordinator::state(cx)));
    }

    fn on_update_open_release(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_update_button("open-update-release", cx);
    }

    fn activate_update_button(&mut self, id: &str, cx: &mut Context<Self>) {
        match id {
            "download-update" => UpdateCoordinator::download(cx),
            "pause-update" => UpdateCoordinator::pause(cx),
            "resume-update" => UpdateCoordinator::resume(cx),
            "retry-update" => UpdateCoordinator::retry(cx),
            "install-update" => UpdateCoordinator::install_and_restart(cx),
            "dismiss-update" | "later-update" => UpdateCoordinator::dismiss(cx),
            "copy-update-error" => self.copy_update_failure_and_dismiss(cx),
            "open-update-release" => {
                if let Some(release) = UpdateCoordinator::state(cx).release() {
                    cx.open_url(&release.release_url);
                }
            }
            "open-update-manual-download" => {
                cx.open_url(manual_update_url(&UpdateCoordinator::state(cx)));
            }
            _ => {}
        }
    }

    /// 从权威状态生成与可见按钮相同的可访问性动作，避免自动化和辅助技术点击隐藏的测试专用入口。
    pub(crate) fn update_accessibility_actions(&self, cx: &App) -> Vec<UpdateAccessibilityAction> {
        let Some(state) = UpdateCoordinator::try_state(cx).filter(UpdateState::is_visible) else {
            return Vec::new();
        };
        update_action_descriptors(&state, &UpdateLabels::for_app(cx))
    }

    /// 可访问性树只分主次两个稳定槽位；动作执行仍复用正常 UI 命令而不复制 updater 状态转换。
    pub(crate) fn activate_update_accessibility_action(
        &mut self,
        slot: usize,
        cx: &mut Context<Self>,
    ) {
        let actions = self.update_accessibility_actions(cx);
        let Some(action) = actions.get(slot) else {
            return;
        };
        self.activate_update_button(action.id, cx);
    }

    pub(crate) fn handle_update_panel_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = UpdateCoordinator::try_state(cx).filter(UpdateState::is_visible) else {
            return false;
        };
        if event.keystroke.key == "escape" {
            UpdateCoordinator::dismiss(cx);
            return true;
        }
        if event.keystroke.key != "tab"
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.function
        {
            return false;
        }

        let (secondary, primary) = update_button_slots(&state);
        let mut handles = Vec::with_capacity(2);
        if secondary {
            handles.push(self.update_secondary_focus_handle.clone());
        }
        if primary {
            handles.push(self.update_primary_focus_handle.clone());
        }
        if handles.is_empty() {
            return false;
        }
        let current = handles.iter().position(|handle| handle.is_focused(window));
        let next = if event.keystroke.modifiers.shift {
            current.map_or(handles.len() - 1, |index| {
                index.checked_sub(1).unwrap_or(handles.len() - 1)
            })
        } else {
            current.map_or(0, |index| (index + 1) % handles.len())
        };
        handles[next].focus(window);
        true
    }

    pub(super) fn render_update_panel(
        &self,
        theme: &Theme,
        bottom_offset: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = UpdateCoordinator::try_state(cx)?;
        if !state.is_visible() {
            return None;
        }
        let labels = UpdateLabels::for_app(cx);
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;

        let mut title = labels.title.to_owned();
        let mut detail: String;
        let mut progress = None;
        let mut primary: Option<(&'static str, String, UpdateClickHandler)> = None;
        let mut secondary: Option<(&'static str, String, UpdateClickHandler)> = None;

        match &state {
            UpdateState::Checking { .. } => detail = labels.checking.to_owned(),
            UpdateState::UpToDate {
                current_version,
                latest_version,
            } => {
                title = labels.up_to_date.to_owned();
                detail = format!("v{current_version} · v{latest_version}");
                primary = Some((
                    "dismiss-update",
                    labels.ok.to_owned(),
                    Self::on_update_dismiss,
                ));
            }
            UpdateState::Available(release) => {
                detail = format!("v{}\n{}", release.version, release.notes.trim());
                if release.system_trust == SystemTrust::Unsigned {
                    detail.push_str(labels.unsigned_warning);
                }
                if UpdateCoordinator::can_self_install() {
                    primary = Some((
                        "download-update",
                        labels.download.to_owned(),
                        Self::on_update_download,
                    ));
                } else {
                    detail.push_str(labels.package_manager_guidance);
                    primary = Some((
                        "open-update-release",
                        labels.open_release.to_owned(),
                        Self::on_update_open_release,
                    ));
                }
                secondary = Some((
                    "later-update",
                    labels.later.to_owned(),
                    Self::on_update_dismiss,
                ));
            }
            UpdateState::Downloading {
                release,
                downloaded,
                total,
                bytes_per_second,
            } => {
                title = format!("{} v{}", labels.downloading, release.version);
                let fraction = if *total == 0 {
                    0.0
                } else {
                    (*downloaded as f32 / *total as f32).clamp(0.0, 1.0)
                };
                progress = Some(fraction);
                detail = format!(
                    "{} / {} · {}/s · {:.0}%",
                    format_bytes(*downloaded),
                    format_bytes(*total),
                    format_bytes(*bytes_per_second),
                    fraction * 100.0
                );
                secondary = Some((
                    "pause-update",
                    labels.pause.to_owned(),
                    Self::on_update_pause,
                ));
            }
            UpdateState::Paused {
                release,
                downloaded,
                total,
            } => {
                title = format!("{} v{}", labels.paused, release.version);
                let fraction = if *total == 0 {
                    0.0
                } else {
                    (*downloaded as f32 / *total as f32).clamp(0.0, 1.0)
                };
                progress = Some(fraction);
                detail = format!("{} / {}", format_bytes(*downloaded), format_bytes(*total));
                primary = Some((
                    "resume-update",
                    labels.resume.to_owned(),
                    Self::on_update_resume,
                ));
                secondary = Some((
                    "later-update",
                    labels.later.to_owned(),
                    Self::on_update_dismiss,
                ));
            }
            UpdateState::Verifying { release } => {
                title = format!("{} v{}", labels.verifying, release.version);
                detail = labels.verifying_detail.to_owned();
                progress = Some(1.0);
            }
            UpdateState::Ready { release, .. } => {
                title = format!("{} v{}", labels.ready, release.version);
                detail = labels.ready_detail.to_owned();
                if release.system_trust == SystemTrust::Unsigned {
                    detail.push_str(labels.unsigned_warning);
                }
                progress = Some(1.0);
                primary = Some((
                    "install-update",
                    labels.restart_install.to_owned(),
                    Self::on_update_install,
                ));
                secondary = Some((
                    "later-update",
                    labels.later.to_owned(),
                    Self::on_update_dismiss,
                ));
            }
            UpdateState::Succeeded { version, message } => {
                title = format!("{} v{}", labels.updated, version);
                detail = message.clone();
                progress = Some(1.0);
                primary = Some((
                    "dismiss-update",
                    labels.ok.to_owned(),
                    Self::on_update_dismiss,
                ));
            }
            UpdateState::Failed {
                message,
                retryable,
                release,
            } => {
                title = labels.failed.to_owned();
                detail = message.clone();
                if *retryable {
                    primary = Some((
                        "retry-update",
                        labels.retry.to_owned(),
                        Self::on_update_retry,
                    ));
                    secondary = Some((
                        "copy-update-error",
                        labels.copy_close.to_owned(),
                        Self::on_update_copy_and_dismiss,
                    ));
                } else if release.is_some() || detail.contains("manual download:") {
                    primary = Some((
                        "open-update-manual-download",
                        labels.open_release.to_owned(),
                        Self::on_update_open_manual_download,
                    ));
                    secondary = Some((
                        "copy-update-error",
                        labels.copy_close.to_owned(),
                        Self::on_update_copy_and_dismiss,
                    ));
                } else {
                    secondary = Some((
                        "copy-update-error",
                        labels.copy_close.to_owned(),
                        Self::on_update_copy_and_dismiss,
                    ));
                }
            }
            UpdateState::Idle => return None,
        }

        let button = |id: &'static str,
                      label: String,
                      primary: bool,
                      focus_handle: &FocusHandle,
                      cx: &mut Context<Self>| {
            let wb = &c.workbench;
            let (background, hover, text) = if primary {
                (wb.accent, wb.accent_hover, wb.text_inverse)
            } else {
                (wb.control_surface, wb.control_hover, wb.text_primary)
            };
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .h(px(28.0))
                .px(px(10.0))
                .tab_index(0)
                .track_focus(focus_handle)
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.0))
                .border(px(d.dialog_border_width))
                .border_color(background)
                .bg(background)
                .hover(move |this| this.bg(hover))
                .focus(move |this| this.border_color(c.workbench.focus_ring))
                .active(|this| this.opacity(0.9))
                .cursor_pointer()
                .whitespace_nowrap()
                .text_size(px(t.dialog_button_size))
                .text_color(text)
                .child(label)
                .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        this.activate_update_button(id, cx);
                        cx.stop_propagation();
                    }
                }))
        };

        let mut actions = div().flex().items_center().justify_end().gap(px(8.0));
        if let Some((id, label, handler)) = secondary {
            let control = button(id, label, false, &self.update_secondary_focus_handle, cx)
                .on_click(cx.listener(handler));
            actions = actions.child(control);
        }
        if let Some((id, label, handler)) = primary {
            let control = button(id, label, true, &self.update_primary_focus_handle, cx)
                .on_click(cx.listener(handler));
            actions = actions.child(control);
        }

        let panel = div()
            .id("update-panel")
            .debug_selector(|| "update-panel".to_owned())
            .absolute()
            .right(px(12.0))
            .bottom(px(bottom_offset + 10.0))
            .w(px(380.0))
            .max_w(relative(0.92))
            .p(px(14.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .rounded(px(12.0))
            .border(px(d.dialog_border_width))
            .border_color(c.workbench.border_subtle)
            .bg(c.workbench.glass_strong_surface)
            .shadow_lg()
            .child(
                div()
                    .text_size(px(t.dialog_title_size))
                    .font_weight(t.dialog_title_weight.to_font_weight())
                    .text_color(c.workbench.text_primary)
                    .child(title),
            )
            .child(
                div()
                    .id("update-detail-scroll")
                    .debug_selector(|| "update-detail-scroll".to_owned())
                    // 发布说明和系统信任提示都属于关键更新信息；长内容应可滚动，不能静默裁掉。
                    .max_h(px(220.0))
                    .overflow_y_scroll()
                    // 错误文本含日志路径和手动入口，换行后才能在窄窗口完整复制阅读。
                    .whitespace_normal()
                    .text_size(px(t.dialog_body_size))
                    .line_height(rems(t.text_line_height))
                    .text_color(c.workbench.text_secondary)
                    .children(detail.lines().map(|line| div().child(line.to_owned()))),
            );
        let panel = if let Some(fraction) = progress {
            panel.child(
                div()
                    .id("update-progress-track")
                    .debug_selector(|| "update-progress-track".to_owned())
                    .w_full()
                    .h(px(6.0))
                    .overflow_hidden()
                    .rounded(px(999.0))
                    .bg(c.workbench.control_surface)
                    .child(
                        div()
                            .id("update-progress-fill")
                            .debug_selector(|| "update-progress-fill".to_owned())
                            .h_full()
                            .w(relative(fraction))
                            .rounded(px(999.0))
                            .bg(c.workbench.accent),
                    ),
            )
        } else {
            panel
        };
        Some(panel.child(actions).into_any_element())
    }
}

fn update_button_slots(state: &UpdateState) -> (bool, bool) {
    match state {
        UpdateState::UpToDate { .. } | UpdateState::Succeeded { .. } => (false, true),
        UpdateState::Available(_) | UpdateState::Paused { .. } | UpdateState::Ready { .. } => {
            (true, true)
        }
        UpdateState::Downloading { .. } => (true, false),
        UpdateState::Failed {
            message,
            retryable,
            release,
        } => (
            true,
            *retryable || release.is_some() || message.contains("manual download:"),
        ),
        UpdateState::Idle | UpdateState::Checking { .. } | UpdateState::Verifying { .. } => {
            (false, false)
        }
    }
}

/// 统一主次动作及标签顺序，保证 UIA、键盘焦点槽和屏幕上的按钮不会随重构漂移。
fn update_action_descriptors(
    state: &UpdateState,
    labels: &UpdateLabels,
) -> Vec<UpdateAccessibilityAction> {
    let mut actions = Vec::with_capacity(2);
    match state {
        UpdateState::UpToDate { .. } | UpdateState::Succeeded { .. } => {
            actions.push(UpdateAccessibilityAction {
                id: "dismiss-update",
                label: labels.ok.to_owned(),
            });
        }
        UpdateState::Available(_) => {
            actions.push(UpdateAccessibilityAction {
                id: "later-update",
                label: labels.later.to_owned(),
            });
            let can_install = UpdateCoordinator::can_self_install();
            actions.push(UpdateAccessibilityAction {
                id: if can_install {
                    "download-update"
                } else {
                    "open-update-release"
                },
                label: if can_install {
                    labels.download.to_owned()
                } else {
                    labels.open_release.to_owned()
                },
            });
        }
        UpdateState::Downloading { .. } => actions.push(UpdateAccessibilityAction {
            id: "pause-update",
            label: labels.pause.to_owned(),
        }),
        UpdateState::Paused { .. } => {
            actions.push(UpdateAccessibilityAction {
                id: "later-update",
                label: labels.later.to_owned(),
            });
            actions.push(UpdateAccessibilityAction {
                id: "resume-update",
                label: labels.resume.to_owned(),
            });
        }
        UpdateState::Ready { .. } => {
            actions.push(UpdateAccessibilityAction {
                id: "later-update",
                label: labels.later.to_owned(),
            });
            actions.push(UpdateAccessibilityAction {
                id: "install-update",
                label: labels.restart_install.to_owned(),
            });
        }
        UpdateState::Failed {
            message,
            retryable,
            release,
        } => {
            actions.push(UpdateAccessibilityAction {
                id: "copy-update-error",
                label: labels.copy_close.to_owned(),
            });
            if *retryable {
                actions.push(UpdateAccessibilityAction {
                    id: "retry-update",
                    label: labels.retry.to_owned(),
                });
            } else if release.is_some() || message.contains("manual download:") {
                actions.push(UpdateAccessibilityAction {
                    id: "open-update-manual-download",
                    label: labels.open_release.to_owned(),
                });
            }
        }
        UpdateState::Idle | UpdateState::Checking { .. } | UpdateState::Verifying { .. } => {}
    }
    actions
}

/// Prefers the signed release page when it is still attached to the failure;
/// restored terminal results fall back to the stable repository downloads.
fn manual_update_url(state: &UpdateState) -> &str {
    state
        .release()
        .map(|release| release.release_url.as_str())
        .filter(|url| !url.is_empty())
        .unwrap_or("https://github.com/kongweiguang/gmark/releases")
}

struct UpdateLabels {
    title: &'static str,
    checking: &'static str,
    up_to_date: &'static str,
    downloading: &'static str,
    paused: &'static str,
    verifying: &'static str,
    verifying_detail: &'static str,
    ready: &'static str,
    ready_detail: &'static str,
    failed: &'static str,
    updated: &'static str,
    download: &'static str,
    pause: &'static str,
    resume: &'static str,
    retry: &'static str,
    restart_install: &'static str,
    later: &'static str,
    ok: &'static str,
    copy_close: &'static str,
    unsigned_warning: &'static str,
    package_manager_guidance: &'static str,
    open_release: &'static str,
}

impl UpdateLabels {
    fn for_app(cx: &App) -> Self {
        if cx
            .global::<I18nManager>()
            .current_language_id()
            .starts_with("zh")
        {
            Self {
                title: "软件更新",
                checking: "正在检查更新…",
                up_to_date: "已经是最新版本",
                downloading: "正在下载",
                paused: "下载已暂停",
                verifying: "正在验证",
                verifying_detail: "正在校验更新包完整性与签名…",
                ready: "更新已准备好",
                ready_detail: "保存工作后即可重启并完成安装。",
                failed: "更新失败",
                updated: "更新完成",
                download: "下载更新",
                pause: "暂停",
                resume: "继续下载",
                retry: "重试",
                restart_install: "重启并安装",
                later: "稍后",
                ok: "好",
                copy_close: "复制并关闭",
                unsigned_warning: "\n此版本暂未经过系统代码签名，Windows SmartScreen 或 macOS Gatekeeper 可能要求确认。",
                package_manager_guidance: "\n当前安装不受应用内更新器管理；请使用系统包管理器，或从发布页下载安装包完成一次迁移。",
                open_release: "打开发布页",
            }
        } else {
            Self {
                title: "Software Update",
                checking: "Checking for updates…",
                up_to_date: "You're up to date",
                downloading: "Downloading",
                paused: "Download paused",
                verifying: "Verifying",
                verifying_detail: "Checking update integrity and signature…",
                ready: "Update ready",
                ready_detail: "Save your work, then restart to finish installing.",
                failed: "Update failed",
                updated: "Update complete",
                download: "Download Update",
                pause: "Pause",
                resume: "Resume",
                retry: "Retry",
                restart_install: "Restart and Install",
                later: "Later",
                ok: "OK",
                copy_close: "Copy & Close",
                unsigned_warning: "\nThis release is not yet code-signed. Windows SmartScreen or macOS Gatekeeper may ask for confirmation.",
                package_manager_guidance: "\nThis installation is not managed by the in-app updater. Use your system package manager or install once from the release page to migrate.",
                open_release: "Open Release Page",
            }
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    const KIB: f64 = 1024.0;
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / MIB)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
#[path = "../../tests/unit/editor/update.rs"]
mod tests;
