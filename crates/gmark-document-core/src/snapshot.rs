// @author kongweiguang

use std::ops::Range;

use thiserror::Error;

use crate::DocumentRevision;

pub trait DocumentSnapshot: Send + Sync {
    fn revision(&self) -> DocumentRevision;
    fn len(&self) -> u64;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>, SnapshotError>;
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SnapshotError {
    #[error("invalid snapshot byte range {start}..{end} for length {len}")]
    InvalidRange { start: u64, end: u64, len: u64 },
    #[error("snapshot range does not fit this platform")]
    RangeTooLarge,
    #[error("snapshot read failed: {0}")]
    Read(String),
}

impl SnapshotError {
    pub fn new(message: impl Into<String>) -> Self {
        Self::Read(message.into())
    }
}
