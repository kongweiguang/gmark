// @author kongweiguang

//! Editor window rendering: centered scrollable block column,
//! unsaved-changes overlay dialog, custom scrollbar, and deferred
//! operations (focus, scroll, save, window title).

use std::time::{Duration, Instant};

use gpui::*;

use super::{
    Editor, InfoDialogKind, ScrollbarGeometry, SplitResizeSession, ViewMode, focus_modes, tree,
    workspace::{workspace_panel_width_for_viewport, workspace_uses_overlay},
};
use crate::app_menu::dispatch_menu_action_for_editor;
use crate::components::CalloutVariant;
use crate::components::{AddLanguageConfig, AddThemeConfig, Block, NoRecentFiles};
use crate::config::{EditorSettings, WorkspaceSidebarPosition};
use crate::i18n::{I18nManager, I18nStrings};
use crate::perf;
use crate::theme::{Theme, ThemeDimensions, ThemeManager};
use crate::window_chrome::{custom_titlebar_height, render_custom_titlebar};

pub(crate) const ABOUT_GITHUB_URL: &str = "https://github.com/kongweiguang/gmark";

/// Rows within this many pixels of the viewport stay mounted, so a fast flick
/// paints them before they scroll in instead of showing a blank edge.
const RENDER_OVERDRAW_PX: f32 = 800.0;
const CHEVRON_RIGHT_ICON: &str = "icon/ui/chevron-right.svg";
const MENU_LAUNCHER_ICON: &str = "icon/gmark-icon.svg";
// Canonical 应用图标包含 G 与 M↓ 两层细节，20px 才能在高密度标题栏中保持辨识度。
const MENU_LAUNCHER_ICON_SIZE: f32 = 20.0;
const EXPORT_PROGRESS_ICON: &str = "icon/ui/file-output.svg";
const CLOSE_ICON: &str = "icon/ui/close.svg";
const MENU_ICON_SLOT: f32 = 18.0;
const MENU_SHORTCUT_SLOT: f32 = 64.0;
#[derive(Clone, Copy)]
pub(super) enum DocumentToolbarAction {
    QuickOpen,
    Find,
    CommandPalette,
}

impl DocumentToolbarAction {
    pub(super) fn index(self) -> usize {
        match self {
            Self::QuickOpen => 0,
            Self::Find => 1,
            Self::CommandPalette => 2,
        }
    }
}
const MENU_SHORTCUT_GAP: f32 = 8.0;
const FLOATING_PANEL_MARGIN: f32 = 8.0;
const SPLIT_DIVIDER_HIT_WIDTH: f32 = 7.0;
const SPLIT_PANE_MIN_WIDTH: f32 = 280.0;
const SPLIT_KEYBOARD_STEP: f32 = 0.01;
const SPLIT_KEYBOARD_LARGE_STEP: f32 = 0.05;
const EDITOR_SCROLLBAR_HIT_WIDTH: f32 = 14.0;
const EDITOR_SCROLLBAR_HOVER_WIDTH: f32 = 10.0;
const EDITOR_READING_TOP_PADDING: f32 = 48.0;

fn split_pane_ratio_bounds(available_width: f32) -> (f32, f32) {
    let available_width = available_width.max(1.0);
    let minimum = (SPLIT_PANE_MIN_WIDTH / available_width).clamp(0.3, 0.5);
    (minimum, 1.0 - minimum)
}

fn clamped_split_pane_ratio(ratio: f32, available_width: f32) -> f32 {
    let (minimum, maximum) = split_pane_ratio_bounds(available_width);
    ratio.clamp(minimum, maximum)
}

fn editor_tab_strip_insets(
    position: WorkspaceSidebarPosition,
    docked_workspace_width: f32,
) -> (f32, f32) {
    match position {
        WorkspaceSidebarPosition::Left => (docked_workspace_width, 0.0),
        WorkspaceSidebarPosition::Right => (0.0, docked_workspace_width),
    }
}

pub(crate) fn editor_top_padding(typewriter_mode: bool, viewport_height: f32) -> f32 {
    if typewriter_mode {
        (viewport_height.max(0.0) * super::focus_modes::TYPEWRITER_VIEWPORT_RATIO)
            .max(EDITOR_READING_TOP_PADDING)
    } else {
        EDITOR_READING_TOP_PADDING
    }
}

pub(super) fn editor_bottom_padding(viewport_height: f32, dimensions: &ThemeDimensions) -> f32 {
    let scroll_trigger_padding = (dimensions.block_min_height * 0.75).max(16.0);
    dimensions.editor_padding + scroll_trigger_padding + viewport_height.max(0.0) * 0.5
}

pub(super) fn menu_icon_slot(icon: Option<&'static str>, color: Hsla) -> Div {
    div()
        .size(px(MENU_ICON_SLOT))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .children(icon.map(|path| svg().path(path).size(px(15.0)).text_color(color)))
}

pub(super) fn compact_menu_panel_height(
    rows: usize,
    separators: usize,
    dimensions: &ThemeDimensions,
) -> f32 {
    let elements = rows + separators;
    dimensions.dialog_border_width * 2.0
        + dimensions.menu_panel_padding * 2.0
        + rows as f32 * dimensions.menu_item_height
        + separators as f32
            * (dimensions.menu_separator_height + dimensions.menu_separator_margin_y * 2.0)
        + elements.saturating_sub(1) as f32 * dimensions.menu_panel_gap
}

#[path = "render_facade_parts/layout.rs"]
mod layout;
#[path = "render_facade_parts/view.rs"]
mod view;

#[cfg(test)]
pub(crate) use layout::open_about_github_url;
pub(in crate::editor) use layout::{
    DialogButtonKind, DialogTitleIcon, RenderedRowCache, clamped_floating_panel_origin,
    dialog_actions, dialog_body, dialog_button, dialog_content, dialog_panel,
    dialog_title_with_icon, floating_submenu_x, modal_overlay, supports_in_window_menu,
};

#[cfg(test)]
use layout::{
    INTEGRATED_MENU_LEFT, RenderedRowSpacingInfo, callout_row_top_gap, editor_text_font_for_family,
    import_menu_split_index, in_window_menu_chrome_layout, menu_bar_button_width,
    menu_items_visual_height_with_gaps, menu_panel_left, menu_panel_width_for_labels,
    menu_shortcut_slot, menu_shortcut_text, owned_menu_item_labels, rendered_row_top_gap,
    scrollable_import_menu_scroll_height, supports_in_window_menu_for_target_os,
    tibetan_font_fallbacks_for_target_os, top_level_menu_button_width, visible_menu_button_count,
};
#[cfg(test)]
use view::submenu_bridge_geometry;
#[cfg(test)]
#[path = "../../tests/unit/editor/render.rs"]
mod tests;
