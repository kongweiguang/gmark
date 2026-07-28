// @author kongweiguang

use super::*;

/// Deserialization adapter for `ThemeDimensions` with backward-compatible defaults.
#[derive(Deserialize)]
struct ThemeDimensionsDe {
    editor_padding: f32,
    block_gap: f32,
    block_min_height: f32,
    block_padding_y: f32,
    block_padding_x: f32,
    nested_block_indent: f32,
    list_marker_gap: f32,
    list_marker_width: f32,
    ordered_list_marker_width: f32,
    task_checkbox_size: Option<f32>,
    task_checkbox_radius: Option<f32>,
    task_checkbox_border_width: Option<f32>,
    task_checkbox_check_size: Option<f32>,
    h1_padding_bottom: f32,
    h1_margin_bottom: f32,
    cursor_width: f32,
    underline_thickness: f32,
    h1_border_width: f32,
    quote_border_width: f32,
    quote_padding_left: f32,
    callout_padding_x: Option<f32>,
    callout_padding_y: Option<f32>,
    callout_body_gap: Option<f32>,
    callout_radius: Option<f32>,
    callout_border_width: Option<f32>,
    callout_header_gap: Option<f32>,
    callout_header_margin_bottom: Option<f32>,
    footnote_padding_x: Option<f32>,
    footnote_padding_y: Option<f32>,
    footnote_radius: Option<f32>,
    footnote_badge_padding_x: Option<f32>,
    footnote_badge_padding_y: Option<f32>,
    separator_thickness: Option<f32>,
    separator_inset_x: Option<f32>,
    separator_margin_y: Option<f32>,
    code_block_padding_y: f32,
    code_block_padding_x: f32,
    code_bg_pad_x: f32,
    code_bg_pad_y: f32,
    code_bg_radius: f32,
    code_language_input_width: Option<f32>,
    code_language_input_height: Option<f32>,
    code_language_input_padding_x: Option<f32>,
    code_language_input_padding_y: Option<f32>,
    code_language_input_radius: Option<f32>,
    code_language_input_border_width: Option<f32>,
    code_language_input_gap: Option<f32>,
    table_cell_padding_x: Option<f32>,
    table_cell_padding_y: Option<f32>,
    table_cell_min_height: Option<f32>,
    table_append_button_extent: Option<f32>,
    table_append_button_inset: Option<f32>,
    table_append_activation_band: Option<f32>,
    image_radius: Option<f32>,
    image_root_max_height: Option<f32>,
    image_cell_max_height: Option<f32>,
    image_root_placeholder_height: Option<f32>,
    image_cell_placeholder_height: Option<f32>,
    image_caption_gap: Option<f32>,
    scrollbar_width: f32,
    scrollbar_right: f32,
    centered_shrink_start: f32,
    centered_shrink_end: f32,
    centered_min_ratio: f32,
    centered_max_width: Option<f32>,
    dialog_width: f32,
    dialog_padding: f32,
    dialog_gap: f32,
    dialog_radius: f32,
    dialog_border_width: f32,
    dialog_button_height: f32,
    dialog_button_gap: f32,
    dialog_button_padding_x: f32,
    menu_bar_height: Option<f32>,
    menu_bar_padding_x: Option<f32>,
    menu_bar_padding_y: Option<f32>,
    menu_bar_gap: Option<f32>,
    menu_bar_button_width: Option<f32>,
    menu_bar_button_height: Option<f32>,
    menu_bar_button_padding_x: Option<f32>,
    menu_bar_button_radius: Option<f32>,
    menu_text_size: Option<f32>,
    menu_panel_top: Option<f32>,
    menu_panel_width: Option<f32>,
    menu_panel_padding: Option<f32>,
    menu_panel_gap: Option<f32>,
    menu_panel_radius: Option<f32>,
    menu_item_height: Option<f32>,
    menu_item_padding_x: Option<f32>,
    menu_item_radius: Option<f32>,
    menu_separator_margin_x: Option<f32>,
    menu_separator_margin_y: Option<f32>,
    menu_separator_height: Option<f32>,
    context_menu_panel_width: Option<f32>,
    context_menu_submenu_width: Option<f32>,
    context_menu_submenu_gap: Option<f32>,
    context_menu_axis_panel_width: Option<f32>,
    table_insert_dialog_width: Option<f32>,
    table_insert_stepper_gap: Option<f32>,
    table_insert_stepper_button_size: Option<f32>,
    table_insert_stepper_value_min_width: Option<f32>,
    table_insert_stepper_value_padding_x: Option<f32>,
    table_insert_stepper_radius: Option<f32>,
    view_mode_toggle_left: Option<f32>,
    view_mode_toggle_bottom: Option<f32>,
    view_mode_toggle_padding_x: Option<f32>,
    view_mode_toggle_padding_y: Option<f32>,
    view_mode_toggle_min_width: Option<f32>,
    view_mode_toggle_radius: Option<f32>,
    view_mode_toggle_border_width: Option<f32>,
    view_mode_toggle_text_size: Option<f32>,
    status_bar_height: Option<f32>,
    status_bar_padding_x: Option<f32>,
    status_bar_item_gap: Option<f32>,
    status_bar_text_size: Option<f32>,
}

impl<'de> Deserialize<'de> for ThemeDimensions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = ThemeDimensionsDe::deserialize(deserializer)?;
        Ok(Self {
            editor_padding: raw.editor_padding,
            block_gap: raw.block_gap,
            block_min_height: raw.block_min_height,
            block_padding_y: raw.block_padding_y,
            block_padding_x: raw.block_padding_x,
            nested_block_indent: raw.nested_block_indent,
            list_marker_gap: raw.list_marker_gap,
            list_marker_width: raw.list_marker_width,
            ordered_list_marker_width: raw.ordered_list_marker_width,
            task_checkbox_size: raw.task_checkbox_size.unwrap_or(14.0),
            task_checkbox_radius: raw.task_checkbox_radius.unwrap_or(4.0),
            task_checkbox_border_width: raw.task_checkbox_border_width.unwrap_or(1.0),
            task_checkbox_check_size: raw.task_checkbox_check_size.unwrap_or(10.0),
            h1_padding_bottom: raw.h1_padding_bottom,
            h1_margin_bottom: raw.h1_margin_bottom,
            cursor_width: raw.cursor_width,
            underline_thickness: raw.underline_thickness,
            h1_border_width: raw.h1_border_width,
            quote_border_width: raw.quote_border_width,
            quote_padding_left: raw.quote_padding_left,
            callout_padding_x: raw.callout_padding_x.unwrap_or(14.0),
            callout_padding_y: raw.callout_padding_y.unwrap_or(10.0),
            callout_body_gap: raw.callout_body_gap.unwrap_or(8.0),
            callout_radius: raw.callout_radius.unwrap_or(10.0),
            callout_border_width: raw.callout_border_width.unwrap_or(4.0),
            callout_header_gap: raw.callout_header_gap.unwrap_or(6.0),
            callout_header_margin_bottom: raw.callout_header_margin_bottom.unwrap_or(6.0),
            footnote_padding_x: raw.footnote_padding_x.unwrap_or(10.0),
            footnote_padding_y: raw.footnote_padding_y.unwrap_or(6.0),
            footnote_radius: raw.footnote_radius.unwrap_or(6.0),
            footnote_badge_padding_x: raw.footnote_badge_padding_x.unwrap_or(4.0),
            footnote_badge_padding_y: raw.footnote_badge_padding_y.unwrap_or(1.0),
            separator_thickness: raw.separator_thickness.unwrap_or(1.0),
            separator_inset_x: raw.separator_inset_x.unwrap_or(40.0),
            separator_margin_y: raw.separator_margin_y.unwrap_or(10.0),
            code_block_padding_y: raw.code_block_padding_y,
            code_block_padding_x: raw.code_block_padding_x,
            code_bg_pad_x: raw.code_bg_pad_x,
            code_bg_pad_y: raw.code_bg_pad_y,
            code_bg_radius: raw.code_bg_radius,
            code_language_input_width: raw.code_language_input_width.unwrap_or(156.0),
            code_language_input_height: raw.code_language_input_height.unwrap_or(18.0),
            code_language_input_padding_x: raw.code_language_input_padding_x.unwrap_or(8.0),
            code_language_input_padding_y: raw.code_language_input_padding_y.unwrap_or(3.0),
            code_language_input_radius: raw.code_language_input_radius.unwrap_or(6.0),
            code_language_input_border_width: raw.code_language_input_border_width.unwrap_or(1.0),
            code_language_input_gap: raw.code_language_input_gap.unwrap_or(8.0),
            table_cell_padding_x: raw.table_cell_padding_x.unwrap_or(10.0),
            table_cell_padding_y: raw.table_cell_padding_y.unwrap_or(8.0),
            table_cell_min_height: raw.table_cell_min_height.unwrap_or(42.0),
            table_append_button_extent: raw.table_append_button_extent.unwrap_or(16.0),
            table_append_button_inset: raw.table_append_button_inset.unwrap_or(8.0),
            table_append_activation_band: raw.table_append_activation_band.unwrap_or(18.0),
            image_radius: raw.image_radius.unwrap_or(12.0),
            image_root_max_height: raw.image_root_max_height.unwrap_or(420.0),
            image_cell_max_height: raw.image_cell_max_height.unwrap_or(180.0),
            image_root_placeholder_height: raw.image_root_placeholder_height.unwrap_or(260.0),
            image_cell_placeholder_height: raw.image_cell_placeholder_height.unwrap_or(120.0),
            image_caption_gap: raw.image_caption_gap.unwrap_or(8.0),
            scrollbar_width: raw.scrollbar_width,
            scrollbar_right: raw.scrollbar_right,
            centered_shrink_start: raw.centered_shrink_start,
            centered_shrink_end: raw.centered_shrink_end,
            centered_min_ratio: raw.centered_min_ratio,
            centered_max_width: raw.centered_max_width.unwrap_or(1200.0),
            dialog_width: raw.dialog_width,
            dialog_padding: raw.dialog_padding,
            dialog_gap: raw.dialog_gap,
            dialog_radius: raw.dialog_radius,
            dialog_border_width: raw.dialog_border_width,
            dialog_button_height: raw.dialog_button_height,
            dialog_button_gap: raw.dialog_button_gap,
            dialog_button_padding_x: raw.dialog_button_padding_x,
            menu_bar_height: raw.menu_bar_height.unwrap_or(32.0),
            menu_bar_padding_x: raw.menu_bar_padding_x.unwrap_or(10.0),
            menu_bar_padding_y: raw.menu_bar_padding_y.unwrap_or(4.0),
            menu_bar_gap: raw.menu_bar_gap.unwrap_or(2.0),
            menu_bar_button_width: raw.menu_bar_button_width.unwrap_or(48.0),
            menu_bar_button_height: raw.menu_bar_button_height.unwrap_or(24.0),
            menu_bar_button_padding_x: raw.menu_bar_button_padding_x.unwrap_or(8.0),
            menu_bar_button_radius: raw.menu_bar_button_radius.unwrap_or(5.0),
            menu_text_size: raw.menu_text_size.unwrap_or(12.0),
            menu_panel_top: raw.menu_panel_top.unwrap_or(30.0),
            menu_panel_width: raw.menu_panel_width.unwrap_or(180.0),
            menu_panel_padding: raw.menu_panel_padding.unwrap_or(4.0),
            menu_panel_gap: raw.menu_panel_gap.unwrap_or(1.0),
            menu_panel_radius: raw.menu_panel_radius.unwrap_or(10.0),
            menu_item_height: raw.menu_item_height.unwrap_or(28.0),
            menu_item_padding_x: raw.menu_item_padding_x.unwrap_or(8.0),
            menu_item_radius: raw.menu_item_radius.unwrap_or(7.0),
            menu_separator_margin_x: raw.menu_separator_margin_x.unwrap_or(6.0),
            menu_separator_margin_y: raw.menu_separator_margin_y.unwrap_or(3.0),
            menu_separator_height: raw.menu_separator_height.unwrap_or(1.0),
            context_menu_panel_width: raw.context_menu_panel_width.unwrap_or(132.0),
            context_menu_submenu_width: raw.context_menu_submenu_width.unwrap_or(148.0),
            context_menu_submenu_gap: raw.context_menu_submenu_gap.unwrap_or(2.0),
            context_menu_axis_panel_width: raw.context_menu_axis_panel_width.unwrap_or(164.0),
            table_insert_dialog_width: raw.table_insert_dialog_width.unwrap_or(380.0),
            table_insert_stepper_gap: raw.table_insert_stepper_gap.unwrap_or(8.0),
            table_insert_stepper_button_size: raw.table_insert_stepper_button_size.unwrap_or(32.0),
            table_insert_stepper_value_min_width: raw
                .table_insert_stepper_value_min_width
                .unwrap_or(56.0),
            table_insert_stepper_value_padding_x: raw
                .table_insert_stepper_value_padding_x
                .unwrap_or(10.0),
            table_insert_stepper_radius: raw.table_insert_stepper_radius.unwrap_or(8.0),
            view_mode_toggle_left: raw.view_mode_toggle_left.unwrap_or(12.0),
            view_mode_toggle_bottom: raw.view_mode_toggle_bottom.unwrap_or(12.0),
            view_mode_toggle_padding_x: raw.view_mode_toggle_padding_x.unwrap_or(8.0),
            view_mode_toggle_padding_y: raw.view_mode_toggle_padding_y.unwrap_or(4.0),
            view_mode_toggle_min_width: raw.view_mode_toggle_min_width.unwrap_or(88.0),
            view_mode_toggle_radius: raw.view_mode_toggle_radius.unwrap_or(999.0),
            view_mode_toggle_border_width: raw.view_mode_toggle_border_width.unwrap_or(1.0),
            view_mode_toggle_text_size: raw.view_mode_toggle_text_size.unwrap_or(11.0),
            status_bar_height: raw.status_bar_height.unwrap_or(24.0),
            status_bar_padding_x: raw.status_bar_padding_x.unwrap_or(12.0),
            status_bar_item_gap: raw.status_bar_item_gap.unwrap_or(12.0),
            status_bar_text_size: raw.status_bar_text_size.unwrap_or(11.0),
        })
    }
}

/// Top-level theme combining colors, dimensions, typography and placeholders.
///
/// Serializes the built-in theme tokens for fixtures and export tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub colors: ThemeColors,
    pub dimensions: ThemeDimensions,
    pub typography: ThemeTypography,
    pub placeholders: Placeholders,
}

impl Theme {
    /// Returns the Xcode-inspired dark theme.
    pub fn xcode_dark() -> Self {
        Self {
            name: "Xcode Dark".into(),
            colors: ThemeColors {
                editor_background: Hsla::from(rgba(0x1f1f24ff)),
                source_mode_block_bg: Hsla::from(rgba(0x292a30ff)),
                comment_bg: Hsla::from(rgba(0x7f8c9830)),
                text_default: Hsla::from(rgba(0xf5f5f7ff)),
                text_link: Hsla::from(rgba(0x0a84ffff)),
                text_placeholder: Hsla::from(rgba(0x8f8f98cc)),
                text_h1: Hsla::from(rgba(0xf5f5f7ff)),
                text_h2: Hsla::from(rgba(0xf5f5f7ff)),
                text_h3: Hsla::from(rgba(0xf5f5f7ff)),
                text_h4: Hsla::from(rgba(0xf5f5f7ff)),
                text_h5: Hsla::from(rgba(0xf5f5f7ff)),
                text_h6: Hsla::from(rgba(0xf5f5f7ff)),
                border_h1: Hsla::from(rgba(0x5b5c66ff)),
                border_h2: Hsla::from(rgba(0x5b5c66cc)),
                text_quote: Hsla::from(rgba(0xc8c8d0ff)),
                border_quote: Hsla::from(rgba(0x7d7e87ff)),
                callout_note_bg: Hsla::from(rgba(0x94a3b81f)),
                callout_note_border: Hsla::from(rgba(0x94a3b4ff)),
                callout_tip_bg: Hsla::from(rgba(0x1d4ed81f)),
                callout_tip_border: Hsla::from(rgba(0x60a5faff)),
                callout_important_bg: Hsla::from(rgba(0xa78bfa1f)),
                callout_important_border: Hsla::from(rgba(0xa78bfaff)),
                callout_warning_bg: Hsla::from(rgba(0xfb71851f)),
                callout_warning_border: Hsla::from(rgba(0xfb7185ff)),
                callout_caution_bg: Hsla::from(rgba(0xdc26261f)),
                callout_caution_border: Hsla::from(rgba(0xf87171ff)),
                footnote_bg: Hsla::from(rgba(0x292a30ff)),
                footnote_border: Hsla::from(rgba(0x7d7e8752)),
                footnote_badge_bg: Hsla::from(rgba(0xc8c8d024)),
                footnote_badge_text: Hsla::from(rgba(0xc8c8d0cc)),
                footnote_backref: Hsla::from(rgba(0x8f8f98ff)),
                task_checkbox_border: Hsla::from(rgba(0x7d7e87ff)),
                task_checkbox_bg: Hsla::from(rgba(0x00000000)),
                task_checkbox_checked_bg: Hsla::from(rgba(0x0a84ffff)),
                task_checkbox_check: Hsla::from(rgba(0xffffffff)),
                separator_color: Hsla::from(rgba(0x7d7e87ff)),
                code_bg: Hsla::from(rgba(0x202127ff)),
                code_text: Hsla::from(rgba(0xf5f5f7ff)),
                code_language_input_bg: Hsla::from(rgba(0x292a30ff)),
                code_language_input_border: Hsla::from(rgba(0x5b5c66cc)),
                code_language_input_text: Hsla::from(rgba(0xf5f5f7ff)),
                code_language_input_placeholder: Hsla::from(rgba(0x8f8f98cc)),
                code_syntax_comment: Hsla::from(rgba(0x7f8c98ff)),
                code_syntax_keyword: Hsla::from(rgba(0xff7ab2ff)),
                code_syntax_string: Hsla::from(rgba(0xfc6a5dff)),
                code_syntax_number: Hsla::from(rgba(0xd0bf69ff)),
                code_syntax_type: Hsla::from(rgba(0x5dd8ffff)),
                code_syntax_function: Hsla::from(rgba(0x67b7a4ff)),
                code_syntax_constant: Hsla::from(rgba(0xa8a8ffff)),
                code_syntax_variable: Hsla::from(rgba(0xf5f5f7ff)),
                code_syntax_property: Hsla::from(rgba(0x9cdcfeff)),
                code_syntax_operator: Hsla::from(rgba(0xff7ab2ff)),
                code_syntax_punctuation: Hsla::from(rgba(0xc8c8d0ff)),
                table_border: Hsla::from(rgba(0x484950ff)),
                table_header_bg: Hsla::from(rgba(0x292a30ff)),
                table_cell_bg: Hsla::from(rgba(0x23242aff)),
                table_cell_active_outline: Hsla::from(rgba(0x0a84ffff)),
                table_axis_preview_bg: Hsla::from(rgba(0x0a84ff2e)),
                table_axis_selected_bg: Hsla::from(rgba(0x0a84ff55)),
                table_append_button_bg: Hsla::from(rgba(0x303139ff)),
                table_append_button_hover: Hsla::from(rgba(0x3b3c46ff)),
                table_append_button_text: Hsla::from(rgba(0xf5f5f7ff)),
                image_placeholder_bg: Hsla::from(rgba(0x25262cff)),
                image_placeholder_border: Hsla::from(rgba(0x5b5c66ff)),
                image_placeholder_text: Hsla::from(rgba(0xc8c8d0ff)),
                image_caption_text: Hsla::from(rgba(0x8f8f98ff)),
                scrollbar_thumb: Hsla::from(rgba(0xc8c8d0b8)),
                cursor: Hsla::from(rgba(0xf5f5f7ff)),
                selection: Hsla::from(rgba(0x0a84ff4d)),
                dialog_backdrop: Hsla::from(rgba(0x000000b8)),
                dialog_surface: Hsla::from(rgba(0x292a30ff)),
                dialog_border: Hsla::from(rgba(0x484950ff)),
                dialog_title: Hsla::from(rgba(0xf5f5f7ff)),
                dialog_body: Hsla::from(rgba(0xd1d1d6ff)),
                dialog_muted: Hsla::from(rgba(0x8f8f98ff)),
                dialog_primary_button_bg: Hsla::from(rgba(0x0a84ffff)),
                dialog_primary_button_hover: Hsla::from(rgba(0x0077e6ff)),
                dialog_primary_button_text: Hsla::from(rgba(0xffffffff)),
                dialog_secondary_button_bg: Hsla::from(rgba(0x3a3b43ff)),
                dialog_secondary_button_hover: Hsla::from(rgba(0x484950ff)),
                dialog_secondary_button_text: Hsla::from(rgba(0xf5f5f7ff)),
                // Doubles as the destructive menu-item text color (e.g. Delete
                // Row/Column), so it must stay legible on the dark menu surface
                // rather than the muted red used previously.
                dialog_danger_button_bg: Hsla::from(rgba(0xef4444ff)),
                dialog_danger_button_hover: Hsla::from(rgba(0xdc2626ff)),
                dialog_danger_button_text: Hsla::from(rgba(0xfef2f2ff)),
                status_bar_background: Hsla::from(rgba(0x292a30ff)),
                status_bar_text: Hsla::from(rgba(0xd1d1d6cc)),
                status_bar_text_dim: Hsla::from(rgba(0x8f8f98ff)),
                status_bar_button_hover: Hsla::from(rgba(0x3b3c46ff)),
                chrome_background: Hsla::from(rgba(0x24252aff)),
                chrome_hover: Hsla::from(rgba(0x35363fff)),
                sidebar_background: Hsla::from(rgba(0x25262cff)),
                tab_strip_background: Hsla::from(rgba(0x25262cff)),
                tab_active_background: Hsla::from(rgba(0x1f1f24ff)),
            },
            dimensions: ThemeDimensions {
                editor_padding: 24.0,
                block_gap: 6.0,
                block_min_height: 28.0,
                block_padding_y: 4.0,
                block_padding_x: 12.0,
                nested_block_indent: 20.0,
                list_marker_gap: 8.0,
                list_marker_width: 12.0,
                ordered_list_marker_width: 20.0,
                task_checkbox_size: 14.0,
                task_checkbox_radius: 4.0,
                task_checkbox_border_width: 1.0,
                task_checkbox_check_size: 10.0,
                h1_padding_bottom: 4.0,
                h1_margin_bottom: 4.0,
                cursor_width: 2.0,
                underline_thickness: 1.0,
                h1_border_width: 1.0,
                quote_border_width: 3.0,
                quote_padding_left: 12.0,
                callout_padding_x: 14.0,
                callout_padding_y: 10.0,
                callout_body_gap: 8.0,
                callout_radius: 10.0,
                callout_border_width: 4.0,
                callout_header_gap: 6.0,
                callout_header_margin_bottom: 6.0,
                footnote_padding_x: 10.0,
                footnote_padding_y: 6.0,
                footnote_radius: 6.0,
                footnote_badge_padding_x: 4.0,
                footnote_badge_padding_y: 1.0,
                separator_thickness: 1.0,
                separator_inset_x: 40.0,
                separator_margin_y: 10.0,
                code_block_padding_y: 8.0,
                code_block_padding_x: 12.0,
                code_bg_pad_x: 3.0,
                code_bg_pad_y: 1.0,
                code_bg_radius: 4.0,
                code_language_input_width: 156.0,
                code_language_input_height: 18.0,
                code_language_input_padding_x: 8.0,
                code_language_input_padding_y: 3.0,
                code_language_input_radius: 6.0,
                code_language_input_border_width: 1.0,
                code_language_input_gap: 8.0,
                table_cell_padding_x: 10.0,
                table_cell_padding_y: 8.0,
                table_cell_min_height: 42.0,
                table_append_button_extent: 16.0,
                table_append_button_inset: 8.0,
                table_append_activation_band: 18.0,
                image_radius: 12.0,
                image_root_max_height: 420.0,
                image_cell_max_height: 180.0,
                image_root_placeholder_height: 260.0,
                image_cell_placeholder_height: 120.0,
                image_caption_gap: 8.0,
                scrollbar_width: 6.0,
                scrollbar_right: 6.0,
                centered_shrink_start: 1100.0,
                centered_shrink_end: 2200.0,
                centered_min_ratio: 0.58,
                centered_max_width: 1200.0,
                dialog_width: 520.0,
                dialog_padding: 20.0,
                dialog_gap: 14.0,
                dialog_radius: 18.0,
                dialog_border_width: 1.0,
                dialog_button_height: 36.0,
                dialog_button_gap: 10.0,
                dialog_button_padding_x: 14.0,
                menu_bar_height: 32.0,
                menu_bar_padding_x: 10.0,
                menu_bar_padding_y: 4.0,
                menu_bar_gap: 2.0,
                menu_bar_button_width: 48.0,
                menu_bar_button_height: 24.0,
                menu_bar_button_padding_x: 8.0,
                menu_bar_button_radius: 5.0,
                menu_text_size: 12.0,
                menu_panel_top: 30.0,
                menu_panel_width: 180.0,
                menu_panel_padding: 4.0,
                menu_panel_gap: 1.0,
                menu_panel_radius: 10.0,
                menu_item_height: 28.0,
                menu_item_padding_x: 8.0,
                menu_item_radius: 7.0,
                menu_separator_margin_x: 6.0,
                menu_separator_margin_y: 3.0,
                menu_separator_height: 1.0,
                context_menu_panel_width: 132.0,
                context_menu_submenu_width: 148.0,
                context_menu_submenu_gap: 2.0,
                context_menu_axis_panel_width: 164.0,
                table_insert_dialog_width: 380.0,
                table_insert_stepper_gap: 8.0,
                table_insert_stepper_button_size: 32.0,
                table_insert_stepper_value_min_width: 56.0,
                table_insert_stepper_value_padding_x: 10.0,
                table_insert_stepper_radius: 8.0,
                view_mode_toggle_left: 12.0,
                view_mode_toggle_bottom: 12.0,
                view_mode_toggle_padding_x: 8.0,
                view_mode_toggle_padding_y: 4.0,
                view_mode_toggle_min_width: 88.0,
                view_mode_toggle_radius: 999.0,
                view_mode_toggle_border_width: 1.0,
                view_mode_toggle_text_size: 11.0,
                status_bar_height: 24.0,
                status_bar_padding_x: 12.0,
                status_bar_item_gap: 12.0,
                status_bar_text_size: 11.0,
            },
            typography: ThemeTypography {
                text_size: 16.0,
                text_line_height: 1.6,
                h1_size: 32.0,
                h1_weight: FontWeightDef::Bold,
                h2_size: 24.0,
                h2_weight: FontWeightDef::Bold,
                h3_size: 20.0,
                h3_weight: FontWeightDef::Semibold,
                h4_size: 18.0,
                h4_weight: FontWeightDef::Semibold,
                h5_size: 16.0,
                h5_weight: FontWeightDef::Semibold,
                h6_size: 14.0,
                h6_weight: FontWeightDef::Semibold,
                code_size: 15.0,
                dialog_title_size: 20.0,
                dialog_title_weight: FontWeightDef::Semibold,
                dialog_body_size: 14.0,
                dialog_body_weight: FontWeightDef::Normal,
                dialog_button_size: 14.0,
                dialog_button_weight: FontWeightDef::Medium,
            },
            placeholders: Placeholders {
                empty_editing: String::new(),
            },
        }
    }

    /// Returns the Xcode-inspired light theme.
    ///
    /// The light theme intentionally reuses the default layout and typography
    /// tokens so it can focus on palette differences.
    pub fn xcode_light() -> Self {
        let base = Self::xcode_dark();
        Self {
            name: "Xcode Light".into(),
            colors: ThemeColors {
                // 浅色主题使用带轻微玉石底色的分层中性色，避免大面积纯白造成眩光。
                editor_background: Hsla::from(rgba(0xfafafdff)),
                source_mode_block_bg: Hsla::from(rgba(0xf2f2f7ff)),
                comment_bg: Hsla::from(rgba(0xfff4cc66)),
                text_default: Hsla::from(rgba(0x1d1d1fff)),
                text_link: Hsla::from(rgba(0x007affff)),
                text_placeholder: Hsla::from(rgba(0x6e6e73cc)),
                text_h1: Hsla::from(rgba(0x1d1d1fff)),
                text_h2: Hsla::from(rgba(0x1d1d1fff)),
                text_h3: Hsla::from(rgba(0x1d1d1fff)),
                text_h4: Hsla::from(rgba(0x1d1d1fff)),
                text_h5: Hsla::from(rgba(0x1d1d1fff)),
                text_h6: Hsla::from(rgba(0x1d1d1fff)),
                border_h1: Hsla::from(rgba(0xd4d7cfff)),
                border_h2: Hsla::from(rgba(0xe2e5ddff)),
                text_quote: Hsla::from(rgba(0x515154ff)),
                border_quote: Hsla::from(rgba(0x8e8e93ff)),
                callout_note_bg: Hsla::from(rgba(0x0a66c214)),
                callout_note_border: Hsla::from(rgba(0x0a66c2ff)),
                callout_tip_bg: Hsla::from(rgba(0x16a34a14)),
                callout_tip_border: Hsla::from(rgba(0x16a34aff)),
                callout_important_bg: Hsla::from(rgba(0x7c3aed14)),
                callout_important_border: Hsla::from(rgba(0x7c3aedff)),
                callout_warning_bg: Hsla::from(rgba(0xf9731614)),
                callout_warning_border: Hsla::from(rgba(0xf97316ff)),
                callout_caution_bg: Hsla::from(rgba(0xdc262614)),
                callout_caution_border: Hsla::from(rgba(0xdc2626ff)),
                footnote_bg: Hsla::from(rgba(0xf2f4eeff)),
                footnote_border: Hsla::from(rgba(0xd4d7cfff)),
                footnote_badge_bg: Hsla::from(rgba(0xe8ebe3ff)),
                footnote_badge_text: Hsla::from(rgba(0x515154ff)),
                footnote_backref: Hsla::from(rgba(0x0a66c2ff)),
                task_checkbox_border: Hsla::from(rgba(0x8e8e93ff)),
                task_checkbox_bg: Hsla::from(rgba(0xf7f8f3ff)),
                task_checkbox_checked_bg: Hsla::from(rgba(0x0a66c2ff)),
                task_checkbox_check: Hsla::from(rgba(0xffffffff)),
                separator_color: Hsla::from(rgba(0xd4d7cfff)),
                code_bg: Hsla::from(rgba(0xf0f2ecff)),
                code_text: Hsla::from(rgba(0x1d1d1fff)),
                code_language_input_bg: Hsla::from(rgba(0xf7f8f3ff)),
                code_language_input_border: Hsla::from(rgba(0xd4d7cfff)),
                code_language_input_text: Hsla::from(rgba(0x1d1d1fff)),
                code_language_input_placeholder: Hsla::from(rgba(0x6e6e73cc)),
                code_syntax_comment: Hsla::from(rgba(0x5d6c79ff)),
                code_syntax_keyword: Hsla::from(rgba(0xad3da4ff)),
                code_syntax_string: Hsla::from(rgba(0xd12f1bff)),
                code_syntax_number: Hsla::from(rgba(0x272ad8ff)),
                code_syntax_type: Hsla::from(rgba(0x0b4f79ff)),
                code_syntax_function: Hsla::from(rgba(0x326d74ff)),
                code_syntax_constant: Hsla::from(rgba(0x703daaff)),
                code_syntax_variable: Hsla::from(rgba(0x1d1d1fff)),
                code_syntax_property: Hsla::from(rgba(0x0b4f79ff)),
                code_syntax_operator: Hsla::from(rgba(0xad3da4ff)),
                code_syntax_punctuation: Hsla::from(rgba(0x4a4a4fff)),
                table_border: Hsla::from(rgba(0xd4d7cfff)),
                table_header_bg: Hsla::from(rgba(0xf0f2ecff)),
                table_cell_bg: Hsla::from(rgba(0xf7f8f3ff)),
                table_cell_active_outline: Hsla::from(rgba(0x007affff)),
                table_axis_preview_bg: Hsla::from(rgba(0x007aff1f)),
                table_axis_selected_bg: Hsla::from(rgba(0x007aff3d)),
                table_append_button_bg: Hsla::from(rgba(0xecefe8ff)),
                table_append_button_hover: Hsla::from(rgba(0xe3e7deff)),
                table_append_button_text: Hsla::from(rgba(0x49494fff)),
                image_placeholder_bg: Hsla::from(rgba(0xf2f4eeff)),
                image_placeholder_border: Hsla::from(rgba(0xd4d7cfff)),
                image_placeholder_text: Hsla::from(rgba(0x515154ff)),
                image_caption_text: Hsla::from(rgba(0x6e6e73ff)),
                scrollbar_thumb: Hsla::from(rgba(0x8e8e93b8)),
                cursor: Hsla::from(rgba(0x1d1d1fff)),
                selection: Hsla::from(rgba(0x0a66c22e)),
                dialog_backdrop: Hsla::from(rgba(0x1d1d1f66)),
                dialog_surface: Hsla::from(rgba(0xfafbf7ff)),
                dialog_border: Hsla::from(rgba(0xd4d7cfff)),
                dialog_title: Hsla::from(rgba(0x1d1d1fff)),
                dialog_body: Hsla::from(rgba(0x3a3a3cff)),
                dialog_muted: Hsla::from(rgba(0x6e6e73ff)),
                dialog_primary_button_bg: Hsla::from(rgba(0x0071e3ff)),
                dialog_primary_button_hover: Hsla::from(rgba(0x0068d1ff)),
                dialog_primary_button_text: Hsla::from(rgba(0xffffffff)),
                dialog_secondary_button_bg: Hsla::from(rgba(0xecefe8ff)),
                dialog_secondary_button_hover: Hsla::from(rgba(0xe3e7deff)),
                dialog_secondary_button_text: Hsla::from(rgba(0x1d1d1fff)),
                dialog_danger_button_bg: Hsla::from(rgba(0xdc2626ff)),
                dialog_danger_button_hover: Hsla::from(rgba(0xb91c1cff)),
                dialog_danger_button_text: Hsla::from(rgba(0xffffffff)),
                status_bar_background: Hsla::from(rgba(0xecefe8ff)),
                status_bar_text: Hsla::from(rgba(0x49494fff)),
                status_bar_text_dim: Hsla::from(rgba(0x77777eff)),
                status_bar_button_hover: Hsla::from(rgba(0xe3e7deff)),
                chrome_background: Hsla::from(rgba(0xf1f3edff)),
                chrome_hover: Hsla::from(rgba(0xe5e8e1ff)),
                sidebar_background: Hsla::from(rgba(0xf3f5efff)),
                tab_strip_background: Hsla::from(rgba(0xecefe8ff)),
                tab_active_background: Hsla::from(rgba(0xf7f8f3ff)),
            },
            dimensions: base.dimensions,
            typography: base.typography,
            placeholders: base.placeholders,
        }
    }

    /// Returns the Darcula-inspired JetBrains dark theme.
    pub fn jetbrains_dark() -> Self {
        let mut theme = Self::xcode_dark();
        theme.name = "JetBrains Dark".into();
        let colors = &mut theme.colors;
        colors.editor_background = Hsla::from(rgba(0x2b2b2bff));
        colors.source_mode_block_bg = Hsla::from(rgba(0x313335ff));
        colors.comment_bg = Hsla::from(rgba(0x80808030));
        colors.text_default = Hsla::from(rgba(0xa9b7c6ff));
        colors.text_link = Hsla::from(rgba(0x589df6ff));
        colors.text_placeholder = Hsla::from(rgba(0x808080cc));
        colors.text_h1 = Hsla::from(rgba(0xa9b7c6ff));
        colors.text_h2 = Hsla::from(rgba(0xa9b7c6ff));
        colors.text_h3 = Hsla::from(rgba(0xa9b7c6ff));
        colors.text_h4 = Hsla::from(rgba(0xa9b7c6ff));
        colors.text_h5 = Hsla::from(rgba(0xa9b7c6ff));
        colors.text_h6 = Hsla::from(rgba(0xa9b7c6ff));
        colors.border_h1 = Hsla::from(rgba(0x55585aff));
        colors.border_h2 = Hsla::from(rgba(0x55585acc));
        colors.text_quote = Hsla::from(rgba(0x9da5b4ff));
        colors.border_quote = Hsla::from(rgba(0x6b6b6bff));
        colors.callout_note_bg = Hsla::from(rgba(0x557aa830));
        colors.callout_note_border = Hsla::from(rgba(0x9876aaff));
        colors.callout_tip_bg = Hsla::from(rgba(0x62975530));
        colors.callout_tip_border = Hsla::from(rgba(0x6a8759ff));
        colors.callout_important_bg = Hsla::from(rgba(0x9876aa30));
        colors.callout_important_border = Hsla::from(rgba(0x9876aaff));
        colors.callout_warning_bg = Hsla::from(rgba(0xcc783230));
        colors.callout_warning_border = Hsla::from(rgba(0xcc7832ff));
        colors.callout_caution_bg = Hsla::from(rgba(0xbc3f3cff));
        colors.callout_caution_border = Hsla::from(rgba(0xbc3f3cff));
        colors.footnote_bg = Hsla::from(rgba(0x323232ff));
        colors.footnote_border = Hsla::from(rgba(0x55585aff));
        colors.footnote_badge_bg = Hsla::from(rgba(0x55585a66));
        colors.footnote_badge_text = Hsla::from(rgba(0xa9b7c6ff));
        colors.footnote_backref = Hsla::from(rgba(0x589df6ff));
        colors.task_checkbox_border = Hsla::from(rgba(0x808080ff));
        colors.task_checkbox_checked_bg = Hsla::from(rgba(0x3574f0ff));
        colors.task_checkbox_check = Hsla::from(rgba(0xffffffff));
        colors.separator_color = Hsla::from(rgba(0x55585aff));
        colors.code_bg = Hsla::from(rgba(0x313335ff));
        colors.code_text = Hsla::from(rgba(0xa9b7c6ff));
        colors.code_language_input_bg = Hsla::from(rgba(0x3c3f41ff));
        colors.code_language_input_border = Hsla::from(rgba(0x55585aff));
        colors.code_language_input_text = Hsla::from(rgba(0xa9b7c6ff));
        colors.code_language_input_placeholder = Hsla::from(rgba(0x808080cc));
        colors.code_syntax_comment = Hsla::from(rgba(0x808080ff));
        colors.code_syntax_keyword = Hsla::from(rgba(0xcc7832ff));
        colors.code_syntax_string = Hsla::from(rgba(0x6a8759ff));
        colors.code_syntax_number = Hsla::from(rgba(0x6897bbff));
        colors.code_syntax_type = Hsla::from(rgba(0xffc66dff));
        colors.code_syntax_function = Hsla::from(rgba(0xffc66dff));
        colors.code_syntax_constant = Hsla::from(rgba(0x9876aaff));
        colors.code_syntax_variable = Hsla::from(rgba(0xa9b7c6ff));
        colors.code_syntax_property = Hsla::from(rgba(0xa9b7c6ff));
        colors.code_syntax_operator = Hsla::from(rgba(0xa9b7c6ff));
        colors.code_syntax_punctuation = Hsla::from(rgba(0xa9b7c6ff));
        colors.table_border = Hsla::from(rgba(0x55585aff));
        colors.table_header_bg = Hsla::from(rgba(0x3c3f41ff));
        colors.table_cell_bg = Hsla::from(rgba(0x323232ff));
        colors.table_cell_active_outline = Hsla::from(rgba(0x3574f0ff));
        colors.table_axis_preview_bg = Hsla::from(rgba(0x3574f033));
        colors.table_axis_selected_bg = Hsla::from(rgba(0x3574f055));
        colors.table_append_button_bg = Hsla::from(rgba(0x45494bff));
        colors.table_append_button_hover = Hsla::from(rgba(0x55585aff));
        colors.table_append_button_text = Hsla::from(rgba(0xa9b7c6ff));
        colors.image_placeholder_bg = Hsla::from(rgba(0x323232ff));
        colors.image_placeholder_border = Hsla::from(rgba(0x55585aff));
        colors.image_placeholder_text = Hsla::from(rgba(0xa9b7c6ff));
        colors.image_caption_text = Hsla::from(rgba(0x808080ff));
        colors.scrollbar_thumb = Hsla::from(rgba(0x808080b8));
        colors.cursor = Hsla::from(rgba(0xa9b7c6ff));
        colors.selection = Hsla::from(rgba(0x214283ff));
        colors.dialog_backdrop = Hsla::from(rgba(0x000000b8));
        colors.dialog_surface = Hsla::from(rgba(0x3c3f41ff));
        colors.dialog_border = Hsla::from(rgba(0x55585aff));
        colors.dialog_title = Hsla::from(rgba(0xa9b7c6ff));
        colors.dialog_body = Hsla::from(rgba(0xa9b7c6ff));
        colors.dialog_muted = Hsla::from(rgba(0x808080ff));
        colors.dialog_primary_button_bg = Hsla::from(rgba(0x3574f0ff));
        colors.dialog_primary_button_hover = Hsla::from(rgba(0x2f68d8ff));
        colors.dialog_primary_button_text = Hsla::from(rgba(0xffffffff));
        colors.dialog_secondary_button_bg = Hsla::from(rgba(0x45494bff));
        colors.dialog_secondary_button_hover = Hsla::from(rgba(0x55585aff));
        colors.dialog_secondary_button_text = Hsla::from(rgba(0xa9b7c6ff));
        colors.status_bar_background = Hsla::from(rgba(0x3c3f41ff));
        colors.status_bar_text = Hsla::from(rgba(0xa9b7c6cc));
        colors.status_bar_text_dim = Hsla::from(rgba(0x808080ff));
        colors.status_bar_button_hover = Hsla::from(rgba(0x55585aff));
        colors.chrome_background = Hsla::from(rgba(0x3c3f41ff));
        colors.chrome_hover = Hsla::from(rgba(0x4b4f51ff));
        colors.sidebar_background = Hsla::from(rgba(0x313335ff));
        colors.tab_strip_background = Hsla::from(rgba(0x313335ff));
        colors.tab_active_background = Hsla::from(rgba(0x2b2b2bff));
        theme
    }

    /// Returns the IntelliJ-inspired light theme.
    pub fn jetbrains_light() -> Self {
        let mut theme = Self::xcode_light();
        theme.name = "JetBrains Light".into();
        let colors = &mut theme.colors;
        colors.editor_background = Hsla::from(rgba(0xfffffeff));
        colors.source_mode_block_bg = Hsla::from(rgba(0xf2f2f2ff));
        colors.comment_bg = Hsla::from(rgba(0x8c8c8c24));
        colors.text_default = Hsla::from(rgba(0x2b2b2bff));
        colors.text_link = Hsla::from(rgba(0x3574f0ff));
        colors.text_placeholder = Hsla::from(rgba(0x8c8c8ccc));
        colors.text_h1 = Hsla::from(rgba(0x2b2b2bff));
        colors.text_h2 = Hsla::from(rgba(0x2b2b2bff));
        colors.text_h3 = Hsla::from(rgba(0x2b2b2bff));
        colors.text_h4 = Hsla::from(rgba(0x2b2b2bff));
        colors.text_h5 = Hsla::from(rgba(0x2b2b2bff));
        colors.text_h6 = Hsla::from(rgba(0x2b2b2bff));
        colors.border_h1 = Hsla::from(rgba(0xd7d7d7ff));
        colors.border_h2 = Hsla::from(rgba(0xe5e5e5ff));
        colors.text_quote = Hsla::from(rgba(0x5f6368ff));
        colors.border_quote = Hsla::from(rgba(0xb7b7b7ff));
        colors.callout_note_bg = Hsla::from(rgba(0x3574f01a));
        colors.callout_note_border = Hsla::from(rgba(0x3574f0ff));
        colors.callout_tip_bg = Hsla::from(rgba(0x067d171a));
        colors.callout_tip_border = Hsla::from(rgba(0x067d17ff));
        colors.callout_important_bg = Hsla::from(rgba(0x7a3e9d1a));
        colors.callout_important_border = Hsla::from(rgba(0x7a3e9dff));
        colors.callout_warning_bg = Hsla::from(rgba(0x9d6c001a));
        colors.callout_warning_border = Hsla::from(rgba(0x9d6c00ff));
        colors.callout_caution_bg = Hsla::from(rgba(0xcc3f3f1a));
        colors.callout_caution_border = Hsla::from(rgba(0xcc3f3fff));
        colors.footnote_bg = Hsla::from(rgba(0xf2f2f2ff));
        colors.footnote_border = Hsla::from(rgba(0xd7d7d7ff));
        colors.footnote_badge_bg = Hsla::from(rgba(0xe8e8e8ff));
        colors.footnote_badge_text = Hsla::from(rgba(0x5f6368ff));
        colors.footnote_backref = Hsla::from(rgba(0x3574f0ff));
        colors.task_checkbox_border = Hsla::from(rgba(0x8c8c8cff));
        colors.task_checkbox_checked_bg = Hsla::from(rgba(0x3574f0ff));
        colors.task_checkbox_check = Hsla::from(rgba(0xffffffff));
        colors.separator_color = Hsla::from(rgba(0xd7d7d7ff));
        colors.code_bg = Hsla::from(rgba(0xf2f2f2ff));
        colors.code_text = Hsla::from(rgba(0x2b2b2bff));
        colors.code_language_input_bg = Hsla::from(rgba(0xfffffeff));
        colors.code_language_input_border = Hsla::from(rgba(0xd7d7d7ff));
        colors.code_language_input_text = Hsla::from(rgba(0x2b2b2bff));
        colors.code_language_input_placeholder = Hsla::from(rgba(0x8c8c8ccc));
        colors.code_syntax_comment = Hsla::from(rgba(0x8c8c8cff));
        colors.code_syntax_keyword = Hsla::from(rgba(0x0033b3ff));
        colors.code_syntax_string = Hsla::from(rgba(0x067d17ff));
        colors.code_syntax_number = Hsla::from(rgba(0x1750ebff));
        colors.code_syntax_type = Hsla::from(rgba(0x000000ff));
        colors.code_syntax_function = Hsla::from(rgba(0x00627aff));
        colors.code_syntax_constant = Hsla::from(rgba(0x871094ff));
        colors.code_syntax_variable = Hsla::from(rgba(0x2b2b2bff));
        colors.code_syntax_property = Hsla::from(rgba(0x00627aff));
        colors.code_syntax_operator = Hsla::from(rgba(0x0033b3ff));
        colors.code_syntax_punctuation = Hsla::from(rgba(0x5f6368ff));
        colors.table_border = Hsla::from(rgba(0xd7d7d7ff));
        colors.table_header_bg = Hsla::from(rgba(0xf2f2f2ff));
        colors.table_cell_bg = Hsla::from(rgba(0xfffffeff));
        colors.table_cell_active_outline = Hsla::from(rgba(0x3574f0ff));
        colors.table_axis_preview_bg = Hsla::from(rgba(0x3574f01f));
        colors.table_axis_selected_bg = Hsla::from(rgba(0x3574f03d));
        colors.table_append_button_bg = Hsla::from(rgba(0xe8e8e8ff));
        colors.table_append_button_hover = Hsla::from(rgba(0xd7d7d7ff));
        colors.table_append_button_text = Hsla::from(rgba(0x2b2b2bff));
        colors.image_placeholder_bg = Hsla::from(rgba(0xf2f2f2ff));
        colors.image_placeholder_border = Hsla::from(rgba(0xd7d7d7ff));
        colors.image_placeholder_text = Hsla::from(rgba(0x5f6368ff));
        colors.image_caption_text = Hsla::from(rgba(0x8c8c8cff));
        colors.scrollbar_thumb = Hsla::from(rgba(0x8c8c8cb8));
        colors.cursor = Hsla::from(rgba(0x2b2b2bff));
        colors.selection = Hsla::from(rgba(0x3574f03d));
        colors.dialog_backdrop = Hsla::from(rgba(0x2b2b2b66));
        colors.dialog_surface = Hsla::from(rgba(0xfffffeff));
        colors.dialog_border = Hsla::from(rgba(0xd7d7d7ff));
        colors.dialog_title = Hsla::from(rgba(0x2b2b2bff));
        colors.dialog_body = Hsla::from(rgba(0x3c4043ff));
        colors.dialog_muted = Hsla::from(rgba(0x8c8c8cff));
        colors.dialog_primary_button_bg = Hsla::from(rgba(0x3574f0ff));
        colors.dialog_primary_button_hover = Hsla::from(rgba(0x2f68d8ff));
        colors.dialog_primary_button_text = Hsla::from(rgba(0xffffffff));
        colors.dialog_secondary_button_bg = Hsla::from(rgba(0xe8e8e8ff));
        colors.dialog_secondary_button_hover = Hsla::from(rgba(0xd7d7d7ff));
        colors.dialog_secondary_button_text = Hsla::from(rgba(0x2b2b2bff));
        colors.status_bar_background = Hsla::from(rgba(0xf2f2f2ff));
        colors.status_bar_text = Hsla::from(rgba(0x5f6368cc));
        colors.status_bar_text_dim = Hsla::from(rgba(0x8c8c8cff));
        colors.status_bar_button_hover = Hsla::from(rgba(0xe5e5e5ff));
        colors.chrome_background = Hsla::from(rgba(0xf2f2f2ff));
        colors.chrome_hover = Hsla::from(rgba(0xe5e5e5ff));
        colors.sidebar_background = Hsla::from(rgba(0xf2f2f2ff));
        colors.tab_strip_background = Hsla::from(rgba(0xf2f2f2ff));
        colors.tab_active_background = Hsla::from(rgba(0xfffffeff));
        theme
    }

    /// Returns the Obsidian-inspired dark theme.
    pub fn obsidian_dark() -> Self {
        let mut theme = Self::xcode_dark();
        theme.name = "Obsidian Dark".into();
        let colors = &mut theme.colors;
        // Obsidian 的紫色只承担链接、选中和焦点角色，正文工作区保持稳定实色。
        colors.editor_background = Hsla::from(rgba(0x202020ff));
        colors.source_mode_block_bg = Hsla::from(rgba(0x262626ff));
        colors.comment_bg = Hsla::from(rgba(0x7f6df226));
        colors.text_default = Hsla::from(rgba(0xdcdddeff));
        colors.text_link = Hsla::from(rgba(0x8b7cf6ff));
        colors.text_placeholder = Hsla::from(rgba(0x888888cc));
        colors.text_h1 = Hsla::from(rgba(0xffffffff));
        colors.text_h2 = Hsla::from(rgba(0xf2f2f2ff));
        colors.text_h3 = Hsla::from(rgba(0xe8e8e8ff));
        colors.text_h4 = Hsla::from(rgba(0xdcdddeff));
        colors.text_h5 = Hsla::from(rgba(0xc9c9c9ff));
        colors.text_h6 = Hsla::from(rgba(0xb3b3b3ff));
        colors.border_h1 = Hsla::from(rgba(0x484848ff));
        colors.border_h2 = Hsla::from(rgba(0x363636ff));
        colors.text_quote = Hsla::from(rgba(0xb3b3b3ff));
        colors.border_quote = Hsla::from(rgba(0x7f6df2ff));
        colors.callout_note_bg = Hsla::from(rgba(0x7f6df224));
        colors.callout_note_border = Hsla::from(rgba(0x8b7cf6ff));
        colors.callout_tip_bg = Hsla::from(rgba(0x53dfb51f));
        colors.callout_tip_border = Hsla::from(rgba(0x53dfb5ff));
        colors.callout_important_bg = Hsla::from(rgba(0xc678dd1f));
        colors.callout_important_border = Hsla::from(rgba(0xc678ddff));
        colors.callout_warning_bg = Hsla::from(rgba(0xe5b5671f));
        colors.callout_warning_border = Hsla::from(rgba(0xe5b567ff));
        colors.callout_caution_bg = Hsla::from(rgba(0xe06c751f));
        colors.callout_caution_border = Hsla::from(rgba(0xe06c75ff));
        colors.footnote_bg = Hsla::from(rgba(0x262626ff));
        colors.footnote_border = Hsla::from(rgba(0x484848ff));
        colors.footnote_badge_bg = Hsla::from(rgba(0x3a3a3aff));
        colors.footnote_badge_text = Hsla::from(rgba(0xdcdddeff));
        colors.footnote_backref = Hsla::from(rgba(0x8b7cf6ff));
        colors.task_checkbox_border = Hsla::from(rgba(0x666666ff));
        colors.task_checkbox_checked_bg = Hsla::from(rgba(0x7f6df2ff));
        colors.separator_color = Hsla::from(rgba(0x484848ff));
        colors.code_bg = Hsla::from(rgba(0x171717ff));
        colors.code_text = Hsla::from(rgba(0xdcdddeff));
        colors.code_language_input_bg = Hsla::from(rgba(0x262626ff));
        colors.code_language_input_border = Hsla::from(rgba(0x484848ff));
        colors.code_language_input_text = Hsla::from(rgba(0xdcdddeff));
        colors.code_language_input_placeholder = Hsla::from(rgba(0x888888cc));
        colors.code_syntax_comment = Hsla::from(rgba(0x7f848eff));
        colors.code_syntax_keyword = Hsla::from(rgba(0xc678ddff));
        colors.code_syntax_string = Hsla::from(rgba(0x98c379ff));
        colors.code_syntax_number = Hsla::from(rgba(0xd19a66ff));
        colors.code_syntax_type = Hsla::from(rgba(0x56b6c2ff));
        colors.code_syntax_function = Hsla::from(rgba(0x61afefff));
        colors.code_syntax_constant = Hsla::from(rgba(0xe5c07bff));
        colors.code_syntax_variable = Hsla::from(rgba(0xe06c75ff));
        colors.code_syntax_property = Hsla::from(rgba(0xabb2bfff));
        colors.code_syntax_operator = Hsla::from(rgba(0x56b6c2ff));
        colors.code_syntax_punctuation = Hsla::from(rgba(0xabb2bfff));
        colors.table_border = Hsla::from(rgba(0x484848ff));
        colors.table_header_bg = Hsla::from(rgba(0x2b2b2bff));
        colors.table_cell_bg = Hsla::from(rgba(0x202020ff));
        colors.table_cell_active_outline = Hsla::from(rgba(0x8b7cf6ff));
        colors.table_axis_preview_bg = Hsla::from(rgba(0x7f6df233));
        colors.table_axis_selected_bg = Hsla::from(rgba(0x7f6df255));
        colors.table_append_button_bg = Hsla::from(rgba(0x303030ff));
        colors.table_append_button_hover = Hsla::from(rgba(0x3a3a3aff));
        colors.table_append_button_text = Hsla::from(rgba(0xdcdddeff));
        colors.image_placeholder_bg = Hsla::from(rgba(0x262626ff));
        colors.image_placeholder_border = Hsla::from(rgba(0x484848ff));
        colors.image_placeholder_text = Hsla::from(rgba(0xb3b3b3ff));
        colors.image_caption_text = Hsla::from(rgba(0x888888ff));
        colors.scrollbar_thumb = Hsla::from(rgba(0x666666b8));
        colors.cursor = Hsla::from(rgba(0xdcdddeff));
        colors.selection = Hsla::from(rgba(0x7f6df24d));
        colors.dialog_surface = Hsla::from(rgba(0x262626ff));
        colors.dialog_border = Hsla::from(rgba(0x484848ff));
        colors.dialog_title = Hsla::from(rgba(0xffffffff));
        colors.dialog_body = Hsla::from(rgba(0xdcdddeff));
        colors.dialog_muted = Hsla::from(rgba(0x888888ff));
        colors.dialog_primary_button_bg = Hsla::from(rgba(0x7f6df2ff));
        colors.dialog_primary_button_hover = Hsla::from(rgba(0x8b7cf6ff));
        colors.dialog_secondary_button_bg = Hsla::from(rgba(0x363636ff));
        colors.dialog_secondary_button_hover = Hsla::from(rgba(0x484848ff));
        colors.dialog_secondary_button_text = Hsla::from(rgba(0xdcdddeff));
        colors.status_bar_background = Hsla::from(rgba(0x191919ff));
        colors.status_bar_text = Hsla::from(rgba(0xb3b3b3ff));
        colors.status_bar_text_dim = Hsla::from(rgba(0x888888ff));
        colors.status_bar_button_hover = Hsla::from(rgba(0x363636ff));
        colors.chrome_background = Hsla::from(rgba(0x191919ff));
        colors.chrome_hover = Hsla::from(rgba(0x303030ff));
        colors.sidebar_background = Hsla::from(rgba(0x191919ff));
        colors.tab_strip_background = Hsla::from(rgba(0x161616ff));
        colors.tab_active_background = Hsla::from(rgba(0x202020ff));
        theme
    }

    /// Returns the Obsidian-inspired light theme.
    pub fn obsidian_light() -> Self {
        let mut theme = Self::xcode_light();
        theme.name = "Obsidian Light".into();
        let colors = &mut theme.colors;
        colors.editor_background = Hsla::from(rgba(0xffffffff));
        colors.source_mode_block_bg = Hsla::from(rgba(0xf6f6f6ff));
        colors.comment_bg = Hsla::from(rgba(0x7c3aed1f));
        colors.text_default = Hsla::from(rgba(0x2e3338ff));
        colors.text_link = Hsla::from(rgba(0x6c31e3ff));
        colors.text_placeholder = Hsla::from(rgba(0x7a7f87cc));
        colors.text_h1 = Hsla::from(rgba(0x1f2328ff));
        colors.text_h2 = Hsla::from(rgba(0x252a2fff));
        colors.text_h3 = Hsla::from(rgba(0x2e3338ff));
        colors.text_h4 = Hsla::from(rgba(0x3a4047ff));
        colors.text_h5 = Hsla::from(rgba(0x4b5159ff));
        colors.text_h6 = Hsla::from(rgba(0x5c6370ff));
        colors.border_h1 = Hsla::from(rgba(0xd8d8d8ff));
        colors.border_h2 = Hsla::from(rgba(0xe5e5e5ff));
        colors.text_quote = Hsla::from(rgba(0x5c6370ff));
        colors.border_quote = Hsla::from(rgba(0x7c3aedff));
        colors.callout_note_bg = Hsla::from(rgba(0x7c3aed14));
        colors.callout_note_border = Hsla::from(rgba(0x6c31e3ff));
        colors.callout_important_bg = Hsla::from(rgba(0x9333ea14));
        colors.callout_important_border = Hsla::from(rgba(0x9333eaff));
        colors.footnote_bg = Hsla::from(rgba(0xf6f6f6ff));
        colors.footnote_border = Hsla::from(rgba(0xd8d8d8ff));
        colors.footnote_badge_bg = Hsla::from(rgba(0xe9e5f5ff));
        colors.footnote_badge_text = Hsla::from(rgba(0x4b3f72ff));
        colors.footnote_backref = Hsla::from(rgba(0x6c31e3ff));
        colors.task_checkbox_border = Hsla::from(rgba(0x9a9a9aff));
        colors.task_checkbox_checked_bg = Hsla::from(rgba(0x7c3aedff));
        colors.separator_color = Hsla::from(rgba(0xd8d8d8ff));
        colors.code_bg = Hsla::from(rgba(0xf4f4f4ff));
        colors.code_text = Hsla::from(rgba(0x2e3338ff));
        colors.code_language_input_bg = Hsla::from(rgba(0xffffffff));
        colors.code_language_input_border = Hsla::from(rgba(0xd8d8d8ff));
        colors.code_language_input_text = Hsla::from(rgba(0x2e3338ff));
        colors.code_language_input_placeholder = Hsla::from(rgba(0x7a7f87cc));
        colors.code_syntax_comment = Hsla::from(rgba(0x7a7f87ff));
        colors.code_syntax_keyword = Hsla::from(rgba(0x7c3aedff));
        colors.code_syntax_string = Hsla::from(rgba(0x22863aff));
        colors.code_syntax_number = Hsla::from(rgba(0xb35c00ff));
        colors.code_syntax_type = Hsla::from(rgba(0x087e8bff));
        colors.code_syntax_function = Hsla::from(rgba(0x005cc5ff));
        colors.code_syntax_constant = Hsla::from(rgba(0x9a6700ff));
        colors.code_syntax_variable = Hsla::from(rgba(0xb31d28ff));
        colors.code_syntax_property = Hsla::from(rgba(0x2e3338ff));
        colors.code_syntax_operator = Hsla::from(rgba(0x087e8bff));
        colors.code_syntax_punctuation = Hsla::from(rgba(0x5c6370ff));
        colors.table_border = Hsla::from(rgba(0xd8d8d8ff));
        colors.table_header_bg = Hsla::from(rgba(0xf1f1f1ff));
        colors.table_cell_bg = Hsla::from(rgba(0xffffffff));
        colors.table_cell_active_outline = Hsla::from(rgba(0x7c3aedff));
        colors.table_axis_preview_bg = Hsla::from(rgba(0x7c3aed1f));
        colors.table_axis_selected_bg = Hsla::from(rgba(0x7c3aed3d));
        colors.table_append_button_bg = Hsla::from(rgba(0xeeeeeeff));
        colors.table_append_button_hover = Hsla::from(rgba(0xe2e2e2ff));
        colors.table_append_button_text = Hsla::from(rgba(0x2e3338ff));
        colors.image_placeholder_bg = Hsla::from(rgba(0xf6f6f6ff));
        colors.image_placeholder_border = Hsla::from(rgba(0xd8d8d8ff));
        colors.image_placeholder_text = Hsla::from(rgba(0x5c6370ff));
        colors.image_caption_text = Hsla::from(rgba(0x7a7f87ff));
        colors.scrollbar_thumb = Hsla::from(rgba(0x9a9a9ab8));
        colors.cursor = Hsla::from(rgba(0x2e3338ff));
        colors.selection = Hsla::from(rgba(0x7c3aed2e));
        colors.dialog_surface = Hsla::from(rgba(0xffffffff));
        colors.dialog_border = Hsla::from(rgba(0xd8d8d8ff));
        colors.dialog_title = Hsla::from(rgba(0x1f2328ff));
        colors.dialog_body = Hsla::from(rgba(0x2e3338ff));
        colors.dialog_muted = Hsla::from(rgba(0x7a7f87ff));
        colors.dialog_primary_button_bg = Hsla::from(rgba(0x7c3aedff));
        colors.dialog_primary_button_hover = Hsla::from(rgba(0x6c31e3ff));
        colors.dialog_secondary_button_bg = Hsla::from(rgba(0xeeeeeeff));
        colors.dialog_secondary_button_hover = Hsla::from(rgba(0xe2e2e2ff));
        colors.dialog_secondary_button_text = Hsla::from(rgba(0x2e3338ff));
        colors.status_bar_background = Hsla::from(rgba(0xf1f1f1ff));
        colors.status_bar_text = Hsla::from(rgba(0x5c6370ff));
        colors.status_bar_text_dim = Hsla::from(rgba(0x7a7f87ff));
        colors.status_bar_button_hover = Hsla::from(rgba(0xe2e2e2ff));
        colors.chrome_background = Hsla::from(rgba(0xf1f1f1ff));
        colors.chrome_hover = Hsla::from(rgba(0xe5e5e5ff));
        colors.sidebar_background = Hsla::from(rgba(0xf6f6f6ff));
        colors.tab_strip_background = Hsla::from(rgba(0xeeeeeeff));
        colors.tab_active_background = Hsla::from(rgba(0xffffffff));
        theme
    }

    /// Test-only compatibility aliases for the historical export fixtures.
    /// They resolve to Xcode and are not part of the persisted theme model.
    #[cfg(test)]
    pub fn default_theme() -> Self {
        Self::xcode_dark()
    }

    #[cfg(test)]
    pub fn light_theme() -> Self {
        Self::xcode_light()
    }
}
