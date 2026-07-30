// @author kongweiguang

use std::ops::Range;
use std::sync::Arc;

use csv::ByteRecord;

use super::model::{
    DelimitedFilterOptions, DelimitedIndex, DelimitedIndexOptions, DelimitedRecord,
    RecordCheckpoint,
};
use super::source::{
    DelimitedSource, decode_fields, decode_fields_in_range, extend_synthetic_headers,
    normalized_record_range,
};
use crate::{FileSource, PagedDocumentError, SearchCancellation};

impl DelimitedIndex {
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

    pub(super) fn build_from_source(
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
        columns: Range<usize>,
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
        columns: Range<usize>,
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
