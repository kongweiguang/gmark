// @author kongweiguang

//! Bounded frame codec for gmark crash-recovery journals.

#![forbid(unsafe_code)]

use crc32fast::Hasher;
use thiserror::Error;

const MAGIC: &[u8; 4] = b"GMRJ";
const FORMAT_VERSION: u16 = 1;
pub const HEADER_LEN: usize = 20;
pub const MAX_RECORD_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RecordKind {
    Base = 1,
    Edit = 2,
}

impl RecordKind {
    fn from_byte(value: u8) -> Result<Self, CodecError> {
        match value {
            1 => Ok(Self::Base),
            2 => Ok(Self::Edit),
            _ => Err(CodecError::UnknownRecordKind(value)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedRecord<'a> {
    pub kind: RecordKind,
    pub payload: &'a [u8],
    pub next: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedHeader {
    pub kind: RecordKind,
    pub payload_len: usize,
    pub expected_crc: u32,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CodecError {
    #[error("unsupported recovery journal version {0}")]
    UnsupportedVersion(u16),
    #[error("unknown recovery record kind {0}")]
    UnknownRecordKind(u8),
    #[error("unsupported recovery record flags 0x{0:02x}")]
    UnsupportedFlags(u8),
    #[error("recovery payload length cannot be represented on this platform")]
    PayloadLengthOverflow,
    #[error("recovery payload exceeds {MAX_RECORD_BYTES} bytes")]
    PayloadTooLarge,
}

pub fn encode_record_payload(kind: RecordKind, payload: &[u8]) -> Result<Vec<u8>, CodecError> {
    if payload.len() > MAX_RECORD_BYTES {
        return Err(CodecError::PayloadTooLarge);
    }
    let payload_len =
        u64::try_from(payload.len()).map_err(|_| CodecError::PayloadLengthOverflow)?;
    let mut hasher = Hasher::new();
    hasher.update(payload);
    let crc = hasher.finalize();
    let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.push(kind as u8);
    bytes.push(0);
    bytes.extend_from_slice(&payload_len.to_le_bytes());
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

/// 只解析固定头，供调用方以有界缓冲流式读取大型恢复日志。
pub fn decode_header(header: &[u8]) -> Result<Option<DecodedHeader>, CodecError> {
    let Some(header) = header.get(..HEADER_LEN) else {
        return Ok(None);
    };
    if &header[..4] != MAGIC {
        return Ok(None);
    }
    let version = u16::from_le_bytes([header[4], header[5]]);
    if version != FORMAT_VERSION {
        return Err(CodecError::UnsupportedVersion(version));
    }
    let kind = RecordKind::from_byte(header[6])?;
    if header[7] != 0 {
        return Err(CodecError::UnsupportedFlags(header[7]));
    }
    let payload_len_u64 = u64::from_le_bytes([
        header[8], header[9], header[10], header[11], header[12], header[13], header[14],
        header[15],
    ]);
    let payload_len =
        usize::try_from(payload_len_u64).map_err(|_| CodecError::PayloadLengthOverflow)?;
    if payload_len > MAX_RECORD_BYTES {
        return Err(CodecError::PayloadTooLarge);
    }
    let expected_crc = u32::from_le_bytes([header[16], header[17], header[18], header[19]]);
    Ok(Some(DecodedHeader {
        kind,
        payload_len,
        expected_crc,
    }))
}

/// Decodes one frame at `cursor`.
///
/// `Ok(None)` means an incomplete, bad-magic, or CRC-invalid tail. Callers may retain the last fully
/// verified prefix. Unsupported versions, kinds, flags, and declared sizes are hard format errors.
pub fn decode_record(bytes: &[u8], cursor: usize) -> Result<Option<DecodedRecord<'_>>, CodecError> {
    let Some(header_end) = cursor.checked_add(HEADER_LEN) else {
        return Ok(None);
    };
    let Some(header) = bytes.get(cursor..header_end) else {
        return Ok(None);
    };
    let Some(decoded_header) = decode_header(header)? else {
        return Ok(None);
    };
    let payload_len = decoded_header.payload_len;
    let Some(payload_end) = header_end.checked_add(payload_len) else {
        return Ok(None);
    };
    let Some(payload) = bytes.get(header_end..payload_end) else {
        return Ok(None);
    };
    let mut hasher = Hasher::new();
    hasher.update(payload);
    if hasher.finalize() != decoded_header.expected_crc {
        return Ok(None);
    }
    Ok(Some(DecodedRecord {
        kind: decoded_header.kind,
        payload,
        next: payload_end,
    }))
}

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
