// @author kongweiguang

use super::{
    UpdateLabels, format_bytes, manual_update_url, update_action_descriptors, update_button_slots,
};
use crate::updater::UpdateState;

#[test]
fn update_progress_formats_byte_counts_without_losing_units() {
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1536), "1.5 KiB");
    assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MiB");
}

#[test]
fn update_panel_exposes_stable_keyboard_focus_slots() {
    assert_eq!(update_button_slots(&UpdateState::Idle), (false, false));
    assert_eq!(
        update_button_slots(&UpdateState::UpToDate {
            current_version: "1.0.0".to_owned(),
            latest_version: "1.0.0".to_owned(),
        }),
        (false, true)
    );
    assert_eq!(
        update_button_slots(&UpdateState::Failed {
            release: None,
            message: "offline".to_owned(),
            retryable: true,
        }),
        (true, true)
    );
    assert_eq!(
        update_button_slots(&UpdateState::Failed {
            release: None,
            message: "signature; manual download: https://example.test".to_owned(),
            retryable: false,
        }),
        (true, true)
    );
}

/// Ensures a terminal helper failure retains an actionable browser target even
/// after the original signed release object is no longer in memory.
#[test]
fn terminal_update_failure_has_a_manual_download_action() {
    let state = UpdateState::Failed {
        release: None,
        message: "launch failed; manual download: available".to_owned(),
        retryable: false,
    };
    assert_eq!(
        manual_update_url(&state),
        "https://github.com/kongweiguang/gmark/releases"
    );
    assert_eq!(update_button_slots(&state), (true, true));
}

/// 锁定错误状态的复制与恢复动作顺序，保证 UIA 和键盘主次槽与屏幕按钮一致。
#[test]
fn terminal_failure_accessibility_actions_are_copy_then_manual_download() {
    let state = UpdateState::Failed {
        release: None,
        message: "launch failed; manual download: available".to_owned(),
        retryable: false,
    };
    let labels = UpdateLabels {
        title: "",
        checking: "",
        up_to_date: "",
        downloading: "",
        paused: "",
        verifying: "",
        verifying_detail: "",
        ready: "",
        ready_detail: "",
        failed: "",
        updated: "",
        download: "Download",
        pause: "Pause",
        resume: "Resume",
        retry: "Retry",
        restart_install: "Restart and Install",
        later: "Later",
        ok: "OK",
        copy_close: "Copy & Close",
        unsigned_warning: "",
        package_manager_guidance: "",
        open_release: "Open Release Page",
    };
    let actions = update_action_descriptors(&state, &labels);
    assert_eq!(actions[0].id, "copy-update-error");
    assert_eq!(actions[1].id, "open-update-manual-download");
}
