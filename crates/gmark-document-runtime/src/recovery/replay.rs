// @author kongweiguang

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crc32fast::Hasher;
use gmark_recovery_codec::{DecodedRecord, RecordKind, decode_record};

use super::format::{apply_format_patch, default_source_format, validate_source_format};
use super::types::{StoredBaseRecord, StoredEditRecord, StoredSelection};
use super::{
    RecoveredResidentDocument, RecoveredResidentJournal, ResidentFileFingerprint,
    ResidentRecoveryError, ResidentRecoveryReadStatus,
};

/// Scans every readable resident journal in lexical path order exactly once.
///
/// Unsupported versions and hard schema errors are quarantined as `.journal.invalid`;
/// a valid CRC prefix with a damaged tail is returned with `TruncatedTail` instead.
pub fn load_resident_recovery_journals(
    recovery_dir: impl AsRef<Path>,
) -> Result<Vec<RecoveredResidentJournal>, ResidentRecoveryError> {
    let recovery_dir = recovery_dir.as_ref();
    let entries = match fs::read_dir(recovery_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(ResidentRecoveryError::io("read", recovery_dir, source)),
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "journal")
                && !recovery_journal_is_suppressed(path)
        })
        .collect::<Vec<_>>();
    paths.sort();
    let mut journals = Vec::new();
    for path in paths {
        match replay_resident_recovery_journal_with_metadata(&path) {
            Ok(journal) => journals.push(journal),
            Err(_) => {
                let _ = quarantine_journal(&path);
            }
        }
    }
    Ok(journals)
}

/// A sidecar is written only after a saved journal could neither be deleted nor renamed.
/// Treat metadata failures as suppression too: replaying a possibly stale journal is unsafe.
fn recovery_journal_is_suppressed(journal_path: &Path) -> bool {
    let mut marker_path = journal_path.to_path_buf();
    marker_path.set_extension("journal.suppressed");
    match fs::metadata(&marker_path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(_) => true,
        Ok(_) => {
            // Startup cleanup must never reveal stale recovery: only delete the marker after the
            // original journal has been removed (or is already absent).
            match fs::remove_file(journal_path) {
                Ok(()) => {
                    let _ = fs::remove_file(marker_path);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let _ = fs::remove_file(marker_path);
                }
                Err(_) => {}
            }
            true
        }
    }
}

/// Compatibility projection of the one-pass resident-journal scan.
pub fn load_resident_recovery_documents(
    recovery_dir: impl AsRef<Path>,
) -> Result<Vec<RecoveredResidentDocument>, ResidentRecoveryError> {
    load_resident_recovery_journals(recovery_dir).map(|journals| {
        journals
            .into_iter()
            .map(|journal| journal.document)
            .collect()
    })
}

/// Removes quarantined resident-recovery artifacts while retaining every live journal.
pub fn cleanup_resident_recovery_artifacts(
    recovery_dir: impl AsRef<Path>,
) -> Result<usize, ResidentRecoveryError> {
    let recovery_dir = recovery_dir.as_ref();
    let entries = match fs::read_dir(recovery_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(source) => return Err(ResidentRecoveryError::io("read", recovery_dir, source)),
    };
    let mut removed = 0usize;
    for entry in entries {
        let entry =
            entry.map_err(|source| ResidentRecoveryError::io("read", recovery_dir, source))?;
        let path = entry.path();
        let is_quarantined = path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".journal.invalid"));
        if !is_quarantined {
            continue;
        }
        match fs::remove_file(&path) {
            Ok(()) => removed = removed.saturating_add(1),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(ResidentRecoveryError::io("remove", &path, source)),
        }
    }
    Ok(removed)
}

/// Replays one resident recovery journal, retaining only its final CRC-valid prefix.
pub fn replay_resident_recovery_journal(
    journal_path: impl AsRef<Path>,
) -> Result<RecoveredResidentDocument, ResidentRecoveryError> {
    replay_resident_recovery_journal_with_metadata(journal_path).map(|journal| journal.document)
}

/// Replays one resident journal and retains the wire-level selection-affinity
/// presence required by legacy root adapters.
pub fn replay_resident_recovery_journal_with_metadata(
    journal_path: impl AsRef<Path>,
) -> Result<RecoveredResidentJournal, ResidentRecoveryError> {
    let journal_path = journal_path.as_ref();
    let bytes = fs::read(journal_path)
        .map_err(|source| ResidentRecoveryError::io("read", journal_path, source))?;
    let mut cursor = 0usize;
    let mut base = None;
    let mut source = String::new();
    let mut source_format = None;
    let mut selection = StoredSelection::default();
    let mut view_mode = "rendered".to_owned();
    let mut status = ResidentRecoveryReadStatus::Complete;

    while cursor < bytes.len() {
        let Some(DecodedRecord {
            kind,
            payload,
            next,
        }) = decode_record(&bytes, cursor)?
        else {
            status = ResidentRecoveryReadStatus::TruncatedTail;
            break;
        };
        match kind {
            RecordKind::Base => {
                if base.is_some() {
                    return Err(ResidentRecoveryError::JournalFormat(
                        "recovery journal contains multiple base records".to_owned(),
                    ));
                }
                let record: StoredBaseRecord =
                    serde_json::from_slice(payload).map_err(|source| {
                        ResidentRecoveryError::json("invalid recovery base payload", source)
                    })?;
                source = record.source.clone();
                source_format = Some(
                    record
                        .source_format
                        .clone()
                        .map(Into::into)
                        .unwrap_or_else(|| default_source_format(&source)),
                );
                if let Some(base_selection) = record.selection.clone() {
                    selection = base_selection;
                }
                if let Some(base_view_mode) = record.view_mode.clone() {
                    view_mode = base_view_mode;
                }
                base = Some(record);
            }
            RecordKind::Edit => {
                if base.is_none() {
                    return Err(ResidentRecoveryError::JournalFormat(
                        "recovery edit appears before base record".to_owned(),
                    ));
                }
                let record: StoredEditRecord = match serde_json::from_slice(payload) {
                    Ok(record) => record,
                    Err(_) => {
                        status = ResidentRecoveryReadStatus::TruncatedTail;
                        break;
                    }
                };
                if record.start > record.end
                    || record.end > source.len()
                    || !source.is_char_boundary(record.start)
                    || !source.is_char_boundary(record.end)
                {
                    status = ResidentRecoveryReadStatus::TruncatedTail;
                    break;
                }
                if let Some(format_patch) = record.format_patch {
                    let Some(format) = source_format.as_mut() else {
                        status = ResidentRecoveryReadStatus::TruncatedTail;
                        break;
                    };
                    let removed_newlines = source[record.start..record.end]
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count();
                    let replacement_newlines = record
                        .replacement
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count();
                    let expected_endings = format
                        .endings
                        .len()
                        .saturating_sub(removed_newlines)
                        .saturating_add(replacement_newlines);
                    let patch_in_bounds = format_patch
                        .start
                        .checked_add(format_patch.removed)
                        .is_some_and(|end| end <= format.endings.len());
                    let patched_len = format
                        .endings
                        .len()
                        .saturating_sub(format_patch.removed)
                        .saturating_add(format_patch.inserted.len());
                    if !patch_in_bounds || patched_len != expected_endings {
                        status = ResidentRecoveryReadStatus::TruncatedTail;
                        break;
                    }
                    if apply_format_patch(format, format_patch).is_err() {
                        status = ResidentRecoveryReadStatus::TruncatedTail;
                        break;
                    }
                } else {
                    source_format = None;
                }
                source.replace_range(record.start..record.end, &record.replacement);
                selection = record.selection;
                view_mode = record.view_mode;
            }
        }
        cursor = next;
    }

    let base = base.ok_or_else(|| {
        ResidentRecoveryError::JournalFormat("recovery journal has no valid base record".to_owned())
    })?;
    let source_format = source_format.unwrap_or_else(|| default_source_format(&source));
    validate_source_format(&source, &source_format)?;
    let base_fingerprint = base.fingerprint.clone();
    let file_path = base.file_path.map(PathBuf::from);
    let base_file_changed = match (&file_path, &base.fingerprint) {
        (Some(path), Some(expected)) => fingerprint_resident_file(path)
            .map(|actual| actual != *expected)
            .unwrap_or(true),
        (Some(_), None) => true,
        (None, _) => false,
    };
    Ok(RecoveredResidentJournal {
        document: RecoveredResidentDocument {
            document_id: base.document_id,
            journal_path: journal_path.to_path_buf(),
            file_path,
            source,
            source_format,
            selection: selection.source_selection(),
            view_mode,
            read_status: status,
            base_file_changed,
            base_fingerprint,
        },
        anchor_affinity: selection.anchor_affinity.map(Into::into),
        head_affinity: selection.head_affinity.map(Into::into),
    })
}

/// Calculates the full-content fingerprint used to flag an external base-file change.
pub fn fingerprint_resident_file(
    path: impl AsRef<Path>,
) -> Result<ResidentFileFingerprint, ResidentRecoveryError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|source| ResidentRecoveryError::io("read", path, source))?;
    fingerprint_resident_contents(path, &bytes)
}

fn fingerprint_resident_contents(
    path: &Path,
    contents: &[u8],
) -> Result<ResidentFileFingerprint, ResidentRecoveryError> {
    let metadata =
        fs::metadata(path).map_err(|source| ResidentRecoveryError::io("inspect", path, source))?;
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    let mut hasher = Hasher::new();
    hasher.update(contents);
    Ok(ResidentFileFingerprint {
        path: path.to_string_lossy().into_owned(),
        size: metadata.len(),
        modified_nanos,
        crc32: hasher.finalize(),
    })
}

fn quarantine_journal(path: &Path) -> Result<PathBuf, ResidentRecoveryError> {
    let mut target = path.to_path_buf();
    target.set_extension("journal.invalid");
    if target.exists() {
        let stem = path.file_stem().map_or_else(
            || "recovery".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        target.set_file_name(format!("{stem}-{}.journal.invalid", uuid::Uuid::new_v4()));
    }
    fs::rename(path, &target)
        .map_err(|source| ResidentRecoveryError::io("quarantine", path, source))?;
    Ok(target)
}
