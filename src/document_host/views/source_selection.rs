// @author kongweiguang

//! Source selection synchronization and visual state.

use super::*;

impl DocumentHost {
    pub(super) fn selection_spans_multiple_lines(&self, selection: SourceSelection) -> bool {
        let Some(document) = self.document.as_ref() else {
            return false;
        };
        let range = selection.range();
        let start = document.line_for_offset(range.start);
        let end = document.line_for_offset(range.end.saturating_sub(1));
        start.zip(end).is_some_and(|(start, end)| start != end)
    }

    pub(super) fn set_source_selection(
        &mut self,
        selection: SourceSelection,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        document.set_source_selection(selection);
        document.view_state.source.selection = document.source_selection();
        let normalized = document.source_selection().range();
        let start_line = document
            .line_for_offset(normalized.start)
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or_default();
        let end_offset = if normalized.is_empty() {
            normalized.end
        } else {
            normalized.end.saturating_sub(1)
        };
        let end_line = document
            .line_for_offset(end_offset)
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or(start_line);
        self.selection_anchor = document
            .line_for_offset(selection.anchor.byte_offset)
            .and_then(|line| usize::try_from(line).ok());
        self.selected_lines = Some(start_line..end_line.saturating_add(1));
        self.error = None;
        cx.notify();
    }

    pub(super) fn sync_source_selection_visuals(&mut self, cx: &mut Context<Self>) {
        let rows = self
            .source_row_blocks
            .iter()
            .map(|(line, block)| (*line, block.clone()))
            .collect::<Vec<_>>();
        for (line, block) in rows {
            self.apply_source_selection_visual(line, &block, cx);
        }
    }

    pub(super) fn apply_source_selection_visual(
        &self,
        line: usize,
        block: &Entity<Block>,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let Some(row) = self.displayed_screen_lines.row(line) else {
            return;
        };
        let selection = document.source_selection();
        let normalized = selection.range();
        let intersection_start = normalized.start.max(row.content_range.start);
        let intersection_end = normalized.end.min(row.content_range.end);
        let is_active_local = self
            .active_edit
            .as_ref()
            .is_some_and(|active| active.line == line)
            && !self.selection_spans_multiple_lines(selection);
        let search_range = normalized
            .is_empty()
            .then(|| self.selected_search_range(line))
            .flatten()
            .filter(|_| {
                self.active_edit
                    .as_ref()
                    .is_none_or(|active| active.line != line)
            });
        block.update(cx, |block, cx| {
            if is_active_local {
                block.editor_selection_range = None;
            } else if intersection_start < intersection_end {
                block.editor_selection_range = Some(
                    usize::try_from(intersection_start - row.content_range.start)
                        .unwrap_or_default()
                        ..usize::try_from(intersection_end - row.content_range.start)
                            .unwrap_or(block.display_text().len()),
                );
            } else {
                block.editor_selection_range = search_range;
            }
            cx.notify();
        });
    }
}
