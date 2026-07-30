// @author kongweiguang

use csv::Position;
use serde::{Deserialize, Serialize};

use super::source::DelimitedSource;

pub(super) const DELIMITED_SIDECAR_VERSION: u32 = 2;
pub(super) const DELIMITED_SIDECAR_SAMPLE_BYTES: u64 = 64 * 1024;
pub(super) const MAX_DELIMITED_SIDECAR_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const DELIMITED_CACHE_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
pub(super) struct CachedDelimitedPayload {
    pub(super) version: u32,
    pub(super) len: u64,
    pub(super) modified_nanos: Option<u128>,
    pub(super) sampled_hash: u32,
    pub(super) delimiter: u8,
    pub(super) has_headers: bool,
    pub(super) checkpoint_records: u64,
    pub(super) checkpoint_bytes: u64,
    pub(super) physical_records: u64,
    pub(super) max_fields: usize,
    pub(super) checkpoints: Vec<CachedRecordCheckpoint>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct CachedDelimitedEnvelope {
    pub(super) payload: CachedDelimitedPayload,
    pub(super) checksum: u32,
}

#[derive(Serialize, Deserialize)]
pub(super) struct CachedRecordCheckpoint {
    pub(super) physical_record: u64,
    pub(super) byte: u64,
    pub(super) line: u64,
    pub(super) record: u64,
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
pub(super) struct RecordCheckpoint {
    pub(super) physical_record: u64,
    pub(super) position: Position,
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
    pub(super) source: DelimitedSource,
    pub(super) options: DelimitedIndexOptions,
    pub(super) headers: Vec<String>,
    pub(super) physical_records: u64,
    pub(super) max_fields: usize,
    pub(super) checkpoints: Vec<RecordCheckpoint>,
}
