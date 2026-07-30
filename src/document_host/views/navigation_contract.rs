// @author kongweiguang

//! Sidebar target contracts and state snapshots.

use super::*;

pub(super) const MAX_STRUCTURED_CACHED_ROWS: usize = STRUCTURED_OVERSCAN_ROWS * 6;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DocumentSidebarTarget {
    Column { column: usize },
    StructuredRow { row: u64, offset: u64, json: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocumentSidebarNodeSnapshot {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) secondary: String,
    pub(crate) depth: usize,
    pub(crate) expandable: bool,
    pub(crate) target: DocumentSidebarTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocumentSidebarMetadata {
    pub(crate) length: u64,
    pub(crate) lines: u64,
    pub(crate) encoding: String,
    pub(crate) line_endings: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocumentSidebarSnapshot {
    pub(crate) format: DocumentMenuFormat,
    pub(crate) metadata: DocumentSidebarMetadata,
    pub(crate) document_epoch: u64,
    pub(crate) revision: u64,
    pub(crate) generation: u64,
    pub(crate) nodes: Vec<DocumentSidebarNodeSnapshot>,
}

pub(super) fn prune_structured_row_cache<T>(
    rows: &mut BTreeMap<u64, T>,
    requested_center: u64,
    max_rows: usize,
) {
    while rows.len() > max_rows {
        let first = rows.first_key_value().map(|(row, _)| *row);
        let last = rows.last_key_value().map(|(row, _)| *row);
        let evicted = match (first, last) {
            (Some(first), Some(last))
                if requested_center.saturating_sub(first)
                    >= last.saturating_sub(requested_center) =>
            {
                first
            }
            (_, Some(last)) => last,
            _ => break,
        };
        rows.remove(&evicted);
    }
}

impl DocumentHost {
    fn structured_sidebar_row(&self, display_row: u64) -> Option<StructuredRow> {
        if let Some(node) = self.json_node_at(display_row) {
            return self.json_rows.get(&node.path()).cloned();
        }
        self.structured_rows
            .values()
            .find(|row| row.index == display_row)
            .cloned()
    }

    /// Read-only navigation projection. Only already indexed/cached rows are
    /// returned; rendering a large document therefore never triggers a full
    /// parse or a second complete in-memory copy.
    pub(crate) fn document_sidebar_snapshot(&self) -> DocumentSidebarSnapshot {
        let format = self.document_menu_format();
        let mut nodes = Vec::new();
        match format {
            DocumentMenuFormat::Csv | DocumentMenuFormat::Tsv => {
                if let Some(StructuredIndex::Delimited(index)) = self.structured_index.as_ref() {
                    nodes.extend(index.headers().iter().enumerate().map(|(column, header)| {
                        let label = if header.trim().is_empty() {
                            format!("Column {}", column + 1)
                        } else {
                            header.clone()
                        };
                        DocumentSidebarNodeSnapshot {
                            id: format!("column:{column}"),
                            label,
                            secondary: (column + 1).to_string(),
                            depth: 0,
                            expandable: false,
                            target: DocumentSidebarTarget::Column { column },
                        }
                    }));
                }
            }
            DocumentMenuFormat::Json | DocumentMenuFormat::JsonLines => {
                let count = self
                    .json_root_index()
                    .map_or_else(
                        || {
                            self.structured_index
                                .as_ref()
                                .map_or(0, StructuredIndex::row_count)
                        },
                        |root| self.json_visible_count(&[], root),
                    )
                    .min(128);
                for display_row in 0..count {
                    let Some(row) = self.structured_sidebar_row(display_row) else {
                        continue;
                    };
                    let path = self
                        .json_node_at(display_row)
                        .map(|node| node.path())
                        .unwrap_or_default();
                    nodes.push(DocumentSidebarNodeSnapshot {
                        id: if path.is_empty() {
                            format!("row:{display_row}")
                        } else {
                            format!(
                                "json:{}",
                                path.iter()
                                    .map(u64::to_string)
                                    .collect::<Vec<_>>()
                                    .join("/")
                            )
                        },
                        label: row.cells.first().cloned().unwrap_or_default(),
                        secondary: row.cells.get(1).cloned().unwrap_or_default(),
                        depth: row.depth,
                        expandable: self
                            .json_node_at(display_row)
                            .is_some_and(|node| self.json_child_indexes.contains_key(&node.path())),
                        target: DocumentSidebarTarget::StructuredRow {
                            row: display_row,
                            offset: row.byte_range.start,
                            json: format == DocumentMenuFormat::Json,
                        },
                    });
                }
            }
            DocumentMenuFormat::Markdown | DocumentMenuFormat::Text => {}
        }
        DocumentSidebarSnapshot {
            format,
            metadata: DocumentSidebarMetadata {
                length: self.document_length(),
                lines: self.document_line_count(),
                encoding: self.encoding_label(),
                line_endings: self.document_line_ending_label(),
            },
            document_epoch: self.document_epoch,
            revision: self
                .document
                .as_ref()
                .map_or(0, |document| document.revision()),
            generation: self.structured_generation,
            nodes,
        }
    }
}
