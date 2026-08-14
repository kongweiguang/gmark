// @author kongweiguang

use super::{MAX_PASTE_OUTPUT_BYTES, checked_paste_output_len};

/// 粘贴结果恰好到达 64 MiB 时允许，超过一字节或范围算术溢出时整体拒绝。
#[::core::prelude::v1::test]
fn paste_output_limit_is_checked_before_editing() {
    assert_eq!(
        checked_paste_output_len(1, &(0..1), MAX_PASTE_OUTPUT_BYTES),
        Some(MAX_PASTE_OUTPUT_BYTES)
    );
    assert!(
        checked_paste_output_len(1, &(0..1), MAX_PASTE_OUTPUT_BYTES + 1)
            .is_some_and(|output| output > MAX_PASTE_OUTPUT_BYTES)
    );
    assert!(checked_paste_output_len(1, &(1..2), 0).is_none());
    let invalid_range = std::ops::Range {
        start: usize::MAX,
        end: 0,
    };
    assert!(checked_paste_output_len(usize::MAX, &invalid_range, 1).is_none());
}
