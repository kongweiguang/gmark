// @author kongweiguang

use crate::theme::ThemeColors;
use gmark_json_graph::JsonValueKind;
use gpui::Hsla;

/// JSON 图谱只消费现有主题语义色，避免给自定义主题增加一次性 schema。
#[derive(Clone, Copy)]
pub(super) struct JsonGraphPalette {
    pub(super) canvas: Hsla,
    pub(super) surface: Hsla,
    pub(super) text: Hsla,
    pub(super) muted: Hsla,
    pub(super) accent: Hsla,
    pub(super) search: Hsla,
    pub(super) grid: Hsla,
    pub(super) edge: Hsla,
    branches: [Hsla; 6],
    string: Hsla,
    number: Hsla,
    boolean: Hsla,
    null: Hsla,
}

impl JsonGraphPalette {
    pub(super) fn from_theme(colors: &ThemeColors) -> Self {
        Self {
            canvas: colors.editor_background,
            surface: colors.dialog_surface,
            text: colors.text_default,
            muted: colors.dialog_muted,
            accent: colors.text_link,
            search: colors.code_syntax_constant,
            grid: colors.dialog_border.opacity(0.22),
            edge: colors.dialog_border.opacity(0.7),
            branches: [
                colors.code_syntax_function,
                colors.code_syntax_string,
                colors.code_syntax_keyword,
                colors.code_syntax_number,
                colors.code_syntax_constant,
                colors.code_syntax_type,
            ],
            string: colors.code_syntax_string,
            number: colors.code_syntax_number,
            boolean: colors.code_syntax_keyword,
            null: colors.code_syntax_constant,
        }
    }

    /// 根节点保持中性；第一层分支及其后代稳定继承同一语法色。
    pub(super) fn branch(self, branch: Option<usize>, neutral: Hsla) -> Hsla {
        branch.map_or(neutral, |index| self.branches[index % self.branches.len()])
    }

    pub(super) fn value(self, kind: JsonValueKind) -> Hsla {
        match kind {
            JsonValueKind::String => self.string,
            JsonValueKind::Number => self.number,
            JsonValueKind::Boolean => self.boolean,
            JsonValueKind::Null => self.null,
            JsonValueKind::Object | JsonValueKind::Array => self.muted,
        }
    }
}
