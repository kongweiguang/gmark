// @author kongweiguang

use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::Path;

use gmark_recovery_codec::{HEADER_LEN, RecordKind, decode_header, encode_record_payload};
use serde::Serialize;

use super::{PagedRecoveryBase, RECOVERY_CHUNK_BYTES, SAMPLE_BYTES};
use crate::{FileSource, PagedDocumentError};

pub(super) enum FrameRead {
    End,
    Frame(RecordKind, Vec<u8>),
    Truncated,
}

pub(super) fn read_frame(reader: &mut BufReader<File>) -> Result<FrameRead, PagedDocumentError> {
    let mut header = [0u8; HEADER_LEN];
    let mut read = 0usize;
    while read < header.len() {
        let count = reader
            .read(&mut header[read..])
            .map_err(|error| PagedDocumentError::Recovery(error.to_string()))?;
        if count == 0 {
            return if read == 0 {
                Ok(FrameRead::End)
            } else {
                Ok(FrameRead::Truncated)
            };
        }
        read += count;
    }
    let Some(decoded) =
        decode_header(&header).map_err(|error| PagedDocumentError::Recovery(error.to_string()))?
    else {
        return Ok(FrameRead::Truncated);
    };
    let mut payload = vec![0u8; decoded.payload_len];
    if let Err(error) = reader.read_exact(&mut payload) {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            return Ok(FrameRead::Truncated);
        }
        return Err(PagedDocumentError::Recovery(error.to_string()));
    }
    if crc32fast::hash(&payload) != decoded.expected_crc {
        return Ok(FrameRead::Truncated);
    }
    Ok(FrameRead::Frame(decoded.kind, payload))
}

pub(super) fn verify_base(
    source: &FileSource,
    base: &PagedRecoveryBase,
) -> Result<(), PagedDocumentError> {
    let identity = source.identity()?;
    if identity.len != base.len
        || identity.modified_nanos != base.modified_nanos
        || sampled_hash(source, identity.len)? != base.sampled_hash
    {
        return Err(PagedDocumentError::SourceChanged);
    }
    Ok(())
}

pub(super) fn sampled_hash(source: &FileSource, len: u64) -> Result<u32, PagedDocumentError> {
    let mut hasher = crc32fast::Hasher::new();
    for start in [
        0,
        len.saturating_sub(SAMPLE_BYTES) / 2,
        len.saturating_sub(SAMPLE_BYTES),
    ] {
        let end = (start + SAMPLE_BYTES).min(len);
        if start < end {
            hasher.update(&source.read_range(start, end)?);
        }
    }
    Ok(hasher.finalize())
}

pub(super) fn utf8_chunks(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return vec![""];
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < text.len() {
        let mut end = (start + RECOVERY_CHUNK_BYTES).min(text.len());
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            end = text[start..]
                .char_indices()
                .nth(1)
                .map_or(text.len(), |(offset, _)| start + offset);
        }
        chunks.push(&text[start..end]);
        start = end;
    }
    chunks
}

pub(super) fn encode_json_record(
    kind: RecordKind,
    value: &impl Serialize,
) -> Result<Vec<u8>, PagedDocumentError> {
    let payload = serde_json::to_vec(value)
        .map_err(|error| PagedDocumentError::Recovery(error.to_string()))?;
    encode_record_payload(kind, &payload)
        .map_err(|error| PagedDocumentError::Recovery(error.to_string()))
}

pub(super) fn append_frames(path: &Path, frames: &[Vec<u8>]) -> Result<(), PagedDocumentError> {
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|source| PagedDocumentError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    for frame in frames {
        file.write_all(frame)
            .map_err(|source| PagedDocumentError::Io {
                path: path.to_path_buf(),
                source,
            })?;
    }
    file.sync_data().map_err(|source| PagedDocumentError::Io {
        path: path.to_path_buf(),
        source,
    })
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), PagedDocumentError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|source| PagedDocumentError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    temporary
        .write_all(bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|source| PagedDocumentError::Io {
            path: temporary.path().to_path_buf(),
            source,
        })?;
    let persisted = temporary
        .persist(path)
        .map_err(|error| PagedDocumentError::Persist {
            path: path.to_path_buf(),
            source: error.error,
        })?;
    persisted
        .sync_all()
        .map_err(|source| PagedDocumentError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    crate::source::sync_parent_directory(parent)?;
    Ok(())
}

pub(super) fn monotonic_timestamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}
