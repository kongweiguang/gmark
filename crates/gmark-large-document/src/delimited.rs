// @author kongweiguang

//! CSV/TSV 稀疏记录索引；只在请求视口时物化字段。

use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

use csv::{ByteRecord, Position, ReaderBuilder};
use serde::{Deserialize, Serialize};

use crate::{FileSource, LargeDocumentError, SearchCancellation};

const DELIMITED_SIDECAR_VERSION: u32 = 1;
const DELIMITED_SIDECAR_SAMPLE_BYTES: u64 = 64 * 1024;
const MAX_DELIMITED_SIDECAR_BYTES: u64 = 64 * 1024 * 1024;
const DELIMITED_CACHE_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct CachedDelimitedPayload {
    version: u32,
    len: u64,
    modified_nanos: Option<u128>,
    sampled_hash: u32,
    delimiter: u8,
    has_headers: bool,
    checkpoint_records: u64,
    checkpoint_bytes: u64,
    physical_records: u64,
    checkpoints: Vec<CachedRecordCheckpoint>,
}

#[derive(Serialize, Deserialize)]
struct CachedDelimitedEnvelope {
    payload: CachedDelimitedPayload,
    checksum: u32,
}

#[derive(Serialize, Deserialize)]
struct CachedRecordCheckpoint {
    physical_record: u64,
    byte: u64,
    line: u64,
    record: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct DelimitedIndexOptions {
    pub delimiter: u8,
    pub has_headers: bool,
    pub checkpoint_records: u64,
    pub checkpoint_bytes: u64,
}

impl Default for DelimitedIndexOptions {
    fn default() -> Self {
        Self {
            delimiter: b',',
            has_headers: true,
            checkpoint_records: 4_096,
            checkpoint_bytes: 4 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug)]
struct RecordCheckpoint {
    physical_record: u64,
    position: Position,
}

#[derive(Clone, Debug)]
pub struct DelimitedRecord {
    pub record_index: u64,
    pub byte_range: std::ops::Range<u64>,
    pub fields: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct DelimitedFilterOptions {
    pub column: Option<usize>,
    pub case_sensitive: bool,
    pub result_limit: usize,
}

impl Default for DelimitedFilterOptions {
    fn default() -> Self {
        Self {
            column: None,
            case_sensitive: false,
            result_limit: 10_000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DelimitedIndex {
    path: std::path::PathBuf,
    options: DelimitedIndexOptions,
    headers: Vec<String>,
    physical_records: u64,
    checkpoints: Vec<RecordCheckpoint>,
}

impl DelimitedIndex {
    /// 跨会话缓存只保存文件身份与稀疏位置；表头仍从源文件读取，sidecar 不含正文。
    pub fn build_cached(
        source: &FileSource,
        options: DelimitedIndexOptions,
        cache_dir: impl AsRef<Path>,
    ) -> Result<Self, LargeDocumentError> {
        Self::build_cached_cancellable(source, options, cache_dir, &SearchCancellation::default())
    }

    pub fn build_cached_cancellable(
        source: &FileSource,
        options: DelimitedIndexOptions,
        cache_dir: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<Self, LargeDocumentError> {
        if cancellation.is_cancelled() {
            return Err(LargeDocumentError::Cancelled);
        }
        let cache_path = Self::sidecar_path(source, options, cache_dir)?;
        if let Ok(Some(index)) = Self::load_sidecar(source, options, &cache_path) {
            return Ok(index);
        }
        let index = Self::build_cancellable(source, options, cancellation)?;
        if index.store_sidecar(source, &cache_path).is_ok()
            && let Some(cache_dir) = cache_path.parent()
        {
            let _ =
                cleanup_delimited_sidecars(cache_dir, &cache_path, DELIMITED_CACHE_BUDGET_BYTES);
        }
        Ok(index)
    }

    pub fn sidecar_path(
        source: &FileSource,
        options: DelimitedIndexOptions,
        cache_dir: impl AsRef<Path>,
    ) -> Result<PathBuf, LargeDocumentError> {
        let identity = source.identity()?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        identity.path.hash(&mut hasher);
        options.delimiter.hash(&mut hasher);
        options.has_headers.hash(&mut hasher);
        Ok(cache_dir.as_ref().join(format!(
            "{:016x}.gmark-delimited-v{DELIMITED_SIDECAR_VERSION}",
            hasher.finish()
        )))
    }

    pub fn build(
        source: &FileSource,
        options: DelimitedIndexOptions,
    ) -> Result<Self, LargeDocumentError> {
        Self::build_cancellable(source, options, &SearchCancellation::default())
    }

    pub fn build_cancellable(
        source: &FileSource,
        options: DelimitedIndexOptions,
        cancellation: &SearchCancellation,
    ) -> Result<Self, LargeDocumentError> {
        let mut reader = reader(source, options)?;
        let mut record = ByteRecord::new();
        let mut physical_records = 0u64;
        let mut checkpoints = Vec::new();
        let mut last_checkpoint_byte = 0u64;
        let mut headers = Vec::new();

        loop {
            if physical_records.is_multiple_of(1_024) && cancellation.is_cancelled() {
                return Err(LargeDocumentError::Cancelled);
            }
            let position = reader.position().clone();
            if !reader
                .read_byte_record(&mut record)
                .map_err(|error| csv_error(source, error))?
            {
                break;
            }
            if physical_records == 0 && options.has_headers {
                headers = decode_fields(&record);
            }
            if physical_records == 0
                || physical_records.is_multiple_of(options.checkpoint_records.max(1))
                || position.byte().saturating_sub(last_checkpoint_byte)
                    >= options.checkpoint_bytes.max(1)
            {
                last_checkpoint_byte = position.byte();
                checkpoints.push(RecordCheckpoint {
                    physical_record: physical_records,
                    position,
                });
            }
            physical_records += 1;
        }

        Ok(Self {
            path: source.path().to_path_buf(),
            options,
            headers,
            physical_records,
            checkpoints,
        })
    }

    pub fn headers(&self) -> &[String] {
        &self.headers
    }

    pub fn record_count(&self) -> u64 {
        self.physical_records
            .saturating_sub(u64::from(self.options.has_headers))
    }

    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    fn load_sidecar(
        source: &FileSource,
        options: DelimitedIndexOptions,
        path: &Path,
    ) -> Result<Option<Self>, LargeDocumentError> {
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) if metadata.len() <= MAX_DELIMITED_SIDECAR_BYTES => metadata,
            Ok(_) | Err(_) => return Ok(None),
        };
        if metadata.len() == 0 {
            return Ok(None);
        }
        let bytes = std::fs::read(path).map_err(|source_error| LargeDocumentError::Io {
            path: path.to_path_buf(),
            source: source_error,
        })?;
        let Ok(envelope) = serde_json::from_slice::<CachedDelimitedEnvelope>(&bytes) else {
            return Ok(None);
        };
        let Ok(payload_bytes) = serde_json::to_vec(&envelope.payload) else {
            return Ok(None);
        };
        let identity = source.identity()?;
        let payload = envelope.payload;
        if envelope.checksum != crc32fast::hash(&payload_bytes)
            || payload.version != DELIMITED_SIDECAR_VERSION
            || payload.len != identity.len
            || payload.modified_nanos != identity.modified_nanos
            || payload.sampled_hash != delimited_sampled_hash(source, identity.len)?
            || payload.delimiter != options.delimiter
            || payload.has_headers != options.has_headers
            || payload.checkpoint_records != options.checkpoint_records
            || payload.checkpoint_bytes != options.checkpoint_bytes
            || !valid_cached_checkpoints(
                &payload.checkpoints,
                payload.physical_records,
                identity.len,
            )
        {
            return Ok(None);
        }

        let headers = read_headers(source, options)?;
        let checkpoints = payload
            .checkpoints
            .into_iter()
            .map(|checkpoint| {
                let mut position = Position::new();
                position
                    .set_byte(checkpoint.byte)
                    .set_line(checkpoint.line)
                    .set_record(checkpoint.record);
                RecordCheckpoint {
                    physical_record: checkpoint.physical_record,
                    position,
                }
            })
            .collect();
        Ok(Some(Self {
            path: source.path().to_path_buf(),
            options,
            headers,
            physical_records: payload.physical_records,
            checkpoints,
        }))
    }

    fn store_sidecar(&self, source: &FileSource, path: &Path) -> Result<(), LargeDocumentError> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent).map_err(|source_error| LargeDocumentError::Io {
            path: parent.to_path_buf(),
            source: source_error,
        })?;
        let identity = source.identity()?;
        let payload = CachedDelimitedPayload {
            version: DELIMITED_SIDECAR_VERSION,
            len: identity.len,
            modified_nanos: identity.modified_nanos,
            sampled_hash: delimited_sampled_hash(source, identity.len)?,
            delimiter: self.options.delimiter,
            has_headers: self.options.has_headers,
            checkpoint_records: self.options.checkpoint_records,
            checkpoint_bytes: self.options.checkpoint_bytes,
            physical_records: self.physical_records,
            checkpoints: self
                .checkpoints
                .iter()
                .map(|checkpoint| CachedRecordCheckpoint {
                    physical_record: checkpoint.physical_record,
                    byte: checkpoint.position.byte(),
                    line: checkpoint.position.line(),
                    record: checkpoint.position.record(),
                })
                .collect(),
        };
        let payload_bytes = serde_json::to_vec(&payload)
            .map_err(|error| cache_data_error(path, error.to_string()))?;
        let envelope = CachedDelimitedEnvelope {
            checksum: crc32fast::hash(&payload_bytes),
            payload,
        };
        let bytes = serde_json::to_vec(&envelope)
            .map_err(|error| cache_data_error(path, error.to_string()))?;
        if bytes.len() as u64 > MAX_DELIMITED_SIDECAR_BYTES {
            return Ok(());
        }
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|source_error| {
            LargeDocumentError::Io {
                path: parent.to_path_buf(),
                source: source_error,
            }
        })?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|source_error| LargeDocumentError::Io {
                path: temporary.path().to_path_buf(),
                source: source_error,
            })?;
        temporary
            .persist(path)
            .map_err(|error| LargeDocumentError::Persist {
                path: path.to_path_buf(),
                source: error.error,
            })?;
        Ok(())
    }

    pub fn read_records(
        &self,
        start: u64,
        count: usize,
    ) -> Result<Vec<DelimitedRecord>, LargeDocumentError> {
        self.read_records_columns(start, count, 0..usize::MAX)
    }

    /// 只解码调用方当前可见的列窗口。CSV 解析器仍需越过完整记录以保持引号换行语义，
    /// 但不会为屏幕外的数千列分配 `String`，因此视口内存只随可见行列增长。
    pub fn read_records_columns(
        &self,
        start: u64,
        count: usize,
        columns: std::ops::Range<usize>,
    ) -> Result<Vec<DelimitedRecord>, LargeDocumentError> {
        if start >= self.record_count() || count == 0 {
            return Ok(Vec::new());
        }
        let target_physical = start + u64::from(self.options.has_headers);
        let checkpoint = self
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.physical_record <= target_physical)
            .ok_or_else(|| LargeDocumentError::InvalidRange {
                start,
                end: start,
                len: self.record_count(),
            })?;
        let source = FileSource::open(&self.path)?;
        let mut reader = reader(&source, self.options)?;
        reader
            .seek(checkpoint.position.clone())
            .map_err(|error| csv_error(&source, error))?;
        let mut physical = checkpoint.physical_record;
        let mut record = ByteRecord::new();
        let mut output = Vec::with_capacity(count);
        while output.len() < count {
            let byte_start = reader.position().byte();
            if !reader
                .read_byte_record(&mut record)
                .map_err(|error| csv_error(&source, error))?
            {
                break;
            }
            let byte_end = reader.position().byte();
            if physical >= target_physical {
                output.push(DelimitedRecord {
                    record_index: physical - u64::from(self.options.has_headers),
                    byte_range: byte_start..byte_end,
                    fields: decode_fields_in_range(&record, columns.clone()),
                });
            }
            physical += 1;
        }
        Ok(output)
    }

    pub fn filter_record_indices(
        &self,
        query: &str,
        options: DelimitedFilterOptions,
        cancellation: &SearchCancellation,
    ) -> Result<Vec<u64>, LargeDocumentError> {
        if query.is_empty() || options.result_limit == 0 {
            return Ok(Vec::new());
        }
        if let Some(column) = options.column
            && !self.headers.is_empty()
            && column >= self.headers.len()
        {
            return Err(LargeDocumentError::InvalidRange {
                start: column as u64,
                end: column as u64,
                len: self.headers.len() as u64,
            });
        }
        let source = FileSource::open(&self.path)?;
        let mut reader = reader(&source, self.options)?;
        let mut record = ByteRecord::new();
        let mut physical = 0u64;
        let folded_query = (!options.case_sensitive).then(|| query.to_lowercase());
        let mut matches = Vec::new();
        while matches.len() < options.result_limit {
            if physical.is_multiple_of(1_024) && cancellation.is_cancelled() {
                return Err(LargeDocumentError::Cancelled);
            }
            if !reader
                .read_byte_record(&mut record)
                .map_err(|error| csv_error(&source, error))?
            {
                break;
            }
            if physical == 0 && self.options.has_headers {
                physical += 1;
                continue;
            }
            let matches_field = |field: &[u8]| {
                if options.case_sensitive {
                    memchr::memmem::find(field, query.as_bytes()).is_some()
                } else {
                    String::from_utf8_lossy(field)
                        .to_lowercase()
                        .contains(folded_query.as_deref().unwrap_or_default())
                }
            };
            let matched = match options.column {
                Some(column) => record.get(column).is_some_and(matches_field),
                None => record.iter().any(matches_field),
            };
            if matched {
                matches.push(physical - u64::from(self.options.has_headers));
            }
            physical += 1;
        }
        Ok(matches)
    }
}

fn read_headers(
    source: &FileSource,
    options: DelimitedIndexOptions,
) -> Result<Vec<String>, LargeDocumentError> {
    if !options.has_headers {
        return Ok(Vec::new());
    }
    let mut reader = reader(source, options)?;
    let mut record = ByteRecord::new();
    if reader
        .read_byte_record(&mut record)
        .map_err(|error| csv_error(source, error))?
    {
        Ok(decode_fields(&record))
    } else {
        Ok(Vec::new())
    }
}

fn valid_cached_checkpoints(
    checkpoints: &[CachedRecordCheckpoint],
    physical_records: u64,
    file_len: u64,
) -> bool {
    if physical_records == 0 {
        return checkpoints.is_empty();
    }
    if checkpoints.first().is_none_or(|checkpoint| {
        checkpoint.physical_record != 0 || checkpoint.byte != 0 || checkpoint.line == 0
    }) {
        return false;
    }
    checkpoints.windows(2).all(|pair| {
        pair[0].physical_record < pair[1].physical_record
            && pair[0].byte < pair[1].byte
            && pair[0].line <= pair[1].line
    }) && checkpoints.iter().all(|checkpoint| {
        checkpoint.physical_record < physical_records
            && checkpoint.byte <= file_len
            && checkpoint.line > 0
    })
}

fn delimited_sampled_hash(source: &FileSource, len: u64) -> Result<u32, LargeDocumentError> {
    let mut hasher = crc32fast::Hasher::new();
    for start in [
        0,
        len.saturating_sub(DELIMITED_SIDECAR_SAMPLE_BYTES) / 2,
        len.saturating_sub(DELIMITED_SIDECAR_SAMPLE_BYTES),
    ] {
        let end = (start + DELIMITED_SIDECAR_SAMPLE_BYTES).min(len);
        if start < end {
            hasher.update(&source.read_range(start, end)?);
        }
    }
    Ok(hasher.finalize())
}

fn cache_data_error(path: &Path, message: String) -> LargeDocumentError {
    LargeDocumentError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, message),
    }
}

fn cleanup_delimited_sidecars(
    cache_dir: &Path,
    keep: &Path,
    budget_bytes: u64,
) -> Result<(), LargeDocumentError> {
    let entries = std::fs::read_dir(cache_dir).map_err(|source_error| LargeDocumentError::Io {
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
                .is_some_and(|name| name.contains(".gmark-delimited-v"))
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

fn reader(
    source: &FileSource,
    options: DelimitedIndexOptions,
) -> Result<csv::Reader<std::fs::File>, LargeDocumentError> {
    ReaderBuilder::new()
        .delimiter(options.delimiter)
        .has_headers(false)
        .flexible(true)
        .from_path(source.path())
        .map_err(|error| csv_error(source, error))
}

fn csv_error(source: &FileSource, error: csv::Error) -> LargeDocumentError {
    LargeDocumentError::Io {
        path: source.path().to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, error),
    }
}

fn decode_fields(record: &ByteRecord) -> Vec<String> {
    record
        .iter()
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect()
}

fn decode_fields_in_range(record: &ByteRecord, columns: std::ops::Range<usize>) -> Vec<String> {
    let count = columns.end.saturating_sub(columns.start);
    record
        .iter()
        .skip(columns.start)
        .take(count)
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/delimited.rs"]
mod tests;
