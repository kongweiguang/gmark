// @author kongweiguang

use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use lru::LruCache;
use memchr::{memchr, memchr_iter};

use crate::{FileSource, LargeDocumentError, SearchCancellation};

const SCAN_BLOCK_BYTES: usize = 8 * 1024 * 1024;
const LINES_PER_PAGE: usize = 4_096;
const MAX_PAGE_SOURCE_BYTES: u64 = 4 * 1024 * 1024;
const SIDECAR_MAGIC: &[u8; 8] = b"GMKLINE\0";
const SIDECAR_VERSION: u32 = 4;
const SIDECAR_SAMPLE_BYTES: u64 = 64 * 1024;
const MAX_SIDECAR_BYTES: u64 = 128 * 1024 * 1024;
const SIDECAR_CACHE_BUDGET_BYTES: u64 = 512 * 1024 * 1024;
const DISK_PAGE_CACHE_COUNT: usize = 16;
const MAX_PAGE_STORAGE_LAYERS: usize = 8;

#[derive(Default)]
struct Utf8StreamValidator {
    trailing: [u8; 4],
    trailing_len: usize,
}

impl Utf8StreamValidator {
    fn push(&mut self, bytes: &[u8]) -> Result<(), LargeDocumentError> {
        if memchr(0, bytes).is_some() {
            return Err(LargeDocumentError::Binary);
        }
        let mut start = 0usize;
        if self.trailing_len > 0 {
            let width = utf8_sequence_width(self.trailing[0]).ok_or(LargeDocumentError::Binary)?;
            let required = width.saturating_sub(self.trailing_len);
            let take = required.min(bytes.len());
            self.trailing[self.trailing_len..self.trailing_len + take]
                .copy_from_slice(&bytes[..take]);
            self.trailing_len += take;
            start = take;
            if self.trailing_len < width {
                return Ok(());
            }
            std::str::from_utf8(&self.trailing[..self.trailing_len])
                .map_err(|_| LargeDocumentError::Binary)?;
            self.trailing_len = 0;
        }

        match std::str::from_utf8(&bytes[start..]) {
            Ok(_) => Ok(()),
            Err(error) if error.error_len().is_some() => Err(LargeDocumentError::Binary),
            Err(error) => {
                let trailing = &bytes[start + error.valid_up_to()..];
                if trailing.is_empty() || trailing.len() > 3 {
                    return Err(LargeDocumentError::Binary);
                }
                self.trailing[..trailing.len()].copy_from_slice(trailing);
                self.trailing_len = trailing.len();
                Ok(())
            }
        }
    }

    fn finish(self) -> Result<(), LargeDocumentError> {
        if self.trailing_len == 0 {
            Ok(())
        } else {
            Err(LargeDocumentError::Binary)
        }
    }
}

fn utf8_sequence_width(lead: u8) -> Option<usize> {
    match lead {
        0x00..=0x7f => Some(1),
        0xc2..=0xdf => Some(2),
        0xe0..=0xef => Some(3),
        0xf0..=0xf4 => Some(4),
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct LinePage {
    first_line: u64,
    first_offset: u64,
    line_count: usize,
    encoded_lengths: Vec<u8>,
    decoded_ends: OnceLock<Vec<u64>>,
}

impl LinePage {
    fn decoded_ends(&self) -> Option<&[u64]> {
        let ends = self.decoded_ends.get_or_init(|| {
            let mut cursor = 0usize;
            let mut end = self.first_offset;
            let mut ends = Vec::with_capacity(self.line_count);
            for _ in 0..self.line_count {
                let Some(length) = decode_varint(&self.encoded_lengths, &mut cursor) else {
                    return Vec::new();
                };
                end = end.saturating_add(length);
                ends.push(end);
            }
            ends
        });
        (ends.len() == self.line_count).then_some(ends.as_slice())
    }
}

#[derive(Debug)]
struct DiskPageCache {
    pages: LruCache<usize, Arc<LinePage>>,
}

impl Default for DiskPageCache {
    fn default() -> Self {
        Self::with_capacity(DISK_PAGE_CACHE_COUNT)
    }
}

impl DiskPageCache {
    fn with_capacity(capacity: usize) -> Self {
        let capacity =
            NonZeroUsize::new(capacity).expect("disk page cache capacity must be non-zero");
        Self {
            pages: LruCache::new(capacity),
        }
    }

    fn get(&mut self, page_index: usize) -> Option<Arc<LinePage>> {
        self.pages.get(&page_index).cloned()
    }

    fn insert(&mut self, page_index: usize, page: Arc<LinePage>) {
        self.pages.put(page_index, page);
    }

    fn len(&self) -> usize {
        self.pages.len()
    }
}

#[derive(Debug)]
struct DiskLinePages {
    file: Mutex<DiskPageFile>,
    first_lines: Vec<u64>,
    record_offsets: Vec<u64>,
    cache: Mutex<DiskPageCache>,
}

#[derive(Debug)]
enum DiskPageFile {
    Persistent(File),
    Temporary(tempfile::NamedTempFile),
}

impl DiskPageFile {
    fn file_mut(&mut self) -> &mut File {
        match self {
            Self::Persistent(file) => file,
            Self::Temporary(file) => file.as_file_mut(),
        }
    }
}

#[derive(Clone, Debug)]
enum LinePages {
    Memory(Arc<Vec<Arc<LinePage>>>),
    Disk(Arc<DiskLinePages>),
    Layered {
        base: Arc<LinePages>,
        base_pages: usize,
        tail: Arc<LinePages>,
    },
}

impl Default for LinePages {
    fn default() -> Self {
        Self::Memory(Arc::new(Vec::new()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineIndexStorageStats {
    pub disk_backed: bool,
    pub page_count: usize,
    pub resident_pages: usize,
    pub resident_encoded_bytes: usize,
    pub resident_decoded_bytes: usize,
    pub max_resident_pages: usize,
}

#[derive(Clone, Debug, Default)]
pub struct LineIndex {
    line_count: u64,
    max_line_bytes: u64,
    pages: LinePages,
}

impl DiskLinePages {
    fn page(&self, index: usize) -> Option<Arc<LinePage>> {
        {
            let mut cache = self
                .cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(page) = cache.get(index) {
                return Some(page);
            }
        }

        let record_offset = *self.record_offsets.get(index)?;
        let page = {
            let mut file = self
                .file
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let file = file.file_mut();
            file.seek(SeekFrom::Start(record_offset)).ok()?;
            let first_line = read_u64(file)?;
            let first_offset = read_u64(file)?;
            let line_count = usize::try_from(read_u32(file)?).ok()?;
            let encoded_len = usize::try_from(read_u32(file)?).ok()?;
            if line_count == 0 || line_count > LINES_PER_PAGE {
                return None;
            }
            let mut encoded_lengths = vec![0; encoded_len];
            file.read_exact(&mut encoded_lengths).ok()?;
            Arc::new(LinePage {
                first_line,
                first_offset,
                line_count,
                encoded_lengths,
                decoded_ends: OnceLock::new(),
            })
        };
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.insert(index, Arc::clone(&page));
        Some(page)
    }

    fn resident_stats(&self) -> (usize, usize, usize) {
        let cache = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            cache.len(),
            cache
                .pages
                .iter()
                .map(|(_, page)| page.encoded_lengths.len())
                .sum(),
            cache
                .pages
                .iter()
                .map(|(_, page)| {
                    page.decoded_ends
                        .get()
                        .map_or(0, |ends| ends.len() * std::mem::size_of::<u64>())
                })
                .sum(),
        )
    }
}

impl LinePages {
    fn len(&self) -> usize {
        match self {
            Self::Memory(pages) => pages.len(),
            Self::Disk(pages) => pages.record_offsets.len(),
            Self::Layered {
                base_pages, tail, ..
            } => base_pages.saturating_add(tail.len()),
        }
    }

    fn page(&self, index: usize) -> Option<Arc<LinePage>> {
        match self {
            Self::Memory(pages) => pages.get(index).cloned(),
            Self::Disk(pages) => pages.page(index),
            Self::Layered {
                base,
                base_pages,
                tail,
            } => {
                if index < *base_pages {
                    base.page(index)
                } else {
                    tail.page(index - *base_pages)
                }
            }
        }
    }

    fn first_line(&self, index: usize) -> Option<u64> {
        match self {
            Self::Memory(pages) => pages.get(index).map(|page| page.first_line),
            Self::Disk(pages) => pages.first_lines.get(index).copied(),
            Self::Layered {
                base,
                base_pages,
                tail,
            } => {
                if index < *base_pages {
                    base.first_line(index)
                } else {
                    tail.first_line(index - *base_pages)
                }
            }
        }
    }

    fn encoded_len(&self, index: usize) -> Option<usize> {
        match self {
            Self::Memory(pages) => pages.get(index).map(|page| page.encoded_lengths.len()),
            Self::Disk(pages) => pages.page(index).map(|page| page.encoded_lengths.len()),
            Self::Layered {
                base,
                base_pages,
                tail,
            } => {
                if index < *base_pages {
                    base.encoded_len(index)
                } else {
                    tail.encoded_len(index - *base_pages)
                }
            }
        }
    }

    fn storage_stats(&self) -> (bool, usize, usize, usize) {
        match self {
            Self::Memory(pages) => (
                false,
                pages.len(),
                pages.iter().map(|page| page.encoded_lengths.len()).sum(),
                pages
                    .iter()
                    .map(|page| {
                        page.decoded_ends
                            .get()
                            .map_or(0, |ends| ends.len() * std::mem::size_of::<u64>())
                    })
                    .sum(),
            ),
            Self::Disk(pages) => {
                let (resident, encoded_bytes, decoded_bytes) = pages.resident_stats();
                (true, resident, encoded_bytes, decoded_bytes)
            }
            Self::Layered {
                base,
                base_pages,
                tail,
            } => {
                let (disk_backed, base_resident, base_bytes, base_decoded) = base.storage_stats();
                let (tail_disk_backed, tail_resident, tail_bytes, tail_decoded) =
                    tail.storage_stats();
                let base_resident = base_resident.min(*base_pages);
                (
                    disk_backed || tail_disk_backed,
                    base_resident.saturating_add(tail_resident),
                    base_bytes.saturating_add(tail_bytes),
                    base_decoded.saturating_add(tail_decoded),
                )
            }
        }
    }

    fn max_resident_pages(&self) -> usize {
        match self {
            Self::Memory(pages) => pages.len(),
            Self::Disk(_) => DISK_PAGE_CACHE_COUNT,
            Self::Layered {
                base,
                base_pages,
                tail,
            } => base
                .max_resident_pages()
                .min(*base_pages)
                .saturating_add(tail.max_resident_pages()),
        }
    }

    fn depth(&self) -> usize {
        match self {
            Self::Memory(_) | Self::Disk(_) => 1,
            Self::Layered { base, tail, .. } => {
                1usize.saturating_add(base.depth().max(tail.depth()))
            }
        }
    }

    fn compact(&self) -> Result<Self, LargeDocumentError> {
        let mut spool = PageSpool::new()?;
        for page_index in 0..self.len() {
            let page = self
                .page(page_index)
                .ok_or_else(|| LargeDocumentError::Io {
                    path: std::env::temp_dir(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "line index page could not be read during compaction",
                    ),
                })?;
            spool.push(page.as_ref().clone())?;
        }
        spool.finish(Vec::new())
    }
}

impl LineIndex {
    pub fn build(source: &FileSource) -> Result<Self, LargeDocumentError> {
        Self::build_cancellable(source, &SearchCancellation::default())
    }

    /// 分块建立行索引；关闭 Tab、文件换代或新 generation 可在每个 8 MiB 边界停止扫描。
    pub fn build_cancellable(
        source: &FileSource,
        cancellation: &SearchCancellation,
    ) -> Result<Self, LargeDocumentError> {
        let file_len = source.identity()?.len;
        let mut builder = LineIndexBuilder::new(file_len);
        let mut validator = Utf8StreamValidator::default();
        let mut offset = 0u64;
        let mut buffer = vec![0; SCAN_BLOCK_BYTES];
        while offset < file_len {
            if cancellation.is_cancelled() {
                return Err(LargeDocumentError::Cancelled);
            }
            let read_len = usize::try_from((file_len - offset).min(buffer.len() as u64))
                .map_err(|_| LargeDocumentError::RangeTooLarge)?;
            source.read_exact_at(offset, &mut buffer[..read_len])?;
            validator.push(&buffer[..read_len])?;
            for relative in memchr_iter(b'\n', &buffer[..read_len]) {
                builder.finish_line(offset + relative as u64 + 1)?;
            }
            offset += read_len as u64;
        }
        validator.finish()?;
        builder.finish_file()?;
        builder.build()
    }

    /// 对同一文件的纯追加只扫描新增字节；旧换行偏移从压缩页重放，不重新读正文。
    pub fn extend_for_append(&self, source: &FileSource) -> Result<Self, LargeDocumentError> {
        self.extend_for_append_cancellable(source, &SearchCancellation::default())
    }

    pub fn extend_for_append_cancellable(
        &self,
        source: &FileSource,
        cancellation: &SearchCancellation,
    ) -> Result<Self, LargeDocumentError> {
        let old_len = self
            .line_range(self.line_count.saturating_sub(1))
            .map_or(0, |range| range.end);
        let new_len = source.identity()?.len;
        if new_len < old_len {
            return Err(LargeDocumentError::SourceChanged);
        }
        // 复用全部旧压缩页，只重开最后一个逻辑行。日志追加不能随着历史总行数
        // 变成 O(n)；成本只允许与最后一页和新增字节相关。
        let mut builder = LineIndexBuilder::resume_append(self, new_len)
            .ok_or(LargeDocumentError::SourceChanged)?;
        let mut validator = Utf8StreamValidator::default();
        let mut offset = old_len;
        let mut buffer = vec![0; SCAN_BLOCK_BYTES];
        while offset < new_len {
            if cancellation.is_cancelled() {
                return Err(LargeDocumentError::Cancelled);
            }
            let read_len = usize::try_from((new_len - offset).min(buffer.len() as u64))
                .map_err(|_| LargeDocumentError::RangeTooLarge)?;
            source.read_exact_at(offset, &mut buffer[..read_len])?;
            validator.push(&buffer[..read_len])?;
            for relative in memchr_iter(b'\n', &buffer[..read_len]) {
                builder.finish_line(offset + relative as u64 + 1)?;
            }
            offset += read_len as u64;
        }
        validator.finish()?;
        builder.finish_file()?;
        builder.build()
    }

    /// 从应用缓存读取内容无关索引；任何过期或损坏都会退化为重建并原子刷新。
    pub fn build_cached(
        source: &FileSource,
        cache_dir: impl AsRef<Path>,
    ) -> Result<Self, LargeDocumentError> {
        Self::build_cached_cancellable(source, cache_dir, &SearchCancellation::default())
    }

    pub fn build_cached_cancellable(
        source: &FileSource,
        cache_dir: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<Self, LargeDocumentError> {
        let cache_path = sidecar_path(source, cache_dir.as_ref())?;
        if cancellation.is_cancelled() {
            return Err(LargeDocumentError::Cancelled);
        }
        if let Ok(Some(index)) = Self::load_sidecar(source, &cache_path) {
            return Ok(index);
        }
        let index = Self::build_cancellable(source, cancellation)?;
        // 缓存不可写不应阻断打开；正文路径仍然完全可用。
        if index.store_sidecar(source, &cache_path).is_ok() {
            if let Some(cache_dir) = cache_path.parent() {
                let _ = cleanup_sidecar_cache(cache_dir, &cache_path, SIDECAR_CACHE_BUDGET_BYTES);
            }
            // 首次构建也立即切换到磁盘页目录，避免 sidecar 已存在但当前会话仍把
            // 全部压缩行长常驻到关闭 Tab 为止。
            if let Ok(Some(disk_index)) = Self::load_sidecar(source, &cache_path) {
                return Ok(disk_index);
            }
        }
        Ok(index)
    }

    pub fn sidecar_path(
        source: &FileSource,
        cache_dir: impl AsRef<Path>,
    ) -> Result<PathBuf, LargeDocumentError> {
        sidecar_path(source, cache_dir.as_ref())
    }

    pub fn line_count(&self) -> u64 {
        self.line_count
    }

    pub fn line_range(&self, line: u64) -> Option<std::ops::Range<u64>> {
        if line >= self.line_count {
            return None;
        }
        let mut low = 0usize;
        let mut high = self.pages.len();
        while low < high {
            let middle = low + (high - low) / 2;
            if self
                .pages
                .first_line(middle)
                .is_some_and(|first| first <= line)
            {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let page_index = low.saturating_sub(1);
        let page = self.pages.page(page_index)?;
        let within = usize::try_from(line - page.first_line).ok()?;
        if within >= page.line_count {
            return None;
        }
        let ends = page.decoded_ends()?;
        let end = *ends.get(within)?;
        let start = if within == 0 {
            page.first_offset
        } else {
            *ends.get(within - 1)?
        };
        Some(start..end)
    }

    pub fn max_line_bytes(&self) -> u64 {
        self.max_line_bytes
    }

    pub fn storage_stats(&self) -> LineIndexStorageStats {
        let (disk_backed, resident_pages, resident_encoded_bytes, resident_decoded_bytes) =
            self.pages.storage_stats();
        LineIndexStorageStats {
            disk_backed,
            page_count: self.pages.len(),
            resident_pages,
            resident_encoded_bytes,
            resident_decoded_bytes,
            max_resident_pages: self.pages.max_resident_pages(),
        }
    }

    /// 返回包含给定字节偏移的逻辑行；文件尾偏移归入最后一行。
    pub fn line_for_offset(&self, offset: u64) -> Option<u64> {
        if self.line_count == 0 {
            return None;
        }
        let newline = self.first_newline_after(offset);
        Some(newline.min(self.line_count.saturating_sub(1)))
    }

    pub fn newline_count(&self) -> u64 {
        self.line_count.saturating_sub(1)
    }

    /// 返回字节范围内完整包含的换行数量；换行位置使用其后一字节偏移表示。
    pub fn newline_count_in(&self, range: std::ops::Range<u64>) -> u64 {
        if range.start >= range.end {
            return 0;
        }
        let first = self.first_newline_after(range.start);
        let last = self.first_newline_after(range.end);
        last.saturating_sub(first)
    }

    pub fn newline_offset_in(
        &self,
        range: std::ops::Range<u64>,
        relative_index: u64,
    ) -> Option<u64> {
        let first = self.first_newline_after(range.start);
        let index = first.checked_add(relative_index)?;
        let offset = self.newline_offset(index)?;
        (offset <= range.end).then_some(offset)
    }

    fn newline_offset(&self, index: u64) -> Option<u64> {
        if index >= self.newline_count() {
            return None;
        }
        self.line_range(index).map(|range| range.end)
    }

    fn first_newline_after(&self, offset: u64) -> u64 {
        let mut low = 0u64;
        let mut high = self.newline_count();
        while low < high {
            let middle = low + (high - low) / 2;
            if self
                .newline_offset(middle)
                .is_some_and(|value| value <= offset)
            {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low
    }

    fn load_sidecar(source: &FileSource, path: &Path) -> Result<Option<Self>, LargeDocumentError> {
        let metadata = match std::fs::metadata(path) {
            Ok(metadata)
                if metadata.len() >= SIDECAR_MAGIC.len() as u64 + 4
                    && metadata.len() <= MAX_SIDECAR_BYTES =>
            {
                metadata
            }
            _ => return Ok(None),
        };
        let file = std::fs::File::open(path).map_err(|source| LargeDocumentError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        parse_sidecar_stream(source, file, metadata.len(), path)
    }

    fn store_sidecar(&self, source: &FileSource, path: &Path) -> Result<(), LargeDocumentError> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent).map_err(|source| LargeDocumentError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        let identity = source.identity()?;
        let sample = sampled_hash(source, identity.len)?;
        let encoded_bytes = (0..self.pages.len()).try_fold(0u64, |total, page| {
            total.checked_add(self.pages.encoded_len(page)? as u64)
        });
        let estimated_len = encoded_bytes
            .and_then(|bytes| bytes.checked_add(self.pages.len() as u64 * 24))
            .and_then(|bytes| bytes.checked_add(77));
        if estimated_len.is_none_or(|len| len > MAX_SIDECAR_BYTES) {
            return Ok(());
        }
        let os_file_id = identity.os_file_id.as_ref().map(stable_hash);
        for page_index in 0..self.pages.len() {
            let Some(page) = self.pages.page(page_index) else {
                return Ok(());
            };
            if page.line_count > u32::MAX as usize || page.encoded_lengths.len() > u32::MAX as usize
            {
                return Ok(());
            }
        }
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|source| LargeDocumentError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        let temporary_path = temporary.path().to_path_buf();
        let checksum = {
            let mut writer = ChecksummedWriter::new(&mut temporary);
            write_all(&mut writer, SIDECAR_MAGIC, &temporary_path)?;
            write_u32(&mut writer, SIDECAR_VERSION, &temporary_path)?;
            write_u64(&mut writer, identity.len, &temporary_path)?;
            write_u128(
                &mut writer,
                identity.modified_nanos.unwrap_or_default(),
                &temporary_path,
            )?;
            write_u8(&mut writer, u8::from(os_file_id.is_some()), &temporary_path)?;
            write_u64(&mut writer, os_file_id.unwrap_or_default(), &temporary_path)?;
            write_u32(&mut writer, sample, &temporary_path)?;
            write_u64(&mut writer, self.line_count, &temporary_path)?;
            write_u64(&mut writer, self.max_line_bytes, &temporary_path)?;
            write_u64(&mut writer, self.pages.len() as u64, &temporary_path)?;
            for page_index in 0..self.pages.len() {
                let page = self
                    .pages
                    .page(page_index)
                    .ok_or_else(|| LargeDocumentError::Io {
                        path: temporary_path.clone(),
                        source: std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "line index page could not be read",
                        ),
                    })?;
                write_u64(&mut writer, page.first_line, &temporary_path)?;
                write_u64(&mut writer, page.first_offset, &temporary_path)?;
                write_u32(&mut writer, page.line_count as u32, &temporary_path)?;
                write_u32(
                    &mut writer,
                    page.encoded_lengths.len() as u32,
                    &temporary_path,
                )?;
                write_all(&mut writer, &page.encoded_lengths, &temporary_path)?;
            }
            writer.finish()
        };
        write_u32(&mut temporary, checksum, &temporary_path)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|source| LargeDocumentError::Io {
                path: temporary.path().to_path_buf(),
                source,
            })?;
        temporary
            .persist(path)
            .map_err(|error| LargeDocumentError::Persist {
                path: path.to_path_buf(),
                source: error.error,
            })?;
        Ok(())
    }
}
#[path = "index_parts/sidecar.rs"]
mod sidecar;
use sidecar::{
    ChecksummedWriter, LineIndexBuilder, PageSpool, cleanup_sidecar_cache, decode_varint,
    parse_sidecar_stream, read_u32, read_u64, sampled_hash, sidecar_path, stable_hash, write_all,
    write_u8, write_u32, write_u64, write_u128,
};

#[cfg(test)]
#[path = "../tests/unit/index.rs"]
mod tests;
