// @author kongweiguang

//! 崩溃恢复 journal：独立 Base + UTF-8 最小补丁、逐 record CRC 和可截断重放。

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context as _, bail};
use crc32fast::Hasher;
use serde::{Deserialize, Serialize};

use gmark_document::{LineEnding, SourceFormatSnapshot};
use gmark_large_document::{SourceAffinity, SourceAnchor, SourceSelection};
use gmark_recovery_codec::{DecodedRecord, RecordKind, decode_record, encode_record_payload};
const MAX_EDITS_BEFORE_COMPACTION: usize = 256;
const COMPACTION_OVERHEAD_BYTES: u64 = 8 * 1024 * 1024;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RecoveryLineEnding {
    Lf,
    CrLf,
    Cr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct RecoverySourceFormat {
    utf8_bom: bool,
    endings: Vec<RecoveryLineEnding>,
    dominant: RecoveryLineEnding,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct RecoveryFormatPatch {
    start: usize,
    removed: usize,
    inserted: Vec<RecoveryLineEnding>,
    utf8_bom: bool,
    dominant: RecoveryLineEnding,
}

impl From<LineEnding> for RecoveryLineEnding {
    fn from(value: LineEnding) -> Self {
        match value {
            LineEnding::Lf => Self::Lf,
            LineEnding::CrLf => Self::CrLf,
            LineEnding::Cr => Self::Cr,
        }
    }
}

impl From<RecoveryLineEnding> for LineEnding {
    fn from(value: RecoveryLineEnding) -> Self {
        match value {
            RecoveryLineEnding::Lf => Self::Lf,
            RecoveryLineEnding::CrLf => Self::CrLf,
            RecoveryLineEnding::Cr => Self::Cr,
        }
    }
}

impl From<&SourceFormatSnapshot> for RecoverySourceFormat {
    fn from(value: &SourceFormatSnapshot) -> Self {
        Self {
            utf8_bom: value.utf8_bom,
            endings: value.endings.iter().copied().map(Into::into).collect(),
            dominant: value.dominant.into(),
        }
    }
}

impl From<RecoverySourceFormat> for SourceFormatSnapshot {
    fn from(value: RecoverySourceFormat) -> Self {
        Self {
            utf8_bom: value.utf8_bom,
            endings: value.endings.into_iter().map(Into::into).collect(),
            dominant: value.dominant.into(),
        }
    }
}

pub(crate) struct RecoveryJournal {
    document_id: String,
    journal_path: PathBuf,
    file_path: Option<PathBuf>,
    base_fingerprint: Option<FileFingerprint>,
    base_source: String,
    last_source: String,
    base_format: SourceFormatSnapshot,
    last_format: SourceFormatSnapshot,
    initialized: bool,
    edit_count: usize,
}

impl RecoveryJournal {
    pub(crate) fn create(
        recovery_dir: &Path,
        file_path: Option<PathBuf>,
        source: String,
    ) -> anyhow::Result<Self> {
        let document = gmark_document::SourceDocument::new(&source);
        Self::create_formatted(
            recovery_dir,
            file_path,
            document.text(),
            document.source_format(),
        )
    }

    pub(crate) fn create_formatted(
        recovery_dir: &Path,
        file_path: Option<PathBuf>,
        source: String,
        source_format: SourceFormatSnapshot,
    ) -> anyhow::Result<Self> {
        validate_source_format(&source, &source_format)?;
        fs::create_dir_all(recovery_dir).with_context(|| {
            format!("failed to create recovery dir '{}'", recovery_dir.display())
        })?;
        let document_id = uuid::Uuid::new_v4().to_string();
        let base_fingerprint = file_path
            .as_deref()
            .and_then(|path| fingerprint_file(path).ok());
        Ok(Self {
            journal_path: recovery_dir.join(format!("{document_id}.journal")),
            document_id,
            file_path,
            base_fingerprint,
            base_source: source.clone(),
            last_source: source,
            base_format: source_format.clone(),
            last_format: source_format,
            initialized: false,
            edit_count: 0,
        })
    }

    pub(crate) fn resume(document: &RecoveredDocument) -> Self {
        Self {
            document_id: document.document_id.clone(),
            journal_path: document.journal_path.clone(),
            file_path: document.file_path.clone(),
            base_fingerprint: document.base_fingerprint.clone(),
            base_source: document.source.clone(),
            last_source: document.source.clone(),
            base_format: document.source_format.clone(),
            last_format: document.source_format.clone(),
            initialized: true,
            edit_count: 0,
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
        validate_source_format(source, &source_format)?;
        let text_edit = minimal_edit(&self.last_source, source);
        if text_edit.is_none() && self.last_format == source_format {
            return Ok(false);
        }
        let (range, replacement) = text_edit.unwrap_or((0..0, ""));
        if !self.initialized {
            self.write_base()?;
            self.initialized = true;
        }
        if self.should_compact(source.len()) {
            self.write_compacted_base(source, &source_format, selection, view_mode)?;
            self.base_source.clear();
            self.base_source.push_str(source);
            self.last_source.clear();
            self.last_source.push_str(source);
            self.base_format = source_format.clone();
            self.last_format = source_format;
            self.edit_count = 0;
            return Ok(true);
        }
        let edit = EditRecord {
            start: range.start,
            end: range.end,
            replacement: replacement.to_owned(),
            selection,
            view_mode: view_mode.to_owned(),
            format_patch: Some(build_format_patch(&self.last_format, &source_format)),
        };
        append_record(&self.journal_path, RecordKind::Edit, &edit)?;
        self.last_source.clear();
        self.last_source.push_str(source);
        self.last_format = source_format;
        self.edit_count = self.edit_count.saturating_add(1);
        Ok(true)
    }

    /// 成功保存或明确丢弃后删除未完成 session；下次编辑从新基线开始。
    #[cfg(test)]
    pub(crate) fn checkpoint(
        &mut self,
        file_path: Option<PathBuf>,
        source: String,
    ) -> anyhow::Result<()> {
        let document = gmark_document::SourceDocument::new(&source);
        self.checkpoint_formatted(file_path, document.text(), document.source_format())
    }

    pub(crate) fn checkpoint_formatted(
        &mut self,
        file_path: Option<PathBuf>,
        source: String,
        source_format: SourceFormatSnapshot,
    ) -> anyhow::Result<()> {
        validate_source_format(&source, &source_format)?;
        match fs::remove_file(&self.journal_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("failed to remove recovery journal"),
        }
        self.file_path = file_path;
        self.base_fingerprint = self
            .file_path
            .as_deref()
            .and_then(|path| fingerprint_file(path).ok());
        self.base_source = source.clone();
        self.last_source = source;
        self.base_format = source_format.clone();
        self.last_format = source_format;
        self.initialized = false;
        self.edit_count = 0;
        Ok(())
    }

    fn write_base(&self) -> anyhow::Result<()> {
        let base = BaseRecord {
            document_id: self.document_id.clone(),
            file_path: self
                .file_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            fingerprint: self.base_fingerprint.clone(),
            source: self.base_source.clone(),
            source_format: Some((&self.base_format).into()),
            selection: None,
            view_mode: None,
        };
        let bytes = encode_record(RecordKind::Base, &base)?;
        gmark_document::atomic_write(&self.journal_path, &bytes)
            .map_err(anyhow::Error::new)
            .context("failed to initialize recovery journal")
    }

    fn should_compact(&self, source_len: usize) -> bool {
        if self.edit_count >= MAX_EDITS_BEFORE_COMPACTION {
            return true;
        }
        let limit = u64::try_from(source_len)
            .unwrap_or(u64::MAX)
            .saturating_mul(4)
            .saturating_add(COMPACTION_OVERHEAD_BYTES);
        fs::metadata(&self.journal_path)
            .map(|metadata| metadata.len() > limit)
            .unwrap_or(false)
    }

    fn write_compacted_base(
        &self,
        source: &str,
        source_format: &SourceFormatSnapshot,
        selection: RecoverySelection,
        view_mode: &str,
    ) -> anyhow::Result<()> {
        let base = BaseRecord {
            document_id: self.document_id.clone(),
            file_path: self
                .file_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            fingerprint: self.base_fingerprint.clone(),
            source: source.to_owned(),
            source_format: Some(source_format.into()),
            selection: Some(selection),
            view_mode: Some(view_mode.to_owned()),
        };
        let bytes = encode_record(RecordKind::Base, &base)?;
        gmark_document::atomic_write(&self.journal_path, &bytes)
            .map_err(anyhow::Error::new)
            .context("failed to compact recovery journal")
    }
}

pub(crate) fn load_recovery_documents(
    recovery_dir: &Path,
) -> anyhow::Result<Vec<RecoveredDocument>> {
    let entries = match fs::read_dir(recovery_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("failed to read recovery directory"),
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "journal")
        })
        .collect::<Vec<_>>();
    paths.sort();
    let mut documents = Vec::new();
    for path in paths {
        match replay_journal(&path) {
            Ok(document) => documents.push(document),
            Err(error) => {
                let quarantine = quarantine_journal(&path).ok();
                eprintln!(
                    "failed to replay recovery journal '{}': {error}; quarantined={}",
                    path.display(),
                    quarantine
                        .as_deref()
                        .map_or_else(|| "false".to_owned(), |path| path.display().to_string())
                );
            }
        }
    }
    Ok(documents)
}

fn quarantine_journal(path: &Path) -> anyhow::Result<PathBuf> {
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
        .with_context(|| format!("failed to quarantine recovery journal '{}'", path.display()))?;
    Ok(target)
}

fn validate_source_format(source: &str, format: &SourceFormatSnapshot) -> anyhow::Result<()> {
    let newline_count = source.bytes().filter(|byte| *byte == b'\n').count();
    if newline_count != format.endings.len() {
        bail!(
            "source format has {} endings for {newline_count} newlines",
            format.endings.len()
        );
    }
    Ok(())
}

fn default_source_format(source: &str) -> SourceFormatSnapshot {
    SourceFormatSnapshot {
        utf8_bom: false,
        endings: vec![LineEnding::Lf; source.bytes().filter(|byte| *byte == b'\n').count()],
        dominant: LineEnding::Lf,
    }
}

fn build_format_patch(
    previous: &SourceFormatSnapshot,
    current: &SourceFormatSnapshot,
) -> RecoveryFormatPatch {
    let prefix = previous
        .endings
        .iter()
        .zip(&current.endings)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = previous.endings[prefix..]
        .iter()
        .rev()
        .zip(current.endings[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    RecoveryFormatPatch {
        start: prefix,
        removed: previous.endings.len() - prefix - suffix,
        inserted: current.endings[prefix..current.endings.len() - suffix]
            .iter()
            .copied()
            .map(Into::into)
            .collect(),
        utf8_bom: current.utf8_bom,
        dominant: current.dominant.into(),
    }
}

fn apply_format_patch(
    format: &mut SourceFormatSnapshot,
    patch: RecoveryFormatPatch,
) -> anyhow::Result<()> {
    let end = patch
        .start
        .checked_add(patch.removed)
        .context("recovery format patch range overflow")?;
    if end > format.endings.len() {
        bail!("recovery format patch is outside the ending table");
    }
    format
        .endings
        .splice(patch.start..end, patch.inserted.into_iter().map(Into::into));
    format.utf8_bom = patch.utf8_bom;
    format.dominant = patch.dominant.into();
    Ok(())
}

pub(crate) fn replay_journal(path: &Path) -> anyhow::Result<RecoveredDocument> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read recovery journal '{}'", path.display()))?;
    let mut cursor = 0usize;
    let mut base = None;
    let mut source = String::new();
    let mut source_format = None;
    let mut selection = RecoverySelection {
        start: 0,
        end: 0,
        reversed: false,
        anchor_affinity: None,
        head_affinity: None,
    };
    let mut view_mode = "rendered".to_owned();
    let mut status = RecoveryReadStatus::Complete;

    while cursor < bytes.len() {
        let Some(DecodedRecord {
            kind,
            payload,
            next,
        }) = decode_record(&bytes, cursor)?
        else {
            status = RecoveryReadStatus::TruncatedTail;
            break;
        };
        match kind {
            RecordKind::Base => {
                if base.is_some() {
                    bail!("recovery journal contains multiple base records");
                }
                let record: BaseRecord =
                    serde_json::from_slice(payload).context("invalid recovery base payload")?;
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
                    bail!("recovery edit appears before base record");
                }
                let record: EditRecord = match serde_json::from_slice(payload) {
                    Ok(record) => record,
                    Err(_) => {
                        status = RecoveryReadStatus::TruncatedTail;
                        break;
                    }
                };
                if record.start > record.end
                    || record.end > source.len()
                    || !source.is_char_boundary(record.start)
                    || !source.is_char_boundary(record.end)
                {
                    status = RecoveryReadStatus::TruncatedTail;
                    break;
                }
                if let Some(format_patch) = record.format_patch {
                    let Some(format) = source_format.as_mut() else {
                        status = RecoveryReadStatus::TruncatedTail;
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
                        status = RecoveryReadStatus::TruncatedTail;
                        break;
                    }
                    apply_format_patch(format, format_patch)
                        .expect("已验证的恢复格式补丁必须可应用");
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

    let base = base.context("recovery journal has no valid base record")?;
    let source_format = source_format.unwrap_or_else(|| default_source_format(&source));
    validate_source_format(&source, &source_format)?;
    let base_fingerprint = base.fingerprint.clone();
    let file_path = base.file_path.map(PathBuf::from);
    let base_file_changed = match (&file_path, &base.fingerprint) {
        (Some(path), Some(expected)) => fingerprint_file(path)
            .map(|actual| actual != *expected)
            .unwrap_or(true),
        (Some(_), None) => true,
        (None, _) => false,
    };
    Ok(RecoveredDocument {
        document_id: base.document_id,
        journal_path: path.to_path_buf(),
        file_path,
        source,
        source_format,
        selection,
        view_mode,
        read_status: status,
        base_file_changed,
        base_fingerprint,
    })
}

fn encode_record<T: Serialize>(kind: RecordKind, payload: &T) -> anyhow::Result<Vec<u8>> {
    let payload = serde_json::to_vec(payload).context("failed to serialize recovery record")?;
    encode_record_payload(kind, &payload).map_err(anyhow::Error::new)
}

fn append_record<T: Serialize>(path: &Path, kind: RecordKind, payload: &T) -> anyhow::Result<()> {
    let bytes = encode_record(kind, payload)?;
    let mut file = OpenOptions::new()
        .create(false)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open recovery journal '{}'", path.display()))?;
    file.write_all(&bytes)
        .context("failed to append recovery record")?;
    file.flush().context("failed to flush recovery record")?;
    file.sync_data().context("failed to sync recovery record")?;
    Ok(())
}

fn minimal_edit<'a>(previous: &str, current: &'a str) -> Option<(Range<usize>, &'a str)> {
    if previous == current {
        return None;
    }
    let prefix = previous
        .chars()
        .zip(current.chars())
        .take_while(|(left, right)| left == right)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    let suffix = previous[prefix..]
        .chars()
        .rev()
        .zip(current[prefix..].chars().rev())
        .take_while(|(left, right)| left == right)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    Some((
        prefix..previous.len() - suffix,
        &current[prefix..current.len() - suffix],
    ))
}

pub(crate) fn fingerprint_file(path: &Path) -> anyhow::Result<FileFingerprint> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to fingerprint '{}'", path.display()))?;
    fingerprint_contents(path, &bytes)
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

#[cfg(test)]
#[path = "../tests/unit/recovery.rs"]
mod tests;
