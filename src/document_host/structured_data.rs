// @author kongweiguang

//! Structured-document indexing, validation, and bounded cell readers.

use super::*;

pub(super) fn search_document_reader(
    document: Option<&SharedDocument>,
    provisional_source: Option<&FileSource>,
    query: &str,
    options: SearchOptions,
    cancellation: &SearchCancellation,
) -> Result<Vec<SearchMatch>, gmark_paged_document::PagedDocumentError> {
    if let Some(document) = document {
        document.search(query, options, cancellation)
    } else if let Some(source) = provisional_source {
        search_file_source(source, query, options, cancellation)
    } else {
        Ok(Vec::new())
    }
}

pub(super) fn build_structured_index(
    source: &FileSource,
    lines: &LineIndex,
    format: DocumentFormat,
    cancellation: &SearchCancellation,
    snapshot: Option<Arc<[u8]>>,
) -> Result<Option<StructuredIndex>, gmark_paged_document::PagedDocumentError> {
    match format {
        DocumentFormat::Delimited { delimiter } => {
            let options = DelimitedIndexOptions {
                delimiter,
                ..DelimitedIndexOptions::default()
            };
            let cache_dir = index_cache_dir();
            let index = if let Some(snapshot) = snapshot {
                DelimitedIndex::build_snapshot_cancellable(snapshot, options, cancellation)
            } else if let Some(cache_dir) = cache_dir {
                match DelimitedIndex::build_cached_cancellable(
                    source,
                    options,
                    cache_dir,
                    cancellation,
                ) {
                    Ok(index) => Ok(index),
                    Err(gmark_paged_document::PagedDocumentError::Io { .. }) => {
                        eprintln!("large-document index cache write failed; using uncached build");
                        DelimitedIndex::build_cancellable(source, options, cancellation)
                    }
                    Err(error) => Err(error),
                }
            } else {
                DelimitedIndex::build_cancellable(source, options, cancellation)
            }?;
            Ok(Some(StructuredIndex::Delimited(index)))
        }
        DocumentFormat::Markdown => {
            MarkdownTableIndex::detect_all_cancellable(source, lines.clone(), cancellation).map(
                |tables| {
                    (!tables.is_empty()).then_some(StructuredIndex::MarkdownTables {
                        tables,
                        selected: 0,
                    })
                },
            )
        }
        DocumentFormat::Json => {
            let options = JsonIndexOptions::default();
            let cache_dir = index_cache_dir();
            let index = if let Some(cache_dir) = cache_dir {
                match JsonIndex::build_cached_cancellable(source, options, cache_dir, cancellation)
                {
                    Ok(index) => Ok(index),
                    Err(gmark_paged_document::PagedDocumentError::Io { .. }) => {
                        eprintln!("large-document JSON cache write failed; using uncached build");
                        JsonIndex::build_cancellable(source, options, cancellation)
                    }
                    Err(error) => Err(error),
                }
            } else {
                JsonIndex::build_cancellable(source, options, cancellation)
            }?;
            Ok(Some(StructuredIndex::Json {
                index,
                source: source.clone(),
            }))
        }
        DocumentFormat::JsonLines => {
            let (lines, source, record_count) = if let Some(snapshot) = snapshot {
                let ranges = snapshot_line_ranges(&snapshot);
                validate_json_lines_snapshot(&snapshot, &ranges, cancellation)?;
                let lines = StructuredLines::Snapshot(ranges.into());
                let record_count = structured_json_lines_record_count(&lines);
                (
                    lines,
                    StructuredTextSource::Snapshot(snapshot),
                    record_count,
                )
            } else {
                validate_json_lines_cancellable(source, lines, cancellation)?;
                let lines = StructuredLines::File(lines.clone());
                let record_count = structured_json_lines_record_count(&lines);
                (
                    lines,
                    StructuredTextSource::File(source.clone()),
                    record_count,
                )
            };
            Ok(Some(StructuredIndex::JsonLines {
                lines,
                source,
                record_count,
            }))
        }
        DocumentFormat::PlainText => Ok(None),
    }
}

/// Returns the persistent large-document cache only when its cache root can be
/// validated and created safely. A missing/unusable cache must never turn into
/// a persistent temporary directory; callers fall back to an uncached build.
fn index_cache_dir() -> Option<PathBuf> {
    let dirs = match gmark_config::AppDirs::from_system() {
        Ok(dirs) => dirs,
        Err(error) => {
            eprintln!("large-document index cache disabled: {error:#}");
            return None;
        }
    };
    let cache_dir = dirs.large_document_indexes_dir();
    if let Err(error) = dirs.ensure_cache_parent(&cache_dir.join(".gmark-index-root")) {
        eprintln!("large-document index cache disabled: {error:#}");
        return None;
    }
    Some(cache_dir)
}

/// Build the structured variants that can consume a resident Controller
/// snapshot directly.  This keeps untitled/registry-backed tables and JSONL
/// views functional without materializing a second authoritative body.
pub(super) fn build_structured_index_from_snapshot(
    snapshot: Arc<[u8]>,
    format: DocumentFormat,
    cancellation: &SearchCancellation,
) -> Result<Option<StructuredIndex>, gmark_paged_document::PagedDocumentError> {
    match format {
        DocumentFormat::Delimited { delimiter } => {
            let options = DelimitedIndexOptions {
                delimiter,
                ..DelimitedIndexOptions::default()
            };
            DelimitedIndex::build_snapshot_cancellable(snapshot, options, cancellation)
                .map(StructuredIndex::Delimited)
                .map(Some)
        }
        DocumentFormat::JsonLines => {
            let ranges = snapshot_line_ranges(&snapshot);
            validate_json_lines_snapshot(&snapshot, &ranges, cancellation)?;
            let lines = StructuredLines::Snapshot(ranges.into());
            let record_count = structured_json_lines_record_count(&lines);
            Ok(Some(StructuredIndex::JsonLines {
                lines,
                source: StructuredTextSource::Snapshot(snapshot),
                record_count,
            }))
        }
        // Markdown/JSON indexes retain file-backed readers today.  Callers
        // may fall back to the existing file identity when one exists.
        DocumentFormat::Markdown | DocumentFormat::Json | DocumentFormat::PlainText => Ok(None),
    }
}

pub(super) fn structured_json_lines_record_count(lines: &StructuredLines) -> u64 {
    lines
        .line_count()
        .checked_sub(1)
        .filter(|last| {
            lines
                .line_range(*last)
                .is_some_and(|range| range.start == range.end)
        })
        .unwrap_or_else(|| lines.line_count())
}

fn snapshot_line_ranges(bytes: &[u8]) -> Vec<Range<u64>> {
    let mut ranges = Vec::new();
    let mut start = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            ranges.push(start as u64..(index + 1) as u64);
            start = index + 1;
        }
    }
    ranges.push(start as u64..bytes.len() as u64);
    ranges
}

fn validate_json_lines_snapshot(
    bytes: &[u8],
    lines: &[Range<u64>],
    cancellation: &SearchCancellation,
) -> Result<(), PagedDocumentError> {
    for (line, range) in lines.iter().enumerate() {
        if line.is_multiple_of(1_024) && cancellation.is_cancelled() {
            return Err(PagedDocumentError::Cancelled);
        }
        let value = bytes[range.start as usize..range.end as usize]
            .strip_suffix(b"\n")
            .unwrap_or(&bytes[range.start as usize..range.end as usize]);
        let value = value.strip_suffix(b"\r").unwrap_or(value);
        if value.is_empty() && line + 1 == lines.len() {
            continue;
        }
        serde_json::from_slice::<serde_json::Value>(value).map_err(|error| {
            PagedDocumentError::InvalidJson {
                offset: range
                    .start
                    .saturating_add((error.column() as u64).saturating_sub(1)),
                message: error.to_string(),
            }
        })?;
    }
    Ok(())
}

pub(super) fn read_json_cells(
    index: &JsonIndex,
    source: &FileSource,
    item: u64,
) -> Result<Vec<String>, gmark_paged_document::PagedDocumentError> {
    let Some((key, value)) = index.item_key_value_ranges(item)? else {
        return Ok(Vec::new());
    };
    let label = if let Some(key) = key {
        let complete = key.end.saturating_sub(key.start) <= STRUCTURED_CELL_BYTES as u64;
        let end = key.end.min(key.start + STRUCTURED_CELL_BYTES as u64);
        let bytes = source.read_range(key.start, end)?;
        if complete {
            serde_json::from_slice::<String>(&bytes)
                .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).into_owned())
        } else {
            let mut label = String::from_utf8_lossy(&bytes).into_owned();
            label.push('…');
            label
        }
    } else {
        item.to_string()
    };
    Ok(vec![label, read_json_preview(source, value)?])
}

fn read_json_preview(
    source: &FileSource,
    range: Range<u64>,
) -> Result<String, gmark_paged_document::PagedDocumentError> {
    let end = range.end.min(range.start + STRUCTURED_CELL_BYTES as u64);
    let bytes = source.read_range(range.start, end)?;
    let mut preview = String::from_utf8_lossy(&bytes).replace(['\r', '\n'], " ");
    if end < range.end {
        preview.push('…');
    }
    Ok(preview)
}

pub(super) fn truncate_cell(mut value: String) -> String {
    if value.len() <= STRUCTURED_CELL_BYTES {
        return value;
    }
    let mut end = STRUCTURED_CELL_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value.push('…');
    value
}
