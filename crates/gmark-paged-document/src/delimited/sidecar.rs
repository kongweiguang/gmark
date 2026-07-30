// @author kongweiguang

use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

use csv::{ByteRecord, Position};

use super::model::{
    CachedDelimitedEnvelope, CachedDelimitedPayload, CachedRecordCheckpoint,
    DELIMITED_CACHE_BUDGET_BYTES, DELIMITED_SIDECAR_SAMPLE_BYTES, DELIMITED_SIDECAR_VERSION,
    DelimitedIndex, DelimitedIndexOptions, MAX_DELIMITED_SIDECAR_BYTES, RecordCheckpoint,
};
use super::source::{DelimitedSource, csv_error, decode_fields, extend_synthetic_headers, reader};
use crate::{FileSource, PagedDocumentError, SearchCancellation};

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

pub(super) fn cleanup_delimited_sidecars(
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
