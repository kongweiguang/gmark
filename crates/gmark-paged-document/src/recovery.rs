// @author kongweiguang

//! Content-free base identity plus CRC-framed edit recovery for disk-backed documents.

use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use gmark_document_core::{SourceAffinity, SourceSelection, TextEncoding};
use gmark_recovery_codec::{HEADER_LEN, RecordKind, decode_header, encode_record_payload};
use serde::{Deserialize, Serialize};

use crate::{
    FileSource, LineIndex, PagedDocumentError, PieceDocument, PreparedUtf8Source,
    prepare_utf8_source,
};

const RECOVERY_CHUNK_BYTES: usize = 16 * 1024 * 1024;
const SAMPLE_BYTES: u64 = 64 * 1024;
static NEXT_JOURNAL_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagedRecoveryReadStatus {
    Complete,
    TruncatedTail,
}

/// Recovery 与实时 Source 共用同一个 anchor/affinity 真值，不能在落盘时退化为 Range。
pub type PagedRecoverySelection = SourceSelection;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagedRecoveryBase {
    pub path: PathBuf,
    pub len: u64,
    pub modified_nanos: Option<u128>,
    pub sampled_hash: u32,
    pub encoding: TextEncoding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PagedRecoveryCommand {
    Replace {
        range: Range<u64>,
        chunks: Vec<String>,
    },
    Undo,
    Redo,
}

pub struct RecoveredPagedDocument {
    pub base: PagedRecoveryBase,
    pub journal: PagedRecoveryJournal,
    pub prepared_source: PreparedUtf8Source,
    pub document: PieceDocument,
    pub selection: Option<PagedRecoverySelection>,
    pub view_mode: String,
    pub read_status: PagedRecoveryReadStatus,
}

pub struct PagedRecoveryJournal {
    path: PathBuf,
    next_transaction: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BaseRecord {
    path: PathBuf,
    len: u64,
    modified_nanos: Option<u128>,
    sampled_hash: u32,
    encoding: RecoveryEncoding,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RecoveryEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Legacy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
enum EditRecord {
    ReplaceChunk {
        transaction: u64,
        start: u64,
        end: u64,
        chunk_index: u32,
        chunk_count: u32,
        text: String,
        selection: Option<SelectionRecord>,
        view_mode: String,
    },
    Undo {
        selection: Option<SelectionRecord>,
        view_mode: String,
    },
    Redo {
        selection: Option<SelectionRecord>,
        view_mode: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SelectionRecord {
    start: u64,
    end: u64,
    reversed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    anchor_affinity: Option<RecoveryAffinity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    head_affinity: Option<RecoveryAffinity>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RecoveryAffinity {
    Before,
    After,
}

struct PendingReplace {
    transaction: u64,
    range: Range<u64>,
    chunk_count: u32,
    chunks: Vec<String>,
    selection: Option<PagedRecoverySelection>,
    view_mode: String,
}

enum FrameRead {
    End,
    Frame(RecordKind, Vec<u8>),
    Truncated,
}

impl PagedRecoveryJournal {
    pub fn create(
        recovery_dir: impl AsRef<Path>,
        source: &FileSource,
        encoding: TextEncoding,
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

pub fn replay_paged_recovery(
    journal_path: impl AsRef<Path>,
) -> Result<RecoveredPagedDocument, PagedDocumentError> {
    let journal_path = journal_path.as_ref();
    let (base, commands, selection, view_mode, read_status, next_transaction) =
        read_journal(journal_path)?;
    let source = FileSource::open(&base.path)?;
    verify_base(&source, &base)?;
    let prepared_source = prepare_utf8_source(source, base.encoding.clone())?;
    let index = LineIndex::build(prepared_source.source())?;
    let mut document = PieceDocument::open(prepared_source.source().clone(), index)?;
    for command in commands {
        match command {
            PagedRecoveryCommand::Replace { range, chunks } => {
                document.replace_text_chunks(range, chunks.iter().map(String::as_str))?;
            }
            PagedRecoveryCommand::Undo => {
                if !document.undo() {
                    return Err(PagedDocumentError::Recovery(
                        "recovery undo has no matching edit".to_owned(),
                    ));
                }
            }
            PagedRecoveryCommand::Redo => {
                if !document.redo() {
                    return Err(PagedDocumentError::Recovery(
                        "recovery redo has no matching undo".to_owned(),
                    ));
                }
            }
        }
    }
    Ok(RecoveredPagedDocument {
        base,
        journal: PagedRecoveryJournal {
            path: journal_path.to_path_buf(),
            next_transaction,
        },
        prepared_source,
        document,
        selection,
        view_mode,
        read_status,
    })
}

pub fn list_paged_recovery_journals(
    recovery_dir: impl AsRef<Path>,
) -> Result<Vec<PathBuf>, PagedDocumentError> {
    let recovery_dir = recovery_dir.as_ref();
    let entries = match fs::read_dir(recovery_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(PagedDocumentError::Io {
                path: recovery_dir.to_path_buf(),
                source,
            });
        }
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "large-journal")
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

pub fn inspect_paged_recovery_base(
    journal_path: impl AsRef<Path>,
) -> Result<PagedRecoveryBase, PagedDocumentError> {
    let journal_path = journal_path.as_ref();
    let file = File::open(journal_path).map_err(|source| PagedDocumentError::Io {
        path: journal_path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let FrameRead::Frame(RecordKind::Base, payload) = read_frame(&mut reader)? else {
        return Err(PagedDocumentError::Recovery(
            "large recovery journal has no valid base frame".to_owned(),
        ));
    };
    let record: BaseRecord = serde_json::from_slice(&payload)
        .map_err(|error| PagedDocumentError::Recovery(error.to_string()))?;
    let source = FileSource::open(&record.path)?;
    Ok(PagedRecoveryBase {
        path: record.path,
        len: record.len,
        modified_nanos: record.modified_nanos,
        sampled_hash: record.sampled_hash,
        encoding: record.encoding.resolve(&source)?,
    })
}

pub fn paged_recovery_has_edits(
    journal_path: impl AsRef<Path>,
) -> Result<bool, PagedDocumentError> {
    let journal_path = journal_path.as_ref();
    let file = File::open(journal_path).map_err(|source| PagedDocumentError::Io {
        path: journal_path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    if !matches!(
        read_frame(&mut reader)?,
        FrameRead::Frame(RecordKind::Base, _)
    ) {
        return Err(PagedDocumentError::Recovery(
            "large recovery journal has no valid base frame".to_owned(),
        ));
    }
    Ok(matches!(
        read_frame(&mut reader)?,
        FrameRead::Frame(RecordKind::Edit, _)
    ))
}

type JournalRead = (
    PagedRecoveryBase,
    Vec<PagedRecoveryCommand>,
    Option<PagedRecoverySelection>,
    String,
    PagedRecoveryReadStatus,
    u64,
);

fn read_journal(path: &Path) -> Result<JournalRead, PagedDocumentError> {
    let file = File::open(path).map_err(|source| PagedDocumentError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let mut base = None;
    let mut commands = Vec::new();
    let mut pending: Option<PendingReplace> = None;
    let mut selection = None;
    let mut view_mode = "source".to_owned();
    let mut status = PagedRecoveryReadStatus::Complete;
    let mut max_transaction = 0u64;
    loop {
        let (kind, payload) = match read_frame(&mut reader)? {
            FrameRead::End => break,
            FrameRead::Truncated => {
                status = PagedRecoveryReadStatus::TruncatedTail;
                break;
            }
            FrameRead::Frame(kind, payload) => (kind, payload),
        };
        match kind {
            RecordKind::Base => {
                if base.is_some() || !commands.is_empty() || pending.is_some() {
                    return Err(PagedDocumentError::Recovery(
                        "recovery journal contains an out-of-order base".to_owned(),
                    ));
                }
                let record: BaseRecord = serde_json::from_slice(&payload)
                    .map_err(|error| PagedDocumentError::Recovery(error.to_string()))?;
                let source = FileSource::open(&record.path)?;
                base = Some(PagedRecoveryBase {
                    path: record.path,
                    len: record.len,
                    modified_nanos: record.modified_nanos,
                    sampled_hash: record.sampled_hash,
                    encoding: record.encoding.resolve(&source)?,
                });
            }
            RecordKind::Edit => {
                if base.is_none() {
                    return Err(PagedDocumentError::Recovery(
                        "recovery edit appears before its base".to_owned(),
                    ));
                }
                let record: EditRecord = match serde_json::from_slice(&payload) {
                    Ok(record) => record,
                    Err(_) => {
                        status = PagedRecoveryReadStatus::TruncatedTail;
                        break;
                    }
                };
                if !consume_edit(
                    record,
                    &mut pending,
                    &mut commands,
                    &mut selection,
                    &mut view_mode,
                    &mut max_transaction,
                )? {
                    status = PagedRecoveryReadStatus::TruncatedTail;
                    break;
                }
            }
        }
    }
    if pending.is_some() {
        status = PagedRecoveryReadStatus::TruncatedTail;
    }
    let base = base.ok_or_else(|| {
        PagedDocumentError::Recovery("recovery journal has no valid base".to_owned())
    })?;
    Ok((
        base,
        commands,
        selection,
        view_mode,
        status,
        max_transaction.wrapping_add(1).max(1),
    ))
}

fn consume_edit(
    record: EditRecord,
    pending: &mut Option<PendingReplace>,
    commands: &mut Vec<PagedRecoveryCommand>,
    selection: &mut Option<PagedRecoverySelection>,
    view_mode: &mut String,
    max_transaction: &mut u64,
) -> Result<bool, PagedDocumentError> {
    match record {
        EditRecord::ReplaceChunk {
            transaction,
            start,
            end,
            chunk_index,
            chunk_count,
            text,
            selection: next_selection,
            view_mode: next_view_mode,
        } => {
            *max_transaction = (*max_transaction).max(transaction);
            if start > end || chunk_count == 0 {
                return Ok(false);
            }
            if chunk_index == 0 {
                if pending.is_some() {
                    return Ok(false);
                }
                *pending = Some(PendingReplace {
                    transaction,
                    range: start..end,
                    chunk_count,
                    chunks: Vec::with_capacity(chunk_count as usize),
                    selection: next_selection.map(Into::into),
                    view_mode: next_view_mode,
                });
            }
            let Some(current) = pending.as_mut() else {
                return Ok(false);
            };
            if current.transaction != transaction
                || current.range != (start..end)
                || current.chunk_count != chunk_count
                || chunk_index as usize != current.chunks.len()
            {
                return Ok(false);
            }
            current.chunks.push(text);
            if current.chunks.len() == current.chunk_count as usize {
                let completed = pending
                    .take()
                    .expect("checked pending recovery transaction");
                *selection = completed.selection;
                *view_mode = completed.view_mode;
                commands.push(PagedRecoveryCommand::Replace {
                    range: completed.range,
                    chunks: completed.chunks,
                });
            }
            Ok(true)
        }
        EditRecord::Undo {
            selection: next_selection,
            view_mode: next_view_mode,
        } => {
            if pending.is_some() {
                return Ok(false);
            }
            *selection = next_selection.map(Into::into);
            *view_mode = next_view_mode;
            commands.push(PagedRecoveryCommand::Undo);
            Ok(true)
        }
        EditRecord::Redo {
            selection: next_selection,
            view_mode: next_view_mode,
        } => {
            if pending.is_some() {
                return Ok(false);
            }
            *selection = next_selection.map(Into::into);
            *view_mode = next_view_mode;
            commands.push(PagedRecoveryCommand::Redo);
            Ok(true)
        }
    }
}

fn read_frame(reader: &mut BufReader<File>) -> Result<FrameRead, PagedDocumentError> {
    let mut header = [0u8; HEADER_LEN];
    let mut read = 0usize;
    while read < header.len() {
        let count = reader
            .read(&mut header[read..])
            .map_err(|error| PagedDocumentError::Recovery(error.to_string()))?;
        if count == 0 {
            return if read == 0 {
                Ok(FrameRead::End)
            } else {
                Ok(FrameRead::Truncated)
            };
        }
        read += count;
    }
    let Some(decoded) =
        decode_header(&header).map_err(|error| PagedDocumentError::Recovery(error.to_string()))?
    else {
        return Ok(FrameRead::Truncated);
    };
    let mut payload = vec![0u8; decoded.payload_len];
    if let Err(error) = reader.read_exact(&mut payload) {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            return Ok(FrameRead::Truncated);
        }
        return Err(PagedDocumentError::Recovery(error.to_string()));
    }
    if crc32fast::hash(&payload) != decoded.expected_crc {
        return Ok(FrameRead::Truncated);
    }
    Ok(FrameRead::Frame(decoded.kind, payload))
}

fn verify_base(source: &FileSource, base: &PagedRecoveryBase) -> Result<(), PagedDocumentError> {
    let identity = source.identity()?;
    if identity.len != base.len
        || identity.modified_nanos != base.modified_nanos
        || sampled_hash(source, identity.len)? != base.sampled_hash
    {
        return Err(PagedDocumentError::SourceChanged);
    }
    Ok(())
}

fn sampled_hash(source: &FileSource, len: u64) -> Result<u32, PagedDocumentError> {
    let mut hasher = crc32fast::Hasher::new();
    for start in [
        0,
        len.saturating_sub(SAMPLE_BYTES) / 2,
        len.saturating_sub(SAMPLE_BYTES),
    ] {
        let end = (start + SAMPLE_BYTES).min(len);
        if start < end {
            hasher.update(&source.read_range(start, end)?);
        }
    }
    Ok(hasher.finalize())
}

fn utf8_chunks(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return vec![""];
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < text.len() {
        let mut end = (start + RECOVERY_CHUNK_BYTES).min(text.len());
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            end = text[start..]
                .char_indices()
                .nth(1)
                .map_or(text.len(), |(offset, _)| start + offset);
        }
        chunks.push(&text[start..end]);
        start = end;
    }
    chunks
}

fn encode_json_record(
    kind: RecordKind,
    value: &impl Serialize,
) -> Result<Vec<u8>, PagedDocumentError> {
    let payload = serde_json::to_vec(value)
        .map_err(|error| PagedDocumentError::Recovery(error.to_string()))?;
    encode_record_payload(kind, &payload)
        .map_err(|error| PagedDocumentError::Recovery(error.to_string()))
}

fn append_frames(path: &Path, frames: &[Vec<u8>]) -> Result<(), PagedDocumentError> {
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|source| PagedDocumentError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    for frame in frames {
        file.write_all(frame)
            .map_err(|source| PagedDocumentError::Io {
                path: path.to_path_buf(),
                source,
            })?;
    }
    file.sync_data().map_err(|source| PagedDocumentError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), PagedDocumentError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|source| PagedDocumentError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    temporary
        .write_all(bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|source| PagedDocumentError::Io {
            path: temporary.path().to_path_buf(),
            source,
        })?;
    let persisted = temporary
        .persist(path)
        .map_err(|error| PagedDocumentError::Persist {
            path: path.to_path_buf(),
            source: error.error,
        })?;
    persisted
        .sync_all()
        .map_err(|source| PagedDocumentError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    crate::source::sync_parent_directory(parent)?;
    Ok(())
}

fn monotonic_timestamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

impl RecoveryEncoding {
    fn from_encoding(encoding: &TextEncoding) -> Self {
        match encoding {
            TextEncoding::Utf8 { bom: false } => Self::Utf8,
            TextEncoding::Utf8 { bom: true } => Self::Utf8Bom,
            TextEncoding::Utf16Le => Self::Utf16Le,
            TextEncoding::Utf16Be => Self::Utf16Be,
            TextEncoding::Legacy(_) => Self::Legacy,
        }
    }

    fn resolve(self, source: &FileSource) -> Result<TextEncoding, PagedDocumentError> {
        match self {
            Self::Utf8 => Ok(TextEncoding::Utf8 { bom: false }),
            Self::Utf8Bom => Ok(TextEncoding::Utf8 { bom: true }),
            Self::Utf16Le => Ok(TextEncoding::Utf16Le),
            Self::Utf16Be => Ok(TextEncoding::Utf16Be),
            Self::Legacy => crate::probe_file(source.path(), crate::ProbeOptions::default())
                .map(|probe| probe.encoding),
        }
    }
}

impl From<PagedRecoverySelection> for SelectionRecord {
    fn from(selection: PagedRecoverySelection) -> Self {
        let range = selection.range();
        Self {
            start: range.start,
            end: range.end,
            reversed: selection.reversed(),
            anchor_affinity: Some(selection.anchor.affinity.into()),
            head_affinity: Some(selection.head.affinity.into()),
        }
    }
}

impl From<SelectionRecord> for PagedRecoverySelection {
    fn from(selection: SelectionRecord) -> Self {
        let mut restored =
            SourceSelection::from_range(selection.start..selection.end, selection.reversed);
        if let Some(affinity) = selection.anchor_affinity {
            restored.anchor.affinity = affinity.into();
        }
        if let Some(affinity) = selection.head_affinity {
            restored.head.affinity = affinity.into();
        }
        restored
    }
}

impl From<SourceAffinity> for RecoveryAffinity {
    fn from(affinity: SourceAffinity) -> Self {
        match affinity {
            SourceAffinity::Before => Self::Before,
            SourceAffinity::After => Self::After,
        }
    }
}

impl From<RecoveryAffinity> for SourceAffinity {
    fn from(affinity: RecoveryAffinity) -> Self {
        match affinity {
            RecoveryAffinity::Before => Self::Before,
            RecoveryAffinity::After => Self::After,
        }
    }
}
