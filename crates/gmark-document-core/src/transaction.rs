// @author kongweiguang

use std::ops::Range;
use std::sync::Arc;

use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentRevision(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SourceAffinity {
    #[default]
    Before,
    After,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceAnchor {
    pub byte_offset: u64,
    pub affinity: SourceAffinity,
}

impl SourceAnchor {
    pub const fn new(byte_offset: u64, affinity: SourceAffinity) -> Self {
        Self {
            byte_offset,
            affinity,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceSelection {
    pub anchor: SourceAnchor,
    pub head: SourceAnchor,
}

impl SourceSelection {
    pub const fn collapsed(byte_offset: u64, affinity: SourceAffinity) -> Self {
        let anchor = SourceAnchor::new(byte_offset, affinity);
        Self {
            anchor,
            head: anchor,
        }
    }

    pub fn from_range(range: Range<u64>, reversed: bool) -> Self {
        if range.is_empty() {
            return Self::collapsed(range.start, SourceAffinity::Before);
        }
        let start = SourceAnchor::new(range.start, SourceAffinity::Before);
        let end = SourceAnchor::new(range.end, SourceAffinity::After);
        if reversed {
            Self {
                anchor: end,
                head: start,
            }
        } else {
            Self {
                anchor: start,
                head: end,
            }
        }
    }

    pub fn range(self) -> Range<u64> {
        self.anchor.byte_offset.min(self.head.byte_offset)
            ..self.anchor.byte_offset.max(self.head.byte_offset)
    }

    pub fn reversed(self) -> bool {
        self.head.byte_offset < self.anchor.byte_offset
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEdit {
    pub range: Range<u64>,
    pub replacement: Arc<str>,
}

impl SourceEdit {
    pub fn new(range: Range<u64>, replacement: impl Into<Arc<str>>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub base_revision: DocumentRevision,
    pub edits: Vec<SourceEdit>,
}

impl Transaction {
    pub fn new(base_revision: DocumentRevision, edits: Vec<SourceEdit>) -> Self {
        Self {
            base_revision,
            edits,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EditError {
    #[error("stale document revision: expected {expected:?}, got {actual:?}")]
    StaleRevision {
        expected: DocumentRevision,
        actual: DocumentRevision,
    },
    #[error("invalid source byte range {start}..{end} for document length {len}")]
    InvalidRange { start: u64, end: u64, len: u64 },
    #[error("edit range is not on a UTF-8 boundary")]
    InvalidUtf8Boundary,
    #[error("document revision overflow")]
    RevisionOverflow,
    #[error("source byte offset does not fit this platform")]
    OffsetOverflow,
}
