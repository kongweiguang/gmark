// @author kongweiguang

//! Measured layout inputs for the virtualized structured-document surface.

use super::*;

#[derive(Clone)]
pub(super) struct StructuredPanelLayout {
    pub(super) column_widths: Vec<f32>,
    pub(super) visible_headers: Vec<(usize, String)>,
    pub(super) width: f32,
    pub(super) column_count: usize,
    pub(super) json_structure: bool,
    pub(super) structured_live: bool,
    pub(super) row_count: usize,
    pub(super) loading_text: String,
}

impl DocumentHost {
    pub(super) fn structured_panel_layout(&self, strings: &I18nStrings) -> StructuredPanelLayout {
        let mut headers = self
            .structured_index
            .as_ref()
            .map(|index| index.localized_headers(strings))
            .unwrap_or_default();
        for (target, value) in &self.structured_cell_overrides {
            if target.record.is_none()
                && let Some(header) = headers.get_mut(target.column)
            {
                header.clone_from(value);
            }
        }

        let column_widths = headers
            .iter()
            .enumerate()
            .map(|(column, header)| {
                let sampled_chars = self
                    .structured_rows
                    .values()
                    .chain(self.json_rows.values())
                    .filter_map(|row| {
                        column
                            .checked_sub(row.column_start)
                            .and_then(|relative| row.cells.get(relative))
                    })
                    .map(|cell| cell.chars().take(48).count())
                    .fold(header.chars().take(48).count(), usize::max);
                (28.0 + sampled_chars as f32 * 7.2).clamp(96.0, 374.0)
            })
            .collect::<Vec<_>>();
        let column_count = headers.len();
        let visible_headers = headers
            .into_iter()
            .enumerate()
            .skip(self.structured_column_window_start)
            .take(STRUCTURED_COLUMN_WINDOW)
            .filter(|(column, _)| !self.hidden_structured_columns.contains(column))
            .collect::<Vec<_>>();
        let width = 76.0
            + visible_headers
                .iter()
                .map(|(column, _)| {
                    column_widths
                        .get(*column)
                        .copied()
                        .unwrap_or(STRUCTURED_CELL_WIDTH)
                })
                .sum::<f32>()
                .max(STRUCTURED_CELL_WIDTH);
        let json_structure = matches!(self.structured_index, Some(StructuredIndex::Json { .. }));
        let structured_live = self.view_mode == DocumentHostViewMode::Live
            && matches!(self.structured_index, Some(StructuredIndex::Delimited(_)));
        let row_count = if json_structure {
            self.json_root_index()
                .map_or(0, |root| self.json_visible_count(&[], root))
        } else {
            self.structured_index
                .as_ref()
                .map_or(0, StructuredIndex::row_count)
        };

        StructuredPanelLayout {
            column_widths,
            visible_headers,
            width,
            column_count,
            json_structure,
            structured_live,
            row_count: usize::try_from(row_count).unwrap_or(usize::MAX),
            loading_text: strings.large_document_text("loading"),
        }
    }
}
