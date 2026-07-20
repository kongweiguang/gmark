// @author kongweiguang

use std::ops::Range;
use std::path::Path;

use crate::{
    EncodedSavePlan, ExternalChange, FileIdentity, FileSource, LargeDocumentError, LineIndex,
    PieceDocument, SearchCancellation, SearchMatch, SearchOptions,
};

/// 单次视口读取的硬上限，调用方不能通过异常窗口把整条超长行物化进内存。
pub const DEFAULT_VIEWPORT_COLUMN_BYTES: u64 = 64 * 1024;
pub const MAX_VIEWPORT_ROWS: usize = 4_096;
pub const MAX_VIEWPORT_OVERSCAN_ROWS: usize = 512;
pub const MAX_SYSTEM_CLIPBOARD_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionTransfer {
    Clipboard,
    ExportFile,
}

pub const fn selection_transfer_for_len(byte_len: u64) -> SelectionTransfer {
    if byte_len <= MAX_SYSTEM_CLIPBOARD_BYTES {
        SelectionTransfer::Clipboard
    } else {
        SelectionTransfer::ExportFile
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SourceAffinity {
    #[default]
    Before,
    After,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceAnchor {
    pub byte_offset: u64,
    pub affinity: SourceAffinity,
}

impl SourceAnchor {
    pub const fn new(byte_offset: u64, affinity: SourceAffinity) -> Self {
        Self {
            byte_offset,
            affinity,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceSelection {
    pub anchor: SourceAnchor,
    pub head: SourceAnchor,
}

impl SourceSelection {
    pub const fn collapsed(byte_offset: u64, affinity: SourceAffinity) -> Self {
        let anchor = SourceAnchor::new(byte_offset, affinity);
        Self {
            anchor,
            head: anchor,
        }
    }

    pub fn from_range(range: Range<u64>, reversed: bool) -> Self {
        if range.is_empty() {
            return Self::collapsed(range.start, SourceAffinity::Before);
        }
        let start = SourceAnchor::new(range.start, SourceAffinity::Before);
        let end = SourceAnchor::new(range.end, SourceAffinity::After);
        if reversed {
            Self {
                anchor: end,
                head: start,
            }
        } else {
            Self {
                anchor: start,
                head: end,
            }
        }
    }

    pub fn range(self) -> Range<u64> {
        self.anchor.byte_offset.min(self.head.byte_offset)
            ..self.anchor.byte_offset.max(self.head.byte_offset)
    }

    pub fn reversed(self) -> bool {
        self.head.byte_offset < self.anchor.byte_offset
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewportRequest {
    pub start_line: u64,
    pub rows: usize,
    pub overscan_rows: usize,
    pub column_start: u64,
    pub column_bytes: u64,
    pub generation: u64,
}

impl ViewportRequest {
    pub fn bounded(
        start_line: u64,
        rows: usize,
        overscan_rows: usize,
        column_start: u64,
        generation: u64,
    ) -> Self {
        Self {
            start_line,
            rows: rows.min(MAX_VIEWPORT_ROWS),
            overscan_rows: overscan_rows.min(MAX_VIEWPORT_OVERSCAN_ROWS),
            column_start,
            column_bytes: DEFAULT_VIEWPORT_COLUMN_BYTES,
            generation,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewportLine {
    pub line: u64,
    pub source_range: Range<u64>,
    pub content_range: Range<u64>,
    pub text: String,
    pub ending: String,
    pub leading_truncated: bool,
    pub trailing_truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewportSnapshot {
    pub generation: u64,
    pub requested_lines: Range<u64>,
    pub exact_line_count: u64,
    pub lines: Vec<ViewportLine>,
}

/// 磁盘后备文档的纯数据层。GPUI 只能把它克隆到后台 worker，并消费不可变快照。
#[derive(Clone)]
pub struct LargeDocumentBackend {
    document: PieceDocument,
    generation: u64,
}

impl LargeDocumentBackend {
    pub fn new(document: PieceDocument) -> Self {
        Self {
            document,
            generation: 0,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn read_viewport(
        &self,
        request: &ViewportRequest,
    ) -> Result<ViewportSnapshot, LargeDocumentError> {
        self.read_viewport_cancellable(request, &SearchCancellation::default())
    }

    pub fn read_viewport_cancellable(
        &self,
        request: &ViewportRequest,
        cancellation: &SearchCancellation,
    ) -> Result<ViewportSnapshot, LargeDocumentError> {
        let rows = request.rows.min(MAX_VIEWPORT_ROWS);
        let overscan = request.overscan_rows.min(MAX_VIEWPORT_OVERSCAN_ROWS) as u64;
        let start = request.start_line.saturating_sub(overscan);
        let end = request
            .start_line
            .saturating_add(rows as u64)
            .saturating_add(overscan)
            .min(self.document.line_count());
        let column_bytes = request.column_bytes.clamp(1, DEFAULT_VIEWPORT_COLUMN_BYTES);
        let mut lines = Vec::with_capacity(usize::try_from(end - start).unwrap_or_default());
        for line in start..end {
            if cancellation.is_cancelled() {
                return Err(LargeDocumentError::Cancelled);
            }
            if let Some(viewport_line) =
                read_line_window(&self.document, line, request.column_start, column_bytes)?
            {
                lines.push(viewport_line);
            }
        }
        Ok(ViewportSnapshot {
            generation: request.generation,
            requested_lines: start..end,
            exact_line_count: self.document.line_count(),
            lines,
        })
    }

    fn mark_changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

/// 普通 Editor 面向大文档的契约层：选择、编辑、history 与 viewport 共用源码字节坐标。
#[derive(Clone)]
pub struct LargeDocumentAdapter {
    backend: LargeDocumentBackend,
    selection: SourceSelection,
    /// 与 PieceTree 持久根逐项对应；正文和 Source selection 必须作为同一个
    /// 用户 transaction 撤销，且数量沿用 PieceTree 的固定生产上限。
    undo_selections: Vec<SourceSelection>,
    redo_selections: Vec<SourceSelection>,
}

impl LargeDocumentAdapter {
    pub fn new(document: PieceDocument) -> Self {
        Self {
            backend: LargeDocumentBackend::new(document),
            selection: SourceSelection::collapsed(0, SourceAffinity::Before),
            undo_selections: Vec::new(),
            redo_selections: Vec::new(),
        }
    }

    pub fn backend(&self) -> &LargeDocumentBackend {
        &self.backend
    }

    /// 当前 Source 内容代次。后台任务只能在代次仍一致时提交会修改正文的结果。
    pub fn revision(&self) -> u64 {
        self.backend.generation()
    }

    pub fn selection(&self) -> (Range<u64>, bool) {
        (self.selection.range(), self.selection.reversed())
    }

    pub fn source_selection(&self) -> SourceSelection {
        self.selection
    }

    pub fn set_selection(&mut self, range: Range<u64>, reversed: bool) {
        let len = self.backend.document.len();
        self.selection =
            SourceSelection::from_range(range.start.min(len)..range.end.min(len), reversed);
    }

    pub fn set_source_selection(&mut self, mut selection: SourceSelection) {
        let len = self.backend.document.len();
        selection.anchor.byte_offset = selection.anchor.byte_offset.min(len);
        selection.head.byte_offset = selection.head.byte_offset.min(len);
        self.selection = selection;
    }

    pub fn read_viewport(
        &self,
        request: &ViewportRequest,
    ) -> Result<ViewportSnapshot, LargeDocumentError> {
        self.backend.read_viewport(request)
    }

    pub fn read_viewport_cancellable(
        &self,
        request: &ViewportRequest,
        cancellation: &SearchCancellation,
    ) -> Result<ViewportSnapshot, LargeDocumentError> {
        self.backend
            .read_viewport_cancellable(request, cancellation)
    }

    pub fn len(&self) -> u64 {
        self.backend.document.len()
    }

    pub fn is_empty(&self) -> bool {
        self.backend.document.is_empty()
    }

    pub fn is_pristine(&self) -> bool {
        self.backend.document.is_pristine()
    }

    pub fn line_count(&self) -> u64 {
        self.backend.document.line_count()
    }

    pub fn line_range(&self, line: u64) -> Option<Range<u64>> {
        self.backend.document.line_range(line)
    }

    pub fn line_for_offset(&self, offset: u64) -> Option<u64> {
        self.backend.document.line_for_offset(offset)
    }

    pub fn line_index(&self) -> LineIndex {
        self.backend.document.line_index()
    }

    pub fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>, LargeDocumentError> {
        self.backend.document.read_range(range)
    }

    pub fn read_range_cancellable(
        &self,
        range: Range<u64>,
        cancellation: &SearchCancellation,
    ) -> Result<Vec<u8>, LargeDocumentError> {
        self.backend
            .document
            .read_range_cancellable(range, cancellation)
    }

    pub fn search(
        &self,
        query: &str,
        options: SearchOptions,
        cancellation: &SearchCancellation,
    ) -> Result<Vec<SearchMatch>, LargeDocumentError> {
        self.backend.document.search(query, options, cancellation)
    }

    pub fn external_change(&self) -> Result<ExternalChange, LargeDocumentError> {
        self.backend.document.external_change()
    }

    pub fn accept_external_append(
        &mut self,
        source: FileSource,
        index: LineIndex,
    ) -> Result<(), LargeDocumentError> {
        self.backend
            .document
            .accept_external_append(source, index)?;
        self.backend.mark_changed();
        self.clamp_selection();
        Ok(())
    }

    pub fn save_atomic_cancellable(
        &mut self,
        path: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<(), LargeDocumentError> {
        self.backend
            .document
            .save_atomic_cancellable(path, cancellation)?;
        self.backend.mark_changed();
        self.clamp_selection();
        Ok(())
    }

    pub fn save_encoded_atomic_cancellable(
        &self,
        plan: &EncodedSavePlan,
        path: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<FileIdentity, LargeDocumentError> {
        plan.save_atomic_cancellable(&self.backend.document, path, cancellation)
    }

    pub fn save_encoded_atomic_as_cancellable(
        &self,
        plan: &EncodedSavePlan,
        path: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<FileIdentity, LargeDocumentError> {
        plan.save_atomic_as_cancellable(&self.backend.document, path, cancellation)
    }

    pub fn save_range_atomic_cancellable(
        &self,
        range: Range<u64>,
        path: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<(), LargeDocumentError> {
        self.backend
            .document
            .save_range_atomic_cancellable(range, path, cancellation)
    }

    pub fn save_encoded_range_atomic_cancellable(
        &self,
        plan: &EncodedSavePlan,
        range: Range<u64>,
        path: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<FileIdentity, LargeDocumentError> {
        plan.save_range_atomic_as_cancellable(&self.backend.document, range, path, cancellation)
    }

    pub fn replace_text(
        &mut self,
        range: Range<u64>,
        replacement: &str,
    ) -> Result<(), LargeDocumentError> {
        let previous_selection = self.selection;
        self.backend
            .document
            .replace_text(range.clone(), replacement)?;
        self.record_undo_selection(previous_selection);
        self.redo_selections.clear();
        let caret = range.start.saturating_add(replacement.len() as u64);
        self.selection = SourceSelection::collapsed(caret, SourceAffinity::After);
        self.backend.mark_changed();
        Ok(())
    }

    /// 派生视图不得直接修改自己的 projection；只能提交带 base revision 的
    /// Source transaction。陈旧 revision 和重叠 range 在触碰正文前即被拒绝。
    pub fn apply_derived_edit(
        &mut self,
        edit: &crate::DerivedEdit,
    ) -> Result<(), LargeDocumentError> {
        if !edit.is_applicable_to(self.revision()) {
            return Err(LargeDocumentError::SourceChanged);
        }
        let edits = edit
            .edits
            .iter()
            .map(|edit| (edit.range.clone(), edit.replacement.clone()))
            .collect::<Vec<_>>();
        let previous_selection = self.selection;
        self.backend.document.replace_text_batch(&edits)?;
        if let Some(first) = edit.edits.iter().min_by_key(|edit| edit.range.start) {
            let caret = first
                .range
                .start
                .saturating_add(first.replacement.len() as u64);
            self.selection = SourceSelection::collapsed(caret, SourceAffinity::After);
        }
        if !edits.is_empty() {
            self.record_undo_selection(previous_selection);
            self.redo_selections.clear();
            self.backend.mark_changed();
        }
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        let changed = self.backend.document.undo();
        if changed {
            self.redo_selections.push(self.selection);
            if let Some(selection) = self.undo_selections.pop() {
                self.selection = selection;
            }
            self.backend.mark_changed();
            self.clamp_selection();
        }
        changed
    }

    pub fn redo(&mut self) -> bool {
        let changed = self.backend.document.redo();
        if changed {
            self.record_undo_selection(self.selection);
            if let Some(selection) = self.redo_selections.pop() {
                self.selection = selection;
            }
            self.backend.mark_changed();
            self.clamp_selection();
        }
        changed
    }

    fn clamp_selection(&mut self) {
        let len = self.backend.document.len();
        self.selection.anchor.byte_offset = self.selection.anchor.byte_offset.min(len);
        self.selection.head.byte_offset = self.selection.head.byte_offset.min(len);
    }

    fn record_undo_selection(&mut self, selection: SourceSelection) {
        if self.undo_selections.len() == crate::piece::DEFAULT_HISTORY_LIMIT {
            self.undo_selections.remove(0);
        }
        self.undo_selections.push(selection);
    }
}

impl From<PieceDocument> for LargeDocumentAdapter {
    fn from(document: PieceDocument) -> Self {
        Self::new(document)
    }
}

fn read_line_window(
    document: &PieceDocument,
    line: u64,
    requested_start: u64,
    maximum_bytes: u64,
) -> Result<Option<ViewportLine>, LargeDocumentError> {
    let Some(line_range) = document.line_range(line) else {
        return Ok(None);
    };
    let tail_start = line_range.end.saturating_sub(2).max(line_range.start);
    let tail = document.read_range(tail_start..line_range.end)?;
    let ending_len = if tail.ends_with(b"\r\n") {
        2
    } else if tail.ends_with(b"\n") || tail.ends_with(b"\r") {
        1
    } else {
        0
    };
    let content_end = line_range.end.saturating_sub(ending_len);
    let content_len = content_end.saturating_sub(line_range.start);
    let relative_start = requested_start.min(content_len.saturating_sub(maximum_bytes));
    let mut start = line_range.start.saturating_add(relative_start);
    if start > line_range.start && start < content_end {
        let probe_end = (start + 4).min(content_end);
        let probe = document.read_range(start..probe_end)?;
        start = start.saturating_add(
            probe
                .iter()
                .take_while(|byte| **byte & 0b1100_0000 == 0b1000_0000)
                .count() as u64,
        );
    }
    let requested_end = start.saturating_add(maximum_bytes).min(content_end);
    let mut bytes = document.read_range(start..requested_end)?;
    let mut end = requested_end;
    if let Err(error) = std::str::from_utf8(&bytes)
        && error.error_len().is_none()
    {
        bytes.truncate(error.valid_up_to());
        end = start.saturating_add(bytes.len() as u64);
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let ending = if end == content_end && ending_len > 0 {
        String::from_utf8_lossy(&tail[tail.len() - ending_len as usize..]).into_owned()
    } else {
        String::new()
    };
    Ok(Some(ViewportLine {
        line,
        source_range: line_range.clone(),
        content_range: start..end,
        text,
        ending,
        leading_truncated: start > line_range.start,
        trailing_truncated: end < content_end,
    }))
}
