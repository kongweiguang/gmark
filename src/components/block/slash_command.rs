// @author kongweiguang

//! Block-local slash command filtering, keyboard interaction, and menu rendering.

use gpui::prelude::FluentBuilder;
use gpui::*;

use super::{
    BLOCK_MENU_COMMANDS, Block, BlockDragPayload, BlockEvent, BlockKind, EditingBlockContext,
    EditingCommandCategory, EditingCommandHistory, EditingCommandId, EditingContext,
    EditingSelectionContext, EditingViewMode, SLASH_COMMANDS, filter_commands,
};
use crate::i18n::I18nStrings;
use crate::theme::Theme;

const MAX_SLASH_QUERY_BYTES: usize = 64;
const SLASH_MENU_WIDTH: f32 = 292.0;
const SLASH_MENU_MAX_HEIGHT: f32 = 304.0;
const SLASH_MENU_GAP: f32 = 4.0;
const SLASH_MENU_VIEWPORT_INSET: f32 = 8.0;
// 单一块入口固定占用内容列内部的左侧预留区，因此普通块、引用和窄窗口共享同一 X 轴。
pub(super) const BLOCK_GUTTER_TEXT_RESERVE: f32 = 20.0;

type BlockGutterAction = dyn Fn(&mut Block, &mut Window, &mut Context<Block>);

struct BlockDragPreview;

impl Render for BlockDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<crate::theme::ThemeManager>().current_arc();
        div()
            .size(px(34.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .bg(theme.colors.dialog_surface)
            .border(px(theme.dimensions.dialog_border_width))
            .border_color(theme.colors.dialog_border)
            .shadow_md()
            .child(
                svg()
                    .path("icon/ui/more-horizontal.svg")
                    .size(px(16.0))
                    .text_color(theme.colors.dialog_muted),
            )
    }
}

pub(crate) type SlashCommand = EditingCommandId;

#[derive(Clone, Debug)]
pub(crate) struct SlashMenuState {
    query: String,
    filtered: Vec<SlashCommand>,
    selected: usize,
    trigger_range: std::ops::Range<usize>,
    recent_count: usize,
    programmatic_text: Option<String>,
    programmatic_allow_raw: bool,
    structural_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SlashMenuPlacement {
    left: f32,
    top: f32,
    width: f32,
    max_height: f32,
    above: bool,
}

fn slash_menu_placement(
    anchor: Bounds<Pixels>,
    viewport: Size<Pixels>,
    desired_height: f32,
) -> SlashMenuPlacement {
    let viewport_width = f32::from(viewport.width);
    let viewport_height = f32::from(viewport.height);
    let width = SLASH_MENU_WIDTH.min((viewport_width - 2.0 * SLASH_MENU_VIEWPORT_INSET).max(1.0));
    let max_left =
        (viewport_width - width - SLASH_MENU_VIEWPORT_INSET).max(SLASH_MENU_VIEWPORT_INSET);
    let left = f32::from(anchor.left()).clamp(SLASH_MENU_VIEWPORT_INSET, max_left);
    let available_above =
        (f32::from(anchor.top()) - SLASH_MENU_GAP - SLASH_MENU_VIEWPORT_INSET).max(0.0);
    let available_below =
        (viewport_height - f32::from(anchor.bottom()) - SLASH_MENU_GAP - SLASH_MENU_VIEWPORT_INSET)
            .max(0.0);
    let desired_height = desired_height.min(SLASH_MENU_MAX_HEIGHT);
    let above = available_below < desired_height && available_above > available_below;
    let max_height = if above {
        available_above
    } else {
        available_below
    }
    .min(desired_height);
    let top = if above {
        f32::from(anchor.top()) - SLASH_MENU_GAP - max_height
    } else {
        f32::from(anchor.bottom()) + SLASH_MENU_GAP
    };
    SlashMenuPlacement {
        left,
        top,
        width,
        max_height,
        above,
    }
}

fn slash_menu_estimated_height(state: &SlashMenuState, theme: &Theme) -> f32 {
    let mut headings = usize::from(state.recent_count > 0);
    let mut previous_category = None;
    for (index, command) in state.filtered.iter().enumerate() {
        if index < state.recent_count {
            continue;
        }
        let category = command.descriptor().category;
        if previous_category != Some(category) {
            headings += 1;
            previous_category = Some(category);
        }
    }
    let item_count = state.filtered.len().max(1);
    let child_count = item_count + headings;
    let item_height = theme.dimensions.menu_item_height.max(32.0);
    let rows = item_count as f32 * item_height + headings as f32 * 24.0;
    let gaps = child_count.saturating_sub(1) as f32 * theme.dimensions.menu_panel_gap;
    (rows + gaps + 2.0 * theme.dimensions.menu_panel_padding).min(SLASH_MENU_MAX_HEIGHT)
}

fn filter_slash_commands(query: &str) -> Vec<SlashCommand> {
    filter_commands(&SLASH_COMMANDS, query)
}
#[path = "slash_command_parts/resolver.rs"]
mod resolver;
#[cfg(test)]
use resolver::{boundary_available_index, selected_available_index, slash_menu_child_index};
#[cfg(test)]
#[path = "../../../tests/unit/components/block/slash_command.rs"]
mod tests;
