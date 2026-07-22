// @author kongweiguang

use std::ops::RangeInclusive;

use gpui::*;

use super::*;

const MAX_TSV_CELLS: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TableCellRectangle {
    pub(super) table_block_id: EntityId,
    pub(super) anchor: TableCellPosition,
    pub(super) focus: TableCellPosition,
}

impl TableCellRectangle {
    pub(super) fn rows(self) -> RangeInclusive<usize> {
        self.anchor.row.min(self.focus.row)..=self.anchor.row.max(self.focus.row)
    }

    pub(super) fn columns(self) -> RangeInclusive<usize> {
        self.anchor.column.min(self.focus.column)..=self.anchor.column.max(self.focus.column)
    }

    fn contains(self, position: TableCellPosition) -> bool {
        self.rows().contains(&position.row) && self.columns().contains(&position.column)
    }
}

pub(super) fn parse_tsv_matrix(text: &str) -> Option<Vec<Vec<String>>> {
    if !text.contains(['\t', '\n', '\r']) {
        return None;
    }
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let normalized = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let rows = normalized
        .split('\n')
        .map(|row| row.split('\t').map(str::to_owned).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if rows.is_empty()
        || width == 0
        || rows.iter().any(|row| row.len() != width)
        || rows.len().saturating_mul(width) > MAX_TSV_CELLS
    {
        return None;
    }
    Some(rows)
}

impl Editor {
    pub(super) fn sync_table_cell_rectangle_highlights(&mut self, cx: &mut Context<Self>) {
        let selection = self.table_cell_rectangle;
        for binding in self.table_cells.values() {
            let selected = selection.is_some_and(|selection| {
                selection.table_block_id == binding.table_block.entity_id()
                    && selection.contains(binding.position)
            });
            binding.cell.update(cx, |cell, cx| {
                let next = if selected {
                    TableAxisHighlight::Selected
                } else {
                    TableAxisHighlight::None
                };
                if cell.table_axis_highlight != next {
                    cell.table_axis_highlight = next;
                    cx.notify();
                }
            });
        }
    }

    pub(super) fn handle_table_cell_selection_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        if self.table_cell_rectangle.is_none() {
            if key != "escape" {
                return false;
            }
            let Some(binding) = self
                .active_entity_id
                .and_then(|id| self.table_cells.get(&id))
                .cloned()
            else {
                return false;
            };
            self.clear_table_axis_selection(cx);
            self.table_cell_rectangle = Some(TableCellRectangle {
                table_block_id: binding.table_block.entity_id(),
                anchor: binding.position,
                focus: binding.position,
            });
            self.sync_table_cell_rectangle_highlights(cx);
            cx.notify();
            return true;
        }

        let selection = self.table_cell_rectangle.expect("checked above");
        if matches!(key, "enter" | "f2" | "escape") {
            self.table_cell_rectangle = None;
            self.sync_table_cell_rectangle_highlights(cx);
            if key != "escape"
                && let Some(table) = self.table_block_by_id(selection.table_block_id, cx)
            {
                self.focus_table_cell_position(&table, selection.focus, cx);
            }
            cx.notify();
            return true;
        }
        if matches!(key, "delete" | "backspace") {
            return self.clear_table_cell_rectangle(selection, cx);
        }
        if !matches!(key, "left" | "right" | "up" | "down") {
            return false;
        }
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            self.table_cell_rectangle = None;
            return true;
        };
        let Some(table) = table_block.read(cx).record.table.as_ref() else {
            self.table_cell_rectangle = None;
            return true;
        };
        let max_row = table.rows.len();
        let max_column = table.column_count().saturating_sub(1);
        let mut focus = selection.focus;
        match key {
            "left" => focus.column = focus.column.saturating_sub(1),
            "right" => focus.column = (focus.column + 1).min(max_column),
            "up" => focus.row = focus.row.saturating_sub(1),
            "down" => focus.row = (focus.row + 1).min(max_row),
            _ => {}
        }
        self.table_cell_rectangle = Some(if event.keystroke.modifiers.shift {
            TableCellRectangle { focus, ..selection }
        } else {
            TableCellRectangle {
                anchor: focus,
                focus,
                ..selection
            }
        });
        self.sync_table_cell_rectangle_highlights(cx);
        cx.notify();
        true
    }

    pub(super) fn clear_table_cell_rectangle(
        &mut self,
        selection: TableCellRectangle,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            return false;
        };
        self.sync_table_record_from_runtime(&table_block, cx);
        let Some(mut table) = table_block.read(cx).record.table.clone() else {
            return false;
        };
        if !table.clear_cell_rectangle(selection.rows(), selection.columns()) {
            return true;
        }
        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        table_block.update(cx, move |block, _cx| block.record.table = Some(table));
        self.rebuild_table_runtimes(cx);
        self.table_cell_rectangle = Some(selection);
        self.sync_table_cell_rectangle_highlights(cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
        true
    }

    pub(super) fn selected_table_cells_tsv(&self, cx: &App) -> Option<String> {
        let selection = self.table_cell_rectangle?;
        let table = self.table_block_by_id(selection.table_block_id, cx)?;
        let table = table.read(cx).record.table.as_ref()?.clone();
        let mut lines = Vec::new();
        for row in selection.rows() {
            let cells = if row == 0 {
                &table.header
            } else {
                &table.rows[row - 1]
            };
            lines.push(
                selection
                    .columns()
                    .map(|column| {
                        cells[column]
                            .visible_text()
                            .replace(['\t', '\n', '\r'], " ")
                    })
                    .collect::<Vec<_>>()
                    .join("\t"),
            );
        }
        Some(lines.join("\n"))
    }

    pub(super) fn paste_table_cells_tsv(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some(matrix) = parse_tsv_matrix(text) else {
            return false;
        };
        let Some(selection) = self.table_cell_rectangle else {
            return false;
        };
        let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
            return false;
        };
        self.sync_table_record_from_runtime(&table_block, cx);
        let Some(mut table) = table_block.read(cx).record.table.clone() else {
            return false;
        };
        let start_row = *selection.rows().start();
        let start_column = *selection.columns().start();
        let required_visual_rows = start_row + matrix.len();
        while table.rows.len() + 1 < required_visual_rows {
            table.append_row();
        }
        let required_columns = start_column + matrix[0].len();
        while table.column_count() < required_columns {
            let alignment = table
                .alignments
                .last()
                .copied()
                .unwrap_or(TableColumnAlignment::Default);
            table.append_column(alignment);
        }
        for (row_offset, values) in matrix.iter().enumerate() {
            let visual_row = start_row + row_offset;
            let cells = if visual_row == 0 {
                &mut table.header
            } else {
                &mut table.rows[visual_row - 1]
            };
            for (column_offset, value) in values.iter().enumerate() {
                cells[start_column + column_offset] = InlineTextTree::plain(value.clone());
            }
        }
        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        table_block.update(cx, move |block, _cx| block.record.table = Some(table));
        self.rebuild_table_runtimes(cx);
        self.table_cell_rectangle = Some(TableCellRectangle {
            table_block_id: selection.table_block_id,
            anchor: TableCellPosition {
                row: start_row,
                column: start_column,
            },
            focus: TableCellPosition {
                row: start_row + matrix.len() - 1,
                column: start_column + matrix[0].len() - 1,
            },
        });
        self.sync_table_cell_rectangle_highlights(cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
        true
    }
}

#[cfg(test)]
#[path = "../../tests/unit/editor/table_selection.rs"]
mod tests;
