// @author kongweiguang

use super::*;

impl Editor {
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
}
