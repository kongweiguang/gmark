// @author kongweiguang

use super::*;

pub(crate) fn serialize_table_cell_markdown(tree: &InlineTextTree) -> String {
    tree.serialize_markdown()
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', " ")
}

/// Returns true when a line is a candidate native table row in the current
/// container scope.
pub fn is_table_candidate_line(line: &str) -> bool {
    strip_table_indent(line)
        .map(str::trim_end)
        .is_some_and(|rest| rest.starts_with('|'))
}

/// Number of pipe-separated cells in `line`, treating outer pipes as optional
/// (GFM) so pipeless rows like `Name | Score` are recognized. Returns `None`
/// for single-column lines so prose containing a stray `|` is not mistaken for
/// a table row.
pub fn table_row_column_count(line: &str) -> Option<usize> {
    split_table_cells(line)
        .map(|cells| cells.len())
        .filter(|count| *count >= 2)
}

/// True when `line` could be a table row, including a pipeless GFM row.
pub fn is_table_row_candidate(line: &str) -> bool {
    table_row_column_count(line).is_some()
}

/// Collects a contiguous table-candidate region in the current container
/// scope.
pub fn collect_table_candidate_region(lines: &[String], start: usize) -> usize {
    let mut index = start + 1;
    while index < lines.len() && is_table_candidate_line(&lines[index]) {
        index += 1;
    }
    index
}

/// Parses a pipe-table region through the pure Markdown value model.
pub fn parse_table_region(lines: &[String]) -> Option<TableData> {
    let document = parse_markdown(&lines.join("\n"));
    document
        .blocks
        .into_iter()
        .find_map(|block| match block.kind {
            MarkdownBlockKind::Table(table) => Some(TableData::from_markdown_value(&table)),
            _ => None,
        })
}

/// Returns true when `line` is a delimiter row of exactly `columns` cells, each
/// a valid alignment specifier.
fn is_delimiter_row(line: &str, columns: usize) -> bool {
    split_table_cells(line).is_some_and(|cells| {
        cells.len() == columns
            && cells
                .iter()
                .all(|cell| parse_alignment_cell(cell).is_some())
    })
}

/// Detects a table that starts at `start` without requiring outer pipes,
/// returning the region end (exclusive) when `lines[start]` is a multi-column
/// header followed by a matching delimiter row. Body rows extend to the next
/// blank line, matching GFM. Returns `None` for ordinary prose so a stray `|`
/// is never mistaken for a table; single-column pipeless candidates are also
/// rejected because they are ambiguous with setext headings.
pub fn collect_pipeless_table_region(lines: &[String], start: usize) -> Option<usize> {
    let header = split_table_cells(lines.get(start)?)?;
    if header.len() < 2 {
        return None;
    }
    if !is_delimiter_row(lines.get(start + 1)?, header.len()) {
        return None;
    }

    let mut end = start + 2;
    while end < lines.len() && !lines[end].trim().is_empty() {
        end += 1;
    }
    Some(end)
}

/// Returns true when a root-level line is a candidate native table row.
pub fn is_root_table_candidate_line(line: &str) -> bool {
    is_table_candidate_line(line)
}

/// Collects a contiguous root-level table candidate region.
pub fn collect_root_table_candidate_region(lines: &[String], start: usize) -> usize {
    collect_table_candidate_region(lines, start)
}

/// Parses a root-level pipe table region into native table data.
pub fn parse_root_table_region(lines: &[String]) -> Option<TableData> {
    parse_table_region(lines)
}

/// Parses a single table body row, normalized to `columns` cells (padded when
/// short, truncated when long). Returns `None` when the line is not a table
/// row at all.
pub fn parse_table_body_row(line: &str, columns: usize) -> Option<Vec<InlineTextTree>> {
    let mut cells = split_table_cells(line)?;
    cells.resize(columns, String::new());
    Some(
        cells
            .into_iter()
            .map(|cell| InlineTextTree::from_markdown(&cell))
            .collect(),
    )
}

/// Parses incomplete GFM table body rows for an explicit merge affordance.
/// Unlike ordinary GFM body parsing, fragment rows must already match the
/// target width so accepting a merge can never silently pad or drop cells.
pub fn parse_table_fragment_rows(
    lines: &[String],
    columns: usize,
) -> Option<Vec<Vec<InlineTextTree>>> {
    if lines.is_empty() || columns < 2 {
        return None;
    }

    let mut rows = Vec::with_capacity(lines.len());
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || !(trimmed.starts_with('|') || trimmed.ends_with('|')) {
            return None;
        }
        let cells = split_table_cells(line)?;
        if cells.len() != columns
            || cells
                .iter()
                .all(|cell| parse_alignment_cell(cell).is_some())
        {
            return None;
        }
        rows.push(
            cells
                .into_iter()
                .map(|cell| InlineTextTree::from_markdown(&cell))
                .collect(),
        );
    }
    Some(rows)
}

/// Serializes native table data to canonical pipe-table Markdown lines.
pub fn serialize_table_markdown_lines(table: &TableData) -> Vec<String> {
    serialize_table_canonical(&table.markdown_value())
        .split('\n')
        .map(ToOwned::to_owned)
        .collect()
}
