// @author kongweiguang

use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;

use csv::{ByteRecord, Position, ReaderBuilder};

use super::model::DelimitedIndexOptions;
use crate::{FileSource, PagedDocumentError};

#[derive(Clone, Debug)]
pub(super) enum DelimitedSource {
    File(PathBuf),
    Snapshot(Arc<[u8]>),
}

impl DelimitedSource {
    pub(super) fn reader(
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

    pub(super) fn len(&self) -> Result<u64, PagedDocumentError> {
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

    pub(super) fn read_range(&self, start: u64, end: u64) -> Result<Vec<u8>, PagedDocumentError> {
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

    pub(super) fn csv_error(&self, error: csv::Error) -> PagedDocumentError {
        let offset = error.position().map_or(0, Position::byte);
        PagedDocumentError::InvalidDelimited {
            offset,
            message: format!("{} ({})", error, self.display_path().display()),
        }
    }
}

pub(super) enum DelimitedReader {
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

pub(super) fn reader(
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

pub(super) fn csv_error(source: &FileSource, error: csv::Error) -> PagedDocumentError {
    let offset = error.position().map_or(0, Position::byte);
    PagedDocumentError::InvalidDelimited {
        offset,
        message: format!("{} ({})", error, source.path().display()),
    }
}

pub(super) fn decode_fields(record: &ByteRecord) -> Vec<String> {
    record
        .iter()
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect()
}

pub(super) fn extend_synthetic_headers(headers: &mut Vec<String>, max_fields: usize) {
    while headers.len() < max_fields {
        headers.push(format!("Column {}", headers.len() + 1));
    }
}

pub(super) fn record_terminator(bytes: &[u8]) -> &'static str {
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

pub(super) fn normalized_record_range(
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

pub(super) fn decode_fields_in_range(
    record: &ByteRecord,
    columns: std::ops::Range<usize>,
) -> Vec<String> {
    let count = columns.end.saturating_sub(columns.start);
    record
        .iter()
        .skip(columns.start)
        .take(count)
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect()
}
