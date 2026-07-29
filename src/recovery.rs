// @author kongweiguang

//! Compatibility adapter for resident crash recovery.
//!
//! Durable journal framing, schema evolution, source-format patches, and replay
//! live in `gmark-document-runtime`. This module preserves the root crate's
//! historical Editor-facing shapes while delegating all durable behavior to that
//! runtime API.

use std::path::{Path, PathBuf};

#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::time::UNIX_EPOCH;

#[cfg(test)]
use crc32fast::Hasher;
use gmark_document::SourceFormatSnapshot;
use gmark_document_core::{
    PersistenceError, RecoveryBackend, RecoveryRecord, SourceAffinity, SourceAnchor,
    SourceSelection,
};
#[cfg(test)]
use gmark_document_runtime::replay_resident_recovery_journal_with_metadata;
use gmark_document_runtime::{
    RecoveredResidentDocument, ResidentFileFingerprint, ResidentRecoveryJournal,
    ResidentRecoveryReadStatus, fingerprint_resident_file, load_resident_recovery_journals,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RecoverySelection {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) reversed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) anchor_affinity: Option<RecoverySelectionAffinity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) head_affinity: Option<RecoverySelectionAffinity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecoverySelectionAffinity {
    Before,
    After,
}

impl RecoverySelection {
    pub(crate) fn from_source_selection(selection: SourceSelection) -> Self {
        let range = selection.range();
        Self {
            start: range.start.min(usize::MAX as u64) as usize,
            end: range.end.min(usize::MAX as u64) as usize,
            reversed: selection.reversed(),
            anchor_affinity: Some(selection.anchor.affinity.into()),
            head_affinity: Some(selection.head.affinity.into()),
        }
    }

    pub(crate) fn source_selection(&self) -> SourceSelection {
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

impl From<SourceAffinity> for RecoverySelectionAffinity {
    fn from(value: SourceAffinity) -> Self {
        match value {
            SourceAffinity::Before => Self::Before,
            SourceAffinity::After => Self::After,
        }
    }
}

impl From<RecoverySelectionAffinity> for SourceAffinity {
    fn from(value: RecoverySelectionAffinity) -> Self {
        match value {
            RecoverySelectionAffinity::Before => Self::Before,
            RecoverySelectionAffinity::After => Self::After,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryReadStatus {
    Complete,
    TruncatedTail,
}

#[derive(Clone, Debug)]
pub(crate) struct RecoveredDocument {
    pub(crate) document_id: String,
    pub(crate) journal_path: PathBuf,
    pub(crate) file_path: Option<PathBuf>,
    pub(crate) source: String,
    pub(crate) source_format: SourceFormatSnapshot,
    pub(crate) selection: RecoverySelection,
    pub(crate) view_mode: String,
    pub(crate) read_status: RecoveryReadStatus,
    pub(crate) base_file_changed: bool,
    base_fingerprint: Option<FileFingerprint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FileFingerprint {
    path: String,
    size: u64,
    modified_nanos: Option<u128>,
    crc32: u32,
}

/// Root-crate compatibility wrapper around the Wave 1 resident runtime journal.
pub(crate) struct RecoveryJournal {
    inner: ResidentRecoveryJournal,
    #[cfg(test)]
    journal_path: PathBuf,
}

impl RecoveryJournal {
    pub(crate) fn create(
        recovery_dir: &Path,
        file_path: Option<PathBuf>,
        source: String,
    ) -> anyhow::Result<Self> {
        Ok(Self::from_runtime(ResidentRecoveryJournal::create(
            recovery_dir,
            file_path,
            source,
        )?))
    }

    #[cfg(test)]
    pub(crate) fn create_formatted(
        recovery_dir: &Path,
        file_path: Option<PathBuf>,
        source: String,
        source_format: SourceFormatSnapshot,
    ) -> anyhow::Result<Self> {
        Ok(Self::from_runtime(
            ResidentRecoveryJournal::create_formatted(
                recovery_dir,
                file_path,
                source,
                source_format,
            )?,
        ))
    }

    pub(crate) fn resume(document: &RecoveredDocument) -> Self {
        let recovered = document.as_runtime();
        Self {
            inner: ResidentRecoveryJournal::resume(&recovered),
            #[cfg(test)]
            journal_path: document.journal_path.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn path(&self) -> &Path {
        &self.journal_path
    }

    #[cfg(test)]
    pub(crate) fn record(
        &mut self,
        source: &str,
        selection: RecoverySelection,
        view_mode: &str,
    ) -> anyhow::Result<bool> {
        let document = gmark_document::SourceDocument::new(source);
        self.record_formatted(source, document.source_format(), selection, view_mode)
    }

    pub(crate) fn record_formatted(
        &mut self,
        source: &str,
        source_format: SourceFormatSnapshot,
        selection: RecoverySelection,
        view_mode: &str,
    ) -> anyhow::Result<bool> {
        Ok(self.inner.record_formatted_with_affinities(
            source,
            source_format,
            selection.source_selection(),
            selection.anchor_affinity.map(Into::into),
            selection.head_affinity.map(Into::into),
            view_mode,
        )?)
    }

    #[cfg(test)]
    pub(crate) fn checkpoint(
        &mut self,
        file_path: Option<PathBuf>,
        source: String,
    ) -> anyhow::Result<()> {
        self.inner.checkpoint(file_path, source)?;
        Ok(())
    }

    pub(crate) fn checkpoint_formatted(
        &mut self,
        file_path: Option<PathBuf>,
        source: String,
        source_format: SourceFormatSnapshot,
    ) -> anyhow::Result<()> {
        self.inner
            .checkpoint_formatted(file_path, source, source_format)?;
        Ok(())
    }

    fn from_runtime(inner: ResidentRecoveryJournal) -> Self {
        Self {
            #[cfg(test)]
            journal_path: inner.path().to_path_buf(),
            inner,
        }
    }
}

impl RecoveryBackend for RecoveryJournal {
    fn record(&mut self, record: &RecoveryRecord) -> Result<(), PersistenceError> {
        RecoveryBackend::record(&mut self.inner, record)
    }
}

impl RecoveredDocument {
    fn from_runtime(
        document: RecoveredResidentDocument,
        anchor_affinity: Option<SourceAffinity>,
        head_affinity: Option<SourceAffinity>,
    ) -> Self {
        let mut selection = RecoverySelection::from_source_selection(document.selection);
        selection.anchor_affinity = anchor_affinity.map(Into::into);
        selection.head_affinity = head_affinity.map(Into::into);
        Self {
            document_id: document.document_id,
            journal_path: document.journal_path,
            file_path: document.file_path,
            source: document.source,
            source_format: document.source_format,
            selection,
            view_mode: document.view_mode,
            read_status: match document.read_status {
                ResidentRecoveryReadStatus::Complete => RecoveryReadStatus::Complete,
                ResidentRecoveryReadStatus::TruncatedTail => RecoveryReadStatus::TruncatedTail,
            },
            base_file_changed: document.base_file_changed,
            base_fingerprint: document.base_fingerprint.map(FileFingerprint::from),
        }
    }

    fn as_runtime(&self) -> RecoveredResidentDocument {
        RecoveredResidentDocument {
            document_id: self.document_id.clone(),
            journal_path: self.journal_path.clone(),
            file_path: self.file_path.clone(),
            source: self.source.clone(),
            source_format: self.source_format.clone(),
            selection: self.selection.source_selection(),
            view_mode: self.view_mode.clone(),
            read_status: match self.read_status {
                RecoveryReadStatus::Complete => ResidentRecoveryReadStatus::Complete,
                RecoveryReadStatus::TruncatedTail => ResidentRecoveryReadStatus::TruncatedTail,
            },
            base_file_changed: self.base_file_changed,
            base_fingerprint: self.base_fingerprint.clone().map(Into::into),
        }
    }
}

impl From<ResidentFileFingerprint> for FileFingerprint {
    fn from(value: ResidentFileFingerprint) -> Self {
        Self {
            path: value.path,
            size: value.size,
            modified_nanos: value.modified_nanos,
            crc32: value.crc32,
        }
    }
}

impl From<FileFingerprint> for ResidentFileFingerprint {
    fn from(value: FileFingerprint) -> Self {
        Self {
            path: value.path,
            size: value.size,
            modified_nanos: value.modified_nanos,
            crc32: value.crc32,
        }
    }
}

pub(crate) fn load_recovery_documents(
    recovery_dir: &Path,
) -> anyhow::Result<Vec<RecoveredDocument>> {
    Ok(load_resident_recovery_journals(recovery_dir)?
        .into_iter()
        .map(|journal| {
            RecoveredDocument::from_runtime(
                journal.document,
                journal.anchor_affinity,
                journal.head_affinity,
            )
        })
        .collect())
}

#[cfg(test)]
pub(crate) fn replay_journal(path: &Path) -> anyhow::Result<RecoveredDocument> {
    let journal = replay_resident_recovery_journal_with_metadata(path)?;
    Ok(RecoveredDocument::from_runtime(
        journal.document,
        journal.anchor_affinity,
        journal.head_affinity,
    ))
}

pub(crate) fn fingerprint_file(path: &Path) -> anyhow::Result<FileFingerprint> {
    Ok(fingerprint_resident_file(path)?.into())
}

#[cfg(test)]
pub(crate) fn fingerprint_contents(
    path: &Path,
    contents: &[u8],
) -> anyhow::Result<FileFingerprint> {
    let metadata = fs::metadata(path)?;
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    let mut hasher = Hasher::new();
    hasher.update(contents);
    Ok(FileFingerprint {
        path: path.to_string_lossy().into_owned(),
        size: metadata.len(),
        modified_nanos,
        crc32: hasher.finalize(),
    })
}

#[cfg(test)]
use gmark_document::LineEnding;
#[cfg(test)]
use gmark_recovery_codec::{RecordKind, encode_record_payload};
#[cfg(test)]
use std::fs::OpenOptions;
#[cfg(test)]
use std::io::Write;

#[cfg(test)]
#[derive(Debug, Serialize, Deserialize)]
struct BaseRecord {
    document_id: String,
    file_path: Option<String>,
    fingerprint: Option<FileFingerprint>,
    source: String,
    #[serde(default)]
    source_format: Option<RecoverySourceFormat>,
    #[serde(default)]
    selection: Option<RecoverySelection>,
    #[serde(default)]
    view_mode: Option<String>,
}

#[cfg(test)]
#[derive(Debug, Serialize, Deserialize)]
struct EditRecord {
    start: usize,
    end: usize,
    replacement: String,
    selection: RecoverySelection,
    view_mode: String,
    #[serde(default)]
    format_patch: Option<RecoveryFormatPatch>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RecoveryLineEnding {
    Lf,
    CrLf,
    Cr,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct RecoverySourceFormat {
    utf8_bom: bool,
    endings: Vec<RecoveryLineEnding>,
    dominant: RecoveryLineEnding,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct RecoveryFormatPatch {
    start: usize,
    removed: usize,
    inserted: Vec<RecoveryLineEnding>,
    utf8_bom: bool,
    dominant: RecoveryLineEnding,
}

#[cfg(test)]
fn default_source_format(source: &str) -> SourceFormatSnapshot {
    SourceFormatSnapshot {
        utf8_bom: false,
        endings: vec![LineEnding::Lf; source.bytes().filter(|byte| *byte == b'\n').count()],
        dominant: LineEnding::Lf,
    }
}

#[cfg(test)]
fn encode_record<T: Serialize>(kind: RecordKind, payload: &T) -> anyhow::Result<Vec<u8>> {
    let payload = serde_json::to_vec(payload)?;
    Ok(encode_record_payload(kind, &payload)?)
}

#[cfg(test)]
fn append_record<T: Serialize>(path: &Path, kind: RecordKind, payload: &T) -> anyhow::Result<()> {
    let bytes = encode_record(kind, payload)?;
    let mut file = OpenOptions::new().create(false).append(true).open(path)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.sync_data()?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/recovery.rs"]
mod tests;
