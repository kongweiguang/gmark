// @author kongweiguang

//! 不构造 DOM 的 JSON 根级稀疏索引。

use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{FileSource, LineIndex, PagedDocumentError, SearchCancellation};
use serde::{Deserialize, Serialize};

const READ_BLOCK_BYTES: u64 = 256 * 1024;
const MAX_JSON_DEPTH: usize = 65_536;
const JSON_SIDECAR_VERSION: u32 = 1;
const JSON_SIDECAR_SAMPLE_BYTES: u64 = 64 * 1024;
const MAX_JSON_SIDECAR_BYTES: u64 = 64 * 1024 * 1024;
const JSON_CACHE_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct CachedJsonPayload {
    version: u32,
    len: u64,
    modified_nanos: Option<u128>,
    sampled_hash: u32,
    checkpoint_items: u64,
    checkpoint_bytes: u64,
    root_kind: u8,
    item_count: u64,
    root_start: u64,
    root_end: u64,
    checkpoints: Vec<CachedJsonCheckpoint>,
}

#[derive(Serialize, Deserialize)]
struct CachedJsonEnvelope {
    payload: CachedJsonPayload,
    checksum: u32,
}

#[derive(Serialize, Deserialize)]
struct CachedJsonCheckpoint {
    item_index: u64,
    byte_offset: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonRootKind {
    Object,
    Array,
    Scalar,
}

/// JSON 容器子项的可选对象键范围与值范围，二者均使用原文件 UTF-8 字节坐标。
pub type JsonItemKeyValueRanges = (Option<std::ops::Range<u64>>, std::ops::Range<u64>);

#[derive(Clone, Copy, Debug)]
pub struct JsonIndexOptions {
    pub checkpoint_items: u64,
    pub checkpoint_bytes: u64,
}

impl Default for JsonIndexOptions {
    fn default() -> Self {
        Self {
            checkpoint_items: 4_096,
            checkpoint_bytes: 4 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug)]
struct JsonCheckpoint {
    item_index: u64,
    byte_offset: u64,
}

#[derive(Clone, Debug)]
pub struct JsonIndex {
    path: std::path::PathBuf,
    root_kind: JsonRootKind,
    item_count: u64,
    root_range: std::ops::Range<u64>,
    checkpoints: Vec<JsonCheckpoint>,
}

impl JsonIndex {
    /// 根级稀疏索引可跨会话复用；缓存只含身份和偏移，不含 key/value 正文。
    pub fn build_cached(
        source: &FileSource,
        options: JsonIndexOptions,
        cache_dir: impl AsRef<Path>,
    ) -> Result<Self, PagedDocumentError> {
        Self::build_cached_cancellable(source, options, cache_dir, &SearchCancellation::default())
    }

    pub fn build_cached_cancellable(
        source: &FileSource,
        options: JsonIndexOptions,
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
        if index.store_sidecar(source, options, &cache_path).is_ok()
            && let Some(cache_dir) = cache_path.parent()
        {
            let _ = cleanup_json_sidecars(cache_dir, &cache_path, JSON_CACHE_BUDGET_BYTES);
        }
        Ok(index)
    }

    pub fn sidecar_path(
        source: &FileSource,
        options: JsonIndexOptions,
        cache_dir: impl AsRef<Path>,
    ) -> Result<PathBuf, PagedDocumentError> {
        let identity = source.identity()?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        identity.path.hash(&mut hasher);
        options.checkpoint_items.hash(&mut hasher);
        options.checkpoint_bytes.hash(&mut hasher);
        Ok(cache_dir.as_ref().join(format!(
            "{:016x}.gmark-json-v{JSON_SIDECAR_VERSION}",
            hasher.finish()
        )))
    }

    pub fn build(
        source: &FileSource,
        options: JsonIndexOptions,
    ) -> Result<Self, PagedDocumentError> {
        Self::build_cancellable(source, options, &SearchCancellation::default())
    }

    pub fn build_cancellable(
        source: &FileSource,
        options: JsonIndexOptions,
        cancellation: &SearchCancellation,
    ) -> Result<Self, PagedDocumentError> {
        let len = source.identity()?.len;
        Self::build_range_cancellable(source, 0..len, options, cancellation)
    }

    pub fn build_range(
        source: &FileSource,
        range: std::ops::Range<u64>,
        options: JsonIndexOptions,
    ) -> Result<Self, PagedDocumentError> {
        Self::build_range_cancellable(source, range, options, &SearchCancellation::default())
    }

    pub fn build_range_cancellable(
        source: &FileSource,
        range: std::ops::Range<u64>,
        options: JsonIndexOptions,
        cancellation: &SearchCancellation,
    ) -> Result<Self, PagedDocumentError> {
        let len = source.identity()?.len;
        if range.start > range.end || range.end > len {
            return Err(PagedDocumentError::InvalidRange {
                start: range.start,
                end: range.end,
                len,
            });
        }
        validate_json_range(source, range.clone(), cancellation)?;
        let mut cursor =
            ByteCursor::new_cancellable(source.clone(), range.start, range.end, cancellation);
        let Some((root_start, first)) = next_non_whitespace(&mut cursor)? else {
            return Err(invalid_json(0, "document is empty"));
        };
        let (root_kind, closing) = match first {
            b'{' => (JsonRootKind::Object, Some(b'}')),
            b'[' => (JsonRootKind::Array, Some(b']')),
            _ => (JsonRootKind::Scalar, None),
        };
        if closing.is_none() {
            return Ok(Self {
                path: source.path().to_path_buf(),
                root_kind,
                item_count: 1,
                root_range: trim_range_end(source, root_start..range.end)?,
                checkpoints: vec![JsonCheckpoint {
                    item_index: 0,
                    byte_offset: root_start,
                }],
            });
        }

        let Some(closing) = closing else {
            return Err(invalid_json(root_start, "missing root closing delimiter"));
        };
        let mut depth = 1u64;
        let mut in_string = false;
        let mut escaped = false;
        let mut item_start = None;
        let mut item_count = 0u64;
        let mut checkpoints = Vec::new();
        let mut last_checkpoint_byte = root_start;
        let root_end = loop {
            let Some((offset, byte)) = cursor.next_byte()? else {
                return Err(invalid_json(range.end, "unterminated root container"));
            };
            if in_string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
                continue;
            }
            if byte == b'"' {
                in_string = true;
                if item_start.is_none() && depth == 1 {
                    item_start = Some(offset);
                    maybe_checkpoint(
                        &mut checkpoints,
                        item_count,
                        offset,
                        &mut last_checkpoint_byte,
                        options,
                    );
                }
                continue;
            }
            if item_start.is_none() && depth == 1 && !byte.is_ascii_whitespace() && byte != closing
            {
                item_start = Some(offset);
                maybe_checkpoint(
                    &mut checkpoints,
                    item_count,
                    offset,
                    &mut last_checkpoint_byte,
                    options,
                );
            }
            match byte {
                b'{' | b'[' => depth += 1,
                b'}' | b']' if depth > 1 => depth -= 1,
                value if value == closing && depth == 1 => {
                    if item_start.is_some() {
                        item_count += 1;
                    }
                    break offset + 1;
                }
                b',' if depth == 1 => {
                    if item_start.is_none() {
                        return Err(invalid_json(offset, "empty root item"));
                    }
                    item_count += 1;
                    item_start = None;
                }
                _ => {}
            }
        };

        Ok(Self {
            path: source.path().to_path_buf(),
            root_kind,
            item_count,
            root_range: root_start..root_end,
            checkpoints,
        })
    }

    pub fn root_kind(&self) -> JsonRootKind {
        self.root_kind
    }

    pub fn item_count(&self) -> u64 {
        self.item_count
    }

    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    fn load_sidecar(
        source: &FileSource,
        options: JsonIndexOptions,
        path: &Path,
    ) -> Result<Option<Self>, PagedDocumentError> {
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) if metadata.len() <= MAX_JSON_SIDECAR_BYTES => metadata,
            Ok(_) | Err(_) => return Ok(None),
        };
        if metadata.len() == 0 {
            return Ok(None);
        }
        let bytes = std::fs::read(path).map_err(|source_error| PagedDocumentError::Io {
            path: path.to_path_buf(),
            source: source_error,
        })?;
        let Ok(envelope) = serde_json::from_slice::<CachedJsonEnvelope>(&bytes) else {
            return Ok(None);
        };
        let Ok(payload_bytes) = serde_json::to_vec(&envelope.payload) else {
            return Ok(None);
        };
        let identity = source.identity()?;
        let payload = envelope.payload;
        let Some(root_kind) = decode_cached_root_kind(payload.root_kind) else {
            return Ok(None);
        };
        if envelope.checksum != crc32fast::hash(&payload_bytes)
            || payload.version != JSON_SIDECAR_VERSION
            || payload.len != identity.len
            || payload.modified_nanos != identity.modified_nanos
            || payload.sampled_hash != json_sampled_hash(source, identity.len)?
            || payload.checkpoint_items != options.checkpoint_items
            || payload.checkpoint_bytes != options.checkpoint_bytes
            || !valid_cached_json_index(&payload, root_kind)
        {
            return Ok(None);
        }
        Ok(Some(Self {
            path: source.path().to_path_buf(),
            root_kind,
            item_count: payload.item_count,
            root_range: payload.root_start..payload.root_end,
            checkpoints: payload
                .checkpoints
                .into_iter()
                .map(|checkpoint| JsonCheckpoint {
                    item_index: checkpoint.item_index,
                    byte_offset: checkpoint.byte_offset,
                })
                .collect(),
        }))
    }

    fn store_sidecar(
        &self,
        source: &FileSource,
        options: JsonIndexOptions,
        path: &Path,
    ) -> Result<(), PagedDocumentError> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent).map_err(|source_error| PagedDocumentError::Io {
            path: parent.to_path_buf(),
            source: source_error,
        })?;
        let identity = source.identity()?;
        let payload = CachedJsonPayload {
            version: JSON_SIDECAR_VERSION,
            len: identity.len,
            modified_nanos: identity.modified_nanos,
            sampled_hash: json_sampled_hash(source, identity.len)?,
            checkpoint_items: options.checkpoint_items,
            checkpoint_bytes: options.checkpoint_bytes,
            root_kind: encode_cached_root_kind(self.root_kind),
            item_count: self.item_count,
            root_start: self.root_range.start,
            root_end: self.root_range.end,
            checkpoints: self
                .checkpoints
                .iter()
                .map(|checkpoint| CachedJsonCheckpoint {
                    item_index: checkpoint.item_index,
                    byte_offset: checkpoint.byte_offset,
                })
                .collect(),
        };
        let payload_bytes = serde_json::to_vec(&payload)
            .map_err(|error| json_cache_data_error(path, error.to_string()))?;
        let envelope = CachedJsonEnvelope {
            checksum: crc32fast::hash(&payload_bytes),
            payload,
        };
        let bytes = serde_json::to_vec(&envelope)
            .map_err(|error| json_cache_data_error(path, error.to_string()))?;
        if bytes.len() as u64 > MAX_JSON_SIDECAR_BYTES {
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

    pub fn item_range(
        &self,
        item_index: u64,
    ) -> Result<Option<std::ops::Range<u64>>, PagedDocumentError> {
        if item_index >= self.item_count {
            return Ok(None);
        }
        if self.root_kind == JsonRootKind::Scalar {
            return Ok(Some(self.root_range.clone()));
        }
        let checkpoint = self
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.item_index <= item_index)
            .ok_or_else(|| invalid_json(self.root_range.start, "missing root checkpoint"))?;
        let source = FileSource::open(&self.path)?;
        let closing = match self.root_kind {
            JsonRootKind::Object => b'}',
            JsonRootKind::Array => b']',
            JsonRootKind::Scalar => {
                return Err(invalid_json(
                    self.root_range.start,
                    "scalar root unexpectedly entered container scan",
                ));
            }
        };
        scan_item_from_checkpoint(
            &source,
            checkpoint,
            item_index,
            self.root_range.end,
            closing,
        )
        .map(Some)
    }

    pub fn item_key_value_ranges(
        &self,
        item_index: u64,
    ) -> Result<Option<JsonItemKeyValueRanges>, PagedDocumentError> {
        let Some(range) = self.item_range(item_index)? else {
            return Ok(None);
        };
        if self.root_kind != JsonRootKind::Object {
            return Ok(Some((None, range)));
        }
        let source = FileSource::open(&self.path)?;
        let key = object_key_range(&source, range.clone())?;
        let Some(value_start) = object_value_start(&source, range.clone())? else {
            return Err(invalid_json(range.start, "object item is missing a value"));
        };
        Ok(Some((Some(key), value_start..range.end)))
    }

    pub fn child_index(
        &self,
        item_index: u64,
        options: JsonIndexOptions,
    ) -> Result<Option<Self>, PagedDocumentError> {
        self.child_index_cancellable(item_index, options, &SearchCancellation::default())
    }

    pub fn child_index_cancellable(
        &self,
        item_index: u64,
        options: JsonIndexOptions,
        cancellation: &SearchCancellation,
    ) -> Result<Option<Self>, PagedDocumentError> {
        let Some(mut range) = self.item_range(item_index)? else {
            return Ok(None);
        };
        let source = FileSource::open(&self.path)?;
        if self.root_kind == JsonRootKind::Object {
            let Some(value_start) = object_value_start(&source, range.clone())? else {
                return Ok(None);
            };
            range.start = value_start;
        }
        let mut cursor =
            ByteCursor::new_cancellable(source.clone(), range.start, range.end, cancellation);
        let Some((_, first)) = next_non_whitespace(&mut cursor)? else {
            return Ok(None);
        };
        if !matches!(first, b'{' | b'[') {
            return Ok(None);
        }
        Self::build_range_cancellable(&source, range, options, cancellation).map(Some)
    }
}

/// 按物理行校验 JSON Lines，不构造记录 DOM，也不保留正文。
/// 空的最终逻辑行来自文件末尾换行，不属于记录；其他空白行按无效记录报告全局字节位置。
pub fn validate_json_lines_cancellable(
    source: &FileSource,
    lines: &LineIndex,
    cancellation: &SearchCancellation,
) -> Result<(), PagedDocumentError> {
    validate_json_lines_from_cancellable(source, lines, 0, cancellation)
}

/// 校验从指定逻辑行开始的 JSONL 后缀；纯追加时从旧末行重验即可，成本只随新增字节增长。
pub fn validate_json_lines_from_cancellable(
    source: &FileSource,
    lines: &LineIndex,
    start_line: u64,
    cancellation: &SearchCancellation,
) -> Result<(), PagedDocumentError> {
    for line in start_line.min(lines.line_count())..lines.line_count() {
        if line.is_multiple_of(1_024) && cancellation.is_cancelled() {
            return Err(PagedDocumentError::Cancelled);
        }
        let Some(range) = lines.line_range(line) else {
            break;
        };
        if range.start == range.end && line + 1 == lines.line_count() {
            continue;
        }
        validate_json_range(source, range, cancellation)?;
    }
    Ok(())
}

fn encode_cached_root_kind(kind: JsonRootKind) -> u8 {
    match kind {
        JsonRootKind::Object => 1,
        JsonRootKind::Array => 2,
        JsonRootKind::Scalar => 3,
    }
}

fn decode_cached_root_kind(value: u8) -> Option<JsonRootKind> {
    match value {
        1 => Some(JsonRootKind::Object),
        2 => Some(JsonRootKind::Array),
        3 => Some(JsonRootKind::Scalar),
        _ => None,
    }
}

fn valid_cached_json_index(payload: &CachedJsonPayload, root_kind: JsonRootKind) -> bool {
    if payload.root_start >= payload.root_end || payload.root_end > payload.len {
        return false;
    }
    match root_kind {
        JsonRootKind::Scalar => {
            if payload.item_count != 1
                || payload.checkpoints.len() != 1
                || payload.checkpoints[0].item_index != 0
                || payload.checkpoints[0].byte_offset != payload.root_start
            {
                return false;
            }
        }
        JsonRootKind::Object | JsonRootKind::Array => {
            if payload.item_count == 0 {
                if !payload.checkpoints.is_empty() {
                    return false;
                }
            } else if payload
                .checkpoints
                .first()
                .is_none_or(|checkpoint| checkpoint.item_index != 0)
            {
                return false;
            }
        }
    }
    payload.checkpoints.iter().all(|checkpoint| {
        checkpoint.item_index < payload.item_count
            && checkpoint.byte_offset >= payload.root_start
            && checkpoint.byte_offset < payload.root_end
    }) && payload.checkpoints.windows(2).all(|pair| {
        pair[0].item_index < pair[1].item_index && pair[0].byte_offset < pair[1].byte_offset
    })
}
#[path = "json_parts/parser.rs"]
mod parser;
use parser::{
    ByteCursor, cleanup_json_sidecars, invalid_json, json_cache_data_error, json_sampled_hash,
    maybe_checkpoint, next_non_whitespace, object_key_range, object_value_start,
    scan_item_from_checkpoint, trim_range_end, validate_json_range,
};
#[cfg(test)]
#[path = "../tests/unit/json.rs"]
mod tests;
