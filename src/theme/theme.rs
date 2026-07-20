// @author kongweiguang

//! Theme data structures and defaults.
//!
//! The theme layer keeps visual tokens out of editor logic so rendering and
//! interaction code can depend on stable semantic names instead of hard-coded
//! values.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, bail};
use gpui::{App, FontWeight, Global, Hsla, WindowAppearance, hsla, rgba};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::config::{
    GmarkConfigDirs, merge_non_empty_json_values, object_without_empty_values,
    prune_empty_json_values, read_json_or_jsonc, sanitize_config_file_stem,
};

/// Serializable font weight that maps to GPUI's [`FontWeight`] constants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontWeightDef {
    /// Thin font weight.
    Thin,
    /// Light font weight.
    Light,
    /// Normal font weight.
    Normal,
    /// Medium font weight.
    Medium,
    /// Semibold font weight.
    Semibold,
    /// Bold font weight.
    Bold,
    /// Extra-bold font weight.
    Extrabold,
    /// Black font weight.
    Black,
}

impl FontWeightDef {
    /// Converts the serialized theme value into GPUI's runtime font weight.
    pub fn to_font_weight(&self) -> FontWeight {
        match self {
            FontWeightDef::Thin => FontWeight::THIN,
            FontWeightDef::Light => FontWeight::LIGHT,
            FontWeightDef::Normal => FontWeight::NORMAL,
            FontWeightDef::Medium => FontWeight::MEDIUM,
            FontWeightDef::Semibold => FontWeight::SEMIBOLD,
            FontWeightDef::Bold => FontWeight::BOLD,
            FontWeightDef::Extrabold => FontWeight::EXTRA_BOLD,
            FontWeightDef::Black => FontWeight::BLACK,
        }
    }
}

/// All configurable colors for the editor UI.
#[derive(Debug, Clone, Serialize)]
pub struct ThemeColors {
    /// Background of the editor scroll area (behind all blocks).
    pub editor_background: Hsla,
    /// Background of the focused raw block in source-editing mode.
    pub source_mode_block_bg: Hsla,
    /// Background used for visible Markdown comment blocks.
    pub comment_bg: Hsla,
    /// Default paragraph / body text colour.
    pub text_default: Hsla,
    /// Inline link text colour in rendered mode.
    pub text_link: Hsla,
    /// Placeholder text shown in empty focused blocks.
    pub text_placeholder: Hsla,
    /// H1 heading text colour.
    pub text_h1: Hsla,
    /// H2 heading text colour.
    pub text_h2: Hsla,
    /// H3 heading text colour.
    pub text_h3: Hsla,
    /// H4 heading text colour.
    pub text_h4: Hsla,
    /// H5 heading text colour.
    pub text_h5: Hsla,
    /// H6 heading text colour.
    pub text_h6: Hsla,
    /// H1 bottom-border colour.
    pub border_h1: Hsla,
    /// H2 bottom-border colour.
    pub border_h2: Hsla,
    /// Quote block text colour.
    pub text_quote: Hsla,
    /// Quote block left-border colour.
    pub border_quote: Hsla,
    /// Note callout background.
    pub callout_note_bg: Hsla,
    /// Note callout accent border/text colour.
    pub callout_note_border: Hsla,
    /// Tip callout background.
    pub callout_tip_bg: Hsla,
    /// Tip callout accent border/text colour.
    pub callout_tip_border: Hsla,
    /// Important callout background.
    pub callout_important_bg: Hsla,
    /// Important callout accent border/text colour.
    pub callout_important_border: Hsla,
    /// Warning callout background.
    pub callout_warning_bg: Hsla,
    /// Warning callout accent border/text colour.
    pub callout_warning_border: Hsla,
    /// Caution callout background.
    pub callout_caution_bg: Hsla,
    /// Caution callout accent border/text colour.
    pub callout_caution_border: Hsla,
    /// Background of footnote definition grouping shells.
    pub footnote_bg: Hsla,
    /// Border colour of footnote definition grouping shells.
    pub footnote_border: Hsla,
    /// Background of the footnote ordinal badge.
    pub footnote_badge_bg: Hsla,
    /// Text colour of the footnote ordinal badge.
    pub footnote_badge_text: Hsla,
    /// Back-reference colour inside footnote headers.
    pub footnote_backref: Hsla,
    /// Border colour of interactive task-list checkboxes.
    pub task_checkbox_border: Hsla,
    /// Background of unchecked task-list checkboxes.
    pub task_checkbox_bg: Hsla,
    /// Background of checked task-list checkboxes.
    pub task_checkbox_checked_bg: Hsla,
    /// Checkmark colour inside checked task-list checkboxes.
    pub task_checkbox_check: Hsla,
    /// Colour of the separator block line.
    pub separator_color: Hsla,
    /// Background of inline code and code-block quads.
    pub code_bg: Hsla,
    /// Text colour inside code blocks.
    pub code_text: Hsla,
    /// Background of the focused code-block language input.
    pub code_language_input_bg: Hsla,
    /// Border colour of the focused code-block language input.
    pub code_language_input_border: Hsla,
    /// Text colour of the focused code-block language input.
    pub code_language_input_text: Hsla,
    /// Placeholder colour of the focused code-block language input.
    pub code_language_input_placeholder: Hsla,
    /// Syntax colour for comments inside code blocks.
    pub code_syntax_comment: Hsla,
    /// Syntax colour for keywords inside code blocks.
    pub code_syntax_keyword: Hsla,
    /// Syntax colour for strings inside code blocks.
    pub code_syntax_string: Hsla,
    /// Syntax colour for numbers inside code blocks.
    pub code_syntax_number: Hsla,
    /// Syntax colour for types and modules inside code blocks.
    pub code_syntax_type: Hsla,
    /// Syntax colour for functions and constructors inside code blocks.
    pub code_syntax_function: Hsla,
    /// Syntax colour for constants inside code blocks.
    pub code_syntax_constant: Hsla,
    /// Syntax colour for variables and parameters inside code blocks.
    pub code_syntax_variable: Hsla,
    /// Syntax colour for properties and attributes inside code blocks.
    pub code_syntax_property: Hsla,
    /// Syntax colour for operators inside code blocks.
    pub code_syntax_operator: Hsla,
    /// Syntax colour for punctuation inside code blocks.
    pub code_syntax_punctuation: Hsla,
    /// Border colour of native table cells.
    pub table_border: Hsla,
    /// Background of native table header cells.
    pub table_header_bg: Hsla,
    /// Background of native table body cells.
    pub table_cell_bg: Hsla,
    /// Outline colour of the active native table cell.
    pub table_cell_active_outline: Hsla,
    /// Preview highlight colour for row/column table-axis selection bands.
    pub table_axis_preview_bg: Hsla,
    /// Selected highlight colour for row/column table-axis selection bands.
    pub table_axis_selected_bg: Hsla,
    /// Background of rendered-mode native table append controls.
    pub table_append_button_bg: Hsla,
    /// Hover background of rendered-mode native table append controls.
    pub table_append_button_hover: Hsla,
    /// Text colour of rendered-mode native table append controls.
    pub table_append_button_text: Hsla,
    /// Background of image placeholders in rendered mode.
    pub image_placeholder_bg: Hsla,
    /// Border colour of image placeholders in rendered mode.
    pub image_placeholder_border: Hsla,
    /// Text colour of image placeholders in rendered mode.
    pub image_placeholder_text: Hsla,
    /// Caption text colour shown below rendered images.
    pub image_caption_text: Hsla,
    /// Scrollbar thumb colour (auto-fading overlay).
    pub scrollbar_thumb: Hsla,
    /// Text-editing cursor (caret) colour.
    pub cursor: Hsla,
    /// Text-selection highlight colour.
    pub selection: Hsla,
    /// Semi-transparent backdrop behind the unsaved-changes dialog.
    pub dialog_backdrop: Hsla,
    /// Background of the unsaved-changes dialog.
    pub dialog_surface: Hsla,
    /// Border colour of the unsaved-changes dialog.
    pub dialog_border: Hsla,
    /// Title text colour in the unsaved-changes dialog.
    pub dialog_title: Hsla,
    /// Body text colour in the unsaved-changes dialog.
    pub dialog_body: Hsla,
    /// Muted / hint text colour in the unsaved-changes dialog.
    pub dialog_muted: Hsla,
    /// Primary (save-and-close) button background.
    pub dialog_primary_button_bg: Hsla,
    /// Primary button hover background.
    pub dialog_primary_button_hover: Hsla,
    /// Primary button text colour.
    pub dialog_primary_button_text: Hsla,
    /// Secondary (cancel) button background.
    pub dialog_secondary_button_bg: Hsla,
    /// Secondary button hover background.
    pub dialog_secondary_button_hover: Hsla,
    /// Secondary button text colour.
    pub dialog_secondary_button_text: Hsla,
    /// Danger (discard-and-close) button background.
    pub dialog_danger_button_bg: Hsla,
    /// Danger button hover background.
    pub dialog_danger_button_hover: Hsla,
    /// Danger button text colour.
    pub dialog_danger_button_text: Hsla,
    /// Background of the editor status bar.
    pub status_bar_background: Hsla,
    /// Primary text colour in the status bar.
    pub status_bar_text: Hsla,
    /// Dimmed/secondary text colour in the status bar.
    pub status_bar_text_dim: Hsla,
    /// Hover background for clickable status bar items.
    pub status_bar_button_hover: Hsla,
    /// Continuous surface behind the titlebar and fallback menu bar.
    pub chrome_background: Hsla,
    /// Hover/pressed surface for controls placed in window chrome.
    pub chrome_hover: Hsla,
    /// Dedicated background of the workspace sidebar.
    pub sidebar_background: Hsla,
    /// Background behind document tabs.
    pub tab_strip_background: Hsla,
    /// Background of the active document tab.
    pub tab_active_background: Hsla,
}
#[path = "theme_parts/dimensions.rs"]
mod dimensions;
pub use dimensions::{Placeholders, ThemeDimensions, ThemeTypography};

#[path = "theme_parts/model.rs"]
mod model;
pub use model::Theme;

#[path = "theme_parts/catalog.rs"]
mod catalog;
use catalog::BUILTIN_THEME_GMARK_LIGHT_NAME;
pub(crate) use catalog::SYSTEM_THEME_ID;
#[cfg(test)]
use catalog::custom_theme_from_value;
#[cfg(test)]
use catalog::resolved_system_theme_id;
pub use catalog::{ThemeCatalogEntry, ThemeManager};
#[cfg(test)]
#[path = "../../tests/unit/theme/theme.rs"]
mod tests;
