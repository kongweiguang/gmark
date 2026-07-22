// @author kongweiguang

//! Contextual formatting toolbar for a simple single-block text selection.

use std::ops::Range;

use gpui::prelude::FluentBuilder;
use gpui::*;

use super::{
    Block, BlockEvent, BlockHostAction, BlockRecord, EditingCommandId, EditingContext,
    EditingSelectionContext, INLINE_COMMANDS, InlineFormat, TRANSFORM_COMMANDS, UndoCaptureKind,
};
use crate::components::markdown::inline::StyleFlag;
use crate::i18n::{I18nManager, I18nStrings};
use crate::theme::Theme;

const TOOLBAR_COMPACT_WIDTH: f32 = 256.0;
const TOOLBAR_EXPANDED_WIDTH: f32 = 320.0;
const TOOLBAR_HEIGHT: f32 = 32.0;
const TOOLBAR_GAP: f32 = 6.0;
const VIEWPORT_INSET: f32 = 8.0;
// Windows 的菜单标题栏与文档标签栏都在 GPUI client viewport 内；附着浮层
// 必须为标签栏预留这一层高度，不能只按整个窗口的 y=0 做碰撞判断。
const DOCUMENT_TAB_STRIP_RESERVE: f32 = 36.0;
const CODE_ICON: &str = "icon/ui/code.svg";
const LINK_ICON: &str = "icon/ui/link.svg";
const MORE_ICON: &str = "icon/ui/more-horizontal.svg";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolbarCommand {
    BlockType,
    Bold,
    Italic,
    Strikethrough,
    Code,
    Link,
    Overflow,
    Underline,
    Highlight,
    Superscript,
    Subscript,
    InlineMath,
    ClearFormatting,
}

impl ToolbarCommand {
    const PRIMARY: [Self; 7] = [
        Self::BlockType,
        Self::Bold,
        Self::Italic,
        Self::Strikethrough,
        Self::Code,
        Self::Link,
        Self::Overflow,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::BlockType => "block-type",
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::Strikethrough => "strikethrough",
            Self::Code => "code",
            Self::Link => "link",
            Self::Overflow => "overflow",
            Self::Underline => "underline",
            Self::Highlight => "highlight",
            Self::Superscript => "superscript",
            Self::Subscript => "subscript",
            Self::InlineMath => "inline-math",
            Self::ClearFormatting => "clear-formatting",
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::BlockType => "T",
            Self::Bold => "B",
            Self::Italic => "I",
            Self::Strikethrough => "S",
            Self::Code => "<>",
            Self::Link => "[]",
            Self::Overflow => "...",
            Self::Underline => "U",
            Self::Highlight => "==",
            Self::Superscript => "x²",
            Self::Subscript => "x₂",
            Self::InlineMath => "$",
            Self::ClearFormatting => "Tx",
        }
    }

    fn label(self, strings: &I18nStrings) -> String {
        match self {
            Self::BlockType => strings
                .slash_commands
                .get("paragraph")
                .cloned()
                .unwrap_or_else(|| "Paragraph".to_owned()),
            Self::Bold => strings.selection_toolbar_bold.clone(),
            Self::Italic => strings.selection_toolbar_italic.clone(),
            Self::Strikethrough => strings.selection_toolbar_strikethrough.clone(),
            Self::Code => strings.selection_toolbar_inline_code.clone(),
            Self::Link => strings.selection_toolbar_link.clone(),
            Self::Overflow => strings.selection_toolbar_more.clone(),
            Self::Underline => strings.selection_toolbar_underline.clone(),
            Self::Highlight => strings
                .slash_commands
                .get("highlight")
                .cloned()
                .unwrap_or_else(|| "Highlight".to_owned()),
            Self::Superscript => strings
                .slash_commands
                .get("superscript")
                .cloned()
                .unwrap_or_else(|| "Superscript".to_owned()),
            Self::Subscript => strings
                .slash_commands
                .get("subscript")
                .cloned()
                .unwrap_or_else(|| "Subscript".to_owned()),
            Self::InlineMath => strings
                .slash_commands
                .get("inline_math")
                .cloned()
                .unwrap_or_else(|| "Inline Math".to_owned()),
            Self::ClearFormatting => strings
                .slash_commands
                .get("clear_formatting")
                .cloned()
                .unwrap_or_else(|| "Clear Formatting".to_owned()),
        }
    }

    fn editing_command(self) -> Option<EditingCommandId> {
        match self {
            Self::BlockType => None,
            Self::Bold => Some(EditingCommandId::Bold),
            Self::Italic => Some(EditingCommandId::Italic),
            Self::Underline => Some(EditingCommandId::Underline),
            Self::Highlight => Some(EditingCommandId::Highlight),
            Self::Superscript => Some(EditingCommandId::Superscript),
            Self::Subscript => Some(EditingCommandId::Subscript),
            Self::InlineMath => Some(EditingCommandId::InlineMath),
            Self::Strikethrough => Some(EditingCommandId::Strikethrough),
            Self::Code => Some(EditingCommandId::InlineCode),
            Self::Link => Some(EditingCommandId::Link),
            Self::ClearFormatting => Some(EditingCommandId::ClearFormatting),
            Self::Overflow => None,
        }
    }
}

#[path = "selection_toolbar_parts/controller.rs"]
mod controller;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ToolbarPosition {
    left: f32,
    top: f32,
    above: bool,
}

fn selection_toolbar_width(horizontal_bounds: Bounds<Pixels>, viewport: Size<Pixels>) -> f32 {
    let viewport_width = f32::from(viewport.width);
    let left_edge = (f32::from(horizontal_bounds.left()) + VIEWPORT_INSET).max(VIEWPORT_INSET);
    let right_edge = f32::from(horizontal_bounds.right()).min(viewport_width) - VIEWPORT_INSET;
    if right_edge - left_edge >= TOOLBAR_EXPANDED_WIDTH {
        TOOLBAR_EXPANDED_WIDTH
    } else {
        TOOLBAR_COMPACT_WIDTH
    }
}

fn toolbar_window_position(
    selection: Bounds<Pixels>,
    horizontal_bounds: Bounds<Pixels>,
    viewport: Size<Pixels>,
    attached_surface_height: f32,
) -> ToolbarPosition {
    let viewport_width = f32::from(viewport.width);
    let viewport_height = f32::from(viewport.height);
    let toolbar_width = selection_toolbar_width(horizontal_bounds, viewport);
    let min_left = (f32::from(horizontal_bounds.left()) + VIEWPORT_INSET).max(VIEWPORT_INSET);
    let right_edge = f32::from(horizontal_bounds.right()).min(viewport_width);
    let max_left = (right_edge - toolbar_width - VIEWPORT_INSET).max(min_left);
    let ideal_left = f32::from(selection.center().x) - toolbar_width / 2.0;
    let left = ideal_left.clamp(min_left, max_left);
    let required_height = TOOLBAR_HEIGHT
        + if attached_surface_height > 0.0 {
            TOOLBAR_GAP + attached_surface_height
        } else {
            0.0
        };
    let available_above = (f32::from(selection.top()) - VIEWPORT_INSET).max(0.0);
    let available_below =
        (viewport_height - f32::from(selection.bottom()) - VIEWPORT_INSET).max(0.0);
    let above = available_above >= required_height
        || (available_below < required_height && available_above > available_below);
    let top = if above {
        f32::from(selection.top()) - TOOLBAR_HEIGHT - TOOLBAR_GAP
    } else {
        f32::from(selection.bottom()) + TOOLBAR_GAP
    };
    ToolbarPosition { left, top, above }
}

fn attached_surface_placement(
    position: ToolbarPosition,
    surface_height: f32,
    viewport_height: f32,
    menu_bar_height: f32,
    status_bar_height: f32,
) -> (bool, f32) {
    let safe_top = menu_bar_height + DOCUMENT_TAB_STRIP_RESERVE + VIEWPORT_INSET;
    let safe_bottom = viewport_height - status_bar_height - VIEWPORT_INSET;
    let available_above = (position.top - TOOLBAR_GAP - safe_top).max(0.0);
    let available_below = (safe_bottom - position.top - TOOLBAR_HEIGHT - TOOLBAR_GAP).max(0.0);
    // 工具栏本身与附着浮层可能需要朝不同方向展开；以真实内容区两侧空间
    // 决定方向并返回可用高度，避免窄窗口中越过标签栏或状态栏。
    let opens_above = available_above >= surface_height
        || (available_below < surface_height && available_above > available_below);
    let available_height = if opens_above {
        available_above
    } else {
        available_below
    };
    (opens_above, available_height)
}

impl Block {
    /// Closes only transient children of contextual editing UI. The text
    /// selection and base selection toolbar remain intact so an outside click
    /// can establish the next caret without silently discarding user state.
    pub(crate) fn dismiss_contextual_editing_popovers(&mut self) -> bool {
        let had_transient = self.dismiss_slash_menu()
            || self.selection_toolbar_keyboard_active
            || self.selection_toolbar_overflow_open
            || self.selection_toolbar_type_menu_open
            || self.selection_toolbar_link_input.is_some();
        self.selection_toolbar_keyboard_active = false;
        self.selection_toolbar_overflow_open = false;
        self.selection_toolbar_type_menu_open = false;
        self.selection_toolbar_link_input = None;
        self.selection_toolbar_link_range = None;
        self.selection_toolbar_link_had_target = false;
        had_transient
    }

    fn selection_toolbar_range(&self) -> Option<Range<usize>> {
        if self.is_read_only()
            || self.uses_raw_text_editing()
            || self.marked_range.is_some()
            || self.showing_rendered_image()
        {
            return None;
        }
        let range = if let Some(range) = self.editor_selection_range.clone() {
            if !self.editor_selection_supports_inline_commands {
                return None;
            }
            self.current_to_clean_range(range)
        } else {
            if self.selected_range.is_empty() {
                return None;
            }
            self.selection_clean_range()
        };
        self.record
            .title
            .selection_supports_toolbar(range.clone())
            .then_some(range)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/components/block/selection_toolbar.rs"]
mod tests;
