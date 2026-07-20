// @author kongweiguang

use super::*;

/// All configurable dimensions (paddings, gaps, sizes) for the editor UI.
#[derive(Debug, Clone, Serialize)]
pub struct ThemeDimensions {
    /// Padding around the editor content area.
    pub editor_padding: f32,
    /// Vertical gap between adjacent blocks.
    pub block_gap: f32,
    /// Minimum height of every block.
    pub block_min_height: f32,
    /// Vertical padding inside each block.
    pub block_padding_y: f32,
    /// Horizontal padding inside each block.
    pub block_padding_x: f32,
    /// Extra horizontal indent per nesting level (list items).
    pub nested_block_indent: f32,
    /// Gap between list marker and its text content.
    pub list_marker_gap: f32,
    /// Minimum width of the bullet list marker column.
    pub list_marker_width: f32,
    /// Minimum width of the ordered-list marker column.
    pub ordered_list_marker_width: f32,
    /// Width and height of the interactive task-list checkbox.
    pub task_checkbox_size: f32,
    /// Corner radius of the task-list checkbox.
    pub task_checkbox_radius: f32,
    /// Border width of the task-list checkbox.
    pub task_checkbox_border_width: f32,
    /// Checkmark font size inside the task-list checkbox.
    pub task_checkbox_check_size: f32,
    /// Extra padding below H1 text.
    pub h1_padding_bottom: f32,
    /// Margin below the H1 bottom border.
    pub h1_margin_bottom: f32,
    /// Width of the text-editing cursor (caret).
    pub cursor_width: f32,
    /// Thickness of the underline decoration.
    pub underline_thickness: f32,
    /// H1 bottom-border thickness.
    pub h1_border_width: f32,
    /// Quote block left-border thickness.
    pub quote_border_width: f32,
    /// Extra left padding between quote border and text.
    pub quote_padding_left: f32,
    /// Horizontal padding inside editor-level callout shells.
    pub callout_padding_x: f32,
    /// Vertical padding inside editor-level callout shells.
    pub callout_padding_y: f32,
    /// Vertical gap between callout body rows.
    pub callout_body_gap: f32,
    /// Corner radius of editor-level callout shells.
    pub callout_radius: f32,
    /// Accent border width of editor-level callout shells.
    pub callout_border_width: f32,
    /// Gap between callout icon and header text.
    pub callout_header_gap: f32,
    /// Vertical margin between the callout header row and the first body row.
    pub callout_header_margin_bottom: f32,
    /// Horizontal padding inside footnote grouping shells.
    pub footnote_padding_x: f32,
    /// Vertical padding inside footnote grouping shells.
    pub footnote_padding_y: f32,
    /// Corner radius of footnote grouping shells.
    pub footnote_radius: f32,
    /// Horizontal padding inside the footnote ordinal badge.
    pub footnote_badge_padding_x: f32,
    /// Vertical padding inside the footnote ordinal badge.
    pub footnote_badge_padding_y: f32,
    /// Thickness of the separator block line.
    pub separator_thickness: f32,
    /// Extra horizontal inset applied to separator blocks.
    pub separator_inset_x: f32,
    /// Vertical margin around separator blocks.
    pub separator_margin_y: f32,
    /// Vertical padding inside a code block.
    pub code_block_padding_y: f32,
    /// Horizontal padding inside a code block.
    pub code_block_padding_x: f32,
    /// Horizontal padding around inline code background quads.
    pub code_bg_pad_x: f32,
    /// Vertical padding around inline code background quads.
    pub code_bg_pad_y: f32,
    /// Corner radius for inline code background quads.
    pub code_bg_radius: f32,
    /// Width of the code-block language input.
    pub code_language_input_width: f32,
    /// Text layout height inside the code-block language input.
    pub code_language_input_height: f32,
    /// Horizontal padding inside the code-block language input.
    pub code_language_input_padding_x: f32,
    /// Vertical padding inside the code-block language input.
    pub code_language_input_padding_y: f32,
    /// Corner radius of the code-block language input.
    pub code_language_input_radius: f32,
    /// Border width of the code-block language input.
    pub code_language_input_border_width: f32,
    /// Gap between code text and the language input.
    pub code_language_input_gap: f32,
    /// Horizontal padding inside native table cells.
    pub table_cell_padding_x: f32,
    /// Vertical padding inside native table cells.
    pub table_cell_padding_y: f32,
    /// Minimum height of native table cells.
    pub table_cell_min_height: f32,
    /// Width of the append-column control and height of the append-row control.
    pub table_append_button_extent: f32,
    /// Inset padding around rendered-mode native table append controls.
    pub table_append_button_inset: f32,
    /// Invisible activation overlap that keeps append controls easy to hover.
    pub table_append_activation_band: f32,
    /// Corner radius of rendered images and image placeholders.
    pub image_radius: f32,
    /// Maximum height of rendered root-paragraph images.
    pub image_root_max_height: f32,
    /// Maximum height of rendered table-cell images.
    pub image_cell_max_height: f32,
    /// Default placeholder height for rendered root-paragraph images.
    pub image_root_placeholder_height: f32,
    /// Default placeholder height for rendered table-cell images.
    pub image_cell_placeholder_height: f32,
    /// Vertical gap between a rendered image and its caption.
    pub image_caption_gap: f32,
    /// Width of the custom scrollbar thumb.
    pub scrollbar_width: f32,
    /// Distance of the scrollbar thumb from the right edge.
    pub scrollbar_right: f32,
    /// Viewport width at which the content column starts shrinking.
    pub centered_shrink_start: f32,
    /// Viewport width at which the content column reaches minimum ratio.
    pub centered_shrink_end: f32,
    /// Minimum content-column width as a fraction of available width.
    pub centered_min_ratio: f32,
    /// Maximum content-column width after responsive centering.
    pub centered_max_width: f32,
    /// Width of the unsaved-changes dialog.
    pub dialog_width: f32,
    /// Padding inside the unsaved-changes dialog.
    pub dialog_padding: f32,
    /// Gap between dialog sections.
    pub dialog_gap: f32,
    /// Corner radius of the unsaved-changes dialog.
    pub dialog_radius: f32,
    /// Border width of the unsaved-changes dialog.
    pub dialog_border_width: f32,
    /// Height of dialog action buttons.
    pub dialog_button_height: f32,
    /// Gap between dialog action buttons.
    pub dialog_button_gap: f32,
    /// Horizontal padding inside dialog action buttons.
    pub dialog_button_padding_x: f32,
    /// Height reserved for the in-window fallback menu bar.
    pub menu_bar_height: f32,
    /// Horizontal padding inside the in-window fallback menu bar.
    pub menu_bar_padding_x: f32,
    /// Vertical padding inside the in-window fallback menu bar.
    pub menu_bar_padding_y: f32,
    /// Gap between top-level menu buttons.
    pub menu_bar_gap: f32,
    /// Minimum width of each top-level menu button.
    pub menu_bar_button_width: f32,
    /// Height of each top-level menu button.
    pub menu_bar_button_height: f32,
    /// Horizontal padding inside top-level menu buttons.
    pub menu_bar_button_padding_x: f32,
    /// Corner radius of top-level menu buttons.
    pub menu_bar_button_radius: f32,
    /// Text size used by menu labels.
    pub menu_text_size: f32,
    /// Top position of the in-window fallback floating menu panel.
    pub menu_panel_top: f32,
    /// Width of the in-window fallback floating menu panel.
    pub menu_panel_width: f32,
    /// Padding inside floating menu panels.
    pub menu_panel_padding: f32,
    /// Gap between items inside floating menu panels.
    pub menu_panel_gap: f32,
    /// Corner radius of floating menu panels.
    pub menu_panel_radius: f32,
    /// Height of each floating menu item.
    pub menu_item_height: f32,
    /// Horizontal padding inside floating menu items.
    pub menu_item_padding_x: f32,
    /// Corner radius of floating menu items.
    pub menu_item_radius: f32,
    /// Horizontal margin around menu separators.
    pub menu_separator_margin_x: f32,
    /// Vertical margin around menu separators.
    pub menu_separator_margin_y: f32,
    /// Height of menu separators.
    pub menu_separator_height: f32,
    /// Width of the root insert context menu panel.
    pub context_menu_panel_width: f32,
    /// Width of the insert-submenu panel.
    pub context_menu_submenu_width: f32,
    /// Horizontal gap between a context menu and its submenu.
    pub context_menu_submenu_gap: f32,
    /// Width of the table-axis context menu panel.
    pub context_menu_axis_panel_width: f32,
    /// Maximum width of the table-insert dialog.
    pub table_insert_dialog_width: f32,
    /// Gap between table-insert stepper label and controls.
    pub table_insert_stepper_gap: f32,
    /// Size of table-insert stepper buttons.
    pub table_insert_stepper_button_size: f32,
    /// Minimum width of the table-insert stepper value pill.
    pub table_insert_stepper_value_min_width: f32,
    /// Horizontal padding inside the table-insert stepper value pill.
    pub table_insert_stepper_value_padding_x: f32,
    /// Corner radius of table-insert stepper controls.
    pub table_insert_stepper_radius: f32,
    /// Left inset of the view-mode toggle.
    pub view_mode_toggle_left: f32,
    /// Bottom inset of the view-mode toggle.
    pub view_mode_toggle_bottom: f32,
    /// Horizontal padding inside the view-mode toggle.
    pub view_mode_toggle_padding_x: f32,
    /// Vertical padding inside the view-mode toggle.
    pub view_mode_toggle_padding_y: f32,
    /// Minimum width of the view-mode toggle.
    pub view_mode_toggle_min_width: f32,
    /// Corner radius of the view-mode toggle.
    pub view_mode_toggle_radius: f32,
    /// Border width of the view-mode toggle.
    pub view_mode_toggle_border_width: f32,
    /// Text size of the view-mode toggle.
    pub view_mode_toggle_text_size: f32,
    /// Height of the status bar.
    pub status_bar_height: f32,
    /// Horizontal padding inside the status bar.
    pub status_bar_padding_x: f32,
    /// Gap between items in the status bar.
    pub status_bar_item_gap: f32,
    /// Font size for status bar text.
    pub status_bar_text_size: f32,
}

/// All configurable typography settings (font sizes, weights, line heights).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeTypography {
    /// Default body text font size.
    pub text_size: f32,
    /// Default body text line height as a ratio of font size.
    pub text_line_height: f32,
    /// H1 heading font size.
    pub h1_size: f32,
    /// H1 heading font weight.
    pub h1_weight: FontWeightDef,
    /// H2 heading font size.
    pub h2_size: f32,
    /// H2 heading font weight.
    pub h2_weight: FontWeightDef,
    /// H3 heading font size.
    pub h3_size: f32,
    /// H3 heading font weight.
    pub h3_weight: FontWeightDef,
    /// H4 heading font size.
    pub h4_size: f32,
    /// H4 heading font weight.
    pub h4_weight: FontWeightDef,
    /// H5 heading font size.
    pub h5_size: f32,
    /// H5 heading font weight.
    pub h5_weight: FontWeightDef,
    /// H6 heading font size.
    pub h6_size: f32,
    /// H6 heading font weight.
    pub h6_weight: FontWeightDef,
    /// Code-block text font size.
    pub code_size: f32,
    /// Dialog title font size.
    pub dialog_title_size: f32,
    /// Dialog title font weight.
    pub dialog_title_weight: FontWeightDef,
    /// Dialog body font size.
    pub dialog_body_size: f32,
    /// Dialog body font weight.
    pub dialog_body_weight: FontWeightDef,
    /// Dialog button font size.
    pub dialog_button_size: f32,
    /// Dialog button font weight.
    pub dialog_button_weight: FontWeightDef,
}

/// Placeholder text shown in empty interactive elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Placeholders {
    /// Text shown in an empty focused block.
    pub empty_editing: String,
}

/// Deserialization adapter for `ThemeColors` with backward-compatible defaults.
#[derive(Deserialize)]
struct ThemeColorsDe {
    editor_background: Hsla,
    source_mode_block_bg: Option<Hsla>,
    block_focused_bg: Option<Hsla>,
    comment_bg: Option<Hsla>,
    text_default: Hsla,
    text_link: Option<Hsla>,
    text_placeholder: Hsla,
    text_h1: Hsla,
    text_h2: Hsla,
    text_h3: Hsla,
    text_h4: Hsla,
    text_h5: Hsla,
    text_h6: Hsla,
    border_h1: Hsla,
    border_h2: Option<Hsla>,
    text_quote: Hsla,
    border_quote: Hsla,
    callout_note_bg: Option<Hsla>,
    callout_note_border: Option<Hsla>,
    callout_tip_bg: Option<Hsla>,
    callout_tip_border: Option<Hsla>,
    callout_important_bg: Option<Hsla>,
    callout_important_border: Option<Hsla>,
    callout_warning_bg: Option<Hsla>,
    callout_warning_border: Option<Hsla>,
    callout_caution_bg: Option<Hsla>,
    callout_caution_border: Option<Hsla>,
    footnote_bg: Option<Hsla>,
    footnote_border: Option<Hsla>,
    footnote_badge_bg: Option<Hsla>,
    footnote_badge_text: Option<Hsla>,
    footnote_backref: Option<Hsla>,
    task_checkbox_border: Option<Hsla>,
    task_checkbox_bg: Option<Hsla>,
    task_checkbox_checked_bg: Option<Hsla>,
    task_checkbox_check: Option<Hsla>,
    separator_color: Option<Hsla>,
    code_bg: Option<Hsla>,
    code_text: Hsla,
    code_language_input_bg: Option<Hsla>,
    code_language_input_border: Option<Hsla>,
    code_language_input_text: Option<Hsla>,
    code_language_input_placeholder: Option<Hsla>,
    code_syntax_comment: Option<Hsla>,
    code_syntax_keyword: Option<Hsla>,
    code_syntax_string: Option<Hsla>,
    code_syntax_number: Option<Hsla>,
    code_syntax_type: Option<Hsla>,
    code_syntax_function: Option<Hsla>,
    code_syntax_constant: Option<Hsla>,
    code_syntax_variable: Option<Hsla>,
    code_syntax_property: Option<Hsla>,
    code_syntax_operator: Option<Hsla>,
    code_syntax_punctuation: Option<Hsla>,
    table_border: Option<Hsla>,
    table_header_bg: Option<Hsla>,
    table_cell_bg: Option<Hsla>,
    table_cell_active_outline: Option<Hsla>,
    table_axis_preview_bg: Option<Hsla>,
    table_axis_selected_bg: Option<Hsla>,
    table_append_button_bg: Option<Hsla>,
    table_append_button_hover: Option<Hsla>,
    table_append_button_text: Option<Hsla>,
    image_placeholder_bg: Option<Hsla>,
    image_placeholder_border: Option<Hsla>,
    image_placeholder_text: Option<Hsla>,
    image_caption_text: Option<Hsla>,
    scrollbar_thumb: Hsla,
    cursor: Hsla,
    selection: Hsla,
    dialog_backdrop: Hsla,
    dialog_surface: Hsla,
    dialog_border: Hsla,
    dialog_title: Hsla,
    dialog_body: Hsla,
    dialog_muted: Hsla,
    dialog_primary_button_bg: Hsla,
    dialog_primary_button_hover: Hsla,
    dialog_primary_button_text: Hsla,
    dialog_secondary_button_bg: Hsla,
    dialog_secondary_button_hover: Hsla,
    dialog_secondary_button_text: Hsla,
    dialog_danger_button_bg: Hsla,
    dialog_danger_button_hover: Hsla,
    dialog_danger_button_text: Hsla,
    status_bar_background: Option<Hsla>,
    status_bar_text: Option<Hsla>,
    status_bar_text_dim: Option<Hsla>,
    status_bar_button_hover: Option<Hsla>,
    chrome_background: Option<Hsla>,
    chrome_hover: Option<Hsla>,
    sidebar_background: Option<Hsla>,
    tab_strip_background: Option<Hsla>,
    tab_active_background: Option<Hsla>,
}

impl<'de> Deserialize<'de> for ThemeColors {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = ThemeColorsDe::deserialize(deserializer)?;
        // 新 chrome token 对旧主题保持可选；缺省时从既有 surface 派生，避免主题升级后出现黑块。
        let chrome_background = raw.chrome_background.unwrap_or(raw.dialog_surface);
        let chrome_hover = raw
            .chrome_hover
            .unwrap_or(raw.dialog_secondary_button_hover);
        let sidebar_background = raw.sidebar_background.unwrap_or(chrome_background);
        let tab_strip_background = raw.tab_strip_background.unwrap_or(chrome_background);
        let tab_active_background = raw.tab_active_background.unwrap_or(raw.editor_background);
        Ok(Self {
            editor_background: raw.editor_background,
            source_mode_block_bg: raw
                .source_mode_block_bg
                .or(raw.block_focused_bg)
                .unwrap_or_else(|| Hsla::from(rgba(0x313131ff))),
            comment_bg: raw
                .comment_bg
                .unwrap_or_else(|| Hsla::from(rgba(0xfbbf2426))),
            text_default: raw.text_default,
            text_link: raw
                .text_link
                .unwrap_or_else(|| Hsla::from(rgba(0x60a5faff))),
            text_placeholder: raw.text_placeholder,
            text_h1: raw.text_h1,
            text_h2: raw.text_h2,
            text_h3: raw.text_h3,
            text_h4: raw.text_h4,
            text_h5: raw.text_h5,
            text_h6: raw.text_h6,
            border_h1: raw.border_h1,
            border_h2: raw
                .border_h2
                .unwrap_or_else(|| Hsla::from(rgba(0xe0e0e0cc))),
            text_quote: raw.text_quote,
            border_quote: raw.border_quote,
            callout_note_bg: raw
                .callout_note_bg
                .unwrap_or_else(|| Hsla::from(rgba(0x94a3b81f))),
            callout_note_border: raw
                .callout_note_border
                .unwrap_or_else(|| Hsla::from(rgba(0x94a3b4ff))),
            callout_tip_bg: raw
                .callout_tip_bg
                .unwrap_or_else(|| Hsla::from(rgba(0x1d4ed81f))),
            callout_tip_border: raw
                .callout_tip_border
                .unwrap_or_else(|| Hsla::from(rgba(0x60a5faff))),
            callout_important_bg: raw
                .callout_important_bg
                .unwrap_or_else(|| Hsla::from(rgba(0xca8a041f))),
            callout_important_border: raw
                .callout_important_border
                .unwrap_or_else(|| Hsla::from(rgba(0xfbbf24ff))),
            callout_warning_bg: raw
                .callout_warning_bg
                .unwrap_or_else(|| Hsla::from(rgba(0xfb71851f))),
            callout_warning_border: raw
                .callout_warning_border
                .unwrap_or_else(|| Hsla::from(rgba(0xfb7185ff))),
            callout_caution_bg: raw
                .callout_caution_bg
                .unwrap_or_else(|| Hsla::from(rgba(0xdc26261f))),
            callout_caution_border: raw
                .callout_caution_border
                .unwrap_or_else(|| Hsla::from(rgba(0xf87171ff))),
            footnote_bg: raw
                .footnote_bg
                .unwrap_or_else(|| Hsla::from(rgba(0x212124ff))),
            footnote_border: raw
                .footnote_border
                .unwrap_or_else(|| Hsla::from(rgba(0x71717a52))),
            footnote_badge_bg: raw
                .footnote_badge_bg
                .unwrap_or_else(|| Hsla::from(rgba(0xa1a1aa24))),
            footnote_badge_text: raw
                .footnote_badge_text
                .unwrap_or_else(|| Hsla::from(rgba(0xd4d4d8cc))),
            footnote_backref: raw
                .footnote_backref
                .unwrap_or_else(|| Hsla::from(rgba(0xa1a1aaff))),
            task_checkbox_border: raw
                .task_checkbox_border
                .unwrap_or_else(|| Hsla::from(rgba(0x71717aff))),
            task_checkbox_bg: raw
                .task_checkbox_bg
                .unwrap_or_else(|| Hsla::from(rgba(0x00000000))),
            task_checkbox_checked_bg: raw
                .task_checkbox_checked_bg
                .unwrap_or_else(|| Hsla::from(rgba(0xf0efedff))),
            task_checkbox_check: raw
                .task_checkbox_check
                .unwrap_or_else(|| Hsla::from(rgba(0x18181bff))),
            separator_color: raw
                .separator_color
                .unwrap_or_else(|| Hsla::from(rgba(0x71717aff))),
            code_bg: raw.code_bg.unwrap_or_else(|| Hsla::from(rgba(0x111827ff))),
            code_text: raw.code_text,
            code_language_input_bg: raw
                .code_language_input_bg
                .unwrap_or_else(|| Hsla::from(rgba(0x343941ff))),
            code_language_input_border: raw
                .code_language_input_border
                .unwrap_or_else(|| Hsla::from(rgba(0x4b5563cc))),
            code_language_input_text: raw
                .code_language_input_text
                .unwrap_or_else(|| Hsla::from(rgba(0xe5e7ebff))),
            code_language_input_placeholder: raw
                .code_language_input_placeholder
                .unwrap_or_else(|| Hsla::from(rgba(0x9ca3afcc))),
            code_syntax_comment: raw
                .code_syntax_comment
                .unwrap_or_else(|| Hsla::from(rgba(0x565f89ff))),
            code_syntax_keyword: raw
                .code_syntax_keyword
                .unwrap_or_else(|| Hsla::from(rgba(0xbb9af7ff))),
            code_syntax_string: raw
                .code_syntax_string
                .unwrap_or_else(|| Hsla::from(rgba(0x9ece6aff))),
            code_syntax_number: raw
                .code_syntax_number
                .unwrap_or_else(|| Hsla::from(rgba(0xff9e64ff))),
            code_syntax_type: raw
                .code_syntax_type
                .unwrap_or_else(|| Hsla::from(rgba(0x2ac3deff))),
            code_syntax_function: raw
                .code_syntax_function
                .unwrap_or_else(|| Hsla::from(rgba(0x7aa2f7ff))),
            code_syntax_constant: raw
                .code_syntax_constant
                .unwrap_or_else(|| Hsla::from(rgba(0xffd166ff))),
            code_syntax_variable: raw
                .code_syntax_variable
                .unwrap_or_else(|| Hsla::from(rgba(0xe5e9f0ff))),
            code_syntax_property: raw
                .code_syntax_property
                .unwrap_or_else(|| Hsla::from(rgba(0x7dcfffcc))),
            code_syntax_operator: raw
                .code_syntax_operator
                .unwrap_or_else(|| Hsla::from(rgba(0x89ddffff))),
            code_syntax_punctuation: raw
                .code_syntax_punctuation
                .unwrap_or_else(|| Hsla::from(rgba(0x9aa5ceff))),
            table_border: raw
                .table_border
                .unwrap_or_else(|| Hsla::from(rgba(0x3f3f46ff))),
            table_header_bg: raw
                .table_header_bg
                .unwrap_or_else(|| Hsla::from(rgba(0x232326ff))),
            table_cell_bg: raw
                .table_cell_bg
                .unwrap_or_else(|| Hsla::from(rgba(0x1d1d20ff))),
            table_cell_active_outline: raw
                .table_cell_active_outline
                .unwrap_or_else(|| Hsla::from(rgba(0x60a5faff))),
            table_axis_preview_bg: raw
                .table_axis_preview_bg
                .unwrap_or_else(|| Hsla::from(rgba(0xf4f4f51a))),
            table_axis_selected_bg: raw
                .table_axis_selected_bg
                .unwrap_or_else(|| Hsla::from(rgba(0xf4f4f533))),
            table_append_button_bg: raw
                .table_append_button_bg
                .unwrap_or_else(|| Hsla::from(rgba(0x27272aff))),
            table_append_button_hover: raw
                .table_append_button_hover
                .unwrap_or_else(|| Hsla::from(rgba(0x3f3f46ff))),
            table_append_button_text: raw
                .table_append_button_text
                .unwrap_or_else(|| Hsla::from(rgba(0xf4f4f5ff))),
            image_placeholder_bg: raw
                .image_placeholder_bg
                .unwrap_or_else(|| Hsla::from(rgba(0x202024ff))),
            image_placeholder_border: raw
                .image_placeholder_border
                .unwrap_or_else(|| Hsla::from(rgba(0x52525bff))),
            image_placeholder_text: raw
                .image_placeholder_text
                .unwrap_or_else(|| Hsla::from(rgba(0xd4d4d8ff))),
            image_caption_text: raw
                .image_caption_text
                .unwrap_or_else(|| Hsla::from(rgba(0xa1a1aaff))),
            scrollbar_thumb: raw.scrollbar_thumb,
            cursor: raw.cursor,
            selection: raw.selection,
            dialog_backdrop: raw.dialog_backdrop,
            dialog_surface: raw.dialog_surface,
            dialog_border: raw.dialog_border,
            dialog_title: raw.dialog_title,
            dialog_body: raw.dialog_body,
            dialog_muted: raw.dialog_muted,
            dialog_primary_button_bg: raw.dialog_primary_button_bg,
            dialog_primary_button_hover: raw.dialog_primary_button_hover,
            dialog_primary_button_text: raw.dialog_primary_button_text,
            dialog_secondary_button_bg: raw.dialog_secondary_button_bg,
            dialog_secondary_button_hover: raw.dialog_secondary_button_hover,
            dialog_secondary_button_text: raw.dialog_secondary_button_text,
            dialog_danger_button_bg: raw.dialog_danger_button_bg,
            dialog_danger_button_hover: raw.dialog_danger_button_hover,
            dialog_danger_button_text: raw.dialog_danger_button_text,
            status_bar_background: raw
                .status_bar_background
                .unwrap_or_else(|| Hsla::from(rgba(0x1c1c1fff))),
            status_bar_text: raw
                .status_bar_text
                .unwrap_or_else(|| Hsla::from(rgba(0xd4d4d8cc))),
            status_bar_text_dim: raw
                .status_bar_text_dim
                .unwrap_or_else(|| Hsla::from(rgba(0x71717aff))),
            status_bar_button_hover: raw
                .status_bar_button_hover
                .unwrap_or_else(|| Hsla::from(rgba(0x3f3f46ff))),
            chrome_background,
            chrome_hover,
            sidebar_background,
            tab_strip_background,
            tab_active_background,
        })
    }
}
