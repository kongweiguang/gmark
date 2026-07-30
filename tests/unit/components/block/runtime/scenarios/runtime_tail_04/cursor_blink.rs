// @author kongweiguang

#[test]
fn cursor_blink_uses_stable_half_second_phases() {
    use super::editing::{CURSOR_BLINK_INTERVAL, cursor_opacity_for_elapsed};

    assert_eq!(CURSOR_BLINK_INTERVAL, std::time::Duration::from_millis(500));
    assert_eq!(cursor_opacity_for_elapsed(std::time::Duration::ZERO), 1.0);
    assert_eq!(
        cursor_opacity_for_elapsed(std::time::Duration::from_millis(499)),
        1.0
    );
    assert_eq!(
        cursor_opacity_for_elapsed(std::time::Duration::from_millis(500)),
        0.0
    );
    assert_eq!(
        cursor_opacity_for_elapsed(std::time::Duration::from_millis(999)),
        0.0
    );
    assert_eq!(
        cursor_opacity_for_elapsed(std::time::Duration::from_millis(1_000)),
        1.0
    );
}

#[gpui::test]
async fn cursor_blink_timer_stops_when_window_becomes_inactive(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph("caret")));

    block.update(cx, |block, cx| block.start_cursor_blink(cx));
    assert!(block.read_with(cx, |block, _cx| block.cursor_blink_task.is_some()));

    block.update(cx, |block, cx| {
        block.set_cursor_blink_window_active(false, cx)
    });
    assert!(block.read_with(cx, |block, _cx| block.cursor_blink_task.is_none()));
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_opacity()), 1.0);
}
