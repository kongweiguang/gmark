// @author kongweiguang

//! Stateful full-document folding parser for callers that can retain a syntax tree.

use crate::{SourceLanguage, fold_ranges};

/// 为常驻源码文档复用 tree-sitter 树的折叠解析器。
///
/// 该类型只保留领域状态；窗口、任务取消和 UI 投影仍应由调用方管理。
#[derive(Default)]
pub struct IncrementalFoldParser {
    document_epoch: Option<u64>,
    language: Option<SourceLanguage>,
    source: String,
    #[cfg(feature = "code-highlight-core")]
    tree: Option<tree_sitter::Tree>,
    last_parse_incremental: bool,
}

impl IncrementalFoldParser {
    /// 解析完整文档。相同 epoch 和语言的局部变更会在可用时复用上一棵语法树。
    pub fn parse(
        &mut self,
        document_epoch: u64,
        language: SourceLanguage,
        source: &str,
    ) -> Vec<crate::FoldRange> {
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

            let Some(grammar) = crate::language::tree_sitter_language(language) else {
                self.tree = None;
                return fold_ranges(language, source);
            };
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&grammar).is_err() {
                self.tree = None;
                return fold_ranges(language, source);
            }
            let Some(tree) = parser.parse(source, self.tree.as_ref()) else {
                self.tree = None;
                return fold_ranges(language, source);
            };
            let ranges = crate::folding::fold_ranges_for_tree(language, source, &tree);
            self.tree = Some(tree);
            ranges
        }

        #[cfg(not(feature = "code-highlight-core"))]
        {
            self.document_epoch = Some(document_epoch);
            self.language = Some(language);
            self.source.clear();
            self.source.push_str(source);
            self.last_parse_incremental = false;
            fold_ranges(language, source)
        }
    }

    /// 上一次解析是否实际把旧树作为增量输入传给 tree-sitter。
    pub const fn last_parse_was_incremental(&self) -> bool {
        self.last_parse_incremental
    }
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
