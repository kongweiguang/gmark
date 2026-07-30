// @author kongweiguang

use std::io::Write;
use std::ops::Range;

use super::super::*;

impl PieceDocument {
    pub fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>, PagedDocumentError> {
        if range.start > range.end || range.end > self.len {
            return Err(PagedDocumentError::InvalidRange {
                start: range.start,
                end: range.end,
                len: self.len,
            });
        }
        let capacity = usize::try_from(range.end - range.start)
            .map_err(|_| PagedDocumentError::RangeTooLarge)?;
        let mut output = Vec::with_capacity(capacity);
        if range.is_empty() {
            return Ok(output);
        }
        let mut cursor = self.pieces.root.cursor::<Bytes>(());
        cursor.seek(&Bytes(range.start), Bias::Right);
        while let Some(piece) = cursor.item() {
            let logical_start = cursor.start().0;
            let logical_end = cursor.end().0;
            let start = range.start.max(logical_start);
            let end = range.end.min(logical_end);
            if start < end {
                let relative = start - logical_start..end - logical_start;
                let bytes = piece.range.start + relative.start..piece.range.start + relative.end;
                match piece.source {
                    PieceSource::Base => {
                        output.extend(self.source()?.read_range(bytes.start, bytes.end)?)
                    }
                    PieceSource::Add => output.extend(self.additions.read(bytes)?),
                }
            }
            if logical_end >= range.end {
                break;
            }
            cursor.next();
        }
        Ok(output)
    }

    /// 分块读取不可变 PieceTree 快照，并在页块之间响应取消。剪贴板任务因此不会在
    /// Tab 已关闭或文件 identity 已变化后继续扫描数十 MiB 的旧文档。
    pub fn read_range_cancellable(
        &self,
        range: Range<u64>,
        cancellation: &SearchCancellation,
    ) -> Result<Vec<u8>, PagedDocumentError> {
        if range.start > range.end || range.end > self.len {
            return Err(PagedDocumentError::InvalidRange {
                start: range.start,
                end: range.end,
                len: self.len,
            });
        }
        if cancellation.is_cancelled() {
            return Err(PagedDocumentError::Cancelled);
        }
        let capacity = usize::try_from(range.end - range.start)
            .map_err(|_| PagedDocumentError::RangeTooLarge)?;
        let mut output = Vec::with_capacity(capacity);
        const COPY_CHUNK: u64 = 1024 * 1024;
        let mut offset = range.start;
        while offset < range.end {
            if cancellation.is_cancelled() {
                return Err(PagedDocumentError::Cancelled);
            }
            let end = offset.saturating_add(COPY_CHUNK).min(range.end);
            output.extend(self.read_range(offset..end)?);
            offset = end;
        }
        Ok(output)
    }

    /// 按逻辑 piece 顺序流式输出，不物化完整文档。
    pub fn write_to(&self, mut output: impl Write) -> Result<(), PagedDocumentError> {
        self.for_each_utf8_chunk(8 * 1024 * 1024, |bytes| {
            output
                .write_all(bytes)
                .map_err(|source| PagedDocumentError::Io {
                    path: self.base_identity.path.clone(),
                    source,
                })
        })
    }

    /// 与 `write_to` 相同，但在每个有界块之间响应后台任务取消。
    pub fn write_to_cancellable(
        &self,
        mut output: impl Write,
        cancellation: &SearchCancellation,
    ) -> Result<(), PagedDocumentError> {
        self.for_each_utf8_chunk(8 * 1024 * 1024, |bytes| {
            if cancellation.is_cancelled() {
                return Err(PagedDocumentError::Cancelled);
            }
            output
                .write_all(bytes)
                .map_err(|source| PagedDocumentError::Io {
                    path: self.base_identity.path.clone(),
                    source,
                })
        })
    }

    /// 仅在 UTF-8 边界切块，供编码器和搜索器保持跨块状态。
    pub fn for_each_utf8_chunk(
        &self,
        chunk_bytes: u64,
        mut callback: impl FnMut(&[u8]) -> Result<(), PagedDocumentError>,
    ) -> Result<(), PagedDocumentError> {
        let chunk_bytes = chunk_bytes.max(4);
        let mut offset = 0u64;
        while offset < self.len {
            let mut end = (offset + chunk_bytes).min(self.len);
            while end < self.len && end > offset && !self.is_char_boundary(end)? {
                end -= 1;
            }
            if end == offset {
                return Err(PagedDocumentError::InvalidUtf8Boundary);
            }
            let bytes = self.read_range(offset..end)?;
            callback(&bytes)?;
            offset = end;
        }
        Ok(())
    }

    /// 遍历一个已验证的 Source 字节范围，并只在 UTF-8 字符边界切块。
    /// 选区导出借此复用整文档编码器，而不物化超大选区。
    pub fn for_each_utf8_range_chunk(
        &self,
        range: Range<u64>,
        chunk_bytes: u64,
        mut callback: impl FnMut(&[u8]) -> Result<(), PagedDocumentError>,
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
        let chunk_bytes = chunk_bytes.max(4);
        let mut offset = range.start;
        while offset < range.end {
            let mut end = offset.saturating_add(chunk_bytes).min(range.end);
            while end < range.end && end > offset && !self.is_char_boundary(end)? {
                end -= 1;
            }
            if end == offset {
                return Err(PagedDocumentError::InvalidUtf8Boundary);
            }
            let bytes = self.read_range(offset..end)?;
            callback(&bytes)?;
            offset = end;
        }
        Ok(())
    }
}
