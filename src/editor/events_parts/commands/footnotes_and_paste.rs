// @author kongweiguang

use super::*;
use crate::editor::PendingResourceInsertion;

impl Editor {
    pub(in crate::editor) fn next_footnote_id(markdown: &str) -> String {
        (1usize..)
            .map(|index| format!("note-{index}"))
            .find(|id| !markdown.contains(&format!("[^{id}]")))
            .unwrap_or_else(|| "note-1".to_owned())
    }

    pub(in crate::editor) fn new_footnote_definition_block(
        cx: &mut Context<Self>,
        id: String,
    ) -> Entity<super::Block> {
        let definition = Self::new_block(
            cx,
            BlockRecord::with_plain_text(BlockKind::FootnoteDefinition, id),
        );
        let body = Self::new_block(cx, BlockRecord::paragraph(String::new()));
        definition.update(cx, |definition, _cx| definition.children = vec![body]);
        definition
    }

    pub(in crate::editor) fn handle_paste_image_request(
        &mut self,
        block: Entity<super::Block>,
        leading: &InlineTextTree,
        source: &PastedImageSource,
        trailing: &InlineTextTree,
        cx: &mut Context<Self>,
    ) {
        if let PastedImageSource::LocalResource(source) = source {
            let behavior = crate::preferences::read_app_preferences()
                .map(|preferences| preferences.resource_insert_behavior())
                .unwrap_or(crate::preferences::ImagePasteBehavior::None);
            if self.file_path.is_none() && behavior != crate::preferences::ImagePasteBehavior::None
            {
                // Save As owns the boundary before any copy side effect. A
                // cancelled or failed save simply drops this pending intent.
                self.pending_resource_insertion = Some(PendingResourceInsertion::Pasted {
                    block,
                    leading: leading.clone(),
                    trailing: trailing.clone(),
                    source: source.clone(),
                });
                self.request_save_document_as(cx);
                return;
            }
        }

        self.complete_paste_image_request(block, leading, source, trailing, cx);
    }

    pub(super) fn complete_paste_image_request(
        &mut self,
        block: Entity<super::Block>,
        leading: &InlineTextTree,
        source: &PastedImageSource,
        trailing: &InlineTextTree,
        cx: &mut Context<Self>,
    ) {
        let (markdown, materialized) = match source {
            PastedImageSource::LocalResource(path) => match self.pasted_resource_markdown(path) {
                Ok((markdown, materialized)) => (markdown, Some(materialized)),
                Err(err) => {
                    self.show_image_paste_error(err, cx);
                    return;
                }
            },
            PastedImageSource::ClipboardImage(_) | PastedImageSource::LocalPath(_) => {
                match self.pasted_image_markdown(source) {
                    Ok(markdown) => (markdown, None),
                    Err(err) => {
                        self.show_image_paste_error(err, cx);
                        return;
                    }
                }
            }
        };

        if self.replace_cross_block_selection_with_text(
            &markdown,
            None,
            false,
            crate::components::UndoCaptureKind::NonCoalescible,
            cx,
        ) {
            return;
        }

        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        let can_insert_image_block = self.view_mode == super::ViewMode::Rendered
            && block.read(cx).kind() == BlockKind::Paragraph
            && self.table_cell_binding(block.entity_id()).is_none()
            && !block.read(cx).uses_raw_text_editing();

        if can_insert_image_block {
            if !self.insert_image_block_after_paragraph(&block, leading, &markdown, trailing, cx) {
                if let Some(materialized) = materialized.as_ref() {
                    materialized.cleanup_if_created();
                }
                self.finalize_pending_undo_capture(cx);
                return;
            }
        } else {
            self.replace_current_block_selection_with_image_text(
                &block, leading, &markdown, trailing, cx,
            );
        }

        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
    }

    pub(in crate::editor) fn jump_to_footnote_definition(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(block) = self
            .footnote_registry
            .binding(id)
            .and_then(|binding| self.focusable_entity_by_id(binding.definition_entity_id))
        {
            self.focus_block_range(&block, 0..0, cx);
            return true;
        }
        let Some(y) = self
            .virtual_surface
            .as_ref()
            .and_then(|surface| surface.footnote_definition_y(id))
        else {
            return false;
        };
        self.pending_virtual_footnote_focus = Some(id.to_string());
        self.scroll_handle
            .set_offset(point(px(0.0), px(-y.max(0.0))));
        cx.notify();
        true
    }

    pub(in crate::editor) fn jump_to_footnote_backref(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some((block, range)) = self.footnote_registry.binding(id).and_then(|binding| {
            let first_reference = binding.first_reference.as_ref()?;
            let block = self.focusable_entity_by_id(first_reference.entity_id)?;
            let range = block
                .read(cx)
                .current_range_for_footnote_occurrence(first_reference.occurrence_index)
                .unwrap_or(0..0);
            Some((block, range))
        }) {
            self.focus_block_range(&block, range, cx);
            return true;
        }
        let Some(y) = self
            .virtual_surface
            .as_ref()
            .and_then(|surface| surface.footnote_first_reference_y(id))
        else {
            return false;
        };
        self.pending_virtual_footnote_backref_focus = Some(id.to_string());
        self.scroll_handle
            .set_offset(point(px(0.0), px(-y.max(0.0))));
        cx.notify();
        true
    }
}
