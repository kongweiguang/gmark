// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn nearest_quote_ancestor(
        &self,
        entity_id: EntityId,
        cx: &App,
    ) -> Option<Entity<super::Block>> {
        let mut current = self.focusable_entity_by_id(entity_id)?;
        loop {
            if current.read(cx).kind().is_quote_container() {
                return Some(current);
            }
            let location = self.document.find_block_location(current.entity_id())?;
            current = location.parent?;
        }
    }

    pub(super) fn topmost_quote_ancestor(
        &self,
        entity_id: EntityId,
        cx: &App,
    ) -> Option<Entity<super::Block>> {
        let mut current = self.nearest_quote_ancestor(entity_id, cx)?;
        loop {
            let Some(location) = self.document.find_block_location(current.entity_id()) else {
                break;
            };
            let Some(parent) = location.parent.clone() else {
                break;
            };
            if !parent.read(cx).kind().is_quote_container() {
                break;
            }
            current = parent;
        }
        Some(current)
    }

    pub(super) fn quote_break_insertion_target(
        &self,
        entity_id: EntityId,
        cx: &App,
    ) -> Option<(Option<Entity<super::Block>>, usize)> {
        let quote_block = self.nearest_quote_ancestor(entity_id, cx)?;
        let location = self.document.find_block_location(quote_block.entity_id())?;
        Some((location.parent.clone(), location.index + 1))
    }

    pub(super) fn callout_break_insertion_target(
        &self,
        entity_id: EntityId,
        cx: &App,
    ) -> Option<(Option<Entity<super::Block>>, usize)> {
        let callout_root = self.topmost_quote_ancestor(entity_id, cx)?;
        let location = self
            .document
            .find_block_location(callout_root.entity_id())?;
        Some((location.parent.clone(), location.index + 1))
    }

    pub(super) fn ensure_callout_body_entry(
        &mut self,
        callout: &Entity<super::Block>,
        cx: &mut Context<Self>,
    ) -> Option<Entity<super::Block>> {
        if !matches!(callout.read(cx).kind(), BlockKind::Callout(_)) {
            return None;
        }

        if let Some(first_child) = callout.read(cx).children.first().cloned() {
            return Some(first_child);
        }

        let body = Self::new_block(cx, BlockRecord::paragraph(String::new()));
        self.document
            .insert_blocks_at(Some(callout.clone()), 0, vec![body.clone()], cx);
        Some(body)
    }

    pub(super) fn materialize_empty_callout_shortcut(
        &mut self,
        block: &Entity<super::Block>,
        cx: &mut Context<Self>,
    ) -> Option<EntityId> {
        if self.view_mode != super::ViewMode::Rendered {
            return None;
        }

        let (kind, title_markdown, has_children) = block.read_with(cx, |block, _cx| {
            (
                block.kind(),
                block.record.title.serialize_markdown(),
                !block.children.is_empty(),
            )
        });
        if kind != BlockKind::Quote || has_children {
            return None;
        }

        let Some((variant, title)) =
            crate::components::CalloutVariant::parse_header_line(&title_markdown)
        else {
            return None;
        };

        block.update(cx, |block, cx| {
            block.record.kind = BlockKind::Callout(variant);
            block
                .record
                .set_title(InlineTextTree::from_markdown(&title));
            block.sync_edit_mode_from_kind();
            block.sync_render_cache();
            block.cursor_blink_epoch = Instant::now();
            cx.notify();
        });
        let body = self.ensure_callout_body_entry(block, cx)?;
        Some(body.entity_id())
    }

    pub(super) fn downgrade_empty_callout_body_to_quote(
        &mut self,
        block: &Entity<super::Block>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(location) = self.document.find_block_location(block.entity_id()) else {
            return false;
        };
        let Some(parent) = location.parent.clone() else {
            return false;
        };

        let (header_markdown, only_child, block_is_empty_leaf) = {
            let parent_ref = parent.read(cx);
            let Some(variant) = parent_ref.kind().callout_variant() else {
                return false;
            };
            let block_ref = block.read(cx);
            (
                variant.header_markdown(&parent_ref.record.title.serialize_markdown()),
                parent_ref.children.len() == 1,
                block_ref.kind() == BlockKind::Paragraph
                    && block_ref.display_text().is_empty()
                    && block_ref.children.is_empty(),
            )
        };
        if !only_child || !block_is_empty_leaf {
            return false;
        }

        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        self.document.with_structure_mutation(cx, |document, cx| {
            let _ = document.remove_block_by_id_raw(block.entity_id(), cx);
            parent.update(cx, |parent, cx| {
                parent.record.kind = BlockKind::Quote;
                parent
                    .record
                    .set_title(InlineTextTree::from_markdown(&header_markdown));
                parent.sync_edit_mode_from_kind();
                parent.sync_render_cache();
                parent.assign_collapsed_selection_offset(0, CollapsedCaretAffinity::Default, None);
                parent.marked_range = None;
                parent.cursor_blink_epoch = Instant::now();
                cx.notify();
            });
        });
        self.focus_block(parent.entity_id());
        self.rebuild_image_runtimes(cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
        true
    }
}
