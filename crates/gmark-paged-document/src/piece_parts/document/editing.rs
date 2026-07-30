// @author kongweiguang

use std::io::Read;
use std::ops::Range;
use std::sync::Arc;

use super::super::*;

impl PieceDocument {
    pub fn accept_external_append(
        &mut self,
        source: FileSource,
        index: LineIndex,
    ) -> Result<(), PagedDocumentError> {
        if !self.undo.is_empty() || !self.redo.is_empty() || self.pieces.piece_count() > 1 {
            return Err(PagedDocumentError::SourceChanged);
        }
        let identity = source.identity()?;
        if identity.len < self.base_identity.len
            || identity.os_file_id != self.base_identity.os_file_id
            || source.sampled_prefix_hash(self.base_identity.len)? != self.base_sample
        {
            return Err(PagedDocumentError::SourceChanged);
        }
        self.len = identity.len;
        self.base_identity = identity;
        self.base_sample = source.sampled_prefix_hash(self.len)?;
        self.base_index = index.clone();
        self.source = Some(source);
        self.pieces = PieceTree::from_iter((self.len > 0).then_some(Piece {
            source: PieceSource::Base,
            range: 0..self.len,
            newlines: index.newline_count(),
        }));
        Ok(())
    }

    pub fn replace_text(
        &mut self,
        range: Range<u64>,
        replacement: &str,
    ) -> Result<(), PagedDocumentError> {
        self.replace_text_chunks(range, std::iter::once(replacement))
    }

    /// 以一个撤销事务写入多个 UTF-8 块，恢复超大粘贴时无需先拼成同等大小的临时字符串。
    pub fn replace_text_chunks<'a>(
        &mut self,
        range: Range<u64>,
        chunks: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), PagedDocumentError> {
        if range.start > range.end || range.end > self.len {
            return Err(PagedDocumentError::InvalidRange {
                start: range.start,
                end: range.end,
                len: self.len,
            });
        }
        if !self.is_char_boundary(range.start)? || !self.is_char_boundary(range.end)? {
            return Err(PagedDocumentError::InvalidUtf8Boundary);
        }
        let mut replacement_len = 0u64;
        let mut replacement_pieces = Vec::new();
        for chunk in chunks {
            if chunk.is_empty() {
                continue;
            }
            replacement_len = replacement_len
                .checked_add(chunk.len() as u64)
                .ok_or(PagedDocumentError::RangeTooLarge)?;
            replacement_pieces.push(Piece {
                source: PieceSource::Add,
                range: self.additions.append(chunk.as_bytes())?,
                newlines: chunk.bytes().filter(|byte| *byte == b'\n').count() as u64,
            });
        }

        let mut cursor = PieceCursor::new(self, 0);
        let mut next = cursor.slice(range.start)?;
        cursor.seek_forward(range.end);
        next.append(PieceTree::from_iter(replacement_pieces));
        next.append(cursor.slice(self.len)?);
        drop(cursor);
        self.record_undo_root(self.pieces.clone(), self.len);
        self.redo.clear();
        self.pieces = next;
        self.len = self.len - (range.end - range.start) + replacement_len;
        Ok(())
    }

    /// 从读取器有界流式安装 UTF-8 替换，并把完整流视为单个撤销事务。
    pub fn replace_text_reader(
        &mut self,
        range: Range<u64>,
        mut reader: impl Read,
    ) -> Result<(), PagedDocumentError> {
        if range.start > range.end || range.end > self.len {
            return Err(PagedDocumentError::InvalidRange {
                start: range.start,
                end: range.end,
                len: self.len,
            });
        }
        if !self.is_char_boundary(range.start)? || !self.is_char_boundary(range.end)? {
            return Err(PagedDocumentError::InvalidUtf8Boundary);
        }

        const CHUNK_BYTES: usize = 1024 * 1024;
        let mut pending = Vec::with_capacity(CHUNK_BYTES + 4);
        let mut scratch = vec![0u8; CHUNK_BYTES];
        let mut replacement_len = 0u64;
        let mut replacement_pieces = Vec::new();
        loop {
            let read = reader
                .read(&mut scratch)
                .map_err(|source| PagedDocumentError::Io {
                    path: std::env::temp_dir(),
                    source,
                })?;
            pending.extend_from_slice(&scratch[..read]);
            let complete = match std::str::from_utf8(&pending) {
                Ok(_) => pending.len(),
                Err(error) if error.error_len().is_none() => error.valid_up_to(),
                Err(_) => return Err(PagedDocumentError::Binary),
            };
            if complete > 0 {
                let bytes = &pending[..complete];
                replacement_len = replacement_len
                    .checked_add(bytes.len() as u64)
                    .ok_or(PagedDocumentError::RangeTooLarge)?;
                replacement_pieces.push(Piece {
                    source: PieceSource::Add,
                    range: self.additions.append(bytes)?,
                    newlines: bytes.iter().filter(|byte| **byte == b'\n').count() as u64,
                });
                pending.drain(..complete);
            }
            if read == 0 {
                break;
            }
        }
        if !pending.is_empty() {
            return Err(PagedDocumentError::Binary);
        }

        let mut cursor = PieceCursor::new(self, 0);
        let mut next = cursor.slice(range.start)?;
        cursor.seek_forward(range.end);
        next.append(PieceTree::from_iter(replacement_pieces));
        next.append(cursor.slice(self.len)?);
        drop(cursor);
        self.record_undo_root(self.pieces.clone(), self.len);
        self.redo.clear();
        self.pieces = next;
        self.len = self.len - (range.end - range.start) + replacement_len;
        Ok(())
    }

    /// 将基于同一 Source revision 的多个不相交编辑作为一个撤销事务提交。
    /// 倒序应用可保持所有 range 都在原始字节坐标系中。
    pub fn replace_text_batch(
        &mut self,
        edits: &[(Range<u64>, Arc<str>)],
    ) -> Result<(), PagedDocumentError> {
        if edits.is_empty() {
            return Ok(());
        }
        let mut ordered = edits.to_vec();
        ordered.sort_by_key(|(range, _)| (range.start, range.end));
        for pair in ordered.windows(2) {
            let previous = &pair[0].0;
            let next = &pair[1].0;
            if previous.end > next.start
                || (previous.is_empty() && next.is_empty() && previous.start == next.start)
            {
                return Err(PagedDocumentError::InvalidTransaction(
                    "derived edit ranges overlap or contain ambiguous inserts".into(),
                ));
            }
        }
        for (range, _) in &ordered {
            if range.start > range.end || range.end > self.len {
                return Err(PagedDocumentError::InvalidRange {
                    start: range.start,
                    end: range.end,
                    len: self.len,
                });
            }
            if !self.is_char_boundary(range.start)? || !self.is_char_boundary(range.end)? {
                return Err(PagedDocumentError::InvalidUtf8Boundary);
            }
        }

        let original_pieces = self.pieces.clone();
        let original_len = self.len;
        let original_undo = self.undo.clone();
        let original_redo = self.redo.clone();
        for (range, replacement) in ordered.iter().rev() {
            if let Err(error) = self.replace_text(range.clone(), replacement) {
                self.pieces = original_pieces;
                self.len = original_len;
                self.undo = original_undo;
                self.redo = original_redo;
                return Err(error);
            }
        }
        self.undo = original_undo;
        self.record_undo_root(original_pieces, original_len);
        self.redo.clear();
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        let Some((pieces, len)) = self.undo.pop() else {
            return false;
        };
        self.redo.push((self.pieces.clone(), self.len));
        self.pieces = pieces;
        self.len = len;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some((pieces, len)) = self.redo.pop() else {
            return false;
        };
        self.record_undo_root(self.pieces.clone(), self.len);
        self.pieces = pieces;
        self.len = len;
        true
    }

    fn record_undo_root(&mut self, pieces: PieceTree, len: u64) {
        if self.undo.len() == DEFAULT_HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push((pieces, len));
    }
}
