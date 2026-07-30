// @author kongweiguang

use std::ops::Range;

use crate::{CancellationSignal, DocumentSnapshot, JsonGraphError};

const READ_CHUNK_BYTES: u64 = 64 * 1024;

pub(super) struct SnapshotCursor<'a> {
    document: &'a dyn DocumentSnapshot,
    range: Range<u64>,
    position: u64,
    chunk_start: u64,
    chunk: Vec<u8>,
    cancellation: &'a dyn CancellationSignal,
}

impl<'a> SnapshotCursor<'a> {
    pub(super) fn new(
        document: &'a dyn DocumentSnapshot,
        range: Range<u64>,
        cancellation: &'a dyn CancellationSignal,
    ) -> Self {
        Self {
            document,
            position: range.start,
            chunk_start: range.start,
            range,
            chunk: Vec::new(),
            cancellation,
        }
    }

    pub(super) fn position(&self) -> u64 {
        self.position
    }

    pub(super) fn peek(&mut self) -> Result<Option<u8>, JsonGraphError> {
        if self.position >= self.range.end {
            return Ok(None);
        }
        self.ensure_chunk()?;
        let index = usize::try_from(self.position.saturating_sub(self.chunk_start))
            .map_err(|_| JsonGraphError::RangeTooLarge)?;
        Ok(self.chunk.get(index).copied())
    }

    pub(super) fn bump(&mut self) -> Result<Option<u8>, JsonGraphError> {
        let byte = self.peek()?;
        if byte.is_some() {
            self.position += 1;
        }
        Ok(byte)
    }

    pub(super) fn skip_whitespace(&mut self) -> Result<(), JsonGraphError> {
        while self.peek()?.is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
        Ok(())
    }

    fn ensure_chunk(&mut self) -> Result<(), JsonGraphError> {
        if self.cancellation.is_cancelled() {
            return Err(JsonGraphError::Cancelled);
        }
        let chunk_end = self.chunk_start.saturating_add(self.chunk.len() as u64);
        if self.position >= self.chunk_start && self.position < chunk_end {
            return Ok(());
        }
        self.chunk_start = self.position;
        let end = self
            .position
            .saturating_add(READ_CHUNK_BYTES)
            .min(self.range.end);
        self.chunk = self.document.read_range(self.position..end)?;
        Ok(())
    }
}
