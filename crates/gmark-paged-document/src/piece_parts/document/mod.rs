// @author kongweiguang

use super::*;

mod editing;
mod persistence;
mod reading;
mod search;

impl PieceDocument {
    pub fn line_count(&self) -> u64 {
        self.pieces.root.summary().newlines + 1
    }

    pub fn line_range(&self, line: u64) -> Option<Range<u64>> {
        if line >= self.line_count() {
            return None;
        }
        let start = if line == 0 {
            0
        } else {
            self.logical_newline_offset(line - 1)?
        };
        let end = self.logical_newline_offset(line).unwrap_or(self.len);
        Some(start..end)
    }

    fn is_char_boundary(&self, offset: u64) -> Result<bool, PagedDocumentError> {
        if offset == 0 || offset == self.len {
            return Ok(true);
        }
        let byte = self.read_range(offset..offset + 1)?[0];
        Ok(byte & 0b1100_0000 != 0b1000_0000)
    }

    fn source(&self) -> Result<&FileSource, PagedDocumentError> {
        self.source.as_ref().ok_or_else(|| PagedDocumentError::Io {
            path: self.base_identity.path.clone(),
            source: std::io::Error::other("base source is temporarily unavailable"),
        })
    }

    pub fn base_source(&self) -> Result<FileSource, PagedDocumentError> {
        self.source().cloned()
    }

    pub(super) fn slice_piece(
        &self,
        piece: &Piece,
        relative: Range<u64>,
    ) -> Result<Piece, PagedDocumentError> {
        let range = piece.range.start + relative.start..piece.range.start + relative.end;
        let newlines = match piece.source {
            PieceSource::Base => self.base_index.newline_count_in(range.clone()),
            PieceSource::Add => self
                .additions
                .read(range.clone())?
                .iter()
                .filter(|byte| **byte == b'\n')
                .count() as u64,
        };
        Ok(Piece {
            source: piece.source,
            range,
            newlines,
        })
    }

    pub(super) fn logical_newline_offset(&self, newline_index: u64) -> Option<u64> {
        let mut cursor = self.pieces.root.cursor::<Dimensions<Newlines, Bytes>>(());
        // seek 的 bool 只表示目标是否恰落在 item 边界；目标位于一个多换行
        // Piece 内部时会返回 false，但 cursor.item() 仍是所需 Piece。
        cursor.seek(&Newlines(newline_index), Bias::Right);
        let piece = cursor.item()?;
        let remaining = newline_index.checked_sub(cursor.start().0.0)?;
        let logical_start = cursor.start().1.0;
        let source_offset = match piece.source {
            PieceSource::Base => self
                .base_index
                .newline_offset_in(piece.range.clone(), remaining)?,
            PieceSource::Add => {
                let bytes = self.additions.read(piece.range.clone()).ok()?;
                let relative =
                    memchr::memchr_iter(b'\n', &bytes).nth(usize::try_from(remaining).ok()?)?;
                piece.range.start + relative as u64 + 1
            }
        };
        Some(logical_start + source_offset - piece.range.start)
    }
}
