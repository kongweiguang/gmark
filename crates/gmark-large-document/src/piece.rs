// @author kongweiguang

use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gpui_sum_tree::{Bias, ContextLessSummary, Dimension, Dimensions, Item, SumTree};
use regex_automata::hybrid::{dfa::DFA, regex::Regex as StreamingRegex};
use regex_automata::{Anchored, Input};

use crate::{FileSource, LargeDocumentError, LineIndex, SourceAffinity, SourceAnchor};

const ADDITION_MEMORY_LIMIT: u64 = 64 * 1024 * 1024;
const SEARCH_CHUNK_BYTES: u64 = 256 * 1024;
// 大文件公开回归要求至少 512 次碎片编辑可完整撤销；1,024 在保留该契约的同时
// 给持久 PieceTree 根数量提供与会话时长无关的硬上界。
pub(crate) const DEFAULT_HISTORY_LIMIT: usize = 1_024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    pub range: Range<u64>,
    pub anchor: SourceAnchor,
    pub head: SourceAnchor,
}

impl SearchMatch {
    fn new(range: Range<u64>) -> Self {
        Self {
            anchor: SourceAnchor::new(range.start, SourceAffinity::Before),
            head: SourceAnchor::new(range.end, SourceAffinity::After),
            range,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalChange {
    Unchanged,
    Appended { from: u64, to: u64 },
    Truncated { len: u64 },
    Replaced,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub result_limit: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            whole_word: false,
            regex: false,
            result_limit: 10_000,
        }
    }
}

#[path = "piece_parts/document.rs"]
mod document;

#[derive(Clone, Default)]
pub struct SearchCancellation {
    cancelled: Arc<AtomicBool>,
}

impl SearchCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// 在完整行索引完成前直接扫描 immutable 磁盘源。临时 PieceTree 只有一个 base piece，
/// 不读取正文也不承诺行坐标；搜索结果始终使用稳定的 source byte offset。
pub fn search_file_source(
    source: &FileSource,
    query: &str,
    options: SearchOptions,
    cancellation: &SearchCancellation,
) -> Result<Vec<SearchMatch>, LargeDocumentError> {
    let identity = source.identity()?;
    let len = identity.len;
    let document = PieceDocument {
        source: Some(source.clone()),
        base_identity: identity,
        base_sample: source.sampled_prefix_hash(len)?,
        base_index: LineIndex::default(),
        pieces: PieceTree::from_iter((len > 0).then_some(Piece {
            source: PieceSource::Base,
            range: 0..len,
            // 搜索只遍历 piece 字节范围，不依赖换行摘要。
            newlines: 0,
        })),
        additions: AppendStore::default(),
        len,
        undo: Vec::new(),
        redo: Vec::new(),
    };
    document.search(query, options, cancellation)
}

#[derive(Clone)]
pub struct PieceDocument {
    source: Option<FileSource>,
    base_identity: crate::FileIdentity,
    base_sample: u32,
    base_index: LineIndex,
    pieces: PieceTree,
    additions: AppendStore,
    len: u64,
    undo: Vec<(PieceTree, u64)>,
    redo: Vec<(PieceTree, u64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PieceSource {
    Base,
    Add,
}

#[derive(Debug)]
struct Piece {
    source: PieceSource,
    range: Range<u64>,
    newlines: u64,
}

#[cfg(test)]
thread_local! {
    static PIECE_CLONE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Clone for Piece {
    fn clone(&self) -> Self {
        #[cfg(test)]
        PIECE_CLONE_COUNT.with(|count| count.set(count.get() + 1));
        Self {
            source: self.source,
            range: self.range.clone(),
            newlines: self.newlines,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct PieceSummary {
    bytes: u64,
    newlines: u64,
    piece_count: u64,
}

impl ContextLessSummary for PieceSummary {
    fn zero() -> Self {
        Self::default()
    }

    fn add_summary(&mut self, summary: &Self) {
        self.bytes += summary.bytes;
        self.newlines += summary.newlines;
        self.piece_count += summary.piece_count;
    }
}

impl Item for Piece {
    type Summary = PieceSummary;

    fn summary(&self, (): ()) -> Self::Summary {
        PieceSummary {
            bytes: self.len(),
            newlines: self.newlines,
            piece_count: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Bytes(u64);

impl<'a> Dimension<'a, PieceSummary> for Bytes {
    fn zero((): ()) -> Self {
        Self(0)
    }

    fn add_summary(&mut self, summary: &'a PieceSummary, (): ()) {
        self.0 += summary.bytes;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Newlines(u64);

impl<'a> Dimension<'a, PieceSummary> for Newlines {
    fn zero((): ()) -> Self {
        Self(0)
    }

    fn add_summary(&mut self, summary: &'a PieceSummary, (): ()) {
        self.0 += summary.newlines;
    }
}

/// PieceTree 的根由 Arc 持有，clone 只复制根指针；编辑只复制 split/concat 路径。
/// 摘要同时承担字节、换行和碎片计数，避免高碎片文档退回全量线性定位。
#[derive(Clone, Debug)]
struct PieceTree {
    root: SumTree<Piece>,
}

impl Default for PieceTree {
    fn default() -> Self {
        Self {
            root: SumTree::new(()),
        }
    }
}

impl PieceTree {
    fn from_piece(piece: Piece) -> Self {
        Self {
            root: SumTree::from_item(piece, ()),
        }
    }

    fn from_iter(pieces: impl IntoIterator<Item = Piece>) -> Self {
        let mut tree = Self::default();
        for piece in pieces {
            tree.push(piece);
        }
        tree
    }

    fn is_empty(&self) -> bool {
        self.root.is_empty()
    }

    fn piece_count(&self) -> u64 {
        self.root.summary().piece_count
    }

    fn first(&self) -> Option<&Piece> {
        self.root.first()
    }

    fn push(&mut self, piece: Piece) {
        if piece.len() == 0 {
            return;
        }
        self.append(Self::from_piece(piece));
    }

    /// 只在物理来源连续时合并边界 Piece；这既限制碎片增长，也不会错误重排
    /// 追加缓冲中按时间写入、但在逻辑文档中倒序出现的片段。
    fn append(&mut self, mut other: Self) {
        if other.is_empty() {
            return;
        }
        if self.is_empty() {
            *self = other;
            return;
        }

        let first = other.first().cloned();
        let should_merge = self
            .root
            .last()
            .zip(first.as_ref())
            .is_some_and(|(last, first)| {
                last.source == first.source && last.range.end == first.range.start
            });
        if should_merge {
            let first = first.expect("non-empty tree has a first piece");
            self.root.update_last(
                |last| {
                    last.range.end = first.range.end;
                    last.newlines += first.newlines;
                },
                (),
            );
            let suffix = {
                let mut cursor = other.root.cursor::<()>(());
                cursor.next();
                cursor.next();
                cursor.suffix()
            };
            other.root = suffix;
        }
        self.root.append(other.root, ());
    }

    #[cfg(test)]
    fn root_identity(&self) -> *const PieceSummary {
        self.root.summary() as *const PieceSummary
    }
}

#[derive(Clone, Default)]
struct AppendStore {
    inner: Arc<Mutex<AppendStoreInner>>,
}

#[derive(Default)]
struct AppendStoreInner {
    memory: Vec<u8>,
    spill: Option<tempfile::NamedTempFile>,
    len: u64,
}

impl AppendStore {
    fn append(&self, bytes: &[u8]) -> Result<Range<u64>, LargeDocumentError> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let start = inner.len;
        let end = start
            .checked_add(bytes.len() as u64)
            .ok_or(LargeDocumentError::RangeTooLarge)?;
        if inner.spill.is_none() && end > ADDITION_MEMORY_LIMIT {
            let mut spill =
                tempfile::NamedTempFile::new().map_err(|source| LargeDocumentError::Io {
                    path: std::env::temp_dir(),
                    source,
                })?;
            spill
                .write_all(&inner.memory)
                .map_err(|source| LargeDocumentError::Io {
                    path: spill.path().to_path_buf(),
                    source,
                })?;
            inner.memory.clear();
            inner.memory.shrink_to_fit();
            inner.spill = Some(spill);
        }
        if let Some(spill) = inner.spill.as_mut() {
            let path = spill.path().to_path_buf();
            spill
                .seek(SeekFrom::End(0))
                .map_err(|source| LargeDocumentError::Io {
                    path: path.clone(),
                    source,
                })?;
            spill
                .write_all(bytes)
                .map_err(|source| LargeDocumentError::Io { path, source })?;
        } else {
            inner.memory.extend_from_slice(bytes);
        }
        inner.len = end;
        Ok(start..end)
    }

    fn read(&self, range: Range<u64>) -> Result<Vec<u8>, LargeDocumentError> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if range.start > range.end || range.end > inner.len {
            return Err(LargeDocumentError::InvalidRange {
                start: range.start,
                end: range.end,
                len: inner.len,
            });
        }
        let len = usize::try_from(range.end - range.start)
            .map_err(|_| LargeDocumentError::RangeTooLarge)?;
        if let Some(spill) = inner.spill.as_mut() {
            let path = spill.path().to_path_buf();
            spill
                .seek(SeekFrom::Start(range.start))
                .map_err(|source| LargeDocumentError::Io {
                    path: path.clone(),
                    source,
                })?;
            let mut output = vec![0; len];
            spill
                .read_exact(&mut output)
                .map_err(|source| LargeDocumentError::Io { path, source })?;
            Ok(output)
        } else {
            let start =
                usize::try_from(range.start).map_err(|_| LargeDocumentError::RangeTooLarge)?;
            let end = usize::try_from(range.end).map_err(|_| LargeDocumentError::RangeTooLarge)?;
            inner.memory.get(start..end).map(ToOwned::to_owned).ok_or(
                LargeDocumentError::InvalidRange {
                    start: range.start,
                    end: range.end,
                    len: inner.len,
                },
            )
        }
    }
}

impl Piece {
    fn len(&self) -> u64 {
        self.range.end - self.range.start
    }
}

struct PieceCursor<'a> {
    document: &'a PieceDocument,
    cursor: gpui_sum_tree::Cursor<'a, 'static, Piece, Bytes>,
    offset: u64,
}

impl<'a> PieceCursor<'a> {
    fn new(document: &'a PieceDocument, offset: u64) -> Self {
        let mut cursor = document.pieces.root.cursor::<Bytes>(());
        cursor.seek(&Bytes(offset), Bias::Right);
        Self {
            document,
            cursor,
            offset,
        }
    }

    fn seek_forward(&mut self, offset: u64) {
        debug_assert!(offset >= self.offset);
        debug_assert!(offset <= self.document.len);
        self.cursor.seek_forward(&Bytes(offset), Bias::Right);
        self.offset = offset;
    }

    /// SumTree 在 item 边界共享完整子树；首尾 Piece 若只命中一部分，则仅重建
    /// 这两个边界 item，绝不复制区间内的全部 Piece。
    fn slice(&mut self, end_offset: u64) -> Result<PieceTree, LargeDocumentError> {
        debug_assert!(end_offset >= self.offset);
        debug_assert!(end_offset <= self.document.len);
        let mut slice = PieceTree::default();
        if let Some(start_piece) = self.cursor.item() {
            let piece_start = self.cursor.start().0;
            let piece_end = self.cursor.end().0;
            let relative_start = self.offset - piece_start;
            let relative_end = end_offset.min(piece_end) - piece_start;
            if relative_start < relative_end {
                slice.push(
                    self.document
                        .slice_piece(start_piece, relative_start..relative_end)?,
                );
            }

            if end_offset > piece_end {
                self.cursor.next();
                slice.append(PieceTree {
                    root: self.cursor.slice(&Bytes(end_offset), Bias::Right),
                });
                if let Some(end_piece) = self.cursor.item() {
                    let relative_end = end_offset - self.cursor.start().0;
                    if relative_end > 0 {
                        slice.push(self.document.slice_piece(end_piece, 0..relative_end)?);
                    }
                }
            }
        }
        self.offset = end_offset;
        Ok(slice)
    }
}

impl PieceDocument {
    pub fn open(source: FileSource, index: LineIndex) -> Result<Self, LargeDocumentError> {
        let len = source.identity()?.len;
        let pieces = PieceTree::from_iter((len > 0).then_some(Piece {
            source: PieceSource::Base,
            range: 0..len,
            newlines: index.newline_count(),
        }));
        Ok(Self {
            base_identity: source.identity()?,
            base_sample: source.sampled_prefix_hash(len)?,
            base_index: index,
            source: Some(source),
            pieces,
            additions: AppendStore::default(),
            len,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    /// 是否已经回到当前磁盘基线。追加缓冲可能仍为 undo/redo 保留历史数据，
    /// 因此只比较逻辑 piece 根，不能用追加缓冲是否为空判断脏状态。
    pub fn is_pristine(&self) -> bool {
        if self.base_identity.len == 0 {
            return self.len == 0 && self.pieces.is_empty();
        }
        self.len == self.base_identity.len
            && self.pieces.piece_count() == 1
            && matches!(
                self.pieces.first(),
                Some(
                Piece {
                    source: PieceSource::Base,
                    range,
                    ..
                }) if range.start == 0 && range.end == self.base_identity.len
            )
    }

    /// 判断逻辑文档是否为空，避免调用方依赖内部长度表示。
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn line_index(&self) -> LineIndex {
        self.base_index.clone()
    }

    pub fn line_for_offset(&self, offset: u64) -> Option<u64> {
        if offset > self.len {
            return None;
        }
        let mut low = 0u64;
        let mut high = self.line_count();
        while low < high {
            let middle = low + (high - low) / 2;
            match self.logical_newline_offset(middle) {
                Some(newline) if newline <= offset => low = middle + 1,
                _ => high = middle,
            }
        }
        Some(low.min(self.line_count().saturating_sub(1)))
    }

    pub fn external_change(&self) -> Result<ExternalChange, LargeDocumentError> {
        let current_source = FileSource::open(&self.base_identity.path)?;
        let current = current_source.identity()?;
        let same_file = current.os_file_id == self.base_identity.os_file_id;
        if !same_file {
            return Ok(ExternalChange::Replaced);
        }
        if current.len > self.base_identity.len {
            if current_source.sampled_prefix_hash(self.base_identity.len)? != self.base_sample {
                return Ok(ExternalChange::Modified);
            }
            return Ok(ExternalChange::Appended {
                from: self.base_identity.len,
                to: current.len,
            });
        }
        if current.len < self.base_identity.len {
            return Ok(ExternalChange::Truncated { len: current.len });
        }
        if current.modified_nanos != self.base_identity.modified_nanos {
            return Ok(ExternalChange::Modified);
        }
        Ok(ExternalChange::Unchanged)
    }
}

fn search_failure(error: impl std::fmt::Display) -> LargeDocumentError {
    LargeDocumentError::Search(error.to_string())
}

fn missing_regex_start() -> LargeDocumentError {
    LargeDocumentError::Search("reverse regex scan ended without a matching start".to_owned())
}

#[cfg(test)]
#[path = "../tests/unit/piece.rs"]
mod piece_tree_tests;
