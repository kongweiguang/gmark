// @author kongweiguang

use super::*;

pub(super) fn json_sampled_hash(source: &FileSource, len: u64) -> Result<u32, PagedDocumentError> {
    let mut hasher = crc32fast::Hasher::new();
    for start in [
        0,
        len.saturating_sub(JSON_SIDECAR_SAMPLE_BYTES) / 2,
        len.saturating_sub(JSON_SIDECAR_SAMPLE_BYTES),
    ] {
        let end = (start + JSON_SIDECAR_SAMPLE_BYTES).min(len);
        if start < end {
            hasher.update(&source.read_range(start, end)?);
        }
    }
    Ok(hasher.finalize())
}

pub(super) fn json_cache_data_error(path: &Path, message: String) -> PagedDocumentError {
    PagedDocumentError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, message),
    }
}

pub(super) fn cleanup_json_sidecars(
    cache_dir: &Path,
    keep: &Path,
    budget_bytes: u64,
) -> Result<(), PagedDocumentError> {
    let entries = std::fs::read_dir(cache_dir).map_err(|source_error| PagedDocumentError::Io {
        path: cache_dir.to_path_buf(),
        source: source_error,
    })?;
    let mut total = std::fs::metadata(keep).map_or(0, |metadata| metadata.len());
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep
            || !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".gmark-json-v"))
        {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        total = total.saturating_add(metadata.len());
        candidates.push((
            metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            metadata.len(),
            path,
        ));
    }
    candidates.sort_by_key(|(modified, _, _)| *modified);
    for (_, len, path) in candidates {
        if total <= budget_bytes {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum ObjectState {
    KeyOrEnd,
    Key,
    Colon,
    Value,
    CommaOrEnd,
}

#[derive(Clone, Copy)]
enum ArrayState {
    ValueOrEnd,
    Value,
    CommaOrEnd,
}

#[derive(Clone, Copy)]
enum JsonFrame {
    Object(ObjectState),
    Array(ArrayState),
}

/// 迭代状态机完整校验 JSON 语法，不构造 DOM，也不缓存字符串或数字正文。
pub(super) fn validate_json_range(
    source: &FileSource,
    range: std::ops::Range<u64>,
    cancellation: &SearchCancellation,
) -> Result<(), PagedDocumentError> {
    let mut cursor =
        ByteCursor::new_cancellable(source.clone(), range.start, range.end, cancellation);
    let Some((offset, first)) = next_non_whitespace(&mut cursor)? else {
        return Err(invalid_json(range.start, "document is empty"));
    };
    let mut frames = Vec::new();
    parse_json_value(&mut cursor, offset, first, &mut frames)?;
    let mut root_complete = frames.is_empty();

    while !root_complete {
        let Some(frame) = frames.last().copied() else {
            return Err(invalid_json(offset, "validator lost the root container"));
        };
        match frame {
            JsonFrame::Object(ObjectState::KeyOrEnd | ObjectState::Key) => {
                let allow_end = matches!(frame, JsonFrame::Object(ObjectState::KeyOrEnd));
                let Some((offset, byte)) = next_non_whitespace(&mut cursor)? else {
                    return Err(invalid_json(range.end, "unterminated object"));
                };
                if byte == b'}' && allow_end {
                    frames.pop();
                    root_complete = frames.is_empty();
                } else if byte == b'"' {
                    parse_json_string(&mut cursor, range.end)?;
                    if let Some(last) = frames.last_mut() {
                        *last = JsonFrame::Object(ObjectState::Colon);
                    }
                } else {
                    return Err(invalid_json(offset, "object key must be a string"));
                }
            }
            JsonFrame::Object(ObjectState::Colon) => {
                let Some((offset, byte)) = next_non_whitespace(&mut cursor)? else {
                    return Err(invalid_json(range.end, "missing ':' after object key"));
                };
                if byte != b':' {
                    return Err(invalid_json(offset, "missing ':' after object key"));
                }
                if let Some(last) = frames.last_mut() {
                    *last = JsonFrame::Object(ObjectState::Value);
                }
            }
            JsonFrame::Object(ObjectState::Value) => {
                let Some((offset, byte)) = next_non_whitespace(&mut cursor)? else {
                    return Err(invalid_json(range.end, "missing object value"));
                };
                if let Some(last) = frames.last_mut() {
                    *last = JsonFrame::Object(ObjectState::CommaOrEnd);
                }
                parse_json_value(&mut cursor, offset, byte, &mut frames)?;
            }
            JsonFrame::Object(ObjectState::CommaOrEnd) => {
                let Some((offset, byte)) = next_non_whitespace(&mut cursor)? else {
                    return Err(invalid_json(range.end, "unterminated object"));
                };
                match byte {
                    b',' => {
                        if let Some(last) = frames.last_mut() {
                            *last = JsonFrame::Object(ObjectState::Key);
                        }
                    }
                    b'}' => {
                        frames.pop();
                        root_complete = frames.is_empty();
                    }
                    _ => return Err(invalid_json(offset, "expected ',' or '}'")),
                }
            }
            JsonFrame::Array(ArrayState::ValueOrEnd | ArrayState::Value) => {
                let allow_end = matches!(frame, JsonFrame::Array(ArrayState::ValueOrEnd));
                let Some((offset, byte)) = next_non_whitespace(&mut cursor)? else {
                    return Err(invalid_json(range.end, "unterminated array"));
                };
                if byte == b']' && allow_end {
                    frames.pop();
                    root_complete = frames.is_empty();
                } else {
                    if let Some(last) = frames.last_mut() {
                        *last = JsonFrame::Array(ArrayState::CommaOrEnd);
                    }
                    parse_json_value(&mut cursor, offset, byte, &mut frames)?;
                }
            }
            JsonFrame::Array(ArrayState::CommaOrEnd) => {
                let Some((offset, byte)) = next_non_whitespace(&mut cursor)? else {
                    return Err(invalid_json(range.end, "unterminated array"));
                };
                match byte {
                    b',' => {
                        if let Some(last) = frames.last_mut() {
                            *last = JsonFrame::Array(ArrayState::Value);
                        }
                    }
                    b']' => {
                        frames.pop();
                        root_complete = frames.is_empty();
                    }
                    _ => return Err(invalid_json(offset, "expected ',' or ']'")),
                }
            }
        }
    }
    if let Some((offset, _)) = next_non_whitespace(&mut cursor)? {
        return Err(invalid_json(offset, "trailing characters after root value"));
    }
    Ok(())
}

fn parse_json_value(
    cursor: &mut ByteCursor,
    offset: u64,
    first: u8,
    frames: &mut Vec<JsonFrame>,
) -> Result<(), PagedDocumentError> {
    match first {
        b'{' => push_json_frame(frames, JsonFrame::Object(ObjectState::KeyOrEnd), offset),
        b'[' => push_json_frame(frames, JsonFrame::Array(ArrayState::ValueOrEnd), offset),
        b'"' => parse_json_string(cursor, cursor.end),
        b't' => consume_json_keyword(cursor, offset, b"rue"),
        b'f' => consume_json_keyword(cursor, offset, b"alse"),
        b'n' => consume_json_keyword(cursor, offset, b"ull"),
        b'-' | b'0'..=b'9' => parse_json_number(cursor, offset, first),
        _ => Err(invalid_json(offset, "expected a JSON value")),
    }
}

fn push_json_frame(
    frames: &mut Vec<JsonFrame>,
    frame: JsonFrame,
    offset: u64,
) -> Result<(), PagedDocumentError> {
    if frames.len() >= MAX_JSON_DEPTH {
        return Err(invalid_json(offset, "JSON nesting limit exceeded"));
    }
    frames.push(frame);
    Ok(())
}

fn parse_json_string(cursor: &mut ByteCursor, end: u64) -> Result<(), PagedDocumentError> {
    while let Some((offset, byte)) = cursor.next_byte()? {
        match byte {
            b'"' => return Ok(()),
            b'\\' => {
                let Some((escape_offset, escape)) = cursor.next_byte()? else {
                    return Err(invalid_json(end, "unterminated string escape"));
                };
                match escape {
                    b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {}
                    b'u' => parse_json_unicode_escape(cursor, escape_offset)?,
                    _ => return Err(invalid_json(escape_offset, "invalid string escape")),
                }
            }
            0x00..=0x1f => {
                return Err(invalid_json(
                    offset,
                    "unescaped control character in string",
                ));
            }
            0x80..=0xff => validate_json_utf8_sequence(cursor, offset, byte)?,
            _ => {}
        }
    }
    Err(invalid_json(end, "unterminated string"))
}

fn parse_json_unicode_escape(
    cursor: &mut ByteCursor,
    escape_offset: u64,
) -> Result<(), PagedDocumentError> {
    let value = parse_hex_quad(cursor, escape_offset)?;
    if (0xd800..=0xdbff).contains(&value) {
        let Some((slash_offset, slash)) = cursor.next_byte()? else {
            return Err(invalid_json(escape_offset, "missing low surrogate"));
        };
        let Some((u_offset, marker)) = cursor.next_byte()? else {
            return Err(invalid_json(slash_offset, "missing low surrogate"));
        };
        if slash != b'\\' || marker != b'u' {
            return Err(invalid_json(u_offset, "missing low surrogate"));
        }
        let low = parse_hex_quad(cursor, u_offset)?;
        if !(0xdc00..=0xdfff).contains(&low) {
            return Err(invalid_json(u_offset, "invalid low surrogate"));
        }
    } else if (0xdc00..=0xdfff).contains(&value) {
        return Err(invalid_json(escape_offset, "unexpected low surrogate"));
    }
    Ok(())
}

fn parse_hex_quad(cursor: &mut ByteCursor, offset: u64) -> Result<u16, PagedDocumentError> {
    let mut value = 0u16;
    for _ in 0..4 {
        let Some((digit_offset, byte)) = cursor.next_byte()? else {
            return Err(invalid_json(offset, "incomplete unicode escape"));
        };
        let digit = match byte {
            b'0'..=b'9' => (byte - b'0') as u16,
            b'a'..=b'f' => (byte - b'a' + 10) as u16,
            b'A'..=b'F' => (byte - b'A' + 10) as u16,
            _ => return Err(invalid_json(digit_offset, "invalid unicode escape")),
        };
        value = (value << 4) | digit;
    }
    Ok(value)
}

fn validate_json_utf8_sequence(
    cursor: &mut ByteCursor,
    offset: u64,
    first: u8,
) -> Result<(), PagedDocumentError> {
    let constraints: &[(u8, u8)] = match first {
        0xc2..=0xdf => &[(0x80, 0xbf)],
        0xe0 => &[(0xa0, 0xbf), (0x80, 0xbf)],
        0xe1..=0xec | 0xee..=0xef => &[(0x80, 0xbf), (0x80, 0xbf)],
        0xed => &[(0x80, 0x9f), (0x80, 0xbf)],
        0xf0 => &[(0x90, 0xbf), (0x80, 0xbf), (0x80, 0xbf)],
        0xf1..=0xf3 => &[(0x80, 0xbf), (0x80, 0xbf), (0x80, 0xbf)],
        0xf4 => &[(0x80, 0x8f), (0x80, 0xbf), (0x80, 0xbf)],
        _ => return Err(invalid_json(offset, "invalid UTF-8 in string")),
    };
    for (minimum, maximum) in constraints {
        let Some((continuation_offset, byte)) = cursor.next_byte()? else {
            return Err(invalid_json(offset, "incomplete UTF-8 in string"));
        };
        if byte < *minimum || byte > *maximum {
            return Err(invalid_json(
                continuation_offset,
                "invalid UTF-8 continuation byte",
            ));
        }
    }
    Ok(())
}

fn consume_json_keyword(
    cursor: &mut ByteCursor,
    offset: u64,
    remaining: &[u8],
) -> Result<(), PagedDocumentError> {
    for expected in remaining {
        let Some((byte_offset, byte)) = cursor.next_byte()? else {
            return Err(invalid_json(offset, "incomplete JSON literal"));
        };
        if byte != *expected {
            return Err(invalid_json(byte_offset, "invalid JSON literal"));
        }
    }
    ensure_json_value_delimiter(cursor, offset)
}

fn parse_json_number(
    cursor: &mut ByteCursor,
    offset: u64,
    first: u8,
) -> Result<(), PagedDocumentError> {
    let mut digit = first;
    if digit == b'-' {
        let Some((digit_offset, next)) = cursor.next_byte()? else {
            return Err(invalid_json(offset, "incomplete number"));
        };
        if !next.is_ascii_digit() {
            return Err(invalid_json(digit_offset, "expected digit after '-'"));
        }
        digit = next;
    }

    let mut next = if digit == b'0' {
        let next = cursor.next_byte()?;
        if next.is_some_and(|(_, byte)| byte.is_ascii_digit()) {
            return Err(invalid_json(
                next.map_or(offset, |value| value.0),
                "leading zero",
            ));
        }
        next
    } else if digit.is_ascii_digit() {
        loop {
            let candidate = cursor.next_byte()?;
            if !candidate.is_some_and(|(_, byte)| byte.is_ascii_digit()) {
                break candidate;
            }
        }
    } else {
        return Err(invalid_json(offset, "invalid number"));
    };

    if next.is_some_and(|(_, byte)| byte == b'.') {
        let Some((fraction_offset, fraction)) = cursor.next_byte()? else {
            return Err(invalid_json(offset, "incomplete fraction"));
        };
        if !fraction.is_ascii_digit() {
            return Err(invalid_json(fraction_offset, "expected fraction digit"));
        }
        next = loop {
            let candidate = cursor.next_byte()?;
            if !candidate.is_some_and(|(_, byte)| byte.is_ascii_digit()) {
                break candidate;
            }
        };
    }

    if next.is_some_and(|(_, byte)| matches!(byte, b'e' | b'E')) {
        let Some((mut exponent_offset, mut exponent)) = cursor.next_byte()? else {
            return Err(invalid_json(offset, "incomplete exponent"));
        };
        if matches!(exponent, b'+' | b'-') {
            let Some(value) = cursor.next_byte()? else {
                return Err(invalid_json(exponent_offset, "incomplete exponent"));
            };
            (exponent_offset, exponent) = value;
        }
        if !exponent.is_ascii_digit() {
            return Err(invalid_json(exponent_offset, "expected exponent digit"));
        }
        next = loop {
            let candidate = cursor.next_byte()?;
            if !candidate.is_some_and(|(_, byte)| byte.is_ascii_digit()) {
                break candidate;
            }
        };
    }

    if let Some((delimiter_offset, delimiter)) = next {
        if !is_json_value_delimiter(delimiter) {
            return Err(invalid_json(
                delimiter_offset,
                "invalid character after number",
            ));
        }
        cursor.rewind_one();
    }
    Ok(())
}

fn ensure_json_value_delimiter(
    cursor: &mut ByteCursor,
    value_offset: u64,
) -> Result<(), PagedDocumentError> {
    if let Some((offset, byte)) = cursor.next_byte()? {
        if !is_json_value_delimiter(byte) {
            return Err(invalid_json(offset, "invalid character after JSON value"));
        }
        cursor.rewind_one();
    } else if value_offset >= cursor.end {
        return Err(invalid_json(value_offset, "incomplete JSON value"));
    }
    Ok(())
}

fn is_json_value_delimiter(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b',' | b']' | b'}')
}

pub(super) fn object_value_start(
    source: &FileSource,
    range: std::ops::Range<u64>,
) -> Result<Option<u64>, PagedDocumentError> {
    let mut cursor = ByteCursor::new(source.clone(), range.start, range.end);
    let mut in_string = false;
    let mut escaped = false;
    while let Some((_, byte)) = cursor.next_byte()? {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b':' => return Ok(next_non_whitespace(&mut cursor)?.map(|(offset, _)| offset)),
            _ => {}
        }
    }
    Ok(None)
}

pub(super) fn object_key_range(
    source: &FileSource,
    range: std::ops::Range<u64>,
) -> Result<std::ops::Range<u64>, PagedDocumentError> {
    let mut cursor = ByteCursor::new(source.clone(), range.start, range.end);
    let Some((start, first)) = next_non_whitespace(&mut cursor)? else {
        return Err(invalid_json(range.start, "object item is empty"));
    };
    if first != b'"' {
        return Err(invalid_json(start, "object key must be a string"));
    }
    let mut escaped = false;
    while let Some((offset, byte)) = cursor.next_byte()? {
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Ok(start..offset + 1);
        }
    }
    Err(invalid_json(range.end, "unterminated object key"))
}

pub(super) fn scan_item_from_checkpoint(
    source: &FileSource,
    checkpoint: &JsonCheckpoint,
    target: u64,
    end: u64,
    closing: u8,
) -> Result<std::ops::Range<u64>, PagedDocumentError> {
    let mut cursor = ByteCursor::new(source.clone(), checkpoint.byte_offset, end);
    let mut current = checkpoint.item_index;
    let mut start = checkpoint.byte_offset;
    let mut depth = 1u64;
    let mut in_string = false;
    let mut escaped = false;
    while let Some((offset, byte)) = cursor.next_byte()? {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' if depth > 1 => depth -= 1,
            b',' if depth == 1 => {
                if current == target {
                    return trim_range_end(source, start..offset);
                }
                current += 1;
                start = next_non_whitespace(&mut cursor)?
                    .map(|(offset, _)| offset)
                    .ok_or_else(|| invalid_json(offset, "missing item after comma"))?;
                cursor.rewind_one();
            }
            value if value == closing && depth == 1 => {
                if current == target {
                    return trim_range_end(source, start..offset);
                }
                break;
            }
            _ => {}
        }
    }
    Err(invalid_json(start, "root item could not be resolved"))
}

pub(super) fn trim_range_end(
    source: &FileSource,
    mut range: std::ops::Range<u64>,
) -> Result<std::ops::Range<u64>, PagedDocumentError> {
    while range.end > range.start {
        let byte = source.read_range(range.end - 1, range.end)?[0];
        if !byte.is_ascii_whitespace() {
            break;
        }
        range.end -= 1;
    }
    Ok(range)
}

pub(super) fn maybe_checkpoint(
    checkpoints: &mut Vec<JsonCheckpoint>,
    item_index: u64,
    offset: u64,
    last_checkpoint_byte: &mut u64,
    options: JsonIndexOptions,
) {
    if item_index == 0
        || item_index.is_multiple_of(options.checkpoint_items.max(1))
        || offset.saturating_sub(*last_checkpoint_byte) >= options.checkpoint_bytes.max(1)
    {
        checkpoints.push(JsonCheckpoint {
            item_index,
            byte_offset: offset,
        });
        *last_checkpoint_byte = offset;
    }
}

pub(super) fn next_non_whitespace(
    cursor: &mut ByteCursor,
) -> Result<Option<(u64, u8)>, PagedDocumentError> {
    while let Some((offset, byte)) = cursor.next_byte()? {
        if !byte.is_ascii_whitespace() {
            return Ok(Some((offset, byte)));
        }
    }
    Ok(None)
}

pub(super) fn invalid_json(offset: u64, message: impl Into<String>) -> PagedDocumentError {
    PagedDocumentError::InvalidJson {
        offset,
        message: message.into(),
    }
}

pub(super) struct ByteCursor {
    source: FileSource,
    next_offset: u64,
    end: u64,
    buffer_start: u64,
    buffer: Vec<u8>,
    buffer_index: usize,
    rewound: Option<(u64, u8)>,
    cancellation: Option<SearchCancellation>,
}

impl ByteCursor {
    fn new(source: FileSource, start: u64, end: u64) -> Self {
        Self {
            source,
            next_offset: start,
            end,
            buffer_start: start,
            buffer: Vec::new(),
            buffer_index: 0,
            rewound: None,
            cancellation: None,
        }
    }

    pub(super) fn new_cancellable(
        source: FileSource,
        start: u64,
        end: u64,
        cancellation: &SearchCancellation,
    ) -> Self {
        let mut cursor = Self::new(source, start, end);
        cursor.cancellation = Some(cancellation.clone());
        cursor
    }

    pub(super) fn next_byte(&mut self) -> Result<Option<(u64, u8)>, PagedDocumentError> {
        if let Some(value) = self.rewound.take() {
            return Ok(Some(value));
        }
        if self.buffer_index >= self.buffer.len() {
            if self
                .cancellation
                .as_ref()
                .is_some_and(SearchCancellation::is_cancelled)
            {
                return Err(PagedDocumentError::Cancelled);
            }
            if self.next_offset >= self.end {
                return Ok(None);
            }
            self.buffer_start = self.next_offset;
            let read_end = (self.next_offset + READ_BLOCK_BYTES).min(self.end);
            self.buffer = self.source.read_range(self.next_offset, read_end)?;
            self.buffer_index = 0;
        }
        let offset = self.buffer_start + self.buffer_index as u64;
        let byte = self.buffer[self.buffer_index];
        self.buffer_index += 1;
        self.next_offset = offset + 1;
        Ok(Some((offset, byte)))
    }

    fn rewind_one(&mut self) {
        if self.buffer_index > 0 {
            let index = self.buffer_index - 1;
            self.rewound = Some((self.buffer_start + index as u64, self.buffer[index]));
        }
    }
}
