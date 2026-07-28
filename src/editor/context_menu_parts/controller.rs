// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn on_editor_surface_mouse_down(
        &mut self,
        _event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismiss_active_contextual_editing_popovers(cx) {
            cx.notify();
        }
    }

    pub(super) fn schedule_context_menu_submenu_close(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.context_menu, Some(ContextMenuState::Insert { .. })) {
            return;
        }

        let weak_editor = cx.entity().downgrade();
        self.context_menu_submenu_close_task = Some(cx.spawn(
            async move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                let _ = weak_editor.update(cx, |editor, cx| {
                    editor.context_menu_submenu_close_task = None;
                    let Some(ContextMenuState::Insert {
                        insert_hovered,
                        submenu_hovered,
                        submenu_open,
                        ..
                    }) = editor.context_menu.as_mut()
                    else {
                        return;
                    };
                    if !*insert_hovered && !*submenu_hovered && *submenu_open {
                        *submenu_open = false;
                        cx.notify();
                    }
                });
            },
        ));
    }

    pub(super) fn set_context_menu_hover_state(
        &mut self,
        hovered: bool,
        submenu: bool,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        let mut should_clear_close = false;
        let mut should_schedule_close = false;

        if let Some(ContextMenuState::Insert {
            insert_hovered,
            submenu_hovered,
            submenu_open,
            ..
        }) = self.context_menu.as_mut()
        {
            if submenu {
                if *submenu_hovered != hovered {
                    *submenu_hovered = hovered;
                    changed = true;
                }
            } else if *insert_hovered != hovered {
                *insert_hovered = hovered;
                changed = true;
            }

            if hovered {
                should_clear_close = true;
                if !*submenu_open {
                    *submenu_open = true;
                    changed = true;
                }
            } else {
                let insert_still_hovered = *insert_hovered;
                let submenu_still_hovered = *submenu_hovered;
                if !insert_still_hovered && !submenu_still_hovered {
                    should_schedule_close = true;
                }
            }
        }

        if should_clear_close {
            self.context_menu_submenu_close_task = None;
        }
        if should_schedule_close {
            self.schedule_context_menu_submenu_close(cx);
        }
        if changed {
            cx.notify();
        }
    }

    pub(in crate::editor) fn on_editor_context_menu_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view_mode != ViewMode::Rendered {
            return;
        }
        cx.stop_propagation();
        self.open_insert_context_menu(event.position, TableInsertTarget::Append, cx);
    }

    pub(in crate::editor) fn on_block_context_menu_mouse_down(
        &mut self,
        entity_id: EntityId,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view_mode != ViewMode::Rendered {
            return;
        }
        cx.stop_propagation();
        if let Some(block) = self.focusable_entity_by_id(entity_id) {
            if block.read(cx).record.resource.is_some() {
                self.close_menu_bar(cx);
                self.context_menu = Some(ContextMenuState::Resource {
                    position: event.position,
                    entity_id,
                });
                self.context_menu_keyboard_item = None;
                self.context_menu_keyboard_submenu_item = None;
                self.context_menu_scroll_handle
                    .set_offset(point(px(0.0), px(0.0)));
                cx.notify();
                return;
            }
            let offset = block.read(cx).index_for_mouse_position(event.position);
            let diagnostic = block
                .read(cx)
                .spelling_diagnostics
                .iter()
                .find(|diagnostic| diagnostic.range.contains(&offset))
                .cloned();
            if let Some(diagnostic) = diagnostic {
                self.close_menu_bar(cx);
                self.context_menu = Some(ContextMenuState::Spelling {
                    position: event.position,
                    entity_id,
                    diagnostic,
                });
                self.context_menu_keyboard_item = None;
                self.context_menu_keyboard_submenu_item = None;
                self.context_menu_scroll_handle
                    .set_offset(point(px(0.0), px(0.0)));
                cx.notify();
                return;
            }
        }
        // 单元格内部保留表格自身的上下文；其余根块都允许在后方插入，
        // 与块操作菜单的插入能力保持一致。
        if self.table_cell_binding(entity_id).is_some() {
            return;
        }
        let target = TableInsertTarget::After(self.root_ancestor_entity_id(entity_id));
        self.open_insert_context_menu(event.position, target, cx);
    }

    pub(in crate::editor) fn on_dismiss_context_menu_overlay(
        &mut self,
        _event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_contextual_overlays(cx);
    }

    pub(in crate::editor) fn on_dismiss_transient_ui(
        &mut self,
        _: &DismissTransientUi,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_external_conflict_dialog {
            self.cancel_external_conflict(cx);
            return;
        }
        if let Some(document_host) = self.document_host.clone() {
            document_host.update(cx, |view, cx| {
                view.on_dismiss_transient_ui(&DismissTransientUi, window, cx);
            });
        }
        self.dismiss_contextual_overlays(cx);
    }

    pub(in crate::editor) fn on_context_menu_insert_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if *hovered {
            self.clear_context_menu_keyboard_cursor(cx);
        }
        self.set_context_menu_hover_state(*hovered, false, cx);
    }

    pub(in crate::editor) fn on_context_menu_submenu_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if *hovered {
            self.clear_context_menu_keyboard_cursor(cx);
        }
        self.set_context_menu_hover_state(*hovered, true, cx);
    }

    pub(in crate::editor) fn on_context_menu_pointer_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if *hovered {
            self.clear_context_menu_keyboard_cursor(cx);
        }
    }

    pub(super) fn apply_spelling_suggestion(
        &mut self,
        suggestion_index: usize,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ContextMenuState::Spelling {
            entity_id,
            diagnostic,
            ..
        }) = self.context_menu.take()
        else {
            return;
        };
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        let Some(replacement) = diagnostic.replacements.get(suggestion_index).cloned() else {
            cx.notify();
            return;
        };
        let Some(block) = self.focusable_entity_by_id(entity_id) else {
            cx.notify();
            return;
        };
        block.update(cx, move |block, cx| {
            if diagnostic.range.end > block.display_text().len()
                || !block
                    .display_text()
                    .is_char_boundary(diagnostic.range.start)
                || !block.display_text().is_char_boundary(diagnostic.range.end)
                || block.display_text()[diagnostic.range.clone()] != diagnostic.original
            {
                return;
            }
            block.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
            block.replace_text_in_visible_range(diagnostic.range, &replacement, None, false, cx);
        });
        cx.notify();
    }

    pub(in crate::editor) fn on_open_table_insert_dialog(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ContextMenuState::Insert { target, .. }) = self.context_menu.take() else {
            return;
        };
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        self.context_menu_submenu_close_task = None;
        self.table_insert_dialog = Some(TableInsertDialogState {
            target,
            body_rows: 2,
            columns: 2,
        });
        cx.notify();
    }

    pub(in crate::editor) fn on_context_menu_insert_command(
        &mut self,
        command: EditingCommandId,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if command == EditingCommandId::Table {
            self.on_open_table_insert_dialog(event, window, cx);
            return;
        }
        let Some(ContextMenuState::Insert { target, .. }) = self.context_menu.take() else {
            return;
        };
        self.context_menu_keyboard_item = None;
        self.context_menu_keyboard_submenu_item = None;
        self.context_menu_submenu_close_task = None;

        if command == EditingCommandId::Resource {
            let (parent, index) = match target {
                TableInsertTarget::After(entity_id) => self
                    .document
                    .find_block_location(entity_id)
                    .map(|location| (location.parent, location.index + 1))
                    .unwrap_or((None, self.document.root_count())),
                TableInsertTarget::Append => (None, self.document.root_count()),
            };
            // Keep the picker cancellable: the placeholder is an unattached
            // entity and only the selected resource is committed to the tree.
            let placeholder = Self::new_block(cx, BlockRecord::paragraph(String::new()));
            Self::prompt_and_insert_resource(
                self,
                placeholder,
                parent,
                index,
                BlockKind::Paragraph,
                InlineTextTree::plain(String::new()),
                0,
                true,
                cx,
            );
            return;
        }
        let inserted = match command.plan() {
            EditingCommandPlan::InsertImage => Self::new_block(cx, BlockRecord::paragraph("![]()")),
            EditingCommandPlan::InsertMath => Self::new_block(cx, BlockRecord::math("$$\n\n$$")),
            EditingCommandPlan::InsertMermaid => Self::new_block(
                cx,
                BlockRecord::mermaid("```mermaid\nflowchart TD\n    A --> B\n```"),
            ),
            EditingCommandPlan::InsertFootnoteDefinition => {
                let id = Self::next_footnote_id(&self.document.markdown_text(cx));
                Self::new_footnote_definition_block(cx, id)
            }
            EditingCommandPlan::InsertFootnoteReference => return,
            EditingCommandPlan::InsertHorizontalRule => Self::new_block(
                cx,
                BlockRecord::new(BlockKind::Separator, InlineTextTree::plain(String::new())),
            ),
            _ => return,
        };

        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        let (parent, index) = match target {
            TableInsertTarget::After(entity_id) => self
                .document
                .find_block_location(entity_id)
                .map(|location| (location.parent, location.index + 1))
                .unwrap_or((None, self.document.root_count())),
            TableInsertTarget::Append => (None, self.document.root_count()),
        };
        self.document
            .insert_blocks_at(parent, index, vec![inserted.clone()], cx);
        self.ensure_trailing_paragraph_after_structural(&inserted, cx);
        self.rebuild_image_runtimes(cx);
        inserted.update(cx, |inserted, cx| {
            let target = match command {
                EditingCommandId::Image => 2,
                EditingCommandId::Math => 3,
                EditingCommandId::Mermaid => "```mermaid\n".len(),
                _ => 0,
            };
            inserted.assign_collapsed_selection_offset(
                target.min(inserted.visible_len()),
                CollapsedCaretAffinity::Default,
                None,
            );
            cx.notify();
        });
        let focus = if command == EditingCommandId::FootnoteDefinition {
            inserted
                .read(cx)
                .children
                .first()
                .map(Entity::entity_id)
                .unwrap_or_else(|| inserted.entity_id())
        } else {
            inserted.entity_id()
        };
        self.focus_block(focus);
        EditingCommandHistory::record(command, cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        self.request_active_block_scroll_into_view(cx);
        cx.notify();
    }

    pub(in crate::editor) fn on_table_rows_decrement(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(dialog) = self.table_insert_dialog.as_mut() {
            dialog.body_rows = dialog.body_rows.saturating_sub(1).max(1);
            cx.notify();
        }
    }

    pub(in crate::editor) fn on_table_rows_increment(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(dialog) = self.table_insert_dialog.as_mut() {
            dialog.body_rows += 1;
            cx.notify();
        }
    }

    pub(in crate::editor) fn on_table_columns_decrement(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(dialog) = self.table_insert_dialog.as_mut() {
            dialog.columns = dialog.columns.saturating_sub(1).max(1);
            cx.notify();
        }
    }

    pub(in crate::editor) fn on_table_columns_increment(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(dialog) = self.table_insert_dialog.as_mut() {
            dialog.columns += 1;
            cx.notify();
        }
    }

    pub(in crate::editor) fn on_cancel_table_insert_dialog(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_table_insert_dialog(cx);
    }

    pub(in crate::editor) fn on_confirm_table_insert_dialog(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.table_insert_dialog.take() else {
            return;
        };

        let table = TableData::new_empty(dialog.body_rows, dialog.columns);
        let new_block = Self::new_table_block(cx, table);
        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);

        match dialog.target {
            TableInsertTarget::After(entity_id) => {
                if let Some(location) = self.document.find_block_location(entity_id) {
                    self.document.insert_blocks_at(
                        location.parent,
                        location.index + 1,
                        vec![new_block.clone()],
                        cx,
                    );
                } else {
                    self.document.insert_blocks_at(
                        None,
                        self.document.root_count(),
                        vec![new_block.clone()],
                        cx,
                    );
                }
            }
            TableInsertTarget::Append => {
                let insert_at = self
                    .document
                    .first_root()
                    .filter(|root| {
                        self.document.root_count() == 1
                            && root.read(cx).kind() == BlockKind::Paragraph
                            && root.read(cx).display_text().is_empty()
                    })
                    .map_or_else(|| self.document.root_count(), |_| 0);
                self.document
                    .insert_blocks_at(None, insert_at, vec![new_block.clone()], cx);
            }
        }

        // A table inserted as the last block in its container leaves no line
        // below it, so in rendered mode the caret cannot move past the table.
        // Add a trailing empty paragraph to land on when nothing follows it.
        self.ensure_trailing_paragraph_after_structural(&new_block, cx);

        // 先提交源码事务，再为最终块实体安装 runtime；反过来会让投影同步清掉刚创建的 cell。
        self.mark_dirty(cx);
        self.rebuild_table_runtimes(cx);
        if let Some(first_cell) = new_block
            .read(cx)
            .table_runtime
            .as_ref()
            .and_then(|runtime| runtime.header.first())
        {
            self.focus_block(first_cell.entity_id());
        }
        EditingCommandHistory::record(EditingCommandId::Table, cx);
        self.finalize_pending_undo_capture(cx);
        self.request_active_block_scroll_into_view(cx);
        cx.notify();
    }

    pub(super) fn active_axis_menu_selection(&self) -> Option<TableAxisSelection> {
        match self.context_menu.as_ref() {
            Some(ContextMenuState::TableAxis { selection, .. }) => Some(*selection),
            _ => None,
        }
    }

    pub(super) fn on_apply_column_alignment(
        &mut self,
        alignment: TableColumnAlignment,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.active_axis_menu_selection() else {
            return;
        };
        if selection.kind != TableAxisKind::Column {
            return;
        }
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            return;
        };
        self.close_context_menu(cx);
        self.set_table_column_alignment(&table_block, selection.index, alignment, cx);
    }

    pub(in crate::editor) fn on_align_table_column_left(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Left is the default, so emit the unmarked `---` form rather than an
        // explicit `:---`; an explicit colon is only kept when the source had
        // one. This keeps the menu's output unchanged from before.
        self.on_apply_column_alignment(TableColumnAlignment::Default, cx);
    }

    pub(in crate::editor) fn on_align_table_column_center(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.on_apply_column_alignment(TableColumnAlignment::Center, cx);
    }

    pub(in crate::editor) fn on_align_table_column_right(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.on_apply_column_alignment(TableColumnAlignment::Right, cx);
    }

    fn apply_selected_table_row_command(
        &mut self,
        offset: usize,
        duplicate: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.active_axis_menu_selection() else {
            return;
        };
        if selection.kind != TableAxisKind::Row {
            return;
        }
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            return;
        };
        self.close_context_menu(cx);
        if duplicate {
            self.duplicate_table_row(&table_block, selection.index, cx);
        } else {
            self.insert_table_row(&table_block, selection.index + offset, cx);
        }
    }

    pub(in crate::editor) fn on_insert_table_row_before(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_selected_table_row_command(0, false, cx);
    }

    pub(in crate::editor) fn on_insert_table_row_after(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_selected_table_row_command(1, false, cx);
    }

    pub(in crate::editor) fn on_duplicate_table_row(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_selected_table_row_command(0, true, cx);
    }

    fn apply_selected_table_column_command(
        &mut self,
        offset: usize,
        duplicate: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.active_axis_menu_selection() else {
            return;
        };
        if selection.kind != TableAxisKind::Column {
            return;
        }
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            return;
        };
        self.close_context_menu(cx);
        if duplicate {
            self.duplicate_table_column(&table_block, selection.index, cx);
        } else {
            self.insert_table_column(&table_block, selection.index + offset, cx);
        }
    }

    pub(in crate::editor) fn on_insert_table_column_before(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_selected_table_column_command(0, false, cx);
    }

    pub(in crate::editor) fn on_insert_table_column_after(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_selected_table_column_command(1, false, cx);
    }

    pub(in crate::editor) fn on_duplicate_table_column(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_selected_table_column_command(0, true, cx);
    }

    pub(in crate::editor) fn on_move_table_row_up(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.active_axis_menu_selection() else {
            return;
        };
        if selection.kind != TableAxisKind::Row || selection.index == 0 {
            return;
        }
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            return;
        };
        self.close_context_menu(cx);
        self.move_table_row(&table_block, selection.index, -1, cx);
    }

    pub(in crate::editor) fn on_move_table_row_down(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.active_axis_menu_selection() else {
            return;
        };
        if selection.kind != TableAxisKind::Row {
            return;
        }
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            return;
        };
        let can_move = table_block
            .read(cx)
            .record
            .table
            .as_ref()
            .map(|table| selection.index < table.rows.len())
            .unwrap_or(false);
        if !can_move {
            return;
        }
        self.close_context_menu(cx);
        self.move_table_row(&table_block, selection.index, 1, cx);
    }

    pub(in crate::editor) fn on_move_table_column_left(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.active_axis_menu_selection() else {
            return;
        };
        if selection.kind != TableAxisKind::Column || selection.index == 0 {
            return;
        }
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            return;
        };
        self.close_context_menu(cx);
        self.move_table_column(&table_block, selection.index, -1, cx);
    }

    pub(in crate::editor) fn on_move_table_column_right(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.active_axis_menu_selection() else {
            return;
        };
        if selection.kind != TableAxisKind::Column {
            return;
        }
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            return;
        };
        let can_move = table_block
            .read(cx)
            .record
            .table
            .as_ref()
            .map(|table| selection.index + 1 < table.column_count())
            .unwrap_or(false);
        if !can_move {
            return;
        }
        self.close_context_menu(cx);
        self.move_table_column(&table_block, selection.index, 1, cx);
    }

    pub(in crate::editor) fn on_delete_table_row(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.active_axis_menu_selection() else {
            return;
        };
        if selection.kind != TableAxisKind::Row {
            return;
        }
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            return;
        };
        let row_count = table_block
            .read(cx)
            .record
            .table
            .as_ref()
            .map(|table| table.rows.len());
        self.close_context_menu(cx);
        // Visual index 0 is the header. The last row is never removed implicitly;
        // users must choose the explicit “Delete Table” command beside it.
        if selection.index == 0 {
            if row_count != Some(0) {
                self.delete_table_header_row(&table_block, cx);
            }
        } else {
            self.delete_table_row(&table_block, selection.index - 1, cx);
        }
    }

    pub(in crate::editor) fn on_toggle_table_headers(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = !crate::config::EditorSettings::show_table_headers(cx);
        crate::config::EditorSettings::set_show_table_headers(cx, next);
        self.close_context_menu(cx);
        // The preference is read while rendering table cells; re-render the
        // editor (and with it every table) to reflect the new styling.
        cx.notify();
    }

    pub(in crate::editor) fn on_delete_table_column(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.active_axis_menu_selection() else {
            return;
        };
        if selection.kind != TableAxisKind::Column {
            return;
        }
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            return;
        };
        let column_count = table_block
            .read(cx)
            .record
            .table
            .as_ref()
            .map(|table| table.column_count());
        self.close_context_menu(cx);
        // The only column is protected; the adjacent explicit table action owns
        // destructive whole-table removal and makes the consequence unambiguous.
        if column_count.is_some_and(|count| count > 1) {
            self.delete_table_column(&table_block, selection.index, cx);
        }
    }

    pub(in crate::editor) fn on_delete_selected_table(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.active_axis_menu_selection() else {
            return;
        };
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            return;
        };
        self.close_context_menu(cx);
        self.remove_table_block(&table_block, cx);
    }

    pub(super) fn render_axis_menu_item(
        theme: &Theme,
        id: &'static str,
        label: String,
        icon: &'static str,
        enabled: bool,
        keyboard_selected: bool,
        danger: bool,
        on_click: fn(&mut Editor, &ClickEvent, &mut Window, &mut Context<Editor>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let text_color = if danger {
            c.dialog_danger_button_bg
        } else if enabled {
            c.dialog_secondary_button_text
        } else {
            c.dialog_muted
        };
        let row = div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .h(px(d.menu_item_height))
            .px(px(d.menu_item_padding_x))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(d.menu_item_radius))
            .bg(if keyboard_selected {
                c.dialog_secondary_button_hover
            } else {
                c.dialog_surface
            })
            .text_size(px(d.menu_text_size))
            .font_weight(t.dialog_body_weight.to_font_weight())
            .text_color(text_color)
            .child(
                menu_icon_slot(Some(icon), text_color).debug_selector(move || format!("{id}-icon")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .on_hover(cx.listener(Self::on_context_menu_pointer_hover));
        if enabled {
            row.hover(|this| this.bg(c.dialog_secondary_button_hover))
                .cursor_pointer()
                .on_click(cx.listener(on_click))
                .into_any_element()
        } else {
            row.into_any_element()
        }
    }
}

impl Editor {
    fn resource_context_block(&self, _cx: &App) -> Option<gpui::Entity<crate::components::Block>> {
        let entity_id = match self.context_menu.as_ref()? {
            ContextMenuState::Resource { entity_id, .. } => *entity_id,
            _ => return None,
        };
        self.focusable_entity_by_id(entity_id)
    }

    /// Context-menu actions must use the runtime's base-directory-resolved
    /// record. The source record intentionally keeps relative Markdown targets
    /// lexical so save-as and round trips do not rewrite user text.
    pub(in crate::editor) fn resource_context_record(&self, cx: &App) -> Option<ResourceRecord> {
        let block = self.resource_context_block(cx)?;
        let base_dir = self.image_base_dir();
        let block = block.read(cx);
        block
            .resource_runtime()
            .map(|runtime| runtime.record.clone())
            .or_else(|| {
                block
                    .record
                    .resource
                    .as_ref()
                    .map(|record| record.with_base_dir(base_dir.as_deref()))
            })
    }

    /// The menu must not probe the filesystem synchronously. Mounted blocks
    /// publish the cached adapter status; an unmounted/test block remains in
    /// Loading until the next runtime-context synchronization.
    pub(in crate::editor) fn resource_context_status(&self, cx: &App) -> Option<ResourceStatus> {
        let block = self.resource_context_block(cx)?;
        let block = block.read(cx);
        block
            .resource_runtime()
            .map(|runtime| runtime.status.clone())
            .or_else(|| {
                block.record.resource.as_ref().map(|record| {
                    if record.is_unsafe_url() {
                        ResourceStatus::UnsafeScheme
                    } else {
                        ResourceStatus::Loading
                    }
                })
            })
    }

    pub(super) fn open_resource_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.resource_context_block(cx) else {
            return;
        };
        let Some(record) = self.resource_context_record(cx) else {
            return;
        };
        self.close_context_menu(cx);
        block.update(cx, |block, cx| {
            block.request_resource_open(&record, cx);
        });
    }

    pub(super) fn reveal_resource_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let block = self.resource_context_block(cx);
        let path = self
            .resource_context_record(cx)
            .as_ref()
            .and_then(ResourceRecord::local_path)
            .map(std::path::Path::to_path_buf);
        self.close_context_menu(cx);
        if let Some(path) = path
            && let Err(error) = crate::resource_io::reveal_local_resource(&path)
        {
            if let Some(block) = block {
                block.update(cx, |block, cx| block.mark_resource_open_failed(cx));
            }
            eprintln!("failed to reveal resource '{}': {error}", path.display());
        }
    }

    pub(super) fn edit_resource_title_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.resource_context_block(cx) else {
            return;
        };
        let Some(previous) = self.resource_context_record(cx) else {
            return;
        };
        self.close_context_menu(cx);
        let label = previous.label.clone();
        let input = cx.new(|cx| {
            let mut input =
                crate::components::Block::with_record(cx, BlockRecord::paragraph(label));
            input.set_source_raw_mode();
            input
        });
        input.read(cx).focus_handle.focus(window);
        self.resource_title_dialog = Some(crate::editor::ResourceTitleDialogState {
            entity_id: block.entity_id(),
            previous,
            input,
        });
        cx.notify();
    }

    pub(in crate::editor) fn cancel_resource_title_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.resource_title_dialog.take() {
            if self.focusable_entity_by_id(dialog.entity_id).is_some() {
                self.focus_block(dialog.entity_id);
            }
            cx.notify();
        }
    }

    pub(in crate::editor) fn confirm_resource_title_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.resource_title_dialog.take() else {
            return;
        };
        let label = dialog.input.read(cx).display_text().to_owned();
        let Some(block) = self.focusable_entity_by_id(dialog.entity_id) else {
            cx.notify();
            return;
        };
        let destination = if dialog.previous.is_local() {
            dialog.previous.destination.replace('\\', "/")
        } else {
            dialog.previous.destination.clone()
        };
        let base_dir = self.image_base_dir();
        let record = ResourceRecord::from_parts(
            label,
            destination,
            dialog.previous.explicit_kind,
            base_dir.as_deref(),
        );
        let markdown = record.to_markdown();

        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        let title = InlineTextTree::from_markdown(&markdown);
        let cursor = title.visible_len();
        Self::set_block_title_and_kind(&block, block.read(cx).kind(), title, cursor, cx);
        self.rebuild_image_runtimes(cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        self.focus_block(dialog.entity_id);
        cx.notify();
    }

    pub(in crate::editor) fn on_resource_title_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "enter" => {
                self.confirm_resource_title_dialog(cx);
                cx.stop_propagation();
            }
            "escape" => {
                self.cancel_resource_title_dialog(cx);
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    pub(in crate::editor) fn on_confirm_resource_title_dialog(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirm_resource_title_dialog(cx);
    }

    pub(in crate::editor) fn on_cancel_resource_title_dialog(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_resource_title_dialog(cx);
    }

    pub(in crate::editor) fn render_resource_title_dialog_overlay(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.resource_title_dialog.as_ref()?;
        let strings = cx.global::<I18nManager>().strings().clone();
        let title = strings
            .slash_commands
            .get("resource_edit_title")
            .cloned()
            .unwrap_or_else(|| "Edit Resource Title".to_owned());
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        Some(
            modal_overlay("resource-title-dialog-overlay", theme)
                .capture_key_down(cx.listener(Self::on_resource_title_dialog_key_down))
                .child(
                    dialog_panel("resource-title-dialog", d.dialog_width.min(480.0), theme)
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation()
                        })
                        .child(
                            crate::editor::render::dialog_content(
                                "resource-title-dialog-content",
                                theme,
                            )
                            .child(dialog_title_with_icon(
                                "resource-title-dialog-title",
                                title,
                                DialogTitleIcon::Files,
                                theme,
                            ))
                            .child(
                                div()
                                    .text_size(px(t.dialog_body_size))
                                    .text_color(c.dialog_body)
                                    .child(
                                        strings
                                            .slash_commands
                                            .get("resource_title_field")
                                            .cloned()
                                            .unwrap_or_else(|| "Title".to_owned()),
                                    ),
                            )
                            .child(
                                div()
                                    .id("resource-title-dialog-input")
                                    .debug_selector(|| "resource-title-dialog-input".to_owned())
                                    .min_h(px(38.0))
                                    .w_full()
                                    .px(px(8.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.0))
                                    .border(px(d.dialog_border_width))
                                    .border_color(c.dialog_border)
                                    .bg(c.source_mode_block_bg)
                                    .child(dialog.input.clone()),
                            ),
                        )
                        .child(
                            dialog_actions(theme)
                                .child(
                                    dialog_button(
                                        "cancel-resource-title-dialog",
                                        strings.open_link_cancel.clone(),
                                        DialogButtonKind::Secondary,
                                        theme,
                                    )
                                    .on_click(cx.listener(Self::on_cancel_resource_title_dialog)),
                                )
                                .child(
                                    dialog_button(
                                        "confirm-resource-title-dialog",
                                        strings.info_dialog_ok.clone(),
                                        DialogButtonKind::Primary,
                                        theme,
                                    )
                                    .on_click(cx.listener(Self::on_confirm_resource_title_dialog)),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(super) fn replace_resource_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.resource_context_block(cx) else {
            return;
        };
        let entity_id = block.entity_id();
        let Some(previous) = self.resource_context_record(cx) else {
            return;
        };
        self.close_context_menu(cx);
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Replace resource".into()),
        });
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let Ok(Ok(Some(paths))) = prompt.await else {
                return;
            };
            let Some(source) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, move |editor, cx| {
                let behavior = crate::preferences::read_app_preferences()
                    .map(|preferences| preferences.resource_insert_behavior())
                    .unwrap_or(crate::preferences::ImagePasteBehavior::None);
                if editor.file_path.is_none()
                    && behavior != crate::preferences::ImagePasteBehavior::None
                {
                    editor.pending_resource_insertion =
                        Some(crate::editor::PendingResourceInsertion::Replace {
                            entity_id,
                            previous,
                            source,
                        });
                    editor.request_save_document_as(cx);
                    return;
                }
                editor.complete_resource_replacement(entity_id, previous, source, cx);
            });
        })
        .detach();
    }

    pub(crate) fn complete_resource_replacement(
        &mut self,
        entity_id: EntityId,
        previous: ResourceRecord,
        source: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        let behavior = crate::preferences::read_app_preferences()
            .map(|preferences| preferences.resource_insert_behavior())
            .unwrap_or(crate::preferences::ImagePasteBehavior::None);
        let document_path = self.file_path.clone();
        let (markdown, materialized) = match crate::resource_io::resource_markdown_for_path(
            &previous.label,
            &source,
            document_path.as_deref(),
            behavior,
            previous.explicit_kind,
        ) {
            Ok(result) => result,
            Err(error) => {
                self.show_image_paste_error(error, cx);
                return;
            }
        };
        let Some(block) = self.focusable_entity_by_id(entity_id) else {
            materialized.cleanup_if_created();
            return;
        };

        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        let title = InlineTextTree::from_markdown(&markdown);
        let cursor = title.visible_len();
        Self::set_block_title_and_kind(&block, block.read(cx).kind(), title, cursor, cx);
        self.rebuild_image_runtimes(cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        self.focus_block(entity_id);
        cx.notify();
    }

    pub(super) fn copy_resource_address_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(destination) = self
            .resource_context_record(cx)
            .map(|resource| resource.destination)
        else {
            return;
        };
        self.close_context_menu(cx);
        cx.write_to_clipboard(ClipboardItem::new_string(destination));
    }

    pub(super) fn convert_resource_to_link_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.resource_context_block(cx) else {
            return;
        };
        let Some(record) = self.resource_context_record(cx) else {
            return;
        };
        self.close_context_menu(cx);
        let markdown = plain_link_markdown(&record);
        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        let title = InlineTextTree::from_markdown(&markdown);
        let cursor = title.visible_len();
        Self::set_block_title_and_kind(&block, block.read(cx).kind(), title, cursor, cx);
        self.rebuild_image_runtimes(cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        self.focus_block(block.entity_id());
        cx.notify();
    }

    pub(super) fn delete_resource_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.resource_context_block(cx) else {
            return;
        };
        self.close_context_menu(cx);
        block.update(cx, |_, cx| {
            cx.emit(crate::components::BlockEvent::RequestDelete)
        });
    }

    pub(super) fn relocate_resource_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.resource_context_block(cx) else {
            return;
        };
        let entity_id = block.entity_id();
        let document_path = self.file_path.clone();
        let Some(previous) = self.resource_context_record(cx) else {
            return;
        };
        self.close_context_menu(cx);
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Relocate resource".into()),
        });
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let Ok(Ok(Some(paths))) = prompt.await else {
                return;
            };
            let Some(source) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, move |editor, cx| {
                let Some(block) = editor.focusable_entity_by_id(entity_id) else {
                    return;
                };
                let Ok((markdown, _materialized)) = crate::resource_io::resource_markdown_for_path(
                    &previous.label,
                    &source,
                    document_path.as_deref(),
                    crate::preferences::ImagePasteBehavior::None,
                    previous.explicit_kind,
                ) else {
                    return;
                };
                editor.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
                let title = InlineTextTree::from_markdown(&markdown);
                let cursor = title.visible_len();
                Self::set_block_title_and_kind(&block, block.read(cx).kind(), title, cursor, cx);
                editor.rebuild_image_runtimes(cx);
                editor.mark_dirty(cx);
                editor.finalize_pending_undo_capture(cx);
                editor.focus_block(entity_id);
                cx.notify();
            });
        })
        .detach();
    }
}

fn plain_link_markdown(record: &ResourceRecord) -> String {
    let marked = record.to_markdown();
    marked
        .rfind(" \"gmark:resource")
        .map(|index| format!("{}{}", &marked[..index], ')'))
        .unwrap_or(marked)
}
