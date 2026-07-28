// @author kongweiguang

use super::{format_bytes, update_button_slots};
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
            message: "signature".to_owned(),
            retryable: false,
        }),
        (true, false)
    );
}
