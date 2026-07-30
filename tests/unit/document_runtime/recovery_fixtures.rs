// @author kongweiguang

//! Test-only codec fixtures for resident recovery compatibility checks.

use super::*;
use crc32fast::Hasher;
use gmark_document::{LineEnding, SourceFormatSnapshot};
use gmark_document_runtime::replay_resident_recovery_journal_with_metadata;
use gmark_recovery_codec::{RecordKind, encode_record_payload};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::UNIX_EPOCH;

impl RecoveryJournal {
    pub(crate) fn create_formatted(
        recovery_dir: &Path,
        file_path: Option<PathBuf>,
        source: String,
        source_format: SourceFormatSnapshot,
    ) -> anyhow::Result<Self> {
        let journal = ResidentRecoveryJournal::create_formatted(
            recovery_dir,
            file_path,
            source,
            source_format,
        )?;
        Ok(Self::from_runtime(journal))
    }

    pub(crate) fn path(&self) -> &Path {
        self.inner.path()
    }

    pub(crate) fn record(
        &mut self,
        source: &str,
        selection: RecoverySelection,
        view_mode: &str,
    ) -> anyhow::Result<bool> {
        let document = gmark_document::SourceDocument::new(source);
        self.record_formatted(source, document.source_format(), selection, view_mode)
    }

    pub(crate) fn checkpoint(
        &mut self,
        file_path: Option<PathBuf>,
        source: String,
    ) -> anyhow::Result<()> {
        self.inner.checkpoint(file_path, source)?;
        Ok(())
    }
}

pub(crate) fn replay_journal(path: &Path) -> anyhow::Result<RecoveredDocument> {
    let journal = replay_resident_recovery_journal_with_metadata(path)?;
    Ok(RecoveredDocument::from_runtime(
        journal.document,
        journal.anchor_affinity,
        journal.head_affinity,
    ))
}

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

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct BaseRecord {
    pub(super) document_id: String,
    pub(super) file_path: Option<String>,
    pub(super) fingerprint: Option<FileFingerprint>,
    pub(super) source: String,
    #[serde(default)]
    pub(super) source_format: Option<RecoverySourceFormat>,
    #[serde(default)]
    pub(super) selection: Option<RecoverySelection>,
    #[serde(default)]
    pub(super) view_mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct EditRecord {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) replacement: String,
    pub(super) selection: RecoverySelection,
    pub(super) view_mode: String,
    #[serde(default)]
    pub(super) format_patch: Option<RecoveryFormatPatch>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RecoveryLineEnding {
    Lf,
    CrLf,
    Cr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct RecoverySourceFormat {
    pub(super) utf8_bom: bool,
    pub(super) endings: Vec<RecoveryLineEnding>,
    pub(super) dominant: RecoveryLineEnding,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct RecoveryFormatPatch {
    pub(super) start: usize,
    pub(super) removed: usize,
    pub(super) inserted: Vec<RecoveryLineEnding>,
    pub(super) utf8_bom: bool,
    pub(super) dominant: RecoveryLineEnding,
}

pub(super) fn default_source_format(source: &str) -> SourceFormatSnapshot {
    SourceFormatSnapshot {
        utf8_bom: false,
        endings: vec![LineEnding::Lf; source.bytes().filter(|byte| *byte == b'\n').count()],
        dominant: LineEnding::Lf,
    }
}

pub(super) fn encode_record<T: Serialize>(
    kind: RecordKind,
    payload: &T,
) -> anyhow::Result<Vec<u8>> {
    let payload = serde_json::to_vec(payload)?;
    Ok(encode_record_payload(kind, &payload)?)
}

pub(super) fn append_record<T: Serialize>(
    path: &Path,
    kind: RecordKind,
    payload: &T,
) -> anyhow::Result<()> {
    let bytes = encode_record(kind, payload)?;
    let mut file = OpenOptions::new().create(false).append(true).open(path)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.sync_data()?;
    Ok(())
}

pub(super) fn selection(offset: usize) -> RecoverySelection {
    RecoverySelection {
        start: offset,
        end: offset,
        reversed: false,
        anchor_affinity: None,
        head_affinity: None,
    }
}

pub(super) fn crc_valid_frame_counts(path: &Path) -> (usize, usize) {
    let bytes = std::fs::read(path).expect("read compacted recovery journal");
    let mut cursor = 0usize;
    let mut bases = 0usize;
    let mut edits = 0usize;
    while cursor < bytes.len() {
        let record = gmark_recovery_codec::decode_record(&bytes, cursor)
            .expect("decode CRC-valid recovery frame")
            .expect("complete recovery frame");
        match record.kind {
            gmark_recovery_codec::RecordKind::Base => bases += 1,
            gmark_recovery_codec::RecordKind::Edit => edits += 1,
        }
        cursor = record.next;
    }
    (bases, edits)
}
