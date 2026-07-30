// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn toggle_task_checked(
        &mut self,
        block: &Entity<super::Block>,
        cx: &mut Context<Self>,
    ) {
        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        block.update(cx, |block, cx| {
            let checked = match block.kind() {
                BlockKind::TaskListItem { checked } => checked,
                _ => return,
            };
            block.record.kind = BlockKind::TaskListItem { checked: !checked };
            block.sync_edit_mode_from_kind();
            block.sync_render_cache();
            block.cursor_blink_epoch = Instant::now();
            cx.notify();
        });
        self.mark_dirty(cx);
        self.request_active_block_scroll_into_view(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
    }
}
