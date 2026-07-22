// @author kongweiguang

use super::*;

pub(super) fn sidecar_path(
    source: &FileSource,
    cache_dir: &Path,
) -> Result<PathBuf, PagedDocumentError> {
    let identity = source.identity()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    identity.path.hash(&mut hasher);
    Ok(cache_dir.join(format!(
        "{:016x}.gmark-lines-v{SIDECAR_VERSION}",
        hasher.finish()
    )))
}

pub(super) fn parse_sidecar_stream(
    source: &FileSource,
    file: std::fs::File,
    file_len: u64,
    path: &Path,
) -> Result<Option<LineIndex>, PagedDocumentError> {
    let payload_len = file_len.saturating_sub(4);
    let mut reader = ChecksummedReader::new(BufReader::new(file).take(payload_len));
    let Some(magic) = read_array::<8>(&mut reader) else {
        return Ok(None);
    };
    let Some(version) = read_u32(&mut reader) else {
        return Ok(None);
    };
    let Some(len) = read_u64(&mut reader) else {
        return Ok(None);
    };
    let Some(modified_nanos) = read_u128(&mut reader) else {
        return Ok(None);
    };
    let Some(has_os_file_id) = read_u8(&mut reader) else {
        return Ok(None);
    };
    let Some(os_file_id) = read_u64(&mut reader) else {
        return Ok(None);
    };
    let Some(sample) = read_u32(&mut reader) else {
        return Ok(None);
    };
    let identity = source.identity()?;
    let current_os_file_id = identity.os_file_id.as_ref().map(stable_hash);
    if magic != *SIDECAR_MAGIC
        || version != SIDECAR_VERSION
        || len != identity.len
        || modified_nanos != identity.modified_nanos.unwrap_or_default()
        || (has_os_file_id != 0) != current_os_file_id.is_some()
        || current_os_file_id.is_some_and(|current| current != os_file_id)
        || sample != sampled_hash(source, identity.len)?
    {
        return Ok(None);
    }
    let Some(line_count) = read_u64(&mut reader) else {
        return Ok(None);
    };
    let Some(max_line_bytes) = read_u64(&mut reader) else {
        return Ok(None);
    };
    let Some(page_count) = read_u64(&mut reader) else {
        return Ok(None);
    };
    let Ok(page_capacity) = usize::try_from(page_count) else {
        return Ok(None);
    };
    if page_count > payload_len / 24 + 1 {
        return Ok(None);
    }
    let mut record_offsets = Vec::with_capacity(page_capacity);
    let mut first_lines = Vec::with_capacity(page_capacity);
    let mut expected_first_line = 0u64;
    for _ in 0..page_capacity {
        let record_offset = reader.bytes_read();
        let Some(first_line) = read_u64(&mut reader) else {
            return Ok(None);
        };
        let Some(_first_offset) = read_u64(&mut reader) else {
            return Ok(None);
        };
        let Some(page_lines) = read_u32(&mut reader) else {
            return Ok(None);
        };
        let Some(encoded_len) = read_u32(&mut reader) else {
            return Ok(None);
        };
        let Ok(encoded_len) = usize::try_from(encoded_len) else {
            return Ok(None);
        };
        if page_lines == 0
            || page_lines as usize > LINES_PER_PAGE
            || first_line != expected_first_line
            || encoded_len as u64 > payload_len.saturating_sub(reader.bytes_read())
        {
            return Ok(None);
        }
        if drain_exact(&mut reader, encoded_len).is_err() {
            return Ok(None);
        }
        expected_first_line = expected_first_line.saturating_add(page_lines as u64);
        record_offsets.push(record_offset);
        first_lines.push(first_line);
    }
    if reader.bytes_read() != payload_len || expected_first_line != line_count {
        return Ok(None);
    }
    let (limited, checksum) = reader.finish();
    let mut file = limited.into_inner().into_inner();
    sidecar_io(path, file.seek(SeekFrom::End(-4)))?;
    let Some(expected_checksum) = read_u32(&mut file) else {
        return Ok(None);
    };
    if checksum != expected_checksum {
        return Ok(None);
    }
    Ok(Some(LineIndex {
        line_count,
        max_line_bytes,
        pages: LinePages::Disk(Arc::new(DiskLinePages {
            file: Mutex::new(DiskPageFile::Persistent(file)),
            first_lines,
            record_offsets,
            cache: Mutex::new(DiskPageCache::default()),
        })),
    }))
}

fn drain_exact(reader: &mut impl Read, mut len: usize) -> std::io::Result<()> {
    let mut buffer = [0u8; 64 * 1024];
    while len > 0 {
        let take = len.min(buffer.len());
        reader.read_exact(&mut buffer[..take])?;
        len -= take;
    }
    Ok(())
}

pub(super) fn sampled_hash(source: &FileSource, len: u64) -> Result<u32, PagedDocumentError> {
    let mut hasher = crc32fast::Hasher::new();
    for start in [
        0,
        len.saturating_sub(SIDECAR_SAMPLE_BYTES) / 2,
        len.saturating_sub(SIDECAR_SAMPLE_BYTES),
    ] {
        let end = (start + SIDECAR_SAMPLE_BYTES).min(len);
        if start < end {
            hasher.update(&source.read_range(start, end)?);
        }
    }
    Ok(hasher.finalize())
}

fn read_array<const N: usize>(reader: &mut impl Read) -> Option<[u8; N]> {
    let mut value = [0; N];
    reader.read_exact(&mut value).ok()?;
    Some(value)
}
fn read_u8(reader: &mut impl Read) -> Option<u8> {
    Some(read_array::<1>(reader)?[0])
}
pub(super) fn read_u32(reader: &mut impl Read) -> Option<u32> {
    Some(u32::from_le_bytes(read_array(reader)?))
}
pub(super) fn read_u64(reader: &mut impl Read) -> Option<u64> {
    Some(u64::from_le_bytes(read_array(reader)?))
}
fn read_u128(reader: &mut impl Read) -> Option<u128> {
    Some(u128::from_le_bytes(read_array(reader)?))
}

struct ChecksummedReader<R> {
    inner: R,
    hasher: crc32fast::Hasher,
    bytes_read: u64,
}

impl<R> ChecksummedReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: crc32fast::Hasher::new(),
            bytes_read: 0,
        }
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    fn finish(self) -> (R, u32) {
        (self.inner, self.hasher.finalize())
    }
}

impl<R: Read> Read for ChecksummedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.hasher.update(&buffer[..read]);
        self.bytes_read = self.bytes_read.saturating_add(read as u64);
        Ok(read)
    }
}

pub(super) struct ChecksummedWriter<'a, W> {
    inner: &'a mut W,
    hasher: crc32fast::Hasher,
}

impl<'a, W> ChecksummedWriter<'a, W> {
    pub(super) fn new(inner: &'a mut W) -> Self {
        Self {
            inner,
            hasher: crc32fast::Hasher::new(),
        }
    }

    pub(super) fn finish(self) -> u32 {
        self.hasher.finalize()
    }
}

impl<W: Write> Write for ChecksummedWriter<'_, W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.hasher.update(&buffer[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn sidecar_io<T>(path: &Path, result: std::io::Result<T>) -> Result<T, PagedDocumentError> {
    result.map_err(|source| PagedDocumentError::Io {
        path: path.to_path_buf(),
        source,
    })
}

pub(super) fn write_all(
    writer: &mut impl Write,
    bytes: &[u8],
    path: &Path,
) -> Result<(), PagedDocumentError> {
    sidecar_io(path, writer.write_all(bytes))
}

pub(super) fn write_u8(
    writer: &mut impl Write,
    value: u8,
    path: &Path,
) -> Result<(), PagedDocumentError> {
    write_all(writer, &[value], path)
}

pub(super) fn write_u32(
    writer: &mut impl Write,
    value: u32,
    path: &Path,
) -> Result<(), PagedDocumentError> {
    write_all(writer, &value.to_le_bytes(), path)
}

pub(super) fn write_u64(
    writer: &mut impl Write,
    value: u64,
    path: &Path,
) -> Result<(), PagedDocumentError> {
    write_all(writer, &value.to_le_bytes(), path)
}

pub(super) fn write_u128(
    writer: &mut impl Write,
    value: u128,
    path: &Path,
) -> Result<(), PagedDocumentError> {
    write_all(writer, &value.to_le_bytes(), path)
}

pub(super) fn stable_hash(value: &impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn cleanup_sidecar_cache(
    cache_dir: &Path,
    keep: &Path,
    budget: u64,
) -> Result<(), PagedDocumentError> {
    let entries = std::fs::read_dir(cache_dir).map_err(|source| PagedDocumentError::Io {
        path: cache_dir.to_path_buf(),
        source,
    })?;
    let mut candidates = Vec::new();
    let mut total = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep
            || !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".gmark-lines-v"))
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
    let keep_len = std::fs::metadata(keep).map_or(0, |metadata| metadata.len());
    total = total.saturating_add(keep_len);
    candidates.sort_by_key(|(modified, _, _)| *modified);
    for (_, len, path) in candidates {
        if total <= budget {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
    Ok(())
}

pub(super) struct PageSpool {
    file: tempfile::NamedTempFile,
    first_lines: Vec<u64>,
    record_offsets: Vec<u64>,
}

impl PageSpool {
    pub(super) fn new() -> Result<Self, PagedDocumentError> {
        let file = tempfile::NamedTempFile::new().map_err(|source| PagedDocumentError::Io {
            path: std::env::temp_dir(),
            source,
        })?;
        Ok(Self {
            file,
            first_lines: Vec::new(),
            record_offsets: Vec::new(),
        })
    }

    pub(super) fn push(&mut self, page: LinePage) -> Result<(), PagedDocumentError> {
        let path = self.file.path().to_path_buf();
        let record_offset = sidecar_io(&path, self.file.as_file_mut().seek(SeekFrom::End(0)))?;
        write_u64(self.file.as_file_mut(), page.first_line, &path)?;
        write_u64(self.file.as_file_mut(), page.first_offset, &path)?;
        write_u32(self.file.as_file_mut(), page.line_count as u32, &path)?;
        write_u32(
            self.file.as_file_mut(),
            page.encoded_lengths.len() as u32,
            &path,
        )?;
        write_all(self.file.as_file_mut(), &page.encoded_lengths, &path)?;
        self.first_lines.push(page.first_line);
        self.record_offsets.push(record_offset);
        Ok(())
    }

    pub(super) fn finish(mut self, pages: Vec<LinePage>) -> Result<LinePages, PagedDocumentError> {
        for page in pages {
            self.push(page)?;
        }
        let path = self.file.path().to_path_buf();
        sidecar_io(&path, self.file.as_file_mut().flush())?;
        Ok(LinePages::Disk(Arc::new(DiskLinePages {
            file: Mutex::new(DiskPageFile::Temporary(self.file)),
            first_lines: self.first_lines,
            record_offsets: self.record_offsets,
            cache: Mutex::new(DiskPageCache::default()),
        })))
    }
}

pub(super) struct LineIndexBuilder {
    file_len: u64,
    line_count: u64,
    line_start: u64,
    max_line_bytes: u64,
    prefix: Option<(LinePages, usize)>,
    spool: Option<PageSpool>,
    pages: Vec<LinePage>,
}

impl LineIndexBuilder {
    pub(super) fn new(file_len: u64) -> Self {
        Self {
            file_len,
            line_count: 0,
            line_start: 0,
            max_line_bytes: 0,
            prefix: None,
            spool: None,
            pages: Vec::new(),
        }
    }

    pub(super) fn resume_append(index: &LineIndex, file_len: u64) -> Option<Self> {
        let final_line = index.line_count.checked_sub(1)?;
        let final_start = index.line_range(final_line)?.start;
        let page_count = index.pages.len();
        let mut last_page = index
            .pages
            .page(page_count.checked_sub(1)?)?
            .as_ref()
            .clone();
        let mut cursor = 0usize;
        let mut lengths = (0..last_page.line_count)
            .map(|_| decode_varint(&last_page.encoded_lengths, &mut cursor))
            .collect::<Option<Vec<_>>>()?;
        lengths.pop()?;
        last_page.encoded_lengths.clear();
        for length in lengths {
            encode_varint(length, &mut last_page.encoded_lengths);
        }
        last_page.line_count = last_page.line_count.saturating_sub(1);
        last_page.decoded_ends = OnceLock::new();
        let pages = (last_page.line_count > 0)
            .then_some(last_page)
            .into_iter()
            .collect();
        Some(Self {
            file_len,
            line_count: final_line,
            line_start: final_start,
            // 保留旧上界是安全的；append 后只可能需要进一步放大。
            max_line_bytes: index.max_line_bytes,
            prefix: Some((index.pages.clone(), page_count.saturating_sub(1))),
            spool: None,
            pages,
        })
    }

    pub(super) fn finish_line(&mut self, end: u64) -> Result<(), PagedDocumentError> {
        let length = end - self.line_start;
        self.max_line_bytes = self.max_line_bytes.max(length);
        let needs_page = self.pages.last().is_none_or(|page| {
            page.line_count == LINES_PER_PAGE
                || self.line_start.saturating_sub(page.first_offset) >= MAX_PAGE_SOURCE_BYTES
        });
        if needs_page {
            if self.pages.len() >= DISK_PAGE_CACHE_COUNT {
                let spool = match self.spool.as_mut() {
                    Some(spool) => spool,
                    None => self.spool.insert(PageSpool::new()?),
                };
                for page in self.pages.drain(..) {
                    spool.push(page)?;
                }
            }
            self.pages.push(LinePage {
                first_line: self.line_count,
                first_offset: self.line_start,
                line_count: 0,
                encoded_lengths: Vec::with_capacity(LINES_PER_PAGE * 2),
                decoded_ends: OnceLock::new(),
            });
        }
        let Some(page) = self.pages.last_mut() else {
            return Ok(());
        };
        encode_varint(length, &mut page.encoded_lengths);
        page.line_count += 1;
        self.line_count += 1;
        self.line_start = end;
        Ok(())
    }

    pub(super) fn finish_file(&mut self) -> Result<(), PagedDocumentError> {
        // 文本编辑坐标必须保留末尾换行后的空行；空文件同样有一个逻辑行。
        if self.line_start <= self.file_len {
            self.finish_line(self.file_len)?;
        }
        Ok(())
    }

    pub(super) fn build(self) -> Result<LineIndex, PagedDocumentError> {
        let tail = match self.spool {
            Some(spool) => spool.finish(self.pages)?,
            None => LinePages::Memory(Arc::new(
                self.pages.into_iter().map(Arc::new).collect::<Vec<_>>(),
            )),
        };
        let mut pages = match self.prefix {
            Some((base, base_pages)) if base_pages > 0 => LinePages::Layered {
                base: Arc::new(base),
                base_pages,
                tail: Arc::new(tail),
            },
            _ => tail,
        };
        if pages.depth() > MAX_PAGE_STORAGE_LAYERS {
            pages = pages.compact()?;
        }
        Ok(LineIndex {
            line_count: self.line_count,
            max_line_bytes: self.max_line_bytes,
            pages,
        })
    }
}

fn encode_varint(mut value: u64, output: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            break;
        }
    }
}

pub(super) fn decode_varint(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*cursor)?;
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}
