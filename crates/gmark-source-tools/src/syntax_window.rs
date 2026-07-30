// @author kongweiguang

//! 有界多行高亮窗口，供虚拟化 Source 行共享语法上下文。

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

use crate::{ByteRange, HighlightResult, HighlightSpan, SourceLanguage, highlight_source};

const MAX_SOURCE_SYNTAX_CONTEXT_BYTES: usize = 1024 * 1024;

/// Source 行共享的有界多行解析窗口。普通渲染直接切片缓存结果；活动行编辑时只替换
/// 当前行后重跑同一窗口，避免退回无上下文的单行解析。
#[derive(Clone, Debug)]
pub struct SourceSyntaxContext {
    window: Arc<SourceSyntaxWindow>,
    row_range: Range<usize>,
}

#[derive(Debug)]
struct SourceSyntaxWindow {
    language: SourceLanguage,
    source: Arc<str>,
    highlight: HighlightResult,
}

impl SourceSyntaxContext {
    /// 判断两份上下文是否指向同一窗口中的同一行，供 UI 避免重复刷新缓存。
    pub fn same_identity(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.window, &other.window) && self.row_range == other.row_range
    }

    /// 返回当前行在完整语法上下文中的高亮；编辑态会在相同窗口重跑解析。
    pub fn highlight(&self, current_row: &str) -> HighlightResult {
        let original_row = self
            .window
            .source
            .get(self.row_range.clone())
            .unwrap_or_default();
        if original_row == current_row {
            return slice_highlight(&self.window.highlight, self.row_range.clone());
        }

        let mut source = String::with_capacity(
            self.window
                .source
                .len()
                .saturating_sub(self.row_range.len())
                .saturating_add(current_row.len()),
        );
        source.push_str(
            self.window
                .source
                .get(..self.row_range.start)
                .unwrap_or_default(),
        );
        let row_start = source.len();
        source.push_str(current_row);
        let row_range = row_start..source.len();
        source.push_str(
            self.window
                .source
                .get(self.row_range.end..)
                .unwrap_or_default(),
        );
        let highlight = highlight_source(self.window.language, &source);
        slice_highlight(&highlight, row_range)
    }
}

/// 把连续 Source 行按 1 MiB 上限组成共享解析窗口。窗口只高亮一次，各行仅保留
/// `Arc` 和自己的局部范围，因此虚拟化行数增长不会重复解析或复制整段源码。
pub fn build_source_syntax_contexts<'a>(
    language: SourceLanguage,
    rows: impl IntoIterator<Item = (usize, &'a str)>,
) -> BTreeMap<usize, SourceSyntaxContext> {
    let mut contexts = BTreeMap::new();
    let mut source = String::new();
    let mut ranges = Vec::new();
    let mut previous_line = None;

    for (line, text) in rows {
        let is_contiguous = previous_line.is_none_or(|previous| line == previous + 1);
        let separator_len = usize::from(!source.is_empty());
        let exceeds_budget = !source.is_empty()
            && source
                .len()
                .saturating_add(separator_len)
                .saturating_add(text.len())
                > MAX_SOURCE_SYNTAX_CONTEXT_BYTES;
        if !is_contiguous || exceeds_budget {
            flush_context(language, &mut source, &mut ranges, &mut contexts);
        }
        if !source.is_empty() {
            source.push('\n');
        }
        let start = source.len();
        source.push_str(text);
        ranges.push((line, start..source.len()));
        previous_line = Some(line);
    }
    flush_context(language, &mut source, &mut ranges, &mut contexts);
    contexts
}

fn flush_context(
    language: SourceLanguage,
    source: &mut String,
    ranges: &mut Vec<(usize, Range<usize>)>,
    contexts: &mut BTreeMap<usize, SourceSyntaxContext>,
) {
    if ranges.is_empty() {
        return;
    }
    let source = Arc::<str>::from(std::mem::take(source));
    let highlight = highlight_source(language, &source);
    let window = Arc::new(SourceSyntaxWindow {
        language,
        source,
        highlight,
    });
    for (line, row_range) in ranges.drain(..) {
        contexts.insert(
            line,
            SourceSyntaxContext {
                window: Arc::clone(&window),
                row_range,
            },
        );
    }
}

fn slice_highlight(highlight: &HighlightResult, range: Range<usize>) -> HighlightResult {
    let spans = highlight
        .spans
        .iter()
        .filter_map(|span| {
            let start = usize::try_from(span.range.start()).ok()?.max(range.start);
            let end = usize::try_from(span.range.end()).ok()?.min(range.end);
            let local_start = u64::try_from(start.saturating_sub(range.start)).ok()?;
            let local_end = u64::try_from(end.saturating_sub(range.start)).ok()?;
            let range = (start < end)
                .then(|| ByteRange::new(local_start, local_end))?
                .ok()?;
            Some(HighlightSpan {
                range,
                class: span.class,
            })
        })
        .collect();
    HighlightResult {
        language: highlight.language,
        spans,
        engine: highlight.engine,
    }
}
