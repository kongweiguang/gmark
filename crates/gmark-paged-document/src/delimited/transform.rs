// @author kongweiguang

use std::io::Write;

use csv::ByteRecord;

use super::model::{DelimitedEdit, DelimitedIndexOptions};
use super::source::{csv_error, decode_fields, reader, record_terminator};
use crate::{FileSource, PagedDocumentError, PieceDocument, SearchCancellation};

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
