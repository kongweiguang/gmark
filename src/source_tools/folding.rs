// @author kongweiguang

use std::collections::{BTreeSet, HashSet};
use std::ops::Range;

use super::SourceLanguageId;

/// 一个可折叠结构使用真实源码坐标；UI 只能派生可见行，不能改写这些坐标。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FoldRegion {
    pub(crate) id: u64,
    pub(crate) kind: &'static str,
    pub(crate) byte_range: Range<u64>,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) depth: usize,
    /// 从语法根开始的子节点序号路径；格式化后用它恢复仍对应同一结构的折叠状态。
    pub(crate) structure_path: Vec<u32>,
    pub(crate) closing: Option<char>,
}

impl FoldRegion {
    pub(crate) fn hidden_line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line)
    }
}

#[derive(Clone, Debug)]
struct EffectiveFold {
    start: usize,
    end: usize,
    hidden_before: usize,
    visible_start: usize,
}

/// 折叠状态与行投影。嵌套折叠状态会保留，但只有最外层折叠参与行数映射。
#[derive(Clone, Debug, Default)]
pub(crate) struct FoldProjectionIndex {
    real_line_count: usize,
    regions: Vec<FoldRegion>,
    collapsed: BTreeSet<u64>,
    effective: Vec<EffectiveFold>,
    hidden_total: usize,
}

/// Resident 文档持有的增量语法树。Paged 窗口彼此不连续，继续使用独立有界解析。
#[derive(Default)]
pub(crate) struct ResidentFoldParser {
    document_epoch: Option<u64>,
    language: Option<SourceLanguageId>,
    source: String,
    #[cfg(feature = "code-highlight-core")]
    tree: Option<tree_sitter::Tree>,
    last_parse_incremental: bool,
}

impl ResidentFoldParser {
    pub(crate) fn parse(
        &mut self,
        document_epoch: u64,
        language: SourceLanguageId,
        source: &str,
    ) -> Vec<FoldRegion> {
        #[cfg(feature = "code-highlight-core")]
        {
            let same_document =
                self.document_epoch == Some(document_epoch) && self.language == Some(language);
            let edit = same_document
                .then(|| incremental_input_edit(&self.source, source))
                .flatten();
            let full_replacement = edit.as_ref().is_some_and(|edit| {
                edit.start_byte == 0
                    && edit.old_end_byte == self.source.len()
                    && edit.new_end_byte == source.len()
            });
            if !same_document || full_replacement {
                self.tree = None;
            } else if let (Some(tree), Some(edit)) = (self.tree.as_mut(), edit.as_ref()) {
                tree.edit(edit);
            }

            self.last_parse_incremental = self.tree.is_some();
            self.document_epoch = Some(document_epoch);
            self.language = Some(language);
            self.source.clear();
            self.source.push_str(source);

            let Some(grammar) = tree_sitter_language(language) else {
                self.tree = None;
                return discover_fold_regions(language, source, 0, 0);
            };
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&grammar).is_err() {
                self.tree = None;
                return discover_fold_regions(language, source, 0, 0);
            }
            let Some(tree) = parser.parse(source, self.tree.as_ref()) else {
                self.tree = None;
                return discover_fold_regions(language, source, 0, 0);
            };
            let starts = line_starts(source);
            let syntax = tree_sitter_regions_from_tree(language, source, &starts, 0, 0, &tree);
            self.tree = Some(tree);
            finish_fold_regions(language, source, &starts, 0, 0, syntax)
        }

        #[cfg(not(feature = "code-highlight-core"))]
        {
            self.document_epoch = Some(document_epoch);
            self.language = Some(language);
            self.source.clear();
            self.source.push_str(source);
            self.last_parse_incremental = false;
            discover_fold_regions(language, source, 0, 0)
        }
    }

    #[cfg(test)]
    fn last_parse_was_incremental(&self) -> bool {
        self.last_parse_incremental
    }
}

impl FoldProjectionIndex {
    pub(crate) fn set_real_line_count(&mut self, real_line_count: usize) {
        if self.real_line_count != real_line_count {
            self.real_line_count = real_line_count;
            self.rebuild();
        }
    }

    pub(crate) fn set_regions(&mut self, real_line_count: usize, mut regions: Vec<FoldRegion>) {
        let collapsed_structure = self
            .regions
            .iter()
            .filter(|region| self.collapsed.contains(&region.id))
            .map(|region| (region.kind, region.structure_path.clone()))
            .collect::<HashSet<_>>();
        regions.sort_by_key(|region| (region.start_line, std::cmp::Reverse(region.end_line)));
        let current = regions
            .iter()
            .map(|region| region.id)
            .collect::<HashSet<_>>();
        let mut next_collapsed = self
            .collapsed
            .iter()
            .filter(|id| current.contains(id))
            .copied()
            .collect::<BTreeSet<_>>();
        for region in &regions {
            if collapsed_structure.contains(&(region.kind, region.structure_path.clone())) {
                next_collapsed.insert(region.id);
            }
        }
        self.collapsed = next_collapsed;
        self.real_line_count = real_line_count;
        self.regions = regions;
        self.rebuild();
    }

    pub(crate) fn replace_window_regions(
        &mut self,
        real_line_count: usize,
        window: Range<usize>,
        regions: Vec<FoldRegion>,
    ) {
        let mut merged = self
            .regions
            .iter()
            .filter(|region| region.end_line < window.start || region.start_line >= window.end)
            .cloned()
            .collect::<Vec<_>>();
        merged.extend(regions);
        self.set_regions(real_line_count, merged);
    }

    pub(crate) fn regions(&self) -> &[FoldRegion] {
        &self.regions
    }

    pub(crate) fn region_starting(&self, line: usize) -> Option<&FoldRegion> {
        self.regions
            .iter()
            .filter(|region| region.start_line == line)
            .max_by_key(|region| region.end_line)
    }

    pub(crate) fn is_collapsed(&self, id: u64) -> bool {
        self.collapsed.contains(&id)
    }

    pub(crate) fn toggle(&mut self, id: u64) -> bool {
        if !self.regions.iter().any(|region| region.id == id) {
            return false;
        }
        if !self.collapsed.remove(&id) {
            self.collapsed.insert(id);
        }
        self.rebuild();
        true
    }

    pub(crate) fn set_collapsed(&mut self, id: u64, collapsed: bool) -> bool {
        if !self.regions.iter().any(|region| region.id == id) {
            return false;
        }
        let changed = if collapsed {
            self.collapsed.insert(id)
        } else {
            self.collapsed.remove(&id)
        };
        if changed {
            self.rebuild();
        }
        changed
    }

    pub(crate) fn collapse_all(&mut self) {
        self.collapsed = self.regions.iter().map(|region| region.id).collect();
        self.rebuild();
    }

    pub(crate) fn expand_all(&mut self) {
        self.collapsed.clear();
        self.rebuild();
    }

    /// 展开包含目标真实行的所有折叠祖先，保证导航不会停在不可见行。
    pub(crate) fn ensure_line_visible(&mut self, line: usize) -> bool {
        let ids = self
            .regions
            .iter()
            .filter(|region| {
                self.collapsed.contains(&region.id)
                    && line > region.start_line
                    && line <= region.end_line
            })
            .map(|region| region.id)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return false;
        }
        for id in ids {
            self.collapsed.remove(&id);
        }
        self.rebuild();
        true
    }

    /// 普通编辑命中结构时立即展开并丢弃该旧区域；未命中的后续区域只按 byte/行差量
    /// 平移，避免后台新语法树发布前 gutter 短暂指向旧坐标。
    pub(crate) fn apply_source_edit(
        &mut self,
        range: Range<u64>,
        start_line: usize,
        end_line: usize,
        replacement: &str,
    ) {
        let removed_lines = end_line.saturating_sub(start_line);
        let inserted_lines = replacement.bytes().filter(|byte| *byte == b'\n').count();
        let byte_delta = replacement.len() as i128 - range.end.saturating_sub(range.start) as i128;
        let line_delta = inserted_lines as i128 - removed_lines as i128;
        let insertion = range.is_empty();
        let mut removed = BTreeSet::new();

        self.regions.retain_mut(|region| {
            let touched = if insertion {
                range.start >= region.byte_range.start && range.start <= region.byte_range.end
            } else {
                range.start < region.byte_range.end && range.end > region.byte_range.start
            };
            if touched {
                removed.insert(region.id);
                return false;
            }
            if region.byte_range.start >= range.end {
                region.byte_range.start = shift_u64(region.byte_range.start, byte_delta);
                region.byte_range.end = shift_u64(region.byte_range.end, byte_delta);
                region.start_line = shift_usize(region.start_line, line_delta);
                region.end_line = shift_usize(region.end_line, line_delta);
            }
            true
        });
        self.collapsed.retain(|id| !removed.contains(id));
        self.real_line_count = shift_usize(self.real_line_count, line_delta).max(1);
        self.rebuild();
    }

    pub(crate) fn visible_line_count(&self) -> usize {
        self.real_line_count.saturating_sub(self.hidden_total)
    }

    pub(crate) fn real_line_for_visible(&self, visible: usize) -> usize {
        let visible = visible.min(self.visible_line_count().saturating_sub(1));
        let count = self
            .effective
            .partition_point(|fold| fold.visible_start < visible);
        let hidden = count.checked_sub(1).map_or(0, |index| {
            let fold = &self.effective[index];
            fold.hidden_before + fold.end - fold.start
        });
        visible
            .saturating_add(hidden)
            .min(self.real_line_count.saturating_sub(1))
    }

    pub(crate) fn visible_line_for_real(&self, real: usize) -> usize {
        let real = real.min(self.real_line_count.saturating_sub(1));
        if let Some(fold) = self
            .effective
            .iter()
            .find(|fold| real > fold.start && real <= fold.end)
        {
            return fold.visible_start;
        }
        let count = self.effective.partition_point(|fold| fold.end < real);
        let hidden = count.checked_sub(1).map_or(0, |index| {
            let fold = &self.effective[index];
            fold.hidden_before + fold.end - fold.start
        });
        real.saturating_sub(hidden)
    }

    fn rebuild(&mut self) {
        self.effective.clear();
        let mut outer_end = None;
        let mut hidden_before = 0usize;
        for region in &self.regions {
            if !self.collapsed.contains(&region.id) || region.hidden_line_count() == 0 {
                continue;
            }
            if outer_end.is_some_and(|end| region.end_line <= end) {
                continue;
            }
            if outer_end.is_some_and(|end| region.start_line <= end) {
                continue;
            }
            self.effective.push(EffectiveFold {
                start: region.start_line,
                end: region.end_line,
                hidden_before,
                visible_start: region.start_line.saturating_sub(hidden_before),
            });
            hidden_before = hidden_before.saturating_add(region.hidden_line_count());
            outer_end = Some(region.end_line);
        }
        self.hidden_total = hidden_before.min(self.real_line_count.saturating_sub(1));
    }
}

fn shift_u64(value: u64, delta: i128) -> u64 {
    u64::try_from(value as i128 + delta).unwrap_or(if delta.is_negative() { 0 } else { u64::MAX })
}

fn shift_usize(value: usize, delta: i128) -> usize {
    usize::try_from(value as i128 + delta).unwrap_or(if delta.is_negative() {
        0
    } else {
        usize::MAX
    })
}

/// 解析完整或有界源码窗口。范围必须使用窗口内的真实 byte/行基址。
pub(crate) fn discover_fold_regions(
    language: SourceLanguageId,
    source: &str,
    byte_base: u64,
    line_base: usize,
) -> Vec<FoldRegion> {
    if !language.supports_folding() || source.is_empty() {
        return Vec::new();
    }
    let line_starts = line_starts(source);
    let regions = tree_sitter_regions(language, source, &line_starts, byte_base, line_base);
    finish_fold_regions(
        language,
        source,
        &line_starts,
        byte_base,
        line_base,
        regions,
    )
}

fn finish_fold_regions(
    language: SourceLanguageId,
    source: &str,
    line_starts: &[usize],
    byte_base: u64,
    line_base: usize,
    mut regions: Vec<FoldRegion>,
) -> Vec<FoldRegion> {
    regions.extend(delimiter_regions(
        language,
        source,
        line_starts,
        byte_base,
        line_base,
    ));
    match language {
        SourceLanguageId::Markdown => {
            regions.extend(markdown_regions(source, line_starts, byte_base, line_base));
        }
        SourceLanguageId::Python | SourceLanguageId::Yaml => {
            regions.extend(indentation_regions(
                source,
                line_starts,
                byte_base,
                line_base,
            ));
        }
        SourceLanguageId::Toml => {
            regions.extend(toml_regions(source, line_starts, byte_base, line_base));
        }
        SourceLanguageId::Bash | SourceLanguageId::Mermaid | SourceLanguageId::Ruby => {
            regions.extend(keyword_regions(
                language,
                source,
                line_starts,
                byte_base,
                line_base,
            ));
        }
        SourceLanguageId::Html => {
            regions.extend(html_regions(source, line_starts, byte_base, line_base));
        }
        _ => {}
    }
    normalize_regions(&mut regions);
    regions
}

#[cfg(feature = "code-highlight-core")]
fn tree_sitter_regions(
    language: SourceLanguageId,
    source: &str,
    starts: &[usize],
    byte_base: u64,
    line_base: usize,
) -> Vec<FoldRegion> {
    let Some(grammar) = tree_sitter_language(language) else {
        return Vec::new();
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    tree_sitter_regions_from_tree(language, source, starts, byte_base, line_base, &tree)
}

#[cfg(feature = "code-highlight-core")]
fn tree_sitter_regions_from_tree(
    language: SourceLanguageId,
    source: &str,
    starts: &[usize],
    byte_base: u64,
    line_base: usize,
    tree: &tree_sitter::Tree,
) -> Vec<FoldRegion> {
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
                .filter(|closing| matches!(closing, '}' | ']' | ')' | '>'));
            push_region(
                &mut output,
                starts,
                byte_base,
                line_base,
                node.start_byte(),
                node.end_byte(),
                "syntax",
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
fn incremental_input_edit(previous: &str, current: &str) -> Option<tree_sitter::InputEdit> {
    if previous == current {
        return None;
    }
    let mut prefix = previous
        .bytes()
        .zip(current.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while prefix > 0 && (!previous.is_char_boundary(prefix) || !current.is_char_boundary(prefix)) {
        prefix -= 1;
    }
    let max_suffix = previous
        .len()
        .saturating_sub(prefix)
        .min(current.len().saturating_sub(prefix));
    let mut suffix = previous
        .as_bytes()
        .iter()
        .rev()
        .zip(current.as_bytes().iter().rev())
        .take(max_suffix)
        .take_while(|(left, right)| left == right)
        .count();
    while suffix > 0
        && (!previous.is_char_boundary(previous.len() - suffix)
            || !current.is_char_boundary(current.len() - suffix))
    {
        suffix -= 1;
    }
    let old_end = previous.len() - suffix;
    let new_end = current.len() - suffix;
    Some(tree_sitter::InputEdit {
        start_byte: prefix,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: point_for_offset(previous, prefix),
        old_end_position: point_for_offset(previous, old_end),
        new_end_position: point_for_offset(current, new_end),
    })
}

#[cfg(feature = "code-highlight-core")]
fn point_for_offset(source: &str, offset: usize) -> tree_sitter::Point {
    let prefix = &source.as_bytes()[..offset];
    let row = prefix.iter().filter(|byte| **byte == b'\n').count();
    let column = prefix
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(prefix.len(), |newline| prefix.len() - newline - 1);
    tree_sitter::Point { row, column }
}

#[cfg(not(feature = "code-highlight-core"))]
fn tree_sitter_regions(
    _: SourceLanguageId,
    _: &str,
    _: &[usize],
    _: u64,
    _: usize,
) -> Vec<FoldRegion> {
    Vec::new()
}

#[cfg(feature = "code-highlight-core")]
fn tree_sitter_language(language: SourceLanguageId) -> Option<tree_sitter::Language> {
    match language {
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::JavaScript | SourceLanguageId::JavaScriptJsx => {
            Some(tree_sitter_javascript::LANGUAGE.into())
        }
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::TypeScriptTsx => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::Json | SourceLanguageId::JsonLines => {
            Some(tree_sitter_json::LANGUAGE.into())
        }
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::Markdown => Some(tree_sitter_md::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::Bash => Some(tree_sitter_bash::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::C => Some(tree_sitter_c::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::CSharp => Some(tree_sitter_c_sharp::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::Css => Some(tree_sitter_css::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::Go => Some(tree_sitter_go::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::Html => Some(tree_sitter_html::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::Java => Some(tree_sitter_java::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::Php => Some(tree_sitter_php::LANGUAGE_PHP.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::Python => Some(tree_sitter_python::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguageId::Ruby => Some(tree_sitter_ruby::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-config")]
        SourceLanguageId::Yaml => Some(tree_sitter_yaml::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-config")]
        SourceLanguageId::Toml => Some(tree_sitter_toml::LANGUAGE.into()),
        _ => None,
    }
}

#[cfg(feature = "code-highlight-core")]
fn foldable_tree_sitter_kind(language: SourceLanguageId, kind: &str) -> bool {
    if matches!(kind, "comment" | "block_comment") {
        return true;
    }
    match language {
        SourceLanguageId::Json | SourceLanguageId::JsonLines => {
            matches!(kind, "object" | "array")
        }
        SourceLanguageId::Markdown => matches!(
            kind,
            "section"
                | "fenced_code_block"
                | "block_quote"
                | "list"
                | "minus_metadata"
                | "plus_metadata"
        ),
        SourceLanguageId::Html => matches!(kind, "element" | "script_element" | "style_element"),
        SourceLanguageId::Css => kind.ends_with("block") || kind.ends_with("rule_set"),
        SourceLanguageId::Python => matches!(
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
        SourceLanguageId::Yaml => matches!(
            kind,
            "block_mapping" | "block_sequence" | "block_node" | "block_scalar" | "document"
        ),
        SourceLanguageId::Toml => matches!(kind, "table" | "table_array_element" | "array"),
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

fn delimiter_regions(
    language: SourceLanguageId,
    source: &str,
    starts: &[usize],
    byte_base: u64,
    line_base: usize,
) -> Vec<FoldRegion> {
    let bytes = source.as_bytes();
    let mut stack: Vec<(u8, usize)> = Vec::new();
    let mut output = Vec::new();
    let mut index = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;
    let mut block_comment_start = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
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
                push_region(
                    &mut output,
                    starts,
                    byte_base,
                    line_base,
                    block_comment_start,
                    index + 2,
                    "comment",
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
                SourceLanguageId::Python
                    | SourceLanguageId::Yaml
                    | SourceLanguageId::Toml
                    | SourceLanguageId::Bash
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
                    push_region(
                        &mut output,
                        starts,
                        byte_base,
                        line_base,
                        start,
                        index + 1,
                        "delimiter",
                        Some(byte as char),
                    );
                }
            }
            _ => {}
        }
        index += 1;
    }
    output
}

fn markdown_regions(
    source: &str,
    starts: &[usize],
    byte_base: u64,
    line_base: usize,
) -> Vec<FoldRegion> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut headings: Vec<(usize, usize)> = Vec::new();
    let mut fence: Option<(usize, &str)> = None;
    for (line, text) in lines.iter().enumerate() {
        let trimmed = text.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = if trimmed.starts_with("```") {
                "```"
            } else {
                "~~~"
            };
            if let Some((start, active)) = fence {
                if active == marker {
                    let end = line_end(starts, source.len(), line);
                    push_region(
                        &mut output,
                        starts,
                        byte_base,
                        line_base,
                        starts[start],
                        end,
                        "fence",
                        None,
                    );
                    fence = None;
                }
            } else {
                fence = Some((line, marker));
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let level = trimmed.bytes().take_while(|byte| *byte == b'#').count();
        if (1..=6).contains(&level) && trimmed.as_bytes().get(level) == Some(&b' ') {
            while let Some((start, previous_level)) = headings.last().copied() {
                if previous_level < level {
                    break;
                }
                headings.pop();
                let end_line = line.saturating_sub(1);
                if end_line > start {
                    let end = line_end(starts, source.len(), end_line);
                    push_region(
                        &mut output,
                        starts,
                        byte_base,
                        line_base,
                        starts[start],
                        end,
                        "heading",
                        None,
                    );
                }
            }
            headings.push((line, level));
        }
    }
    let last = lines.len().saturating_sub(1);
    for (start, _) in headings {
        if last > start {
            push_region(
                &mut output,
                starts,
                byte_base,
                line_base,
                starts[start],
                source.len(),
                "heading",
                None,
            );
        }
    }
    output
}

fn indentation_regions(
    source: &str,
    starts: &[usize],
    byte_base: u64,
    line_base: usize,
) -> Vec<FoldRegion> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for (line, text) in lines.iter().enumerate() {
        if text.trim().is_empty() || text.trim_start().starts_with('#') {
            continue;
        }
        let indent = indentation(text);
        while let Some((start, previous_indent)) = stack.last().copied() {
            if indent > previous_indent {
                break;
            }
            stack.pop();
            let end_line = line.saturating_sub(1);
            if end_line > start {
                push_region(
                    &mut output,
                    starts,
                    byte_base,
                    line_base,
                    starts[start],
                    line_end(starts, source.len(), end_line),
                    "indent",
                    None,
                );
            }
        }
        let trimmed = text.trim_end();
        if trimmed.ends_with(':') || trimmed.ends_with('|') || trimmed.ends_with('>') {
            stack.push((line, indent));
        }
    }
    let last = lines.len().saturating_sub(1);
    for (start, _) in stack {
        if last > start {
            push_region(
                &mut output,
                starts,
                byte_base,
                line_base,
                starts[start],
                source.len(),
                "indent",
                None,
            );
        }
    }
    output
}

fn toml_regions(
    source: &str,
    starts: &[usize],
    byte_base: u64,
    line_base: usize,
) -> Vec<FoldRegion> {
    let lines = source.lines().collect::<Vec<_>>();
    let headers = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim();
            trimmed.starts_with('[') && trimmed.ends_with(']')
        })
        .map(|(line, _)| line)
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    for (index, start) in headers.iter().copied().enumerate() {
        let end_line = headers
            .get(index + 1)
            .copied()
            .unwrap_or(lines.len())
            .saturating_sub(1);
        if end_line > start {
            push_region(
                &mut output,
                starts,
                byte_base,
                line_base,
                starts[start],
                line_end(starts, source.len(), end_line),
                "table",
                None,
            );
        }
    }
    output
}

fn keyword_regions(
    language: SourceLanguageId,
    source: &str,
    starts: &[usize],
    byte_base: u64,
    line_base: usize,
) -> Vec<FoldRegion> {
    let mut output = Vec::new();
    let mut stack: Vec<(usize, &'static str)> = Vec::new();
    for (line, text) in source.lines().enumerate() {
        let trimmed = text.trim();
        let opener = if language == SourceLanguageId::Mermaid && trimmed.starts_with("subgraph") {
            Some("subgraph")
        } else if language == SourceLanguageId::Ruby
            && (trimmed.starts_with("class ")
                || trimmed.starts_with("module ")
                || trimmed.starts_with("def ")
                || trimmed.starts_with("if ")
                || trimmed.starts_with("unless ")
                || trimmed.starts_with("case ")
                || trimmed.starts_with("while ")
                || trimmed.starts_with("for ")
                || trimmed.ends_with(" do"))
        {
            Some("ruby")
        } else if language == SourceLanguageId::Bash
            && (trimmed.starts_with("if ")
                || trimmed.starts_with("for ")
                || trimmed.starts_with("while ")
                || trimmed.starts_with("case ")
                || trimmed.ends_with("() {"))
        {
            Some("shell")
        } else {
            None
        };
        if let Some(kind) = opener {
            stack.push((line, kind));
        }
        let closes = (matches!(language, SourceLanguageId::Mermaid | SourceLanguageId::Ruby)
            && trimmed == "end")
            || (language == SourceLanguageId::Bash
                && matches!(trimmed, "fi" | "done" | "esac" | "}"));
        if closes && let Some((start, kind)) = stack.pop() {
            push_region(
                &mut output,
                starts,
                byte_base,
                line_base,
                starts[start],
                line_end(starts, source.len(), line),
                kind,
                None,
            );
        }
    }
    output
}

fn html_regions(
    source: &str,
    starts: &[usize],
    byte_base: u64,
    line_base: usize,
) -> Vec<FoldRegion> {
    let bytes = source.as_bytes();
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut output = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'<' || bytes.get(index + 1).is_none() {
            index += 1;
            continue;
        }
        let close = bytes.get(index + 1) == Some(&b'/');
        let name_start = index + if close { 2 } else { 1 };
        let mut name_end = name_start;
        while bytes
            .get(name_end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b':' | b'_'))
        {
            name_end += 1;
        }
        if name_end == name_start {
            index += 1;
            continue;
        }
        let name = source[name_start..name_end].to_ascii_lowercase();
        let Some(relative_end) = source[name_end..].find('>') else {
            break;
        };
        let tag_end = name_end + relative_end + 1;
        if close {
            if let Some(position) = stack.iter().rposition(|(candidate, _)| *candidate == name) {
                let (_, start) = stack.remove(position);
                push_region(
                    &mut output,
                    starts,
                    byte_base,
                    line_base,
                    start,
                    tag_end,
                    "element",
                    Some('>'),
                );
            }
        } else if !source[index..tag_end].trim_end().ends_with("/>") && !is_void_html_tag(&name) {
            stack.push((name, index));
        }
        index = tag_end;
    }
    output
}

fn is_void_html_tag(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn normalize_regions(regions: &mut Vec<FoldRegion>) {
    regions.retain(|region| region.end_line > region.start_line);
    regions.sort_by_key(|region| {
        (
            region.start_line,
            std::cmp::Reverse(region.end_line),
            region.byte_range.start,
        )
    });
    regions.dedup_by(|right, left| {
        right.start_line == left.start_line
            && right.end_line == left.end_line
            && right.kind == left.kind
    });
    let mut stack: Vec<(usize, Vec<u32>, u32)> = Vec::new();
    let mut root_ordinal = 0_u32;
    for region in regions.iter_mut() {
        while stack
            .last()
            .is_some_and(|(end, _, _)| *end < region.end_line)
        {
            stack.pop();
        }
        let path = if let Some((_, parent_path, next_child)) = stack.last_mut() {
            let mut path = parent_path.clone();
            path.push(*next_child);
            *next_child = next_child.saturating_add(1);
            path
        } else {
            let path = vec![root_ordinal];
            root_ordinal = root_ordinal.saturating_add(1);
            path
        };
        region.depth = path.len().saturating_sub(1);
        region.structure_path = path.clone();
        stack.push((region.end_line, path, 0));
        region.id = stable_region_id(region.kind, region.byte_range.start, region.depth);
    }
}

fn push_region(
    output: &mut Vec<FoldRegion>,
    starts: &[usize],
    byte_base: u64,
    line_base: usize,
    start: usize,
    end: usize,
    kind: &'static str,
    closing: Option<char>,
) {
    let start_line = line_for_offset(starts, start) + line_base;
    let end_line = line_for_offset(starts, end.saturating_sub(1)) + line_base;
    if end_line <= start_line {
        return;
    }
    output.push(FoldRegion {
        id: 0,
        kind,
        byte_range: byte_base.saturating_add(start as u64)..byte_base.saturating_add(end as u64),
        start_line,
        end_line,
        depth: 0,
        structure_path: Vec::new(),
        closing,
    });
}

fn stable_region_id(kind: &str, start: u64, depth: usize) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in kind
        .bytes()
        .chain(start.to_le_bytes())
        .chain((depth as u64).to_le_bytes())
    {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(source.match_indices('\n').map(|(index, _)| index + 1));
    starts
}

fn line_for_offset(starts: &[usize], offset: usize) -> usize {
    starts
        .partition_point(|start| *start <= offset)
        .saturating_sub(1)
}

fn line_end(starts: &[usize], source_len: usize, line: usize) -> usize {
    starts.get(line + 1).copied().unwrap_or(source_len)
}

fn indentation(line: &str) -> usize {
    line.bytes()
        .take_while(|byte| matches!(*byte, b' ' | b'\t'))
        .map(|byte| if byte == b'\t' { 4 } else { 1 })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_scanner_ignores_braces_inside_strings_and_comments() {
        let source = "{\n  \"fake\": \"} [\",\n  // }\n  \"real\": [\n    1\n  ]\n}\n";
        let regions = discover_fold_regions(SourceLanguageId::Json, source, 0, 0);
        assert!(
            regions
                .iter()
                .any(|region| region.start_line == 0 && region.end_line == 6)
        );
        assert!(
            regions
                .iter()
                .any(|region| region.start_line == 3 && region.end_line == 5)
        );
    }

    #[test]
    fn markdown_headings_and_fences_are_foldable() {
        let source = "# A\ntext\n```rust\nfn main() {}\n```\n## B\nmore\n";
        let regions = discover_fold_regions(SourceLanguageId::Markdown, source, 0, 0);
        assert!(regions.iter().any(|region| region.kind == "fence"));
        assert!(
            regions
                .iter()
                .any(|region| region.kind == "heading" && region.start_line == 0)
        );
    }

    #[test]
    fn projection_preserves_nested_state_and_maps_both_directions() {
        let mut projection = FoldProjectionIndex::default();
        let regions = vec![
            FoldRegion {
                id: 1,
                kind: "outer",
                byte_range: 0..100,
                start_line: 1,
                end_line: 8,
                depth: 0,
                structure_path: vec![0],
                closing: Some('}'),
            },
            FoldRegion {
                id: 2,
                kind: "inner",
                byte_range: 10..50,
                start_line: 3,
                end_line: 5,
                depth: 1,
                structure_path: vec![0, 0],
                closing: Some(']'),
            },
        ];
        projection.set_regions(10, regions);
        projection.toggle(1);
        projection.toggle(2);
        assert_eq!(projection.visible_line_count(), 3);
        assert_eq!(projection.real_line_for_visible(2), 9);
        assert_eq!(projection.visible_line_for_real(7), 1);
        projection.toggle(1);
        assert_eq!(projection.visible_line_count(), 8);
        assert!(projection.is_collapsed(2));
        assert_eq!(projection.visible_line_for_real(5), 3);
    }

    #[test]
    fn navigation_expands_all_collapsed_ancestors() {
        let mut projection = FoldProjectionIndex::default();
        projection.set_regions(
            8,
            vec![
                FoldRegion {
                    id: 1,
                    kind: "outer",
                    byte_range: 0..80,
                    start_line: 0,
                    end_line: 7,
                    depth: 0,
                    structure_path: vec![0],
                    closing: Some('}'),
                },
                FoldRegion {
                    id: 2,
                    kind: "inner",
                    byte_range: 10..50,
                    start_line: 2,
                    end_line: 5,
                    depth: 1,
                    structure_path: vec![0, 0],
                    closing: Some(']'),
                },
            ],
        );
        projection.collapse_all();
        assert!(projection.ensure_line_visible(4));
        assert_eq!(projection.visible_line_count(), 8);
    }

    #[test]
    fn reparsing_after_prefix_edit_preserves_structural_fold_state() {
        let source = "{\n  \"a\": [\n    1\n  ]\n}\n";
        let mut projection = FoldProjectionIndex::default();
        let first = discover_fold_regions(SourceLanguageId::Json, source, 0, 0);
        let inner = first
            .iter()
            .find(|region| region.start_line == 1)
            .unwrap()
            .id;
        projection.set_regions(5, first);
        projection.toggle(inner);

        let changed = format!("\n{source}");
        projection.set_regions(
            6,
            discover_fold_regions(SourceLanguageId::Json, &changed, 0, 0),
        );
        let inner = projection
            .regions()
            .iter()
            .find(|region| region.start_line == 2)
            .unwrap();
        assert!(projection.is_collapsed(inner.id));
    }

    #[test]
    fn edits_expand_touched_folds_and_shift_untouched_following_regions() {
        let mut projection = FoldProjectionIndex::default();
        projection.set_regions(
            12,
            vec![
                FoldRegion {
                    id: 1,
                    kind: "first",
                    byte_range: 0..20,
                    start_line: 0,
                    end_line: 3,
                    depth: 0,
                    structure_path: vec![0],
                    closing: Some('}'),
                },
                FoldRegion {
                    id: 2,
                    kind: "second",
                    byte_range: 30..50,
                    start_line: 6,
                    end_line: 9,
                    depth: 0,
                    structure_path: vec![1],
                    closing: Some('}'),
                },
            ],
        );
        projection.collapse_all();

        projection.apply_source_edit(5..5, 1, 1, "new\nline\n");

        assert!(projection.regions().iter().all(|region| region.id != 1));
        let shifted = projection
            .regions()
            .iter()
            .find(|region| region.id == 2)
            .expect("untouched following fold should remain");
        assert_eq!(shifted.byte_range, 39..59);
        assert_eq!((shifted.start_line, shifted.end_line), (8, 11));
        assert!(projection.is_collapsed(2));
    }

    #[test]
    fn resident_parser_reuses_tree_for_local_edit_and_rebuilds_for_new_epoch() {
        let mut parser = ResidentFoldParser::default();
        let first = "fn main() {\n  run();\n}\n";
        assert!(!parser.parse(1, SourceLanguageId::Rust, first).is_empty());
        assert!(!parser.last_parse_was_incremental());

        let changed = "fn main() {\n  run_twice();\n}\n";
        assert!(!parser.parse(1, SourceLanguageId::Rust, changed).is_empty());
        #[cfg(feature = "code-highlight-core")]
        assert!(parser.last_parse_was_incremental());

        assert!(!parser.parse(2, SourceLanguageId::Rust, changed).is_empty());
        assert!(!parser.last_parse_was_incremental());
    }

    #[test]
    fn every_structured_language_has_a_representative_fold() {
        let cases = [
            (SourceLanguageId::Rust, "fn main() {\n  run();\n}\n"),
            (
                SourceLanguageId::JavaScript,
                "function f() {\n  return 1;\n}\n",
            ),
            (SourceLanguageId::JavaScriptJsx, "const x = {\n  a: 1\n};\n"),
            (
                SourceLanguageId::TypeScript,
                "interface A {\n  x: number;\n}\n",
            ),
            (SourceLanguageId::TypeScriptTsx, "const x = {\n  a: 1\n};\n"),
            (SourceLanguageId::Json, "{\n  \"a\": 1\n}\n"),
            (SourceLanguageId::Markdown, "# A\nbody\nmore\n"),
            (SourceLanguageId::Bash, "if true; then\n  echo ok\nfi\n"),
            (SourceLanguageId::C, "int main() {\n  return 0;\n}\n"),
            (SourceLanguageId::Cpp, "class A {\n  int x;\n};\n"),
            (SourceLanguageId::CSharp, "class A {\n  int X;\n}\n"),
            (SourceLanguageId::Css, "body {\n  color: red;\n}\n"),
            (SourceLanguageId::Go, "func main() {\n  run()\n}\n"),
            (SourceLanguageId::Html, "<div>\n  <span>x</span>\n</div>\n"),
            (SourceLanguageId::Java, "class A {\n  int x;\n}\n"),
            (SourceLanguageId::Php, "function f() {\n  return 1;\n}\n"),
            (SourceLanguageId::Python, "def f():\n    return 1\n\n"),
            (SourceLanguageId::Ruby, "def f\n  1\nend\n"),
            (SourceLanguageId::Yaml, "root:\n  child: 1\n\n"),
            (SourceLanguageId::Toml, "[root]\na = 1\nb = 2\n"),
            (SourceLanguageId::Mermaid, "subgraph A\n  X --> Y\nend\n"),
        ];
        for (language, source) in cases {
            assert!(
                !discover_fold_regions(language, source, 0, 0).is_empty(),
                "{language:?} should expose a fold"
            );
        }
    }
}
