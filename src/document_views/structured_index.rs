// @author kongweiguang

use super::*;

impl StructuredIndex {
    pub(super) fn active_markdown_table(&self) -> Option<&MarkdownTableIndex> {
        let Self::MarkdownTables { tables, selected } = self else {
            return None;
        };
        tables.get(*selected)
    }

    pub(super) fn headers(&self) -> Vec<String> {
        match self {
            Self::Delimited(index) => index.headers().to_vec(),
            Self::MarkdownTables { .. } => self
                .active_markdown_table()
                .map_or_else(Vec::new, |index| index.headers().to_vec()),
            Self::Json { .. } => vec!["Key / Index".to_owned(), "Value".to_owned()],
            Self::JsonLines { .. } => vec!["JSON value".to_owned()],
        }
    }

    pub(super) fn localized_headers(&self, strings: &I18nStrings) -> Vec<String> {
        match self {
            Self::Json { .. } => vec![
                strings.large_document_text("key_or_index"),
                strings.large_document_text("value"),
            ],
            Self::JsonLines { .. } => vec![strings.large_document_text("json_value")],
            _ => self.headers(),
        }
    }

    pub(super) fn row_count(&self) -> u64 {
        match self {
            Self::Delimited(index) => index.record_count(),
            Self::MarkdownTables { .. } => self
                .active_markdown_table()
                .map_or(0, MarkdownTableIndex::row_count),
            Self::Json { index, .. } => index.item_count(),
            Self::JsonLines { record_count, .. } => *record_count,
        }
    }

    pub(super) fn read_rows(
        &self,
        start: u64,
        count: usize,
        columns: Range<usize>,
    ) -> Result<Vec<StructuredRow>, gmark_paged_document::PagedDocumentError> {
        match self {
            Self::Delimited(index) => index
                .read_records_columns(start, count, columns.clone())
                .map(|rows| {
                    let visible_columns = columns
                        .end
                        .min(index.column_count())
                        .saturating_sub(columns.start);
                    rows.into_iter()
                        .map(|row| {
                            let mut cells = row.fields;
                            cells.resize(visible_columns, String::new());
                            StructuredRow {
                                index: row.record_index,
                                byte_range: row.byte_range,
                                column_start: columns.start,
                                cells,
                                depth: 0,
                            }
                        })
                        .collect()
                }),
            Self::MarkdownTables { .. } => self
                .active_markdown_table()
                .map_or_else(|| Ok(Vec::new()), |index| index.read_rows(start, count))
                .map(|rows| {
                    rows.into_iter()
                        .map(|row| StructuredRow {
                            index: row.row_index,
                            byte_range: row.byte_range,
                            column_start: columns.start,
                            cells: row
                                .cells
                                .into_iter()
                                .skip(columns.start)
                                .take(columns.end.saturating_sub(columns.start))
                                .collect(),
                            depth: 0,
                        })
                        .collect()
                }),
            Self::Json { index, source } => {
                let mut rows = Vec::with_capacity(count);
                for item in start..(start + count as u64).min(index.item_count()) {
                    let Some(range) = index.item_range(item)? else {
                        break;
                    };
                    rows.push(StructuredRow {
                        index: item,
                        byte_range: range,
                        column_start: 0,
                        cells: read_json_cells(index, source, item)?,
                        depth: 0,
                    });
                }
                Ok(rows)
            }
            Self::JsonLines {
                lines,
                source,
                record_count,
            } => {
                let mut rows = Vec::with_capacity(count);
                for item in start..(start + count as u64).min(*record_count) {
                    let Some(range) = lines.line_range(item) else {
                        break;
                    };
                    let end = range.end.min(range.start + STRUCTURED_CELL_BYTES as u64);
                    let bytes = source.read_range(range.start..end)?;
                    let mut text = String::from_utf8_lossy(&bytes)
                        .trim_end_matches(['\r', '\n'])
                        .to_owned();
                    if end < range.end {
                        text.push('…');
                    }
                    rows.push(StructuredRow {
                        index: item,
                        byte_range: range,
                        column_start: 0,
                        cells: vec![text],
                        depth: 0,
                    });
                }
                Ok(rows)
            }
        }
    }
}
