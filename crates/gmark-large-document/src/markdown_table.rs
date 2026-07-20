// @author kongweiguang

//! 超大 GFM pipe table 的行级索引与按需单元格切分。

use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::{FileSource, LargeDocumentError, LineIndex, SearchCancellation};

const TABLE_SCAN_BUFFER_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy)]
struct MarkdownFence {
    marker: u8,
    length: usize,
}

#[derive(Clone, Debug)]
pub struct MarkdownTableRow {
    pub row_index: u64,
    pub byte_range: std::ops::Range<u64>,
    pub cells: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MarkdownTableIndex {
    path: std::path::PathBuf,
    lines: LineIndex,
    headers: Vec<String>,
    header_line: u64,
    row_count: u64,
}

impl MarkdownTableIndex {
    pub fn detect(
        source: &FileSource,
        lines: LineIndex,
    ) -> Result<Option<Self>, LargeDocumentError> {
        Ok(Self::detect_all(source, lines)?.into_iter().next())
    }

    /// 单次顺序扫描发现文档内全部 GFM 表格；每一行至多读取一次，避免多表格文档反复扫盘。
    pub fn detect_all(
        source: &FileSource,
        lines: LineIndex,
    ) -> Result<Vec<Self>, LargeDocumentError> {
        Self::detect_all_cancellable(source, lines, &SearchCancellation::default())
    }

    pub fn detect_all_cancellable(
        source: &FileSource,
        lines: LineIndex,
        cancellation: &SearchCancellation,
    ) -> Result<Vec<Self>, LargeDocumentError> {
        let line_count = lines.line_count();
        if line_count < 2 {
            return Ok(Vec::new());
        }

        let file = File::open(source.path()).map_err(|source_error| LargeDocumentError::Io {
            path: source.path().to_path_buf(),
            source: source_error,
        })?;
        let mut reader = BufReader::with_capacity(TABLE_SCAN_BUFFER_BYTES, file);
        let mut previous = Vec::new();
        if !read_next_line(&mut reader, &mut previous, source)? {
            return Ok(Vec::new());
        }
        let mut current = Vec::new();
        let mut previous_line = 0u64;
        let mut fence = None;
        let mut previous_in_fence = update_fence_state(&previous, &mut fence);
        let mut tables = Vec::new();

        while previous_line + 1 < line_count {
            if previous_line.is_multiple_of(1_024) && cancellation.is_cancelled() {
                return Err(LargeDocumentError::Cancelled);
            }
            let delimiter_line = previous_line + 1;
            if !read_next_line(&mut reader, &mut current, source)? {
                break;
            }
            let current_in_fence = update_fence_state(&current, &mut fence);
            if previous_in_fence || current_in_fence || !could_be_delimiter_row(&current) {
                std::mem::swap(&mut previous, &mut current);
                previous_line = delimiter_line;
                previous_in_fence = current_in_fence;
                continue;
            }
            let headers = split_markdown_row(&String::from_utf8_lossy(&previous));
            let delimiters = split_markdown_row(&String::from_utf8_lossy(&current));
            if headers.len() < 2
                || delimiters.len() != headers.len()
                || !delimiters.iter().all(|cell| is_delimiter_cell(cell))
            {
                std::mem::swap(&mut previous, &mut current);
                previous_line = delimiter_line;
                previous_in_fence = current_in_fence;
                continue;
            }

            let header_line = previous_line;
            let mut row_count = 0u64;
            let mut row_line = delimiter_line + 1;
            let mut found_non_row = false;
            while row_line < line_count {
                if row_line.is_multiple_of(1_024) && cancellation.is_cancelled() {
                    return Err(LargeDocumentError::Cancelled);
                }
                if !read_next_line(&mut reader, &mut previous, source)? {
                    break;
                }
                previous_in_fence = update_fence_state(&previous, &mut fence);
                if previous_in_fence
                    || is_blank_line(&previous)
                    || markdown_cell_count(&previous) < 2
                {
                    found_non_row = true;
                    break;
                }
                row_count += 1;
                row_line += 1;
            }
            tables.push(Self {
                path: source.path().to_path_buf(),
                lines: lines.clone(),
                headers,
                header_line,
                row_count,
            });

            if !found_non_row {
                break;
            }
            previous_line = row_line;
        }
        Ok(tables)
    }

    pub fn headers(&self) -> &[String] {
        &self.headers
    }

    pub fn row_count(&self) -> u64 {
        self.row_count
    }

    pub fn header_line(&self) -> u64 {
        self.header_line
    }

    pub fn read_rows(
        &self,
        start: u64,
        count: usize,
    ) -> Result<Vec<MarkdownTableRow>, LargeDocumentError> {
        let source = FileSource::open(&self.path)?;
        let mut rows = Vec::with_capacity(count);
        for row_index in start..(start + count as u64).min(self.row_count) {
            let Some(range) = self.lines.line_range(self.header_line + row_index + 2) else {
                break;
            };
            let bytes = source.read_range(range.start, range.end)?;
            let mut cells = split_markdown_row(&String::from_utf8_lossy(&bytes));
            cells.resize(self.headers.len(), String::new());
            cells.truncate(self.headers.len());
            rows.push(MarkdownTableRow {
                row_index,
                byte_range: range,
                cells,
            });
        }
        Ok(rows)
    }
}

fn update_fence_state(line: &[u8], fence: &mut Option<MarkdownFence>) -> bool {
    let line = line
        .strip_suffix(b"\n")
        .unwrap_or(line)
        .strip_suffix(b"\r")
        .unwrap_or_else(|| line.strip_suffix(b"\n").unwrap_or(line));
    let indentation = line.iter().take_while(|byte| **byte == b' ').count();
    if indentation > 3 || indentation == line.len() {
        return fence.is_some();
    }
    let content = &line[indentation..];
    let marker = content[0];
    if !matches!(marker, b'`' | b'~') {
        return fence.is_some();
    }
    let run = content.iter().take_while(|byte| **byte == marker).count();
    if let Some(open) = *fence {
        if marker == open.marker
            && run >= open.length
            && content[run..].iter().all(|byte| byte.is_ascii_whitespace())
        {
            *fence = None;
        }
        return true;
    }
    if run >= 3 {
        *fence = Some(MarkdownFence {
            marker,
            length: run,
        });
        return true;
    }
    false
}

fn read_next_line(
    reader: &mut BufReader<File>,
    buffer: &mut Vec<u8>,
    source: &FileSource,
) -> Result<bool, LargeDocumentError> {
    buffer.clear();
    reader
        .read_until(b'\n', buffer)
        .map(|read| read > 0)
        .map_err(|source_error| LargeDocumentError::Io {
            path: source.path().to_path_buf(),
            source: source_error,
        })
}

fn could_be_delimiter_row(line: &[u8]) -> bool {
    let mut has_pipe = false;
    let mut hyphens = 0usize;
    for byte in line {
        match byte {
            b'|' => has_pipe = true,
            b'-' => hyphens += 1,
            b':' | b' ' | b'\t' | b'\r' | b'\n' => {}
            _ => return false,
        }
    }
    has_pipe && hyphens >= 6
}

fn is_blank_line(line: &[u8]) -> bool {
    line.iter().all(|byte| byte.is_ascii_whitespace())
}

fn markdown_cell_count(line: &[u8]) -> usize {
    let trimmed = line
        .strip_suffix(b"\n")
        .unwrap_or(line)
        .strip_suffix(b"\r")
        .unwrap_or_else(|| line.strip_suffix(b"\n").unwrap_or(line));
    let mut separators = 0usize;
    let mut escaped = false;
    let mut in_code = false;
    let mut first_separator = None;
    let mut last_separator = None;
    for (index, byte) in trimmed.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' => escaped = true,
            b'`' => in_code = !in_code,
            b'|' if !in_code => {
                separators += 1;
                first_separator.get_or_insert(index);
                last_separator = Some(index);
            }
            _ => {}
        }
    }
    let first_content = trimmed.iter().position(|byte| !byte.is_ascii_whitespace());
    let last_content = trimmed.iter().rposition(|byte| !byte.is_ascii_whitespace());
    let leading = usize::from(first_separator.is_some() && first_separator == first_content);
    let trailing = usize::from(last_separator.is_some() && last_separator == last_content);
    separators
        .saturating_add(1)
        .saturating_sub(leading)
        .saturating_sub(trailing)
}

fn is_delimiter_cell(cell: &str) -> bool {
    let trimmed = cell.trim().trim_matches(':');
    trimmed.len() >= 3 && trimmed.bytes().all(|byte| byte == b'-')
}

fn split_markdown_row(line: &str) -> Vec<String> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    let mut code_ticks = 0usize;
    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '`' {
            code_ticks ^= 1;
            current.push(ch);
            continue;
        }
        if ch == '|' && code_ticks == 0 {
            cells.push(current.trim().to_owned());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    if escaped {
        current.push('\\');
    }
    cells.push(current.trim().to_owned());
    if line.trim_start().starts_with('|') && cells.first().is_some_and(String::is_empty) {
        cells.remove(0);
    }
    if line.trim_end().ends_with('|') && cells.last().is_some_and(String::is_empty) {
        cells.pop();
    }
    cells
}
