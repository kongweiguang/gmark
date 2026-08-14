// @author kongweiguang

use super::*;

const MAX_TABLE_CELLS: usize = 10_000;

/// 用 checked 行列乘加计算表头和正文的总单元格，确保确认前不会发生大分配。
fn checked_table_cell_count(body_rows: usize, columns: usize) -> Option<usize> {
    body_rows
        .max(1)
        .checked_add(1)
        .and_then(|rows| rows.checked_mul(columns.max(1)))
}

impl Editor {
    /// 表格尺寸错误放在底部状态提示中，保留对话框和用户已填参数，避免超限点击
    /// 产生半张表或把大规模分配放到 GPUI 主线程。
    pub(in crate::editor) fn show_table_limit_notice(&mut self, cx: &mut Context<Self>) {
        self.pane_notice = Some("表格总单元格数不能超过 10,000".into());
        let weak = cx.entity().downgrade();
        self.pane_notice_task = Some(cx.spawn(async move |_this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(3))
                .await;
            let _ = weak.update(cx, |editor, cx| {
                editor.pane_notice = None;
                editor.pane_notice_task = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(in crate::editor) fn open_table_insert_dialog_for_target(
        &mut self,
        target: TableInsertTarget,
        cx: &mut Context<Self>,
    ) {
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

    pub(in crate::editor) fn on_open_table_insert_dialog(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ContextMenuState::Insert { target, .. }) = self.context_menu.take() else {
            return;
        };
        self.open_table_insert_dialog_for_target(target, cx);
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

    /// 增加行前先做 checked 单元格预算，超限时保留对话框而不启动任何表格分配。
    pub(in crate::editor) fn on_table_rows_increment(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let exceeded = self.table_insert_dialog.as_mut().is_some_and(|dialog| {
            match dialog.body_rows.checked_add(1) {
                Some(next_rows)
                    if checked_table_cell_count(next_rows, dialog.columns)
                        .is_some_and(|cells| cells <= MAX_TABLE_CELLS) =>
                {
                    dialog.body_rows = next_rows;
                    false
                }
                _ => true,
            }
        });
        if exceeded {
            self.show_table_limit_notice(cx);
        } else if self.table_insert_dialog.is_some() {
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

    /// 增加列前先做 checked 单元格预算，确保边界错误只显示提示而不改变尺寸。
    pub(in crate::editor) fn on_table_columns_increment(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let exceeded = self.table_insert_dialog.as_mut().is_some_and(|dialog| {
            match dialog.columns.checked_add(1) {
                Some(next_columns)
                    if checked_table_cell_count(dialog.body_rows, next_columns)
                        .is_some_and(|cells| cells <= MAX_TABLE_CELLS) =>
                {
                    dialog.columns = next_columns;
                    false
                }
                _ => true,
            }
        });
        if exceeded {
            self.show_table_limit_notice(cx);
        } else if self.table_insert_dialog.is_some() {
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

    /// 确认时再次校验总单元格数，防止状态被外部修改后产生半张超限表格。
    pub(in crate::editor) fn on_confirm_table_insert_dialog(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog_state) = self.table_insert_dialog.as_ref() else {
            return;
        };
        if checked_table_cell_count(dialog_state.body_rows, dialog_state.columns)
            .is_none_or(|cells| cells > MAX_TABLE_CELLS)
        {
            self.show_table_limit_notice(cx);
            return;
        }
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
}

#[cfg(test)]
#[path = "../../../../tests/unit/editor/table_insert_limits.rs"]
mod tests;
