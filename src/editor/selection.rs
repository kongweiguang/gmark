// @author kongweiguang

//! Editor-level selection spanning multiple rendered blocks.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use gpui::*;

use super::{
    CrossBlockDrag, CrossBlockSelection, CrossBlockSelectionEndpoint, Editor,
    PreparedSplitProjection, SourceTargetMapping, UndoSelectionSnapshot, ViewMode,
};
use crate::components::markdown::inline::StyleFlag;
use crate::components::{
    Block, BlockKind, Copy, CopyAsMarkdown, Cut, Delete, DeleteBack, EditingCommandId,
    InlineTextTree, UndoCaptureKind, serialize_table_markdown_lines,
};
use crate::perf;

/// Cross-block selection with endpoints ordered by visible block position.
#[derive(Clone, Copy)]
struct NormalizedCrossBlockSelection {
    start: CrossBlockSelectionEndpoint,
    end: CrossBlockSelectionEndpoint,
    start_index: usize,
    end_index: usize,
    reversed: bool,
}

struct CrossBlockInlineTarget {
    entity: Entity<Block>,
    next_title: InlineTextTree,
    source_content_range: Range<usize>,
    replacement: String,
}

impl Editor {
    fn clear_cross_block_selection_visuals(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        for visible in self.document.visible_blocks().to_vec() {
            visible.entity.update(cx, |block, cx| {
                if block.editor_selection_range.take().is_some()
                    || block.editor_selection_supports_inline_commands
                {
                    block.editor_selection_supports_inline_commands = false;
                    changed = true;
                    cx.notify();
                }
            });
        }
        changed
    }

    pub(super) fn clear_cross_block_selection(&mut self, cx: &mut Context<Self>) {
        let had_selection = self.cross_block_selection.take().is_some();
        self.cross_block_drag = None;
        let changed_visuals = self.clear_cross_block_selection_visuals(cx);
        let changed = had_selection || changed_visuals;
        if changed {
            cx.notify();
        }
    }

    fn begin_cross_block_drag_at_point(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let had_selection = self.cross_block_selection.take().is_some();
        let changed_visuals = self.clear_cross_block_selection_visuals(cx);
        let changed = had_selection || changed_visuals;
        self.cross_block_drag = self
            .cross_block_endpoint_for_point(position, cx)
            .map(|anchor| CrossBlockDrag { anchor });
        if changed {
            cx.notify();
        }
    }

    pub(super) fn on_editor_capture_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            cx.propagate();
            return;
        }

        if self.view_mode != ViewMode::Rendered {
            cx.propagate();
            return;
        }

        self.rendered_select_all_cycle = None;
        self.begin_cross_block_drag_at_point(event.position, cx);
        cx.propagate();
    }

    pub(super) fn on_editor_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            return;
        }
        let Some(drag) = self.cross_block_drag else {
            return;
        };
        let Some(focus) = self.cross_block_endpoint_for_point(event.position, cx) else {
            return;
        };

        if self.cross_block_selection.is_none() && drag.anchor.entity_id == focus.entity_id {
            return;
        }

        let selection = CrossBlockSelection {
            anchor: drag.anchor,
            focus,
        };
        if self.cross_block_selection_is_empty(selection) {
            self.cross_block_selection = None;
        } else {
            self.cross_block_selection = Some(selection);
        }
        self.sync_cross_block_selection_visuals(cx);
        cx.notify();
    }

    pub(super) fn on_editor_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cross_block_drag = None;
        self.end_block_pointer_selection_sessions(cx);
    }

    pub(super) fn on_copy_capture(
        &mut self,
        _: &Copy,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(markdown) = self.cross_block_selected_markdown(cx) else {
            cx.propagate();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(markdown));
        cx.stop_propagation();
    }

    pub(super) fn on_copy_as_markdown_capture(
        &mut self,
        _: &CopyAsMarkdown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(markdown) = self.cross_block_selected_markdown(cx) else {
            cx.propagate();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(markdown));
        cx.stop_propagation();
    }

    pub(super) fn on_cut_capture(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(markdown) = self.cross_block_selected_markdown(cx) else {
            cx.propagate();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(markdown));
        self.delete_cross_block_selection(cx);
        cx.stop_propagation();
    }

    pub(super) fn on_delete_capture(
        &mut self,
        _: &Delete,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.delete_cross_block_selection(cx) {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
    }

    pub(super) fn on_delete_back_capture(
        &mut self,
        _: &DeleteBack,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.delete_cross_block_selection(cx) {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
    }

    fn rendered_document_is_fully_selected(&self, cx: &App) -> bool {
        let visible = self.document.visible_blocks().to_vec();
        let Some(first) = visible.first() else {
            return false;
        };
        let Some(last) = visible.last() else {
            return false;
        };
        let Some(selection) = self.cross_block_selection else {
            return false;
        };
        let last_len = last.entity.read(cx).visible_len();
        selection.anchor
            == CrossBlockSelectionEndpoint {
                entity_id: first.entity.entity_id(),
                offset: 0,
            }
            && selection.focus
                == CrossBlockSelectionEndpoint {
                    entity_id: last.entity.entity_id(),
                    offset: last_len,
                }
    }

    fn select_focused_block_text_for_rendered_select_all(
        &mut self,
        block: Entity<Block>,
        cx: &mut Context<Self>,
    ) {
        self.clear_cross_block_selection(cx);
        self.end_block_pointer_selection_sessions(cx);
        self.clear_table_axis_preview(cx);
        self.clear_table_axis_selection(cx);
        block.update(cx, |block, cx| {
            let len = block.visible_len();
            block.selected_range = 0..len;
            block.selection_reversed = false;
            block.marked_range = None;
            block.vertical_motion_x = None;
            block.cursor_blink_epoch = std::time::Instant::now();
            cx.notify();
        });
        self.active_entity_id = Some(block.entity_id());
        cx.notify();
    }

    fn select_all_rendered_document(&mut self, cx: &mut Context<Self>) {
        if self.rendered_document_is_fully_selected(cx) {
            return;
        }

        let visible = self.document.visible_blocks().to_vec();
        let Some(first) = visible.first() else {
            return;
        };
        let Some(last) = visible.last() else {
            return;
        };
        let first_id = first.entity.entity_id();
        let last_id = last.entity.entity_id();
        let last_len = last.entity.read(cx).visible_len();

        self.end_block_pointer_selection_sessions(cx);
        self.dismiss_contextual_overlays(cx);
        self.clear_table_axis_preview(cx);
        self.clear_table_axis_selection(cx);
        for visible in &visible {
            visible.entity.update(cx, |block, cx| {
                let cursor = block.cursor_offset();
                let collapsed = cursor..cursor;
                if block.selected_range != collapsed {
                    block.selected_range = collapsed;
                    cx.notify();
                }
            });
        }

        self.cross_block_drag = None;
        self.cross_block_selection = Some(CrossBlockSelection {
            anchor: CrossBlockSelectionEndpoint {
                entity_id: first_id,
                offset: 0,
            },
            focus: CrossBlockSelectionEndpoint {
                entity_id: last_id,
                offset: last_len,
            },
        });
        self.sync_cross_block_selection_visuals(cx);
        cx.notify();
    }

    pub(super) fn on_rendered_select_all_press(
        &mut self,
        block: Entity<Block>,
        cx: &mut Context<Self>,
    ) {
        if self.view_mode != ViewMode::Rendered {
            self.rendered_select_all_cycle = None;
            return;
        }

        let now = std::time::Instant::now();
        let block_id = block.entity_id();
        let count = match self.rendered_select_all_cycle {
            Some(cycle)
                if cycle.entity_id == block_id
                    && now.duration_since(cycle.last_pressed_at)
                        <= Self::RENDERED_SELECT_ALL_CYCLE_WINDOW =>
            {
                cycle.count.saturating_add(1)
            }
            _ => 1,
        }
        .min(3);

        self.rendered_select_all_cycle = Some(super::RenderedSelectAllCycle {
            entity_id: block_id,
            count,
            last_pressed_at: now,
        });

        if count == 1 {
            self.select_focused_block_text_for_rendered_select_all(block, cx);
        } else {
            self.select_all_rendered_document(cx);
        }
    }

    pub(super) fn cross_block_source_selection_snapshot(
        &self,
        cx: &App,
    ) -> Option<UndoSelectionSnapshot> {
        let normalized = self.normalized_cross_block_selection(cx)?;
        let range = self.cross_block_source_range_for_normalized(normalized, cx)?;
        Some(UndoSelectionSnapshot::from_range(
            range,
            normalized.reversed,
        ))
    }

    pub(super) fn apply_cross_block_selection_snapshot_if_possible(
        &mut self,
        snapshot: &UndoSelectionSnapshot,
        cx: &mut Context<Self>,
    ) -> bool {
        let range = snapshot.range();
        let reversed = snapshot.reversed();
        if range.is_empty() {
            return false;
        }

        let mappings = self.build_source_target_mappings(cx);
        let Some(start) = self.endpoint_for_source_offset(range.start, &mappings, cx) else {
            return false;
        };
        let Some(end) = self.endpoint_for_source_offset(range.end, &mappings, cx) else {
            return false;
        };
        let Some(start_index) = self.document.visible_index_for_entity_id(start.entity_id) else {
            return false;
        };
        let Some(end_index) = self.document.visible_index_for_entity_id(end.entity_id) else {
            return false;
        };
        if start_index == end_index {
            return false;
        }

        self.cross_block_selection = Some(if reversed {
            CrossBlockSelection {
                anchor: end,
                focus: start,
            }
        } else {
            CrossBlockSelection {
                anchor: start,
                focus: end,
            }
        });
        self.cross_block_drag = None;
        self.sync_cross_block_selection_visuals(cx);
        let focus = if reversed { start } else { end };
        self.focus_block(focus.entity_id);
        cx.notify();
        true
    }

    fn cross_block_endpoint_for_point(
        &self,
        position: Point<Pixels>,
        cx: &App,
    ) -> Option<CrossBlockSelectionEndpoint> {
        let mut previous: Option<(Entity<Block>, Bounds<Pixels>)> = None;
        for visible in self.document.visible_blocks() {
            let entity = visible.entity.clone();
            let bounds = entity.read(cx).last_bounds;
            let Some(bounds) = bounds else {
                continue;
            };

            if position.y < bounds.top() {
                if let Some((previous, _)) = previous {
                    let offset = previous.read(cx).visible_len();
                    return Some(CrossBlockSelectionEndpoint {
                        entity_id: previous.entity_id(),
                        offset,
                    });
                }
                return Some(CrossBlockSelectionEndpoint {
                    entity_id: entity.entity_id(),
                    offset: 0,
                });
            }

            if position.y <= bounds.bottom() {
                let offset = entity.read(cx).index_for_mouse_position(position);
                return Some(CrossBlockSelectionEndpoint {
                    entity_id: entity.entity_id(),
                    offset,
                });
            }

            previous = Some((entity, bounds));
        }

        previous.map(|(entity, _)| CrossBlockSelectionEndpoint {
            entity_id: entity.entity_id(),
            offset: entity.read(cx).visible_len(),
        })
    }

    fn cross_block_selection_is_empty(&self, selection: CrossBlockSelection) -> bool {
        let Some(anchor_index) = self
            .document
            .visible_index_for_entity_id(selection.anchor.entity_id)
        else {
            return true;
        };
        let Some(focus_index) = self
            .document
            .visible_index_for_entity_id(selection.focus.entity_id)
        else {
            return true;
        };
        anchor_index == focus_index && selection.anchor.offset == selection.focus.offset
    }

    fn normalized_cross_block_selection(&self, cx: &App) -> Option<NormalizedCrossBlockSelection> {
        let selection = self.cross_block_selection?;
        let anchor = self.clamp_cross_block_endpoint(selection.anchor, cx)?;
        let focus = self.clamp_cross_block_endpoint(selection.focus, cx)?;
        let anchor_index = self
            .document
            .visible_index_for_entity_id(anchor.entity_id)?;
        let focus_index = self.document.visible_index_for_entity_id(focus.entity_id)?;
        let reversed = focus_index < anchor_index
            || (focus_index == anchor_index && focus.offset < anchor.offset);
        let (start, end, start_index, end_index) = if reversed {
            (focus, anchor, focus_index, anchor_index)
        } else {
            (anchor, focus, anchor_index, focus_index)
        };
        if start_index == end_index && start.offset == end.offset {
            return None;
        }
        Some(NormalizedCrossBlockSelection {
            start,
            end,
            start_index,
            end_index,
            reversed,
        })
    }

    fn clamp_cross_block_endpoint(
        &self,
        endpoint: CrossBlockSelectionEndpoint,
        cx: &App,
    ) -> Option<CrossBlockSelectionEndpoint> {
        let entity = self.document.block_entity_by_id(endpoint.entity_id)?;
        let len = entity.read(cx).visible_len();
        Some(CrossBlockSelectionEndpoint {
            entity_id: endpoint.entity_id,
            offset: endpoint.offset.min(len),
        })
    }

    fn sync_cross_block_selection_visuals(&mut self, cx: &mut Context<Self>) {
        let normalized = self.normalized_cross_block_selection(cx);
        let visible_blocks = self.document.visible_blocks().to_vec();
        let inline_commands_safe = normalized.is_some_and(|selection| {
            self.cross_block_selection_supports_inline_commands(selection, cx)
        });
        for (index, visible) in visible_blocks.into_iter().enumerate() {
            let next_range = normalized.and_then(|selection| {
                if index < selection.start_index || index > selection.end_index {
                    return None;
                }
                let block = visible.entity.read(cx);
                let len = block.visible_len();
                let range = if selection.start_index == selection.end_index {
                    selection.start.offset.min(len)..selection.end.offset.min(len)
                } else if index == selection.start_index {
                    selection.start.offset.min(len)..len
                } else if index == selection.end_index {
                    0..selection.end.offset.min(len)
                } else {
                    0..len
                };
                (!range.is_empty()).then_some(range)
            });

            visible.entity.update(cx, |block, cx| {
                let next_support = next_range.is_some() && inline_commands_safe;
                if block.editor_selection_range != next_range
                    || block.editor_selection_supports_inline_commands != next_support
                {
                    block.editor_selection_range = next_range.clone();
                    block.editor_selection_supports_inline_commands = next_support;
                    cx.notify();
                }
            });
        }
    }

    fn cross_block_selection_supports_inline_commands(
        &self,
        selection: NormalizedCrossBlockSelection,
        cx: &App,
    ) -> bool {
        let visible = self.document.visible_blocks();
        (selection.start_index..=selection.end_index).all(|index| {
            let Some(visible_block) = visible.get(index) else {
                return false;
            };
            let block = visible_block.entity.read(cx);
            if block.is_read_only()
                || block.uses_raw_text_editing()
                || block.showing_rendered_image()
                || matches!(block.kind(), BlockKind::Table | BlockKind::Separator)
            {
                return false;
            }
            let len = block.visible_len();
            let current_range = if selection.start_index == selection.end_index {
                selection.start.offset.min(len)..selection.end.offset.min(len)
            } else if index == selection.start_index {
                selection.start.offset.min(len)..len
            } else if index == selection.end_index {
                0..selection.end.offset.min(len)
            } else {
                0..len
            };
            let clean_range = block.current_to_clean_range(current_range);
            clean_range.is_empty() || block.record.title.selection_supports_toolbar(clean_range)
        })
    }

    /// 对兼容的跨块富文本选区一次性应用行内格式；验证、决策和提交均不可拆分。
    pub(super) fn apply_cross_block_inline_command(
        &mut self,
        command: EditingCommandId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(selection) = self.normalized_cross_block_selection(cx) else {
            return false;
        };
        let style = match command {
            EditingCommandId::Bold => Some(StyleFlag::Bold),
            EditingCommandId::Italic => Some(StyleFlag::Italic),
            EditingCommandId::Underline => Some(StyleFlag::Underline),
            EditingCommandId::Strikethrough => Some(StyleFlag::Strikethrough),
            EditingCommandId::InlineCode => Some(StyleFlag::Code),
            EditingCommandId::ClearFormatting => None,
            _ => return false,
        };
        let (mappings, _) = self.build_source_target_mappings_with_block_ranges(cx);
        let mappings = mappings
            .into_iter()
            .map(|mapping| (mapping.entity.entity_id(), mapping))
            .collect::<HashMap<_, _>>();
        let visible = self.document.visible_blocks().to_vec();
        let mut candidates = Vec::new();
        let mut all_styled = style.is_some();

        for index in selection.start_index..=selection.end_index {
            let Some(visible_block) = visible.get(index) else {
                return false;
            };
            let entity = visible_block.entity.clone();
            let block = entity.read(cx);
            if block.is_read_only()
                || block.uses_raw_text_editing()
                || block.showing_rendered_image()
                || matches!(block.kind(), BlockKind::Table | BlockKind::Separator)
            {
                return false;
            }
            let len = block.visible_len();
            let current_range = if selection.start_index == selection.end_index {
                selection.start.offset.min(len)..selection.end.offset.min(len)
            } else if index == selection.start_index {
                selection.start.offset.min(len)..len
            } else if index == selection.end_index {
                0..selection.end.offset.min(len)
            } else {
                0..len
            };
            let clean_range = block.current_to_clean_range(current_range);
            if !clean_range.is_empty()
                && !block
                    .record
                    .title
                    .selection_supports_toolbar(clean_range.clone())
            {
                return false;
            }
            let Some(mapping) = mappings.get(&entity.entity_id()) else {
                return false;
            };
            let title_map = block.record.title.markdown_offset_map();
            let markdown_len = title_map.markdown().len();
            let Some(relative_start) = mapping.content_to_source.first().copied() else {
                return false;
            };
            let Some(relative_end) = mapping.content_to_source.get(markdown_len).copied() else {
                return false;
            };
            if let Some(flag) = style
                && !clean_range.is_empty()
            {
                all_styled &= block
                    .record
                    .title
                    .selection_has_style(clean_range.clone(), flag);
            }
            candidates.push((
                entity.clone(),
                clean_range,
                block.record.title.clone(),
                mapping.full_source_range.start + relative_start
                    ..mapping.full_source_range.start + relative_end,
            ));
        }

        let enabled = style.map(|_| !all_styled);
        let mut targets = Vec::with_capacity(candidates.len());
        let mut changed = false;
        for (entity, clean_range, mut next_title, source_content_range) in candidates {
            let target_changed = if let Some(flag) = style {
                next_title.set_text_style(clean_range.clone(), flag, enabled.unwrap_or(true))
            } else {
                next_title.clear_text_formatting(clean_range.clone())
            };
            changed |= target_changed;
            let replacement = next_title.serialize_markdown();
            targets.push(CrossBlockInlineTarget {
                entity,
                next_title,
                source_content_range,
                replacement,
            });
        }
        if !changed {
            return false;
        }

        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        let virtual_edit = self.virtual_surface.is_some() && self.view_mode == ViewMode::Rendered;
        if virtual_edit {
            if !self.apply_virtual_cross_block_inline_targets(selection, &targets, cx) {
                self.pending_virtual_undo_selection = None;
                return false;
            }
        } else {
            for target in targets {
                target.entity.update(cx, move |block, cx| {
                    block.record.set_title(target.next_title);
                    block.sync_render_cache();
                    cx.notify();
                });
            }
            self.mark_dirty(cx);
            self.sync_cross_block_selection_visuals(cx);
        }
        self.finalize_pending_undo_capture(cx);
        self.request_active_block_scroll_into_view(cx);
        cx.notify();
        true
    }
}

#[path = "selection_parts/controller.rs"]
mod controller;

#[cfg(test)]
#[path = "../../tests/unit/editor/selection.rs"]
mod tests;
