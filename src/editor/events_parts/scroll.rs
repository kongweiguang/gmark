// @author kongweiguang

use super::*;

impl Editor {
    pub(crate) fn on_scrollbar_thumb_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scrollbar_thumb_hovered = *hovered;
        if *hovered {
            self.bump_scrollbar_visibility(cx);
        } else {
            cx.notify();
        }
    }

    pub(crate) fn on_menu_bar_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_menu_bar_hovered(*hovered, cx);
    }

    pub(crate) fn on_menu_panel_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_menu_panel_hovered(*hovered, cx);
    }

    pub(crate) fn on_menu_submenu_panel_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_menu_submenu_panel_hovered(*hovered, cx);
    }

    pub(crate) fn on_menu_submenu_bridge_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_menu_submenu_bridge_hovered(*hovered, cx);
    }

    pub(crate) fn on_editor_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_menu_bar_from_body(cx);
        self.clear_table_axis_preview(cx);
        self.clear_table_axis_selection(cx);
        self.focus_document_end_from_blank_area(event.position, cx);
    }

    /// 点击最后一个块下方的画布空白时，把插入点落到可继续输入的文档末尾。
    ///
    /// 仅在视口已进入文末留白时启用，避免把长文档中间的虚拟占位区
    /// 误判成文档结尾；块自身范围内的点击仍由 Block 精确命中字符位置。
    pub(super) fn focus_document_end_from_blank_area(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.view_mode != super::ViewMode::Rendered {
            return false;
        }

        let max_scroll_y = f32::from(self.scroll_handle.max_offset().height.max(px(0.0)));
        let current_scroll_y = (-f32::from(self.scroll_handle.offset().y)).max(0.0);
        if current_scroll_y + 0.5 < max_scroll_y {
            let viewport_height = f32::from(self.scroll_handle.bounds().size.height.max(px(1.0)));
            let Some(theme) = cx.try_global::<crate::theme::ThemeManager>() else {
                return false;
            };
            let bottom_padding =
                super::render::editor_bottom_padding(viewport_height, &theme.current().dimensions);
            // 底部留白本身计入 max scroll；短文档在 offset=0 时也已经位于文末区域。
            if current_scroll_y + bottom_padding + 0.5 < max_scroll_y {
                return false;
            }
        }

        let Some(last) = self
            .document
            .visible_blocks()
            .last()
            .map(|visible| visible.entity.clone())
        else {
            return false;
        };
        let (Some(bounds), cursor, multiline_structure) = last.read_with(cx, |block, _cx| {
            (
                block.last_bounds,
                block.visible_len(),
                block.kind().is_multiline_text_block(),
            )
        }) else {
            return false;
        };
        if position.y <= bounds.bottom() {
            return false;
        }

        if multiline_structure {
            // fenced code / math 等多行结构没有块外插入点；点击文末留白应创建真正的正文行。
            self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
            self.ensure_trailing_paragraph_after_structural(&last, cx);
            let landing = self
                .document
                .visible_blocks()
                .iter()
                .position(|visible| visible.entity.entity_id() == last.entity_id())
                .and_then(|index| self.document.visible_blocks().get(index + 1))
                .map(|visible| visible.entity.clone());
            if let Some(landing) = landing {
                self.focus_block_range(&landing, 0..0, cx);
                self.mark_dirty(cx);
                self.request_active_block_scroll_into_view(cx);
                self.finalize_pending_undo_capture(cx);
                cx.notify();
                return true;
            }
            self.finalize_pending_undo_capture(cx);
        }

        self.focus_block_range(&last, cursor..cursor, cx);
        cx.notify();
        true
    }

    pub(crate) fn on_editor_scroll_wheel(
        &mut self,
        _event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.split_preview.as_mut() {
            state.scroll_driver = Some(SplitScrollDriver::Source);
        }
        self.bump_scrollbar_visibility(cx);
    }

    pub(crate) fn on_split_preview_scroll_wheel(
        &mut self,
        _event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.split_preview.as_mut() {
            state.scroll_driver = Some(SplitScrollDriver::Preview);
            self.bump_split_preview_scrollbar_visibility(cx);
        }
    }

    pub(crate) fn bump_split_preview_scrollbar_visibility(&mut self, cx: &mut Context<Self>) {
        let duration = Duration::from_millis(900);
        self.split_preview_scrollbar_visible_until = Instant::now() + duration;
        let weak_editor = cx.entity().downgrade();
        self.split_preview_scrollbar_fade_task = Some(cx.spawn(
            async move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
                cx.background_executor()
                    .timer(duration + Duration::from_millis(50))
                    .await;
                let _ = weak_editor.update(cx, |_this, cx| cx.notify());
            },
        ));
        cx.notify();
    }

    pub(crate) fn on_split_preview_scrollbar_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split_preview_scrollbar_hovered = *hovered;
        if *hovered {
            self.bump_split_preview_scrollbar_visibility(cx);
        } else {
            cx.notify();
        }
    }

    pub(crate) fn on_page_up(
        &mut self,
        _: &crate::components::PageUp,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let page = self.scroll_handle.bounds().size.height;
        self.scroll_viewport_by(page, cx);
    }

    pub(crate) fn on_page_down(
        &mut self,
        _: &crate::components::PageDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let page = self.scroll_handle.bounds().size.height;
        self.scroll_viewport_by(-page, cx);
    }

    pub(crate) fn on_jump_to_top(
        &mut self,
        _: &crate::components::JumpToTop,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_vertical_scroll_offset(px(0.0), cx);
    }

    pub(crate) fn on_jump_to_bottom(
        &mut self,
        _: &crate::components::JumpToBottom,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let max_offset_y = self.scroll_handle.max_offset().height.max(px(0.0));
        self.set_vertical_scroll_offset(-max_offset_y, cx);
    }

    /// Scrolls the viewport vertically by `delta`. A positive `delta` moves
    /// toward the start of the document; a negative one moves toward the end.
    /// One page is the current viewport height, so the step tracks window size.
    pub(super) fn scroll_viewport_by(&mut self, delta: Pixels, cx: &mut Context<Self>) {
        let target = self.scroll_handle.offset().y + delta;
        self.set_vertical_scroll_offset(target, cx);
    }

    /// Applies an absolute vertical scroll offset, clamped to the scrollable
    /// range. Offsets run from 0 at the top to `-max_offset` at the bottom.
    pub(super) fn set_vertical_scroll_offset(&mut self, target_y: Pixels, cx: &mut Context<Self>) {
        let max_offset_y = self.scroll_handle.max_offset().height.max(px(0.0));
        let mut offset = self.scroll_handle.offset();
        offset.y = target_y.min(px(0.0)).max(-max_offset_y);
        self.scroll_handle.set_offset(offset);
        // A direct viewport scroll should stick, so cancel any queued pass that
        // would otherwise re-center the active block on the next frame.
        self.pending_scroll_active_block_into_view = false;
        self.pending_scroll_recheck_after_layout = false;
        self.bump_scrollbar_visibility(cx);
        cx.notify();
    }

    pub(crate) fn start_scrollbar_drag(
        &mut self,
        pointer_offset_y: f32,
        track_height: f32,
        thumb_height: f32,
        max_scroll_y: f32,
        cx: &mut Context<Self>,
    ) {
        self.scrollbar_drag = Some(super::ScrollbarDragSession {
            pointer_offset_y: pointer_offset_y.clamp(0.0, thumb_height.max(0.0)),
            track_height,
            thumb_height,
            max_scroll_y,
        });
        self.pending_scroll_active_block_into_view = false;
        self.pending_scroll_recheck_after_layout = false;
        self.bump_scrollbar_visibility(cx);
        cx.notify();
    }

    pub(crate) fn update_scrollbar_drag(
        &mut self,
        pointer_y_in_track: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.scrollbar_drag else {
            return;
        };

        let travel = (drag.track_height - drag.thumb_height).max(0.0);
        let thumb_top = (pointer_y_in_track - drag.pointer_offset_y).clamp(0.0, travel);
        let scroll_y = Self::scroll_offset_for_thumb_top(
            thumb_top,
            drag.track_height,
            drag.thumb_height,
            drag.max_scroll_y,
        );

        let mut offset = self.scroll_handle.offset();
        offset.y = -px(scroll_y);
        self.scroll_handle.set_offset(offset);
        self.bump_scrollbar_visibility(cx);
        cx.notify();
    }

    pub(crate) fn end_scrollbar_drag(&mut self, cx: &mut Context<Self>) {
        if self.scrollbar_drag.take().is_some() {
            self.bump_scrollbar_visibility(cx);
            cx.notify();
        }
    }

    pub(crate) fn start_split_preview_scrollbar_drag(
        &mut self,
        pointer_offset_y: f32,
        track_height: f32,
        thumb_height: f32,
        max_scroll_y: f32,
        cx: &mut Context<Self>,
    ) {
        self.split_preview_scrollbar_drag = Some(super::ScrollbarDragSession {
            pointer_offset_y: pointer_offset_y.clamp(0.0, thumb_height.max(0.0)),
            track_height,
            thumb_height,
            max_scroll_y,
        });
        if let Some(state) = self.split_preview.as_mut() {
            state.scroll_driver = Some(SplitScrollDriver::Preview);
        }
        self.bump_split_preview_scrollbar_visibility(cx);
    }

    pub(crate) fn update_split_preview_scrollbar_drag(
        &mut self,
        pointer_y_in_track: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.split_preview_scrollbar_drag else {
            return;
        };
        let travel = (drag.track_height - drag.thumb_height).max(0.0);
        let thumb_top = (pointer_y_in_track - drag.pointer_offset_y).clamp(0.0, travel);
        let scroll_y = Self::scroll_offset_for_thumb_top(
            thumb_top,
            drag.track_height,
            drag.thumb_height,
            drag.max_scroll_y,
        );
        if let Some(state) = self.split_preview.as_mut() {
            let mut offset = state.scroll_handle.offset();
            offset.y = -px(scroll_y);
            state.scroll_handle.set_offset(offset);
            state.scroll_driver = Some(SplitScrollDriver::Preview);
        }
        self.bump_split_preview_scrollbar_visibility(cx);
    }

    pub(crate) fn end_split_preview_scrollbar_drag(&mut self, cx: &mut Context<Self>) {
        if self.split_preview_scrollbar_drag.take().is_some() {
            self.bump_split_preview_scrollbar_visibility(cx);
        }
    }

    pub(in crate::editor) fn focus_table_cell_position(
        &mut self,
        table_block: &Entity<super::Block>,
        position: TableCellPosition,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(cell) = table_block
            .read(cx)
            .table_runtime
            .as_ref()
            .and_then(|runtime| runtime.cell(position))
        else {
            return false;
        };
        self.focus_block(cell.entity_id());
        cx.notify();
        true
    }

    /// Focus a cell when keyboard navigation enters a table from an adjacent
    /// block. Entering from above lands on the first header cell; entering from
    /// below lands on the first cell of the last body row, falling back to the
    /// header when the table has no body rows.
    pub(super) fn focus_table_entry_cell(
        &mut self,
        table_block: &Entity<super::Block>,
        from_top: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(runtime) = table_block.read(cx).table_runtime.clone() else {
            return false;
        };
        let cell = if from_top {
            runtime.header.first().cloned()
        } else {
            runtime
                .rows
                .last()
                .and_then(|row| row.first())
                .cloned()
                .or_else(|| runtime.header.first().cloned())
        };
        let Some(cell) = cell else {
            return false;
        };
        self.focus_block(cell.entity_id());
        cx.notify();
        true
    }

    /// Move focus from a table edge to the block immediately above (delta < 0)
    /// or below (delta > 0) it, mirroring how plain blocks transfer focus when
    /// the caret leaves their first or last line. When the neighbor is itself a
    /// table, drop into one of its cells so the caret stays editable instead of
    /// landing on the table container. `to_block_start` lands the caret at the
    /// neighbor's start (Block Up/Down semantics) rather than the nearest edge
    /// (Move Up/Down semantics).
    pub(super) fn focus_block_adjacent_to_table(
        &mut self,
        table_block: &Entity<super::Block>,
        delta: i32,
        to_block_start: bool,
        cx: &mut Context<Self>,
    ) {
        let visible = self.document.flatten_visible_blocks();
        let Some(index) = visible
            .iter()
            .position(|visible| visible.entity.entity_id() == table_block.entity_id())
        else {
            return;
        };
        let target_index = if delta < 0 {
            index.checked_sub(1)
        } else {
            Some(index + 1)
        };
        let Some(target) = target_index
            .and_then(|target_index| visible.get(target_index))
            .map(|visible| visible.entity.clone())
        else {
            return;
        };
        if target.read(cx).kind() == BlockKind::Table
            && self.focus_table_entry_cell(&target, delta > 0, cx)
        {
            return;
        }
        self.focus_block(target.entity_id());
        if to_block_start {
            target.update(cx, |target, cx| target.move_to(0, cx));
        } else {
            let prefer_last_line = delta < 0;
            let offset = target
                .read(cx)
                .entry_offset_for_vertical_focus(prefer_last_line, None);
            target.update(cx, move |target, cx| {
                target.move_to_with_preferred_x(offset, None, cx);
            });
        }
        cx.notify();
    }

    pub(super) fn focus_table_cell_horizontal_neighbor(
        &mut self,
        table_block: &Entity<super::Block>,
        position: TableCellPosition,
        delta: i32,
        cx: &mut Context<Self>,
    ) {
        let Some(runtime) = table_block.read(cx).table_runtime.clone() else {
            return;
        };
        let columns = runtime.header.len();
        let total_rows = 1 + runtime.rows.len();
        if columns == 0 || total_rows == 0 {
            return;
        }

        let linear = position.row * columns + position.column;
        let next = if delta < 0 {
            linear.checked_sub(delta.unsigned_abs() as usize)
        } else {
            linear.checked_add(delta as usize)
        };
        let Some(next) = next else {
            return;
        };
        if next >= total_rows * columns {
            return;
        }

        let next_position = TableCellPosition {
            row: next / columns,
            column: next % columns,
        };
        let _ = self.focus_table_cell_position(table_block, next_position, cx);
    }

    pub(super) fn focus_table_cell_vertical_neighbor(
        &mut self,
        table_block: &Entity<super::Block>,
        position: TableCellPosition,
        delta: i32,
        cx: &mut Context<Self>,
    ) {
        let Some(runtime) = table_block.read(cx).table_runtime.clone() else {
            return;
        };
        let max_row = runtime.rows.len();
        let next_row = if delta < 0 {
            position.row.checked_sub(delta.unsigned_abs() as usize)
        } else {
            position.row.checked_add(delta as usize)
        };
        // Moving past the first/last row leaves the table for the adjacent
        // block rather than stopping at the edge.
        let Some(next_row) = next_row.filter(|row| *row <= max_row) else {
            self.focus_block_adjacent_to_table(table_block, delta, false, cx);
            return;
        };

        let next_position = TableCellPosition {
            row: next_row,
            column: position.column.min(runtime.header.len().saturating_sub(1)),
        };
        let _ = self.focus_table_cell_position(table_block, next_position, cx);
    }

    pub(super) fn on_table_cell_event(
        &mut self,
        binding: super::TableCellBinding,
        event: &BlockEvent,
        cx: &mut Context<Self>,
    ) {
        if Self::block_event_clears_cross_block_selection(event) {
            self.rendered_select_all_cycle = None;
            self.clear_cross_block_selection(cx);
        }

        match event {
            BlockEvent::Changed => {
                self.sync_table_record_from_runtime(&binding.table_block, cx);
                self.rebuild_image_runtimes(cx);
                self.mark_block_dirty(binding.table_block.entity_id(), cx);
                self.request_active_block_scroll_into_view(cx);
                self.finalize_pending_undo_capture(cx);
            }
            BlockEvent::RequestOpenLink {
                prompt_target,
                open_target,
            } => {
                self.request_open_link_prompt(prompt_target.clone(), open_target.clone(), cx);
            }
            BlockEvent::RequestJumpToFootnoteDefinition { id, .. } => {
                let _ = self.jump_to_footnote_definition(id, cx);
            }
            BlockEvent::RequestJumpToFootnoteBackref { id } => {
                let _ = self.jump_to_footnote_backref(id, cx);
            }
            BlockEvent::RequestTableCellMoveHorizontal { delta } => {
                self.focus_table_cell_horizontal_neighbor(
                    &binding.table_block,
                    binding.position,
                    *delta,
                    cx,
                );
            }
            BlockEvent::RequestTableCellMoveVertical { delta } => {
                self.focus_table_cell_vertical_neighbor(
                    &binding.table_block,
                    binding.position,
                    *delta,
                    cx,
                );
            }
            BlockEvent::RequestNewline { .. } => {
                let Some(location) = self
                    .document
                    .find_block_location(binding.table_block.entity_id())
                else {
                    return;
                };
                self.clear_table_axis_preview(cx);
                self.clear_table_axis_selection(cx);
                self.sync_table_record_from_runtime(&binding.table_block, cx);
                self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
                let new_block = Self::new_block(cx, BlockRecord::paragraph(String::new()));
                self.document.insert_blocks_at(
                    location.parent,
                    location.index + 1,
                    vec![new_block.clone()],
                    cx,
                );
                self.rebuild_image_runtimes(cx);
                self.focus_block(new_block.entity_id());
                self.mark_dirty(cx);
                self.request_active_block_scroll_into_view(cx);
                self.finalize_pending_undo_capture(cx);
                cx.notify();
            }
            BlockEvent::RequestFocus => {
                self.close_menu_bar(cx);
                self.clear_table_axis_preview(cx);
                self.clear_table_axis_selection(cx);
                self.focus_block(binding.cell.entity_id());
                cx.notify();
            }
            BlockEvent::RequestFocusPrev { .. } => {
                self.focus_table_cell_vertical_neighbor(
                    &binding.table_block,
                    binding.position,
                    -1,
                    cx,
                );
            }
            BlockEvent::RequestFocusNext { .. } => {
                self.focus_table_cell_vertical_neighbor(
                    &binding.table_block,
                    binding.position,
                    1,
                    cx,
                );
            }
            // Block Up/Down treat the table as a single block: leave it
            // entirely for the block above/below rather than stepping by cell.
            BlockEvent::RequestBlockUp => {
                self.focus_block_adjacent_to_table(&binding.table_block, -1, true, cx);
            }
            BlockEvent::RequestBlockDown => {
                self.focus_block_adjacent_to_table(&binding.table_block, 1, true, cx);
            }
            _ => {}
        }
    }

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
