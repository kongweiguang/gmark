// @author kongweiguang

//! Source-byte ranges and newline/BOM preservation for Markdown documents.

use std::error::Error;
use std::fmt;

/// A half-open byte range in the original Markdown source.
///
/// The parser never normalizes newline bytes before assigning ranges. This is
/// the invariant that lets adapters move safely between editor bytes and pure
/// Markdown values, including documents with a BOM or mixed line endings.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct SourceRange {
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
}

impl SourceRange {
    /// Creates a range when its bounds are ordered.
    pub fn new(start: usize, end: usize) -> Result<Self, SourceRangeError> {
        if start > end {
            return Err(SourceRangeError::Reversed { start, end });
        }
        Ok(Self { start, end })
    }

    /// Creates an empty source range at `offset`.
    pub const fn empty(offset: usize) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    /// Returns the range length in bytes.
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    /// Returns whether the range is empty.
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Returns whether `offset` lies inside this half-open range.
    pub const fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Returns whether `other` is fully contained by this range.
    pub const fn contains_range(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// Borrows the exact source text covered by this range.
    pub fn slice(self, source: &str) -> Result<&str, SourceRangeError> {
        if self.end > source.len() {
            return Err(SourceRangeError::OutOfBounds {
                range: self,
                source_len: source.len(),
            });
        }
        if !source.is_char_boundary(self.start) || !source.is_char_boundary(self.end) {
            return Err(SourceRangeError::NotCharacterBoundary { range: self });
        }
        Ok(&source[self.start..self.end])
    }

    pub(crate) const fn from_parser(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Failure while constructing or resolving a source range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceRangeError {
    /// The range end precedes its start.
    Reversed { start: usize, end: usize },
    /// The range exceeds the supplied source string.
    OutOfBounds {
        range: SourceRange,
        source_len: usize,
    },
    /// A range splits a UTF-8 code point.
    NotCharacterBoundary { range: SourceRange },
}

impl fmt::Display for SourceRangeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reversed { start, end } => {
                write!(formatter, "source range start {start} exceeds end {end}")
            }
            Self::OutOfBounds { range, source_len } => write!(
                formatter,
                "source range {}..{} exceeds source length {source_len}",
                range.start, range.end
            ),
            Self::NotCharacterBoundary { range } => write!(
                formatter,
                "source range {}..{} splits a UTF-8 character",
                range.start, range.end
            ),
        }
    }
}

impl Error for SourceRangeError {}

/// A source newline spelling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LineEnding {
    /// Line feed (`\n`).
    #[default]
    Lf,
    /// Carriage return + line feed (`\r\n`).
    CrLf,
    /// Legacy carriage return (`\r`).
    Cr,
}

impl LineEnding {
    /// Returns the newline spelling as text.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
        }
    }
}

/// Summary of line-ending use in an original source string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEndingSummary {
    /// The source has no newline.
    None,
    /// Every newline uses the same spelling.
    Uniform(LineEnding),
    /// More than one spelling appears.
    Mixed,
}

/// Exact source-format information that does not require any platform API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFormat {
    /// Whether the source began with a UTF-8 BOM.
    pub has_utf8_bom: bool,
    /// One original spelling for every logical newline in the source.
    pub line_endings: Vec<LineEnding>,
    /// Preferred spelling for newly created newlines.
    pub dominant_ending: LineEnding,
}

impl SourceFormat {
    /// Reads a source string without modifying it.
    pub fn analyze(source: &str) -> Self {
        let (has_utf8_bom, body) = match source.strip_prefix('\u{feff}') {
            Some(body) => (true, body),
            None => (false, source),
        };
        let bytes = body.as_bytes();
        let mut endings = Vec::new();
        let mut cursor = 0usize;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'\r' if bytes.get(cursor + 1) == Some(&b'\n') => {
                    endings.push(LineEnding::CrLf);
                    cursor += 2;
                }
                b'\r' => {
                    endings.push(LineEnding::Cr);
                    cursor += 1;
                }
                b'\n' => {
                    endings.push(LineEnding::Lf);
                    cursor += 1;
                }
                _ => cursor += 1,
            }
        }
        let dominant_ending = dominant_ending(&endings);
        Self {
            has_utf8_bom,
            line_endings: endings,
            dominant_ending,
        }
    }

    /// Summarizes whether source line endings are uniform.
    pub fn line_ending_summary(&self) -> LineEndingSummary {
        let Some(first) = self.line_endings.first().copied() else {
            return LineEndingSummary::None;
        };
        if self.line_endings.iter().all(|ending| *ending == first) {
            LineEndingSummary::Uniform(first)
        } else {
            LineEndingSummary::Mixed
        }
    }

    /// Converts the original text to LF-only text and removes a leading BOM.
    pub fn normalize(&self, source: &str) -> Result<String, SourceFormatError> {
        let expected_bom = source.starts_with('\u{feff}');
        if expected_bom != self.has_utf8_bom {
            return Err(SourceFormatError::BomMismatch);
        }
        let body = match source.strip_prefix('\u{feff}') {
            Some(body) => body,
            None => source,
        };
        let bytes = body.as_bytes();
        let mut normalized = String::with_capacity(body.len());
        let mut segment_start = 0usize;
        let mut cursor = 0usize;
        while cursor < bytes.len() {
            let width = match bytes[cursor] {
                b'\r' if bytes.get(cursor + 1) == Some(&b'\n') => Some(2),
                b'\r' | b'\n' => Some(1),
                _ => None,
            };
            if let Some(width) = width {
                normalized.push_str(&body[segment_start..cursor]);
                normalized.push('\n');
                cursor += width;
                segment_start = cursor;
            } else {
                cursor += 1;
            }
        }
        normalized.push_str(&body[segment_start..]);
        Ok(normalized)
    }

    /// Restores original newline spellings after a non-structural edit.
    pub fn restore(&self, normalized: &str) -> Result<String, SourceFormatError> {
        let newline_count = normalized.bytes().filter(|byte| *byte == b'\n').count();
        if newline_count != self.line_endings.len() {
            return Err(SourceFormatError::NewlineCountMismatch {
                expected: self.line_endings.len(),
                actual: newline_count,
            });
        }
        let mut restored = String::with_capacity(normalized.len() + self.line_endings.len());
        if self.has_utf8_bom {
            restored.push('\u{feff}');
        }
        let mut ending_index = 0usize;
        let mut segment_start = 0usize;
        for (offset, byte) in normalized.bytes().enumerate() {
            if byte != b'\n' {
                continue;
            }
            restored.push_str(&normalized[segment_start..offset]);
            if let Some(ending) = self.line_endings.get(ending_index) {
                restored.push_str(ending.as_str());
            }
            ending_index += 1;
            segment_start = offset + 1;
        }
        restored.push_str(&normalized[segment_start..]);
        Ok(restored)
    }
}

/// Failure while transforming source line endings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceFormatError {
    /// A format snapshot was applied to a source with a different BOM state.
    BomMismatch,
    /// Restoring needs one original spelling for each normalized newline.
    NewlineCountMismatch { expected: usize, actual: usize },
}

impl fmt::Display for SourceFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BomMismatch => {
                formatter.write_str("source BOM does not match its format snapshot")
            }
            Self::NewlineCountMismatch { expected, actual } => write!(
                formatter,
                "expected {expected} normalized newlines, found {actual}"
            ),
        }
    }
}

impl Error for SourceFormatError {}

/// Indexed ranges for parser events and structural blocks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceMap {
    source_len: usize,
    event_ranges: Vec<SourceRange>,
    block_ranges: Vec<SourceRange>,
}

impl SourceMap {
    pub(crate) fn new(source_len: usize, event_ranges: Vec<SourceRange>) -> Self {
        Self {
            source_len,
            event_ranges,
            block_ranges: Vec::new(),
        }
    }

    /// Source length in UTF-8 bytes.
    pub const fn source_len(&self) -> usize {
        self.source_len
    }

    /// Gets the byte range attached to one parser event.
    pub fn event_range(&self, event_index: usize) -> Option<SourceRange> {
        self.event_ranges.get(event_index).copied()
    }

    /// Gets all parser event ranges in source order.
    pub fn event_ranges(&self) -> &[SourceRange] {
        &self.event_ranges
    }

    /// Gets all discovered structural block ranges in depth-first order.
    pub fn block_ranges(&self) -> &[SourceRange] {
        &self.block_ranges
    }

    /// Ensures every recorded range can be sliced from `source`.
    pub fn validate(&self, source: &str) -> Result<(), SourceRangeError> {
        for range in self.event_ranges.iter().chain(&self.block_ranges) {
            range.slice(source)?;
        }
        Ok(())
    }

    pub(crate) fn set_block_ranges(&mut self, block_ranges: Vec<SourceRange>) {
        self.block_ranges = block_ranges;
    }
}

fn dominant_ending(endings: &[LineEnding]) -> LineEnding {
    let mut counts = [0usize; 3];
    for ending in endings {
        let index = match ending {
            LineEnding::Lf => 0,
            LineEnding::CrLf => 1,
            LineEnding::Cr => 2,
        };
        counts[index] += 1;
    }
    let mut best_index = 0usize;
    for index in 1..counts.len() {
        if counts[index] > counts[best_index] {
            best_index = index;
        }
    }
    [LineEnding::Lf, LineEnding::CrLf, LineEnding::Cr][best_index]
}
