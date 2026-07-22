// @author kongweiguang

//! CSV/TSV 稀疏记录索引；只在请求视口时物化字段。

use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use csv::{ByteRecord, Position, ReaderBuilder};
use serde::{Deserialize, Serialize};

use crate::{FileSource, PagedDocumentError, PieceDocument, SearchCancellation};

const DELIMITED_SIDECAR_VERSION: u32 = 2;
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
    max_fields: usize,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DelimitedEdit {
    SetCell {
        record: Option<u64>,
        column: usize,
        value: String,
    },
    InsertRow {
        before: u64,
        fields: Vec<String>,
    },
    DeleteRow {
        record: u64,
    },
    InsertColumn {
        before: usize,
        header: String,
    },
    DeleteColumn {
        column: usize,
    },
}

/// 以字段值生成一条 RFC 4180 兼容记录；终止符由调用方传入以保留源文档格式。
pub fn serialize_delimited_record(fields: &[String], delimiter: u8, terminator: &str) -> String {
    let delimiter = delimiter as char;
    let mut output = String::new();
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            output.push(delimiter);
        }
        let quoted = (fields.len() == 1 && field.is_empty())
            || field.contains(delimiter)
            || field.contains('"')
            || field.contains('\r')
            || field.contains('\n');
        if quoted {
            output.push('"');
            for ch in field.chars() {
                if ch == '"' {
                    output.push('"');
                }
                output.push(ch);
            }
            output.push('"');
        } else {
            output.push_str(field);
        }
    }
    output.push_str(terminator);
    output
}

/// 列变换必须扫描全部记录；结果先写临时文件，再以一个 PieceDocument 撤销事务安装。
pub fn apply_delimited_column_edit(
    document: &PieceDocument,
    options: DelimitedIndexOptions,
    edit: &DelimitedEdit,
    cancellation: &SearchCancellation,
) -> Result<PieceDocument, PagedDocumentError> {
    let (column, inserted_header) = match edit {
        DelimitedEdit::InsertColumn { before, header } => (*before, Some(header.as_str())),
        DelimitedEdit::DeleteColumn { column } => (*column, None),
        _ => {
            return Err(PagedDocumentError::InvalidTransaction(
                "streaming column transform requires a column edit".into(),
            ));
        }
    };
    let mut input = tempfile::NamedTempFile::new().map_err(|source| PagedDocumentError::Io {
        path: std::env::temp_dir(),
        source,
    })?;
    document.write_to_cancellable(input.as_file_mut(), cancellation)?;
    input
        .as_file_mut()
        .sync_all()
        .map_err(|source| PagedDocumentError::Io {
            path: input.path().to_path_buf(),
            source,
        })?;
    let source = FileSource::open(input.path())?;
    let source_len = source.identity()?.len;
    let mut reader = reader(&source, options)?;
    let mut output = tempfile::NamedTempFile::new().map_err(|source| PagedDocumentError::Io {
        path: std::env::temp_dir(),
        source,
    })?;
    let mut record = ByteRecord::new();
    let mut physical = 0u64;
    loop {
        if physical.is_multiple_of(1_024) && cancellation.is_cancelled() {
            return Err(PagedDocumentError::Cancelled);
        }
        let start = reader.position().byte();
        if !reader
            .read_byte_record(&mut record)
            .map_err(|error| csv_error(&source, error))?
        {
            break;
        }
        let end = reader.position().byte();
        let raw_end = if end < source_len {
            (end + 1).min(source_len)
        } else {
            end
        };
        let raw = source.read_range(start, raw_end)?;
        let terminator = record_terminator(&raw);
        let mut fields = decode_fields(&record);
        if let Some(header) = inserted_header {
            let value = if physical == 0 && options.has_headers {
                header.to_owned()
            } else {
                String::new()
            };
            fields.insert(column.min(fields.len()), value);
        } else if column < fields.len() {
            fields.remove(column);
        }
        output
            .write_all(
                serialize_delimited_record(&fields, options.delimiter, terminator).as_bytes(),
            )
            .map_err(|source| PagedDocumentError::Io {
                path: output.path().to_path_buf(),
                source,
            })?;
        physical += 1;
    }
    if physical == 0
        && let Some(header) = inserted_header
    {
        output
            .write_all(
                serialize_delimited_record(&[header.to_owned()], options.delimiter, "").as_bytes(),
            )
            .map_err(|source| PagedDocumentError::Io {
                path: output.path().to_path_buf(),
                source,
            })?;
    }
    output
        .as_file_mut()
        .sync_all()
        .map_err(|source| PagedDocumentError::Io {
            path: output.path().to_path_buf(),
            source,
        })?;
    let mut next = document.clone();
    let reader = output.reopen().map_err(|source| PagedDocumentError::Io {
        path: output.path().to_path_buf(),
        source,
    })?;
    next.replace_text_reader(0..next.len(), reader)?;
    Ok(next)
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
    source: DelimitedSource,
    options: DelimitedIndexOptions,
    headers: Vec<String>,
    physical_records: u64,
    max_fields: usize,
    checkpoints: Vec<RecordCheckpoint>,
}

#[derive(Clone, Debug)]
enum DelimitedSource {
    File(PathBuf),
    Snapshot(Arc<[u8]>),
}

impl DelimitedSource {
    fn reader(
        &self,
        options: DelimitedIndexOptions,
    ) -> Result<csv::Reader<DelimitedReader>, PagedDocumentError> {
        let reader = match self {
            Self::File(path) => {
                DelimitedReader::File(std::fs::File::open(path).map_err(|source| {
                    PagedDocumentError::Io {
                        path: path.clone(),
                        source,
                    }
                })?)
            }
            Self::Snapshot(bytes) => DelimitedReader::Snapshot(Cursor::new(Arc::clone(bytes))),
        };
        Ok(ReaderBuilder::new()
            .delimiter(options.delimiter)
            .has_headers(false)
            .flexible(true)
            .from_reader(reader))
    }

    fn len(&self) -> Result<u64, PagedDocumentError> {
        match self {
            Self::File(path) => std::fs::metadata(path)
                .map(|metadata| metadata.len())
                .map_err(|source| PagedDocumentError::Io {
                    path: path.clone(),
                    source,
                }),
            Self::Snapshot(bytes) => Ok(bytes.len() as u64),
        }
    }

    fn read_range(&self, start: u64, end: u64) -> Result<Vec<u8>, PagedDocumentError> {
        match self {
            Self::File(path) => FileSource::open(path)?.read_range(start, end),
            Self::Snapshot(bytes) => {
                let len = bytes.len() as u64;
                if start > end || end > len {
                    return Err(PagedDocumentError::InvalidRange { start, end, len });
                }
                let start =
                    usize::try_from(start).map_err(|_| PagedDocumentError::RangeTooLarge)?;
                let end = usize::try_from(end).map_err(|_| PagedDocumentError::RangeTooLarge)?;
                Ok(bytes[start..end].to_vec())
            }
        }
    }

    fn display_path(&self) -> PathBuf {
        match self {
            Self::File(path) => path.clone(),
            Self::Snapshot(_) => PathBuf::from("<document-snapshot>"),
        }
    }

    fn csv_error(&self, error: csv::Error) -> PagedDocumentError {
        let offset = error.position().map_or(0, Position::byte);
        PagedDocumentError::InvalidDelimited {
            offset,
            message: format!("{} ({})", error, self.display_path().display()),
        }
    }
}

enum DelimitedReader {
    File(std::fs::File),
    Snapshot(Cursor<Arc<[u8]>>),
}

impl Read for DelimitedReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::File(reader) => reader.read(buffer),
            Self::Snapshot(reader) => reader.read(buffer),
        }
    }
}

impl Seek for DelimitedReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        match self {
            Self::File(reader) => reader.seek(position),
            Self::Snapshot(reader) => reader.seek(position),
        }
    }
}

impl DelimitedIndex {
    /// 跨会话缓存只保存文件身份与稀疏位置；表头仍从源文件读取，sidecar 不含正文。
    pub fn build_cached(
        source: &FileSource,
        options: DelimitedIndexOptions,
        cache_dir: impl AsRef<Path>,
    ) -> Result<Self, PagedDocumentError> {
        Self::build_cached_cancellable(source, options, cache_dir, &SearchCancellation::default())
    }

    pub fn build_cached_cancellable(
        source: &FileSource,
        options: DelimitedIndexOptions,
        cache_dir: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<Self, PagedDocumentError> {
        if cancellation.is_cancelled() {
            return Err(PagedDocumentError::Cancelled);
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
    ) -> Result<PathBuf, PagedDocumentError> {
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
    ) -> Result<Self, PagedDocumentError> {
        Self::build_cancellable(source, options, &SearchCancellation::default())
    }

    pub fn build_cancellable(
        source: &FileSource,
        options: DelimitedIndexOptions,
        cancellation: &SearchCancellation,
    ) -> Result<Self, PagedDocumentError> {
        Self::build_from_source(
            DelimitedSource::File(source.path().to_path_buf()),
            options,
            cancellation,
        )
    }

    /// Resident Provider 直接消费不可变文档快照，不创建影子文件或 sidecar。
    pub fn build_snapshot_cancellable(
        bytes: Arc<[u8]>,
        options: DelimitedIndexOptions,
        cancellation: &SearchCancellation,
    ) -> Result<Self, PagedDocumentError> {
        Self::build_from_source(DelimitedSource::Snapshot(bytes), options, cancellation)
    }

    fn build_from_source(
        source: DelimitedSource,
        options: DelimitedIndexOptions,
        cancellation: &SearchCancellation,
    ) -> Result<Self, PagedDocumentError> {
        let mut reader = source.reader(options)?;
        let mut record = ByteRecord::new();
        let mut physical_records = 0u64;
        let mut checkpoints = Vec::new();
        let mut last_checkpoint_byte = 0u64;
        let mut headers = Vec::new();
        let mut max_fields = 0usize;

        loop {
            if physical_records.is_multiple_of(1_024) && cancellation.is_cancelled() {
                return Err(PagedDocumentError::Cancelled);
            }
            let position = reader.position().clone();
            if !reader
                .read_byte_record(&mut record)
                .map_err(|error| source.csv_error(error))?
            {
                break;
            }
            if physical_records == 0 && options.has_headers {
                headers = decode_fields(&record);
            }
            max_fields = max_fields.max(record.len());
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

        extend_synthetic_headers(&mut headers, max_fields);
        Ok(Self {
            source,
            options,
            headers,
            physical_records,
            max_fields,
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

    pub fn delimiter(&self) -> u8 {
        self.options.delimiter
    }

    pub fn column_count(&self) -> usize {
        self.max_fields
    }

    pub fn read_header(&self) -> Result<Option<DelimitedRecord>, PagedDocumentError> {
        if !self.options.has_headers || self.physical_records == 0 {
            return Ok(None);
        }
        self.read_physical_records(0, 1, 0..usize::MAX)
            .map(|mut records| records.pop())
    }

    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    fn load_sidecar(
        source: &FileSource,
        options: DelimitedIndexOptions,
        path: &Path,
    ) -> Result<Option<Self>, PagedDocumentError> {
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) if metadata.len() <= MAX_DELIMITED_SIDECAR_BYTES => metadata,
            Ok(_) | Err(_) => return Ok(None),
        };
        if metadata.len() == 0 {
            return Ok(None);
        }
        let bytes = std::fs::read(path).map_err(|source_error| PagedDocumentError::Io {
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

        let mut headers = read_headers(source, options)?;
        extend_synthetic_headers(&mut headers, payload.max_fields);
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
            source: DelimitedSource::File(source.path().to_path_buf()),
            options,
            headers,
            physical_records: payload.physical_records,
            max_fields: payload.max_fields,
            checkpoints,
        }))
    }

    fn store_sidecar(&self, source: &FileSource, path: &Path) -> Result<(), PagedDocumentError> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent).map_err(|source_error| PagedDocumentError::Io {
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
            max_fields: self.max_fields,
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
            PagedDocumentError::Io {
                path: parent.to_path_buf(),
                source: source_error,
            }
        })?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|source_error| PagedDocumentError::Io {
                path: temporary.path().to_path_buf(),
                source: source_error,
            })?;
        temporary
            .persist(path)
            .map_err(|error| PagedDocumentError::Persist {
                path: path.to_path_buf(),
                source: error.error,
            })?;
        Ok(())
    }

    pub fn read_records(
        &self,
        start: u64,
        count: usize,
    ) -> Result<Vec<DelimitedRecord>, PagedDocumentError> {
        self.read_records_columns(start, count, 0..usize::MAX)
    }

    /// 只解码调用方当前可见的列窗口。CSV 解析器仍需越过完整记录以保持引号换行语义，
    /// 但不会为屏幕外的数千列分配 `String`，因此视口内存只随可见行列增长。
    pub fn read_records_columns(
        &self,
        start: u64,
        count: usize,
        columns: std::ops::Range<usize>,
    ) -> Result<Vec<DelimitedRecord>, PagedDocumentError> {
        if start >= self.record_count() || count == 0 {
            return Ok(Vec::new());
        }
        let target_physical = start + u64::from(self.options.has_headers);
        let projected_columns = columns
            .end
            .min(self.max_fields)
            .saturating_sub(columns.start);
        self.read_physical_records(target_physical, count, columns)
            .map(|records| {
                records
                    .into_iter()
                    .map(|mut record| {
                        record.record_index = record
                            .record_index
                            .saturating_sub(u64::from(self.options.has_headers));
                        record.fields.resize(projected_columns, String::new());
                        record
                    })
                    .collect()
            })
    }

    fn read_physical_records(
        &self,
        target_physical: u64,
        count: usize,
        columns: std::ops::Range<usize>,
    ) -> Result<Vec<DelimitedRecord>, PagedDocumentError> {
        let checkpoint = self
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.physical_record <= target_physical)
            .ok_or(PagedDocumentError::InvalidRange {
                start: target_physical,
                end: target_physical,
                len: self.physical_records,
            })?;
        let source_len = self.source.len()?;
        let mut reader = self.source.reader(self.options)?;
        reader
            .seek(checkpoint.position.clone())
            .map_err(|error| self.source.csv_error(error))?;
        let mut physical = checkpoint.physical_record;
        let mut record = ByteRecord::new();
        let mut output = Vec::with_capacity(count);
        while output.len() < count {
            let byte_start = reader.position().byte();
            if !reader
                .read_byte_record(&mut record)
                .map_err(|error| self.source.csv_error(error))?
            {
                break;
            }
            let byte_end = reader.position().byte();
            if physical >= target_physical {
                let byte_range =
                    normalized_record_range(&self.source, byte_start, byte_end, source_len)?;
                output.push(DelimitedRecord {
                    record_index: physical,
                    byte_range,
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
    ) -> Result<Vec<u64>, PagedDocumentError> {
        if query.is_empty() || options.result_limit == 0 {
            return Ok(Vec::new());
        }
        if let Some(column) = options.column
            && !self.headers.is_empty()
            && column >= self.headers.len()
        {
            return Err(PagedDocumentError::InvalidRange {
                start: column as u64,
                end: column as u64,
                len: self.headers.len() as u64,
            });
        }
        let mut reader = self.source.reader(self.options)?;
        let mut record = ByteRecord::new();
        let mut physical = 0u64;
        let folded_query = (!options.case_sensitive).then(|| query.to_lowercase());
        let mut matches = Vec::new();
        while matches.len() < options.result_limit {
            if physical.is_multiple_of(1_024) && cancellation.is_cancelled() {
                return Err(PagedDocumentError::Cancelled);
            }
            if !reader
                .read_byte_record(&mut record)
                .map_err(|error| self.source.csv_error(error))?
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
) -> Result<Vec<String>, PagedDocumentError> {
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

fn delimited_sampled_hash(source: &FileSource, len: u64) -> Result<u32, PagedDocumentError> {
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

fn cache_data_error(path: &Path, message: String) -> PagedDocumentError {
    PagedDocumentError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, message),
    }
}

fn cleanup_delimited_sidecars(
    cache_dir: &Path,
    keep: &Path,
    budget_bytes: u64,
) -> Result<(), PagedDocumentError> {
    let entries = std::fs::read_dir(cache_dir).map_err(|source_error| PagedDocumentError::Io {
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
) -> Result<csv::Reader<std::fs::File>, PagedDocumentError> {
    ReaderBuilder::new()
        .delimiter(options.delimiter)
        .has_headers(false)
        .flexible(true)
        .from_path(source.path())
        .map_err(|error| csv_error(source, error))
}

fn csv_error(source: &FileSource, error: csv::Error) -> PagedDocumentError {
    let offset = error.position().map_or(0, Position::byte);
    PagedDocumentError::InvalidDelimited {
        offset,
        message: format!("{} ({})", error, source.path().display()),
    }
}

fn decode_fields(record: &ByteRecord) -> Vec<String> {
    record
        .iter()
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect()
}

fn extend_synthetic_headers(headers: &mut Vec<String>, max_fields: usize) {
    while headers.len() < max_fields {
        headers.push(format!("Column {}", headers.len() + 1));
    }
}

fn record_terminator(bytes: &[u8]) -> &'static str {
    if bytes.ends_with(b"\r\n") {
        "\r\n"
    } else if bytes.ends_with(b"\n") {
        "\n"
    } else if bytes.ends_with(b"\r") {
        "\r"
    } else {
        ""
    }
}

fn normalized_record_range(
    source: &DelimitedSource,
    mut start: u64,
    mut end: u64,
    source_len: u64,
) -> Result<std::ops::Range<u64>, PagedDocumentError> {
    if start > 0 && start < source_len {
        let boundary = source.read_range(start - 1, (start + 1).min(source_len))?;
        if boundary == b"\r\n" {
            start += 1;
        }
    }
    if end > 0 && end < source_len {
        let boundary = source.read_range(end - 1, (end + 1).min(source_len))?;
        if boundary == b"\r\n" {
            end += 1;
        }
    }
    Ok(start.min(source_len)..end.min(source_len))
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
