// @author kongweiguang

use std::fs;
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use gmark_recovery_codec::{RecordKind, encode_record_payload};

use super::frame::{
    append_frames, atomic_write, encode_json_record, monotonic_timestamp, sampled_hash, utf8_chunks,
};
use super::records::{BaseRecord, EditRecord, RecoveryEncoding};
use super::{MAX_RECOVERY_CHUNKS_PER_REPLACE, PagedRecoveryJournal, PagedRecoverySelection};
use crate::{FileSource, PagedDocumentError};

static NEXT_JOURNAL_ID: AtomicU64 = AtomicU64::new(1);

impl PagedRecoveryJournal {
    pub fn create(
        recovery_dir: impl AsRef<Path>,
        source: &FileSource,
        encoding: gmark_document_core::TextEncoding,
    ) -> Result<Self, PagedDocumentError> {
        let recovery_dir = recovery_dir.as_ref();
        fs::create_dir_all(recovery_dir).map_err(|source| PagedDocumentError::Io {
            path: recovery_dir.to_path_buf(),
            source,
        })?;
        let identity = source.identity()?;
        let base = BaseRecord {
            path: identity.path,
            len: identity.len,
            modified_nanos: identity.modified_nanos,
            sampled_hash: sampled_hash(source, identity.len)?,
            encoding: RecoveryEncoding::from_encoding(&encoding),
        };
        let payload = serde_json::to_vec(&base)
            .map_err(|error| PagedDocumentError::Recovery(error.to_string()))?;
        let frame = encode_record_payload(RecordKind::Base, &payload)
            .map_err(|error| PagedDocumentError::Recovery(error.to_string()))?;
        let id = NEXT_JOURNAL_ID.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            "large-{}-{}-{id}.large-journal",
            std::process::id(),
            monotonic_timestamp()
        );
        let path = recovery_dir.join(name);
        atomic_write(&path, &frame)?;
        Ok(Self {
            path,
            next_transaction: 1,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record_replace(
        &mut self,
        range: Range<u64>,
        replacement: &str,
        selection: Option<PagedRecoverySelection>,
        view_mode: &str,
    ) -> Result<(), PagedDocumentError> {
        if range.start > range.end {
            return Err(PagedDocumentError::InvalidRange {
                start: range.start,
                end: range.end,
                len: range.end,
            });
        }
        let chunks = utf8_chunks(replacement);
        let chunk_count = u32::try_from(chunks.len())
            .map_err(|_| PagedDocumentError::Recovery("too many recovery chunks".to_owned()))?;
        if chunk_count > MAX_RECOVERY_CHUNKS_PER_REPLACE {
            return Err(PagedDocumentError::Recovery(
                "recovery replacement exceeds the chunk limit".to_owned(),
            ));
        }
        let transaction = self.next_transaction;
        self.next_transaction = self.next_transaction.wrapping_add(1).max(1);
        let mut frames = Vec::new();
        for (chunk_index, text) in chunks.into_iter().enumerate() {
            let record = EditRecord::ReplaceChunk {
                transaction,
                start: range.start,
                end: range.end,
                chunk_index: chunk_index as u32,
                chunk_count,
                text: text.to_owned(),
                selection: selection.map(Into::into),
                view_mode: view_mode.to_owned(),
            };
            frames.push(encode_json_record(RecordKind::Edit, &record)?);
        }
        append_frames(&self.path, &frames)
    }

    pub fn record_undo(
        &self,
        selection: Option<PagedRecoverySelection>,
        view_mode: &str,
    ) -> Result<(), PagedDocumentError> {
        self.append_command(EditRecord::Undo {
            selection: selection.map(Into::into),
            view_mode: view_mode.to_owned(),
        })
    }

    pub fn record_redo(
        &self,
        selection: Option<PagedRecoverySelection>,
        view_mode: &str,
    ) -> Result<(), PagedDocumentError> {
        self.append_command(EditRecord::Redo {
            selection: selection.map(Into::into),
            view_mode: view_mode.to_owned(),
        })
    }

    pub fn checkpoint(&self) -> Result<(), PagedDocumentError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(PagedDocumentError::Io {
                path: self.path.clone(),
                source,
            }),
        }
    }

    fn append_command(&self, command: EditRecord) -> Result<(), PagedDocumentError> {
        append_frames(
            &self.path,
            &[encode_json_record(RecordKind::Edit, &command)?],
        )
    }
}

impl gmark_document_core::RecoveryBackend for PagedRecoveryJournal {
    fn record(
        &mut self,
        record: &gmark_document_core::RecoveryRecord,
    ) -> Result<(), gmark_document_core::PersistenceError> {
        let view = record.view_id.as_str();
        match &record.action {
            gmark_document_core::RecoveryAction::Transaction(transaction) => {
                if transaction.edits.len() != 1 {
                    return Err(gmark_document_core::PersistenceError::Recovery(
                        "Paged recovery requires one source edit per transaction".into(),
                    ));
                }
                let edit = &transaction.edits[0];
                self.record_replace(
                    edit.range.clone(),
                    &edit.replacement,
                    record.selection,
                    view,
                )
                .map_err(|error| gmark_document_core::PersistenceError::Recovery(error.to_string()))
            }
            gmark_document_core::RecoveryAction::Undo => {
                self.record_undo(record.selection, view).map_err(|error| {
                    gmark_document_core::PersistenceError::Recovery(error.to_string())
                })
            }
            gmark_document_core::RecoveryAction::Redo => {
                self.record_redo(record.selection, view).map_err(|error| {
                    gmark_document_core::PersistenceError::Recovery(error.to_string())
                })
            }
        }
    }
}
