// @author kongweiguang
use std::cmp::Reverse;

use crate::{ByteRange, SourceLanguage};

/// 折叠结构的领域分类，不绑定编辑器图标或可视化策略。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FoldKind {
    Syntax,
    Delimiter,
    Comment,
    MarkdownHeading,
    MarkdownFence,
    Indentation,
    TomlTable,
    BashKeyword,
    MermaidSubgraph,
    RubyKeyword,
    HtmlElement,
}

impl FoldKind {
    /// 为兼容适配器、持久折叠状态和日志提供的稳定分类名。
    pub const fn stable_name(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::Delimiter => "delimiter",
            Self::Comment => "comment",
            Self::MarkdownHeading => "heading",
            Self::MarkdownFence => "fence",
            Self::Indentation => "indent",
            Self::TomlTable => "table",
            Self::BashKeyword => "shell",
            Self::MermaidSubgraph => "subgraph",
            Self::RubyKeyword => "ruby",
            Self::HtmlElement => "element",
        }
    }
}

/// 一个可折叠源码结构。行号为零基且 `end_line` 包含闭合行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldRange {
    /// 在同一源码坐标与结构深度下稳定的身份；调用方可保存折叠状态。
    pub id: u64,
    pub kind: FoldKind,
    pub byte_range: ByteRange,
    pub start_line: usize,
    pub end_line: usize,
    pub depth: usize,
    /// 对成对分隔符保留闭合字符，Markdown/缩进等没有该信息。
    pub closing: Option<char>,
}

impl FoldRange {
    /// 折叠后被隐藏的行数，不包含标题所在的首行。
    pub fn hidden_line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line)
    }
}

/// 解析窗口相对于完整文档的坐标原点，避免在辅助函数间传递松散参数。
#[derive(Clone, Copy)]
pub(crate) struct FoldCoordinates {
    pub(crate) byte_base: u64,
    pub(crate) line_base: usize,
}

/// 在完整源码中发现可折叠区间。
pub fn fold_ranges(language: SourceLanguage, source: &str) -> Vec<FoldRange> {
    fold_ranges_in_window(language, source, 0, 0)
}

/// 在源码窗口中发现可折叠区间，并投影到调用方提供的真实 byte/行基址。
pub fn fold_ranges_in_window(
    language: SourceLanguage,
    source: &str,
    byte_base: u64,
    line_base: usize,
) -> Vec<FoldRange> {
    if !language.supports_folding() || source.is_empty() {
        return Vec::new();
    }

    let starts = line_starts(source);
    let coordinates = FoldCoordinates {
        byte_base,
        line_base,
    };
    #[cfg(feature = "code-highlight-core")]
    let ranges = tree_sitter_ranges(language, source, &starts, coordinates);
    #[cfg(not(feature = "code-highlight-core"))]
    let ranges = Vec::new();
    finish_fold_ranges(language, source, &starts, coordinates, ranges)
}

fn finish_fold_ranges(
    language: SourceLanguage,
    source: &str,
    starts: &[usize],
    coordinates: FoldCoordinates,
    mut ranges: Vec<FoldRange>,
) -> Vec<FoldRange> {
    ranges.extend(delimiter_ranges(language, source, starts, coordinates));
    match language {
        SourceLanguage::Markdown => {
            ranges.extend(crate::folding_structural::markdown_ranges(
                source,
                starts,
                coordinates,
            ));
        }
        SourceLanguage::Python | SourceLanguage::Yaml => {
            ranges.extend(crate::folding_structural::indentation_ranges(
                source,
                starts,
                coordinates,
            ));
        }
        SourceLanguage::Toml => {
            ranges.extend(crate::folding_structural::toml_ranges(
                source,
                starts,
                coordinates,
            ));
        }
        SourceLanguage::Bash | SourceLanguage::Mermaid | SourceLanguage::Ruby => {
            ranges.extend(crate::folding_structural::keyword_ranges(
                language,
                source,
                starts,
                coordinates,
            ));
        }
        SourceLanguage::Html => {
            ranges.extend(crate::folding_structural::html_ranges(
                source,
                starts,
                coordinates,
            ));
        }
        _ => {}
    }
    normalize_ranges(&mut ranges);
    ranges
}

#[cfg(feature = "code-highlight-core")]
fn tree_sitter_ranges(
    language: SourceLanguage,
    source: &str,
    starts: &[usize],
    coordinates: FoldCoordinates,
) -> Vec<FoldRange> {
    let Some(grammar) = crate::language::tree_sitter_language(language) else {
        return Vec::new();
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };

    tree_sitter_ranges_from_tree(language, source, starts, coordinates, &tree)
}

#[cfg(feature = "code-highlight-core")]
pub(crate) fn fold_ranges_for_tree(
    language: SourceLanguage,
    source: &str,
    tree: &tree_sitter::Tree,
) -> Vec<FoldRange> {
    if !language.supports_folding() || source.is_empty() {
        return Vec::new();
    }
    let starts = line_starts(source);
    let coordinates = FoldCoordinates {
        byte_base: 0,
        line_base: 0,
    };
    let ranges = tree_sitter_ranges_from_tree(language, source, &starts, coordinates, tree);
    finish_fold_ranges(language, source, &starts, coordinates, ranges)
}

#[cfg(feature = "code-highlight-core")]
fn tree_sitter_ranges_from_tree(
    language: SourceLanguage,
    source: &str,
    starts: &[usize],
    coordinates: FoldCoordinates,
    tree: &tree_sitter::Tree,
) -> Vec<FoldRange> {
    let mut output = Vec::new();
    let mut pending = vec![tree.root_node()];
    while let Some(node) = pending.pop() {
        if !node.is_error()
            && !node.is_missing()
            && !node.has_error()
            && foldable_tree_sitter_kind(language, node.kind())
        {
            let closing = source
                .as_bytes()
                .get(node.end_byte().saturating_sub(1))
                .copied()
                .filter(u8::is_ascii)
                .map(char::from)
                .filter(|character| matches!(character, '}' | ']' | ')' | '>'));
            push_fold(
                &mut output,
                starts,
                coordinates,
                node.start_byte(),
                node.end_byte(),
                FoldKind::Syntax,
                closing,
            );
        }

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                pending.push(cursor.node());
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
    output
}

#[cfg(feature = "code-highlight-core")]
fn foldable_tree_sitter_kind(language: SourceLanguage, kind: &str) -> bool {
    if matches!(kind, "comment" | "block_comment") {
        return true;
    }
    match language {
        SourceLanguage::Json | SourceLanguage::JsonLines => matches!(kind, "object" | "array"),
        SourceLanguage::Markdown => matches!(
            kind,
            "section"
                | "fenced_code_block"
                | "block_quote"
                | "list"
                | "minus_metadata"
                | "plus_metadata"
        ),
        SourceLanguage::Html => matches!(kind, "element" | "script_element" | "style_element"),
        SourceLanguage::Css => kind.ends_with("block") || kind.ends_with("rule_set"),
        SourceLanguage::Python => matches!(
            kind,
            "class_definition"
                | "function_definition"
                | "if_statement"
                | "for_statement"
                | "while_statement"
                | "try_statement"
                | "with_statement"
                | "match_statement"
                | "list"
                | "dictionary"
                | "set"
        ),
        SourceLanguage::Yaml => matches!(
            kind,
            "block_mapping" | "block_sequence" | "block_node" | "block_scalar" | "document"
        ),
        SourceLanguage::Toml => matches!(kind, "table" | "table_array_element" | "array"),
        _ => {
            kind.ends_with("body")
                || kind.ends_with("block")
                || kind.ends_with("declaration")
                || kind.ends_with("definition")
                || matches!(
                    kind,
                    "class"
                        | "module"
                        | "namespace"
                        | "function_item"
                        | "impl_item"
                        | "trait_item"
                        | "object"
                        | "array"
                        | "compound_statement"
                )
        }
    }
}

fn delimiter_ranges(
    language: SourceLanguage,
    source: &str,
    starts: &[usize],
    coordinates: FoldCoordinates,
) -> Vec<FoldRange> {
    let bytes = source.as_bytes();
    let mut stack = Vec::<(u8, usize)>::new();
    let mut output = Vec::new();
    let mut index = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;
    let mut block_comment_start = 0;

    while let Some(&byte) = bytes.get(index) {
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }
        if block_comment {
            if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                block_comment = false;
                push_fold(
                    &mut output,
                    starts,
                    coordinates,
                    block_comment_start,
                    index + 2,
                    FoldKind::Comment,
                    None,
                );
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active {
                quote = None;
            }
            index += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"' | b'`') {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            line_comment = true;
            index += 2;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            block_comment = true;
            block_comment_start = index;
            index += 2;
            continue;
        }
        if byte == b'#'
            && matches!(
                language,
                SourceLanguage::Python
                    | SourceLanguage::Yaml
                    | SourceLanguage::Toml
                    | SourceLanguage::Bash
            )
        {
            line_comment = true;
            index += 1;
            continue;
        }

        match byte {
            b'{' | b'[' => stack.push((byte, index)),
            b'}' | b']' => {
                let open = if byte == b'}' { b'{' } else { b'[' };
                if let Some(position) = stack.iter().rposition(|(candidate, _)| *candidate == open)
                {
                    let (_, start) = stack.remove(position);
                    push_fold(
                        &mut output,
                        starts,
                        coordinates,
                        start,
                        index + 1,
                        FoldKind::Delimiter,
                        Some(char::from(byte)),
                    );
                }
            }
            _ => {}
        }
        index += 1;
    }
    output
}

pub(crate) fn push_fold(
    output: &mut Vec<FoldRange>,
    starts: &[usize],
    coordinates: FoldCoordinates,
    start: usize,
    end: usize,
    kind: FoldKind,
    closing: Option<char>,
) {
    if start >= end {
        return;
    }
    let start_line = line_for_offset(starts, start).saturating_add(coordinates.line_base);
    let end_line =
        line_for_offset(starts, end.saturating_sub(1)).saturating_add(coordinates.line_base);
    if end_line <= start_line {
        return;
    }
    let Ok(start) = u64::try_from(start) else {
        return;
    };
    let Ok(end) = u64::try_from(end) else {
        return;
    };
    let Some(start) = coordinates.byte_base.checked_add(start) else {
        return;
    };
    let Some(end) = coordinates.byte_base.checked_add(end) else {
        return;
    };
    let Ok(byte_range) = ByteRange::new(start, end) else {
        return;
    };
    output.push(FoldRange {
        id: 0,
        kind,
        byte_range,
        start_line,
        end_line,
        depth: 0,
        closing,
    });
}

fn normalize_ranges(ranges: &mut Vec<FoldRange>) {
    ranges.sort_by_key(|range| {
        (
            range.start_line,
            Reverse(range.end_line),
            range.byte_range.start(),
        )
    });
    ranges.dedup_by(|right, left| {
        right.kind == left.kind
            && right.byte_range == left.byte_range
            && right.start_line == left.start_line
            && right.end_line == left.end_line
    });

    let mut stack = Vec::<usize>::new();
    for range in ranges {
        while stack.last().is_some_and(|end| *end < range.end_line) {
            stack.pop();
        }
        range.depth = stack.len();
        range.id = stable_fold_id(range.kind, range.byte_range.start(), range.depth);
        stack.push(range.end_line);
    }
}

fn stable_fold_id(kind: FoldKind, start: u64, depth: usize) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let depth = u64::try_from(depth).unwrap_or(u64::MAX);
    for byte in kind
        .stable_name()
        .bytes()
        .chain(start.to_le_bytes())
        .chain(depth.to_le_bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub(crate) fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(source.match_indices('\n').map(|(index, _)| index + 1));
    starts
}

fn line_for_offset(starts: &[usize], offset: usize) -> usize {
    starts
        .partition_point(|start| *start <= offset)
        .saturating_sub(1)
}

pub(crate) fn line_end(starts: &[usize], source_len: usize, line: usize) -> usize {
    starts.get(line + 1).copied().unwrap_or(source_len)
}
