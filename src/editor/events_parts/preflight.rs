// @author kongweiguang

use super::*;

impl Editor {
    /// 处理不进入文档结构主状态机的高优先级事件。
    ///
    /// 返回 true 表示事件已经完全消费；调用方不得继续执行表格或结构分支。
    pub(super) fn handle_block_event_preflight(
        &mut self,
        block: &Entity<super::Block>,
        event: &BlockEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if let BlockEvent::PrepareUndo { kind } = event {
            self.prepare_undo_capture_from_stable_snapshot(*kind);
            return true;
        }
        if let BlockEvent::RequestReplaceCrossBlockSelection {
            text,
            selected_range_relative,
            mark_inserted_text,
            undo_kind,
        } = event
            && self.replace_cross_block_selection_with_text(
                text,
                selected_range_relative.clone(),
                *mark_inserted_text,
                *undo_kind,
                cx,
            )
        {
            return true;
        }
        if matches!(event, BlockEvent::RequestRenderedSelectAll) {
            self.on_rendered_select_all_press(block.clone(), cx);
            return true;
        }
        if let BlockEvent::RequestPasteImage {
            leading,
            source,
            trailing,
        } = event
        {
            self.handle_paste_image_request(block.clone(), leading, source, trailing, cx);
            return true;
        }
        if let BlockEvent::RequestSlashCommand {
            command,
            trigger_range,
        } = event
        {
            self.apply_slash_command(block.clone(), *command, trigger_range.clone(), cx);
            return true;
        }
        if let BlockEvent::RequestEditingCommand { command } = event {
            match command.plan() {
                EditingCommandPlan::ChangeBlockKind(kind) => {
                    self.set_block_kind_for(block.clone(), kind, cx)
                }
                EditingCommandPlan::ApplyInline(command) => {
                    self.apply_cross_block_inline_command(command, cx);
                }
                _ => {}
            }
            return true;
        }
        if let BlockEvent::RequestMoveBlock { source, placement } = event {
            if *source == block.entity_id() {
                return true;
            }
            let Some(source_location) = self.document.find_block_location(*source) else {
                return true;
            };
            let Some(target_location) = self.document.find_block_location(block.entity_id()) else {
                return true;
            };
            let same_parent = match (&source_location.parent, &target_location.parent) {
                (None, None) => true,
                (Some(left), Some(right)) => left.entity_id() == right.entity_id(),
                _ => false,
            };
            if !same_parent {
                return true;
            }
            let target_index = Self::sibling_drop_insert_index(
                source_location.index,
                target_location.index,
                *placement,
            );
            if target_index == source_location.index {
                return true;
            }
            self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
            self.document.with_structure_mutation(cx, |document, cx| {
                let Some((moved, _)) = document.remove_block_by_id_raw(*source, cx) else {
                    return;
                };
                document.insert_blocks_at_raw(
                    target_location.parent.clone(),
                    target_index,
                    vec![moved],
                    cx,
                );
            });
            self.focus_block(*source);
            self.mark_dirty(cx);
            self.finalize_pending_undo_capture(cx);
            self.request_active_block_scroll_into_view(cx);
            cx.notify();
            return true;
        }
        if let BlockEvent::RequestJumpToTocHeading { target } = event {
            if self.focusable_entity_by_id(*target).is_some() {
                self.focus_block(*target);
                cx.notify();
            }
            return true;
        }
        false
    }
}
