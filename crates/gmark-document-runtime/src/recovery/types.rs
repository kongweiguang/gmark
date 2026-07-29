// @author kongweiguang

use std::io;
use std::path::{Path, PathBuf};

use gmark_document::{AtomicWriteError, LineEnding, SourceFormatSnapshot};
use gmark_document_core::{SourceAffinity, SourceAnchor, SourceSelection};
use gmark_recovery_codec::CodecError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Resident recovery 与实时源码使用相同的 selection/affinity 真值。
pub type ResidentRecoverySelection = SourceSelection;

/// 成功重放的 journal 是否丢弃了不完整或损坏的末尾帧。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResidentRecoveryReadStatus {
    Complete,
    TruncatedTail,
}

/// 创建 journal 时捕获的磁盘基线指纹。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResidentFileFingerprint {
    pub path: String,
    pub size: u64,
    pub modified_nanos: Option<u128>,
    pub crc32: u32,
}

/// 已从 resident journal 重放的文档状态。
#[derive(Clone, Debug)]
pub struct RecoveredResidentDocument {
    pub document_id: String,
    pub journal_path: PathBuf,
    pub file_path: Option<PathBuf>,
    pub source: String,
    pub source_format: SourceFormatSnapshot,
    pub selection: ResidentRecoverySelection,
    pub view_mode: String,
    pub read_status: ResidentRecoveryReadStatus,
    /// `true` 表示原始磁盘文件与首次写入 journal 时的基线不再相同。
    pub base_file_changed: bool,
    pub base_fingerprint: Option<ResidentFileFingerprint>,
}

/// One completed resident-journal scan, including legacy selection-affinity
/// presence that cannot be recovered from the normalized selection alone.
#[derive(Clone, Debug)]
pub struct RecoveredResidentJournal {
    pub document: RecoveredResidentDocument,
    /// `None` means the corresponding affinity field was absent on disk.
    pub anchor_affinity: Option<SourceAffinity>,
    /// `None` means the corresponding affinity field was absent on disk.
    pub head_affinity: Option<SourceAffinity>,
}

/// Resident recovery 的文件、帧和 schema 错误。
#[derive(Debug, Error)]
pub enum ResidentRecoveryError {
    #[error("failed to {operation} resident recovery path '{path}': {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to atomically write resident recovery journal '{path}': {source}")]
    AtomicWrite {
        path: PathBuf,
        #[source]
        source: AtomicWriteError,
    },
    #[error("resident recovery frame codec failed: {0}")]
    Codec(#[from] CodecError),
    #[error("{context}: {source}")]
    Json {
        context: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "resident recovery source format has {ending_count} endings for {newline_count} newlines"
    )]
    InvalidSourceFormat {
        ending_count: usize,
        newline_count: usize,
    },
    #[error("resident recovery journal format error: {0}")]
    JournalFormat(String),
}

impl ResidentRecoveryError {
    pub(super) fn io(operation: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.to_path_buf(),
            source,
        }
    }

    pub(super) fn json(context: &'static str, source: serde_json::Error) -> Self {
        Self::Json { context, source }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct StoredSelection {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) reversed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) anchor_affinity: Option<StoredSelectionAffinity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) head_affinity: Option<StoredSelectionAffinity>,
}

impl StoredSelection {
    pub(super) fn from_source_selection_with_affinities(
        selection: SourceSelection,
        anchor_affinity: Option<SourceAffinity>,
        head_affinity: Option<SourceAffinity>,
    ) -> Self {
        let range = selection.range();
        Self {
            start: range.start.min(usize::MAX as u64) as usize,
            end: range.end.min(usize::MAX as u64) as usize,
            reversed: selection.reversed(),
            anchor_affinity: anchor_affinity.map(Into::into),
            head_affinity: head_affinity.map(Into::into),
        }
    }

    pub(super) fn source_selection(&self) -> SourceSelection {
        let start = self.start.min(self.end);
        let end = self.start.max(self.end);
        let fallback = SourceSelection::from_range(start as u64..end as u64, self.reversed);
        SourceSelection {
            anchor: SourceAnchor::new(
                fallback.anchor.byte_offset,
                self.anchor_affinity
                    .map(Into::into)
                    .unwrap_or(fallback.anchor.affinity),
            ),
            head: SourceAnchor::new(
                fallback.head.byte_offset,
                self.head_affinity
                    .map(Into::into)
                    .unwrap_or(fallback.head.affinity),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StoredSelectionAffinity {
    Before,
    After,
}

impl From<SourceAffinity> for StoredSelectionAffinity {
    fn from(value: SourceAffinity) -> Self {
        match value {
            SourceAffinity::Before => Self::Before,
            SourceAffinity::After => Self::After,
        }
    }
}

impl From<StoredSelectionAffinity> for SourceAffinity {
    fn from(value: StoredSelectionAffinity) -> Self {
        match value {
            StoredSelectionAffinity::Before => Self::Before,
            StoredSelectionAffinity::After => Self::After,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StoredBaseRecord {
    pub(super) document_id: String,
    pub(super) file_path: Option<String>,
    pub(super) fingerprint: Option<ResidentFileFingerprint>,
    pub(super) source: String,
    #[serde(default)]
    pub(super) source_format: Option<StoredSourceFormat>,
    #[serde(default)]
    pub(super) selection: Option<StoredSelection>,
    #[serde(default)]
    pub(super) view_mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StoredEditRecord {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) replacement: String,
    pub(super) selection: StoredSelection,
    pub(super) view_mode: String,
    #[serde(default)]
    pub(super) format_patch: Option<StoredFormatPatch>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StoredLineEnding {
    Lf,
    CrLf,
    Cr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct StoredSourceFormat {
    pub(super) utf8_bom: bool,
    pub(super) endings: Vec<StoredLineEnding>,
    pub(super) dominant: StoredLineEnding,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct StoredFormatPatch {
    pub(super) start: usize,
    pub(super) removed: usize,
    pub(super) inserted: Vec<StoredLineEnding>,
    pub(super) utf8_bom: bool,
    pub(super) dominant: StoredLineEnding,
}

impl From<LineEnding> for StoredLineEnding {
    fn from(value: LineEnding) -> Self {
        match value {
            LineEnding::Lf => Self::Lf,
            LineEnding::CrLf => Self::CrLf,
            LineEnding::Cr => Self::Cr,
        }
    }
}

impl From<StoredLineEnding> for LineEnding {
    fn from(value: StoredLineEnding) -> Self {
        match value {
            StoredLineEnding::Lf => Self::Lf,
            StoredLineEnding::CrLf => Self::CrLf,
            StoredLineEnding::Cr => Self::Cr,
        }
    }
}

impl From<&SourceFormatSnapshot> for StoredSourceFormat {
    fn from(value: &SourceFormatSnapshot) -> Self {
        Self {
            utf8_bom: value.utf8_bom,
            endings: value.endings.iter().copied().map(Into::into).collect(),
            dominant: value.dominant.into(),
        }
    }
}

impl From<StoredSourceFormat> for SourceFormatSnapshot {
    fn from(value: StoredSourceFormat) -> Self {
        Self {
            utf8_bom: value.utf8_bom,
            endings: value.endings.into_iter().map(Into::into).collect(),
            dominant: value.dominant.into(),
        }
    }
}
