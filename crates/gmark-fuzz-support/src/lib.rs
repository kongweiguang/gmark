// @author kongweiguang

//! Shared deterministic oracle for stable corpus replay and libFuzzer.

#![forbid(unsafe_code)]

use std::collections::VecDeque;

use gmark_document::{
    DocumentError, DocumentSnapshot, Revision, SourceDocument, TextEdit, Transaction,
};
use gmark_recovery_codec::{MAX_RECORD_BYTES, decode_record};

const HISTORY_LIMIT: usize = 64;
const MAX_ACTIONS: usize = 256;
const MAX_REPLACEMENT_BYTES: usize = 16;
const RETAINED_SNAPSHOT_LIMIT: usize = 16;
const MAX_RECOVERY_FRAMES: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryFrameRun {
    pub accepted_frames: usize,
    pub accepted_bytes: usize,
}

/// Walks a bounded recovery-journal byte stream and checks the decoder's progress invariants.
///
/// 解码错误和损坏尾部都是合法 fuzz 结果；panic 只表示 codec 返回了越界、零进度或超限记录。
pub fn run_recovery_frame_program(bytes: &[u8]) -> RecoveryFrameRun {
    let mut cursor = 0;
    let mut accepted_frames = 0;
    while accepted_frames < MAX_RECOVERY_FRAMES {
        let record = match decode_record(bytes, cursor) {
            Ok(Some(record)) => record,
            Ok(None) | Err(_) => break,
        };
        assert!(record.next > cursor, "decoder must make forward progress");
        assert!(record.next <= bytes.len(), "decoder advanced past input");
        assert!(record.payload.len() <= MAX_RECORD_BYTES);
        cursor = record.next;
        accepted_frames += 1;
    }
    RecoveryFrameRun {
        accepted_frames,
        accepted_bytes: cursor,
    }
}

/// Replays a byte program against `SourceDocument` and a plain-String state-machine oracle.
///
/// 输入格式是 fuzz 内部协议，不是稳定文件格式。任何 panic 都表示文档内核与 oracle 分歧，
/// libFuzzer 会负责最小化输入；稳定 CI 则会报告具体 seed。
pub fn run_source_document_program(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let initial_len = usize::from(cursor.next()) % 64;
    let initial = String::from_utf8_lossy(cursor.take(initial_len)).into_owned();
    let mut document = SourceDocument::with_history_limit(&initial, HISTORY_LIMIT);
    let mut oracle = Oracle::new(document.text());
    let mut retained_snapshots = VecDeque::new();
    retain_snapshot(&mut retained_snapshots, &document);

    let mut actions = 0;
    while cursor.remaining() > 0 && actions < MAX_ACTIONS {
        actions += 1;
        match cursor.next() % 6 {
            0 | 1 => apply_generated_transaction(
                &mut cursor,
                &mut document,
                &mut oracle,
                &mut retained_snapshots,
            ),
            2 => apply_undo(&mut document, &mut oracle),
            3 => apply_redo(&mut document, &mut oracle),
            4 => reject_stale_transaction(&mut cursor, &mut document, &oracle),
            _ => reject_invalid_range(&mut document, &oracle),
        }
        assert_document_matches(&document, &oracle);
        assert_snapshots_stable(&retained_snapshots);
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }

    fn next(&mut self) -> u8 {
        let value = self.data.get(self.offset).copied().unwrap_or(0);
        self.offset = self.offset.saturating_add(1).min(self.data.len());
        value
    }

    fn take(&mut self, requested: usize) -> &'a [u8] {
        let end = self.offset.saturating_add(requested).min(self.data.len());
        let bytes = &self.data[self.offset..end];
        self.offset = end;
        bytes
    }
}

struct Oracle {
    text: String,
    undo: VecDeque<String>,
    redo: Vec<String>,
}

impl Oracle {
    fn new(text: String) -> Self {
        Self {
            text,
            undo: VecDeque::new(),
            redo: Vec::new(),
        }
    }

    fn record_edit(&mut self, next: String) {
        if self.undo.len() == HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(std::mem::replace(&mut self.text, next));
        self.redo.clear();
    }

    fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.text, previous));
        true
    }

    fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        if self.undo.len() == HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(std::mem::replace(&mut self.text, next));
        true
    }
}

fn apply_generated_transaction(
    cursor: &mut Cursor<'_>,
    document: &mut SourceDocument,
    oracle: &mut Oracle,
    retained_snapshots: &mut VecDeque<(DocumentSnapshot, String)>,
) {
    let boundaries = char_boundaries(&oracle.text);
    let mut points = [
        usize::from(cursor.next()) % boundaries.len(),
        usize::from(cursor.next()) % boundaries.len(),
        usize::from(cursor.next()) % boundaries.len(),
        usize::from(cursor.next()) % boundaries.len(),
    ];
    points.sort_unstable();
    let replacement_one = take_replacement(cursor);
    let replacement_two = take_replacement(cursor);
    let mut edits = vec![TextEdit::new(
        boundaries[points[0]]..boundaries[points[1]],
        replacement_one,
    )];
    if cursor.next().is_multiple_of(2) {
        edits.push(TextEdit::new(
            boundaries[points[2]]..boundaries[points[3]],
            replacement_two,
        ));
    }

    let next = apply_reference_edits(&oracle.text, &edits);
    let revision_before = document.revision();
    document
        .apply_transaction(Transaction::new(document.revision(), edits))
        .expect("generated transaction must be valid");
    assert_eq!(document.revision().get(), revision_before.get() + 1);
    oracle.record_edit(next);
    retain_snapshot(retained_snapshots, document);
}

fn apply_undo(document: &mut SourceDocument, oracle: &mut Oracle) {
    let revision_before = document.revision();
    let expected_change = oracle.undo();
    let actual = document.undo().expect("undo must not fail");
    assert_eq!(actual.is_some(), expected_change);
    assert_eq!(
        document.revision().get(),
        revision_before.get() + u64::from(expected_change)
    );
}

fn apply_redo(document: &mut SourceDocument, oracle: &mut Oracle) {
    let revision_before = document.revision();
    let expected_change = oracle.redo();
    let actual = document.redo().expect("redo must not fail");
    assert_eq!(actual.is_some(), expected_change);
    assert_eq!(
        document.revision().get(),
        revision_before.get() + u64::from(expected_change)
    );
}

fn reject_stale_transaction(
    cursor: &mut Cursor<'_>,
    document: &mut SourceDocument,
    oracle: &Oracle,
) {
    let boundaries = char_boundaries(&oracle.text);
    let point = boundaries[usize::from(cursor.next()) % boundaries.len()];
    let revision_before = document.revision();
    let text_before = document.text();
    let stale_revision = Revision::from_u64(revision_before.get() + 1);
    let error = document
        .apply_transaction(Transaction::new(
            stale_revision,
            vec![TextEdit::new(point..point, "stale")],
        ))
        .expect_err("stale transaction must be rejected");
    assert!(matches!(error, DocumentError::StaleRevision { .. }));
    assert_eq!(document.revision(), revision_before);
    assert_eq!(document.text(), text_before);
}

fn reject_invalid_range(document: &mut SourceDocument, oracle: &Oracle) {
    let revision_before = document.revision();
    let text_before = document.text();
    let invalid_offset = oracle.text.len().saturating_add(1);
    let error = document
        .apply_transaction(Transaction::new(
            document.revision(),
            vec![TextEdit::new(invalid_offset..invalid_offset, "invalid")],
        ))
        .expect_err("out-of-range transaction must be rejected");
    assert!(matches!(error, DocumentError::InvalidRange { .. }));
    assert_eq!(document.revision(), revision_before);
    assert_eq!(document.text(), text_before);
}

fn take_replacement(cursor: &mut Cursor<'_>) -> String {
    let len = usize::from(cursor.next()) % (MAX_REPLACEMENT_BYTES + 1);
    String::from_utf8_lossy(cursor.take(len)).into_owned()
}

fn char_boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
        .collect()
}

fn apply_reference_edits(source: &str, edits: &[TextEdit]) -> String {
    let mut result = source.to_owned();
    for edit in edits.iter().rev() {
        result.replace_range(edit.range().clone(), edit.replacement());
    }
    result
}

fn retain_snapshot(
    snapshots: &mut VecDeque<(DocumentSnapshot, String)>,
    document: &SourceDocument,
) {
    if snapshots.len() == RETAINED_SNAPSHOT_LIMIT {
        snapshots.pop_front();
    }
    snapshots.push_back((document.snapshot(), document.text()));
}

fn assert_document_matches(document: &SourceDocument, oracle: &Oracle) {
    assert_eq!(document.text(), oracle.text);
    assert_eq!(document.len(), oracle.text.len());
    assert_eq!(document.is_empty(), oracle.text.is_empty());
    assert_eq!(document.can_undo(), !oracle.undo.is_empty());
    assert_eq!(document.can_redo(), !oracle.redo.is_empty());
}

fn assert_snapshots_stable(snapshots: &VecDeque<(DocumentSnapshot, String)>) {
    for (snapshot, expected) in snapshots {
        assert_eq!(snapshot.text(), *expected);
        assert_eq!(snapshot.len(), expected.len());
    }
}
