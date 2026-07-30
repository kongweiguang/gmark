// @author kongweiguang

//! Native Markdown table data model plus parse and serialize helpers.
//!
//! Tables are supported as native blocks at the root level and inside
//! quote-like containers in rendered mode. More complex nested contexts that
//! are still outside the runtime-safe subset continue to use raw-Markdown
//! fallback paths.

use gmark_markdown::{
    BlockKind as MarkdownBlockKind, Table as MarkdownTable, TableCell as MarkdownTableCell,
    parse_markdown, serialize_table_canonical,
};
use gpui::{Entity, FontStyle, FontWeight, Pixels, SharedString, TextRun, Window, px};

use crate::components::{Block, InlineTextTree};
use crate::theme::Theme;

/// Horizontal alignment declared by the table's delimiter row.
pub use gmark_markdown::TableAlignment as TableColumnAlignment;

/// Axis kinds addressable by rendered-mode native table maintenance UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAxisKind {
    /// Table row axis.
    Row,
    /// Table column axis.
    Column,
}

/// A row or column marker inside one native table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableAxisMarker {
    pub kind: TableAxisKind,
    pub index: usize,
}

/// Visual emphasis level used when previewing or selecting table axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableAxisHighlight {
    /// No axis emphasis.
    #[default]
    None,
    /// Hover preview emphasis.
    Preview,
    /// Persistent selected-axis emphasis.
    Selected,
}

/// Persistent cell contents for a native table block.
#[derive(Debug, Clone)]
pub struct TableData {
    pub header: Vec<InlineTextTree>,
    pub rows: Vec<Vec<InlineTextTree>>,
    pub alignments: Vec<TableColumnAlignment>,
}

impl PartialEq for TableData {
    fn eq(&self, other: &Self) -> bool {
        self.header == other.header
            && self.rows == other.rows
            && self.alignments == other.alignments
    }
}

impl Eq for TableData {}

impl TableData {
    /// Projects a pure Markdown table into the editor-owned, interactive cell
    /// model. Editing state stays in this package; parsing and canonical
    /// Markdown spelling stay in `gmark-markdown`.
    pub(crate) fn from_markdown_value(value: &MarkdownTable) -> Self {
        let mut value = value.clone();
        value.normalize_shape();
        Self {
            header: value
                .header
                .iter()
                .map(table_cell_from_markdown_value)
                .collect(),
            rows: value
                .rows
                .iter()
                .map(|row| row.iter().map(table_cell_from_markdown_value).collect())
                .collect(),
            alignments: value.alignments,
        }
    }

    /// Exposes a rendering-neutral table snapshot for canonical Markdown
    /// serialization. This creates no GPUI entities and has no I/O effects.
    pub(crate) fn markdown_value(&self) -> MarkdownTable {
        let mut value = MarkdownTable {
            alignments: self.alignments.clone(),
            header: self
                .header
                .iter()
                .map(table_cell_to_markdown_value)
                .collect(),
            rows: self
                .rows
                .iter()
                .map(|row| row.iter().map(table_cell_to_markdown_value).collect())
                .collect(),
        };
        value.normalize_shape();
        value
    }

    /// Creates an empty table with one header row, `body_rows` body rows, and
    /// `columns` left-aligned columns.
    pub fn new_empty(body_rows: usize, columns: usize) -> Self {
        let columns = columns.max(1);
        let header = (0..columns)
            .map(|_| InlineTextTree::plain(String::new()))
            .collect::<Vec<_>>();
        let rows = (0..body_rows.max(1))
            .map(|_| {
                (0..columns)
                    .map(|_| InlineTextTree::plain(String::new()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let alignments = vec![TableColumnAlignment::Default; columns];
        Self {
            header,
            rows,
            alignments,
        }
    }

    pub(crate) fn column_count(&self) -> usize {
        self.header
            .len()
            .max(self.alignments.len())
            .max(self.rows.iter().map(Vec::len).max().unwrap_or(0))
            .max(1)
    }

    fn normalize_shape(&mut self) {
        let columns = self.column_count();
        while self.header.len() < columns {
            self.header.push(InlineTextTree::plain(String::new()));
        }
        while self.alignments.len() < columns {
            self.alignments.push(TableColumnAlignment::Default);
        }
        for row in &mut self.rows {
            while row.len() < columns {
                row.push(InlineTextTree::plain(String::new()));
            }
        }
    }

    /// Appends one empty body row while preserving the current column count.
    pub fn append_row(&mut self) {
        self.normalize_shape();
        let columns = self.column_count();
        self.rows.push(
            (0..columns)
                .map(|_| InlineTextTree::plain(String::new()))
                .collect(),
        );
    }

    /// Appends one empty column to the header and every body row.
    pub fn append_column(&mut self, alignment: TableColumnAlignment) {
        self.normalize_shape();
        self.header.push(InlineTextTree::plain(String::new()));
        self.alignments.push(alignment);
        for row in &mut self.rows {
            row.push(InlineTextTree::plain(String::new()));
        }
    }

    /// Inserts an empty row at a visual index (`0` is the header).
    pub fn insert_empty_visual_row(&mut self, visual_index: usize) -> bool {
        self.normalize_shape();
        if visual_index > self.rows.len() + 1 {
            return false;
        }
        let empty = (0..self.column_count())
            .map(|_| InlineTextTree::plain(String::new()))
            .collect::<Vec<_>>();
        if visual_index == 0 {
            let previous = std::mem::replace(&mut self.header, empty);
            self.rows.insert(0, previous);
        } else {
            self.rows.insert(visual_index - 1, empty);
        }
        true
    }

    /// Duplicates one visual row immediately after itself.
    pub fn duplicate_visual_row(&mut self, visual_index: usize) -> bool {
        self.normalize_shape();
        let row = if visual_index == 0 {
            self.header.clone()
        } else if let Some(row) = self.rows.get(visual_index - 1) {
            row.clone()
        } else {
            return false;
        };
        self.rows.insert(visual_index, row);
        true
    }

    pub fn insert_empty_column(&mut self, column: usize, alignment: TableColumnAlignment) -> bool {
        self.normalize_shape();
        if column > self.column_count() {
            return false;
        }
        self.header
            .insert(column, InlineTextTree::plain(String::new()));
        self.alignments.insert(column, alignment);
        for row in &mut self.rows {
            row.insert(column, InlineTextTree::plain(String::new()));
        }
        true
    }

    /// Duplicates one column immediately to its right, including alignment.
    pub fn duplicate_column(&mut self, column: usize) -> bool {
        self.normalize_shape();
        if column >= self.column_count() {
            return false;
        }
        self.header.insert(column + 1, self.header[column].clone());
        self.alignments.insert(column + 1, self.alignments[column]);
        for row in &mut self.rows {
            row.insert(column + 1, row[column].clone());
        }
        true
    }

    /// Clears a rectangular visual-cell selection without changing table shape.
    pub fn clear_cell_rectangle(
        &mut self,
        rows: std::ops::RangeInclusive<usize>,
        columns: std::ops::RangeInclusive<usize>,
    ) -> bool {
        self.normalize_shape();
        let (row_start, row_end) = (*rows.start(), *rows.end());
        let (column_start, column_end) = (*columns.start(), *columns.end());
        if row_start > row_end
            || column_start > column_end
            || row_end > self.rows.len()
            || column_end >= self.column_count()
        {
            return false;
        }
        let mut changed = false;
        for visual_row in row_start..=row_end {
            let row = if visual_row == 0 {
                &mut self.header
            } else {
                &mut self.rows[visual_row - 1]
            };
            for cell in &mut row[column_start..=column_end] {
                changed |= !cell.visible_text().is_empty();
                *cell = InlineTextTree::plain(String::new());
            }
        }
        changed
    }

    /// Sets the alignment of one column if it exists.
    pub fn set_column_alignment(&mut self, column: usize, alignment: TableColumnAlignment) {
        self.normalize_shape();
        if let Some(slot) = self.alignments.get_mut(column) {
            *slot = alignment;
        }
    }

    /// Swaps two rows addressed by their visual index, where row `0` is the
    /// header and rows `1..=rows.len()` are the body rows. Swapping the header
    /// with the first body row exchanges header and body content, mirroring how
    /// the row handles treat the header as just another movable row.
    pub fn swap_visual_rows(&mut self, row_a: usize, row_b: usize) {
        self.normalize_shape();
        let total = self.rows.len() + 1;
        if row_a >= total || row_b >= total || row_a == row_b {
            return;
        }
        match (row_a, row_b) {
            (0, other) | (other, 0) => {
                std::mem::swap(&mut self.header, &mut self.rows[other - 1]);
            }
            (a, b) => self.rows.swap(a - 1, b - 1),
        }
    }

    /// Swaps two columns across header, body, and alignment vectors.
    pub fn swap_columns(&mut self, col_a: usize, col_b: usize) {
        self.normalize_shape();
        let columns = self.column_count();
        if col_a >= columns || col_b >= columns || col_a == col_b {
            return;
        }

        self.header.swap(col_a, col_b);
        self.alignments.swap(col_a, col_b);
        for row in &mut self.rows {
            row.swap(col_a, col_b);
        }
    }

    /// Removes one body row while preserving at least one body row.
    pub fn remove_body_row(&mut self, row_index: usize) {
        self.normalize_shape();
        if row_index >= self.rows.len() {
            return;
        }
        // A table may be left header-only; the editor removes the whole block
        // when the header itself is then deleted.
        self.rows.remove(row_index);
    }

    /// Removes the header row by promoting the first body row into its place.
    /// Returns false (leaving the table unchanged) when there are no body rows,
    /// since a pipe table must keep a header row.
    pub fn remove_header_row(&mut self) -> bool {
        self.normalize_shape();
        if self.rows.is_empty() {
            return false;
        }
        self.header = self.rows.remove(0);
        true
    }

    /// Removes one column while preserving at least one column.
    pub fn remove_column(&mut self, col_index: usize) {
        self.normalize_shape();
        let columns = self.column_count();
        if columns <= 1 || col_index >= columns {
            return;
        }

        self.header.remove(col_index);
        self.alignments.remove(col_index);
        for row in &mut self.rows {
            row.remove(col_index);
        }
    }
}

fn table_cell_from_markdown_value(cell: &MarkdownTableCell) -> InlineTextTree {
    InlineTextTree::from_markdown_values(&cell.inlines)
}

fn table_cell_to_markdown_value(cell: &InlineTextTree) -> MarkdownTableCell {
    MarkdownTableCell {
        source: gmark_markdown::SourceRange::empty(0),
        inlines: cell.markdown_values(),
    }
}

/// Responsive width fractions shared by every row of a native table.
#[derive(Debug, Clone, PartialEq)]
pub struct TableColumnLayout {
    fractions: Vec<f32>,
}

impl TableColumnLayout {
    pub fn equal(column_count: usize) -> Self {
        let column_count = column_count.max(1);
        let fraction = 1.0 / column_count as f32;
        Self {
            fractions: vec![fraction; column_count],
        }
    }

    #[cfg(test)]
    pub(crate) fn fractions(&self) -> &[f32] {
        &self.fractions
    }

    pub fn fraction(&self, column: usize) -> f32 {
        self.fractions
            .get(column)
            .copied()
            .unwrap_or_else(|| 1.0 / self.fractions.len().max(1) as f32)
    }

    pub fn measure(
        table: &TableData,
        table_width: f32,
        window: &mut Window,
        theme: &Theme,
    ) -> Self {
        let preferred_widths = measure_preferred_column_widths(table, window, theme)
            .into_iter()
            .map(f32::from)
            .collect::<Vec<_>>();
        Self::from_preferred_widths(&preferred_widths, table_width, minimum_column_width(theme))
    }

    pub fn from_preferred_widths(
        preferred_widths: &[f32],
        table_width: f32,
        min_column_width: f32,
    ) -> Self {
        if preferred_widths.is_empty() {
            return Self::equal(1);
        }

        let column_count = preferred_widths.len();
        let safe_table_width = table_width.max(1.0);
        let equal_share = safe_table_width / column_count as f32;
        if preferred_widths
            .iter()
            .all(|preferred| *preferred <= equal_share + f32::EPSILON)
        {
            return Self::equal(column_count);
        }

        let floor_width = min_column_width
            .max(0.0)
            .min(safe_table_width / column_count as f32);
        let weights = preferred_widths
            .iter()
            .map(|preferred| preferred.max(equal_share))
            .collect::<Vec<_>>();
        let mut assigned_widths = vec![0.0; column_count];
        let mut remaining_indices = (0..column_count).collect::<Vec<_>>();
        let mut remaining_width = safe_table_width;

        loop {
            if remaining_indices.is_empty() {
                break;
            }

            let weight_sum = remaining_indices
                .iter()
                .map(|index| weights[*index])
                .sum::<f32>();
            if weight_sum <= f32::EPSILON {
                let share = remaining_width / remaining_indices.len() as f32;
                for index in remaining_indices {
                    assigned_widths[index] = share;
                }
                break;
            }

            let mut newly_floored = Vec::new();
            for index in &remaining_indices {
                let width = remaining_width * (weights[*index] / weight_sum);
                if width < floor_width - f32::EPSILON {
                    newly_floored.push(*index);
                } else {
                    assigned_widths[*index] = width;
                }
            }

            if newly_floored.is_empty() {
                break;
            }

            if newly_floored.len() == remaining_indices.len() {
                let share = remaining_width / remaining_indices.len() as f32;
                for index in remaining_indices {
                    assigned_widths[index] = share;
                }
                break;
            }

            for index in &newly_floored {
                assigned_widths[*index] = floor_width;
                remaining_width -= floor_width;
            }
            remaining_indices.retain(|index| !newly_floored.contains(index));
        }

        let assigned_sum = assigned_widths.iter().sum::<f32>();
        if assigned_sum <= f32::EPSILON {
            return Self::equal(column_count);
        }

        let fractions = assigned_widths
            .into_iter()
            .map(|width| width / assigned_sum)
            .collect::<Vec<_>>();
        Self { fractions }
    }
}

/// Runtime-only location of a cell inside a native table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableCellPosition {
    /// Zero-based visual row. Header is row `0`; first body row is `1`.
    pub row: usize,
    pub column: usize,
}

impl TableCellPosition {
    pub fn is_header(self) -> bool {
        self.row == 0
    }

    pub fn body_row_index(self) -> Option<usize> {
        self.row.checked_sub(1)
    }
}

/// Runtime cell editors attached to one native table block.
#[derive(Clone)]
pub struct TableRuntime {
    pub header: Vec<Entity<Block>>,
    pub rows: Vec<Vec<Entity<Block>>>,
}

impl TableRuntime {
    pub fn cell(&self, position: TableCellPosition) -> Option<Entity<Block>> {
        if position.is_header() {
            self.header.get(position.column).cloned()
        } else {
            self.rows
                .get(position.body_row_index()?)
                .and_then(|row| row.get(position.column))
                .cloned()
        }
    }
}

fn measure_preferred_column_widths(
    table: &TableData,
    window: &mut Window,
    theme: &Theme,
) -> Vec<Pixels> {
    let column_count = table.header.len().max(1);
    let mut preferred_widths = vec![Pixels::ZERO; column_count];

    for (column, cell) in table.header.iter().enumerate() {
        preferred_widths[column] =
            preferred_widths[column].max(measure_cell_preferred_width(cell, true, window, theme));
    }

    for row in &table.rows {
        for (column, cell) in row.iter().enumerate().take(column_count) {
            preferred_widths[column] = preferred_widths[column]
                .max(measure_cell_preferred_width(cell, false, window, theme));
        }
    }

    preferred_widths
}

fn measure_cell_preferred_width(
    cell: &InlineTextTree,
    is_header: bool,
    window: &mut Window,
    theme: &Theme,
) -> Pixels {
    let cache = cell.render_cache();
    let text = cache.visible_text();
    let cell_chrome_width = cell_chrome_width(theme);
    if text.is_empty() {
        return cell_chrome_width;
    }

    let display_text = SharedString::from(text.to_string());
    let mut font = window.text_style().font();
    if is_header && font.weight < FontWeight::MEDIUM {
        font.weight = FontWeight::MEDIUM;
    }
    let base_run = TextRun {
        len: display_text.len(),
        font,
        color: theme.colors.text_default,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let runs = measurement_runs(&cache, &base_run);
    let font_size = px(theme.typography.text_size);

    let text_width = window
        .text_system()
        .shape_text(display_text, font_size, &runs, None, None)
        .ok()
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.width())
                .max()
                .unwrap_or(Pixels::ZERO)
        })
        .unwrap_or(Pixels::ZERO);

    text_width + cell_chrome_width
}

fn measurement_runs(
    cache: &crate::components::InlineRenderCache,
    base_run: &TextRun,
) -> Vec<TextRun> {
    let mut boundaries = vec![0, cache.visible_text().len()];
    for span in cache.spans() {
        boundaries.push(span.range.start);
        boundaries.push(span.range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut runs = Vec::new();
    for boundary_pair in boundaries.windows(2) {
        let start = boundary_pair[0];
        let end = boundary_pair[1];
        if start >= end {
            continue;
        }

        let inline_style = cache.style_at(start);
        let mut font = base_run.font.clone();
        if inline_style.bold && font.weight < FontWeight::BOLD {
            font.weight = FontWeight::BOLD;
        }
        if inline_style.italic {
            font.style = FontStyle::Italic;
        }

        runs.push(TextRun {
            len: end - start,
            font,
            color: base_run.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    if runs.is_empty() {
        vec![base_run.clone()]
    } else {
        runs
    }
}

fn cell_chrome_width(theme: &Theme) -> Pixels {
    px(theme.dimensions.table_cell_padding_x * 2.0 + 2.0)
}

fn minimum_column_width(theme: &Theme) -> f32 {
    theme.dimensions.table_cell_padding_x * 2.0 + theme.typography.text_size * 4.0 + 2.0
}

fn strip_table_indent(line: &str) -> Option<&str> {
    let indent = line.bytes().take_while(|b| *b == b' ').count();
    (indent <= 3).then_some(&line[indent..])
}

fn split_table_cells(line: &str) -> Option<Vec<String>> {
    let rest = strip_table_indent(line)?.trim_end();
    if rest.is_empty() {
        return None;
    }
    // Outer pipes are optional (GFM): strip them when present so pipeless rows
    // like `Name | Score` split the same way as `| Name | Score |`.
    let inner = rest.strip_prefix('|').unwrap_or(rest);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut escaping = false;
    let chars = inner.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    let mut code_ticks = None;

    while index < chars.len() {
        let ch = chars[index];
        if escaping {
            match ch {
                '|' | '\\' => current.push(ch),
                _ => {
                    current.push('\\');
                    current.push(ch);
                }
            }
            escaping = false;
            index += 1;
            continue;
        }

        if ch == '`' {
            let run = chars[index..]
                .iter()
                .take_while(|candidate| **candidate == '`')
                .count();
            current.extend(std::iter::repeat_n('`', run));
            match code_ticks {
                Some(open) if open == run => code_ticks = None,
                None => code_ticks = Some(run),
                _ => {}
            }
            index += run;
            continue;
        }

        match ch {
            '\\' if code_ticks.is_none() => escaping = true,
            '|' if code_ticks.is_none() => {
                cells.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
        index += 1;
    }

    if escaping {
        current.push('\\');
    }
    cells.push(current.trim().to_string());
    Some(cells)
}

fn parse_alignment_cell(cell: &str) -> Option<TableColumnAlignment> {
    let trimmed = cell.trim();
    if trimmed.len() < 3 {
        return None;
    }

    let left = trimmed.starts_with(':');
    let right = trimmed.ends_with(':');
    let core = trimmed.trim_start_matches(':').trim_end_matches(':');
    if core.len() < 3 || !core.chars().all(|ch| ch == '-') {
        return None;
    }

    Some(match (left, right) {
        (true, true) => TableColumnAlignment::Center,
        (false, true) => TableColumnAlignment::Right,
        (true, false) => TableColumnAlignment::Left,
        (false, false) => TableColumnAlignment::Default,
    })
}

#[path = "../table/parsing.rs"]
mod parsing;
pub(crate) use parsing::serialize_table_cell_markdown;
pub use parsing::{
    collect_pipeless_table_region, collect_root_table_candidate_region,
    collect_table_candidate_region, is_root_table_candidate_line, is_table_candidate_line,
    is_table_row_candidate, parse_root_table_region, parse_table_body_row,
    parse_table_fragment_rows, parse_table_region, serialize_table_markdown_lines,
};

#[cfg(test)]
#[path = "../../../../tests/unit/components/markdown/table.rs"]
mod tests;
