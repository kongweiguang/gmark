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
    /// Returns the default Xcode dark palette.
    pub fn xcode_dark() -> Self {
        Self {
            name: "Xcode Dark".into(),
            colors: ThemeColors {
                // Xcode 27 官方产品图中源码编辑器、导航区和工具区使用稳定实色分层。
                editor_background: Hsla::from(rgba(0x292a30ff)),
                source_mode_block_bg: Hsla::from(rgba(0x21222eff)),
                comment_bg: Hsla::from(rgba(0x7f8c9830)),
                text_default: Hsla::from(rgba(0xffffffff)),
                text_link: Hsla::from(rgba(0x0a84ffff)),
                text_placeholder: Hsla::from(rgba(0x8f8f98cc)),
                text_h1: Hsla::from(rgba(0xffffffff)),
                text_h2: Hsla::from(rgba(0xffffffff)),
                text_h3: Hsla::from(rgba(0xffffffff)),
                text_h4: Hsla::from(rgba(0xffffffff)),
                text_h5: Hsla::from(rgba(0xffffffff)),
                text_h6: Hsla::from(rgba(0xffffffff)),
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
                footnote_bg: Hsla::from(rgba(0x21222eff)),
                footnote_border: Hsla::from(rgba(0x7d7e8752)),
                footnote_badge_bg: Hsla::from(rgba(0xc8c8d024)),
                footnote_badge_text: Hsla::from(rgba(0xc8c8d0cc)),
                footnote_backref: Hsla::from(rgba(0x8f8f98ff)),
                task_checkbox_border: Hsla::from(rgba(0x7d7e87ff)),
                task_checkbox_bg: Hsla::from(rgba(0x00000000)),
                task_checkbox_checked_bg: Hsla::from(rgba(0x0a84ffff)),
                task_checkbox_check: Hsla::from(rgba(0xffffffff)),
                separator_color: Hsla::from(rgba(0x7d7e87ff)),
                code_bg: Hsla::from(rgba(0x21222eff)),
                code_text: Hsla::from(rgba(0xffffffff)),
                code_language_input_bg: Hsla::from(rgba(0x21222eff)),
                code_language_input_border: Hsla::from(rgba(0x5b5c66cc)),
                code_language_input_text: Hsla::from(rgba(0xffffffff)),
                code_language_input_placeholder: Hsla::from(rgba(0x8f8f98cc)),
                code_syntax_comment: Hsla::from(rgba(0x7f8c98ff)),
                code_syntax_keyword: Hsla::from(rgba(0xff7ab2ff)),
                code_syntax_string: Hsla::from(rgba(0xfc6a5dff)),
                code_syntax_number: Hsla::from(rgba(0xd0bf69ff)),
                code_syntax_type: Hsla::from(rgba(0x5dd8ffff)),
                code_syntax_function: Hsla::from(rgba(0x67b7a4ff)),
                code_syntax_constant: Hsla::from(rgba(0xa8a8ffff)),
                code_syntax_variable: Hsla::from(rgba(0xffffffff)),
                code_syntax_property: Hsla::from(rgba(0x9cdcfeff)),
                code_syntax_operator: Hsla::from(rgba(0xff7ab2ff)),
                code_syntax_punctuation: Hsla::from(rgba(0xc8c8d0ff)),
                table_border: Hsla::from(rgba(0x484950ff)),
                table_header_bg: Hsla::from(rgba(0x21222eff)),
                table_cell_bg: Hsla::from(rgba(0x292a30ff)),
                table_cell_active_outline: Hsla::from(rgba(0x0a84ffff)),
                table_axis_preview_bg: Hsla::from(rgba(0x0a84ff2e)),
                table_axis_selected_bg: Hsla::from(rgba(0x0a84ff55)),
                table_append_button_bg: Hsla::from(rgba(0x303139ff)),
                table_append_button_hover: Hsla::from(rgba(0x3b3c46ff)),
                table_append_button_text: Hsla::from(rgba(0xffffffff)),
                image_placeholder_bg: Hsla::from(rgba(0x21222eff)),
                image_placeholder_border: Hsla::from(rgba(0x5b5c66ff)),
                image_placeholder_text: Hsla::from(rgba(0xc8c8d0ff)),
                image_caption_text: Hsla::from(rgba(0x8f8f98ff)),
                scrollbar_thumb: Hsla::from(rgba(0xc8c8d0b8)),
                cursor: Hsla::from(rgba(0xffffffff)),
                selection: Hsla::from(rgba(0x0a84ff4d)),
                dialog_backdrop: Hsla::from(rgba(0x000000b8)),
                dialog_surface: Hsla::from(rgba(0x292a30ff)),
                dialog_border: Hsla::from(rgba(0x484950ff)),
                dialog_title: Hsla::from(rgba(0xffffffff)),
                dialog_body: Hsla::from(rgba(0xd1d1d6ff)),
                dialog_muted: Hsla::from(rgba(0x8f8f98ff)),
                dialog_primary_button_bg: Hsla::from(rgba(0x0a84ffff)),
                dialog_primary_button_hover: Hsla::from(rgba(0x0077e6ff)),
                dialog_primary_button_text: Hsla::from(rgba(0xffffffff)),
                dialog_secondary_button_bg: Hsla::from(rgba(0x3a3b43ff)),
                dialog_secondary_button_hover: Hsla::from(rgba(0x484950ff)),
                dialog_secondary_button_text: Hsla::from(rgba(0xffffffff)),
                // Doubles as the destructive menu-item text color (e.g. Delete
                // Row/Column), so it must stay legible on the dark menu surface
                // rather than the muted red used previously.
                dialog_danger_button_bg: Hsla::from(rgba(0xef4444ff)),
                dialog_danger_button_hover: Hsla::from(rgba(0xdc2626ff)),
                dialog_danger_button_text: Hsla::from(rgba(0xfef2f2ff)),
                status_bar_background: Hsla::from(rgba(0x21222eff)),
                status_bar_text: Hsla::from(rgba(0xd1d1d6cc)),
                status_bar_text_dim: Hsla::from(rgba(0x8f8f98ff)),
                status_bar_button_hover: Hsla::from(rgba(0x3b3c46ff)),
                chrome_background: Hsla::from(rgba(0x1c1d2aff)),
                chrome_hover: Hsla::from(rgba(0x292a30ff)),
                sidebar_background: Hsla::from(rgba(0x1c1d2aff)),
                tab_strip_background: Hsla::from(rgba(0x21222eff)),
                tab_active_background: Hsla::from(rgba(0x292a30ff)),
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

    /// Returns the default Xcode light palette.
    ///
    /// The light theme intentionally reuses the default layout and typography
    /// tokens so it can focus on palette differences.
    pub fn xcode_light() -> Self {
        let base = Self::xcode_dark();
        Self {
            name: "Xcode Light".into(),
            colors: ThemeColors {
                editor_background: Hsla::from(rgba(0xffffffff)),
                source_mode_block_bg: Hsla::from(rgba(0xf5f5f5ff)),
                comment_bg: Hsla::from(rgba(0xfff4cc66)),
                text_default: Hsla::from(rgba(0x000000ff)),
                text_link: Hsla::from(rgba(0x007affff)),
                text_placeholder: Hsla::from(rgba(0x6e6e73cc)),
                text_h1: Hsla::from(rgba(0x000000ff)),
                text_h2: Hsla::from(rgba(0x000000ff)),
                text_h3: Hsla::from(rgba(0x000000ff)),
                text_h4: Hsla::from(rgba(0x000000ff)),
                text_h5: Hsla::from(rgba(0x000000ff)),
                text_h6: Hsla::from(rgba(0x000000ff)),
                border_h1: Hsla::from(rgba(0xd1d1d6ff)),
                border_h2: Hsla::from(rgba(0xe5e5eaff)),
                text_quote: Hsla::from(rgba(0x515154ff)),
                border_quote: Hsla::from(rgba(0x8e8e93ff)),
                callout_note_bg: Hsla::from(rgba(0x007aff14)),
                callout_note_border: Hsla::from(rgba(0x007affff)),
                callout_tip_bg: Hsla::from(rgba(0x16a34a14)),
                callout_tip_border: Hsla::from(rgba(0x16a34aff)),
                callout_important_bg: Hsla::from(rgba(0x7c3aed14)),
                callout_important_border: Hsla::from(rgba(0x7c3aedff)),
                callout_warning_bg: Hsla::from(rgba(0xf9731614)),
                callout_warning_border: Hsla::from(rgba(0xf97316ff)),
                callout_caution_bg: Hsla::from(rgba(0xdc262614)),
                callout_caution_border: Hsla::from(rgba(0xdc2626ff)),
                footnote_bg: Hsla::from(rgba(0xf5f5f5ff)),
                footnote_border: Hsla::from(rgba(0xd1d1d6ff)),
                footnote_badge_bg: Hsla::from(rgba(0xe5e5eaff)),
                footnote_badge_text: Hsla::from(rgba(0x515154ff)),
                footnote_backref: Hsla::from(rgba(0x007affff)),
                task_checkbox_border: Hsla::from(rgba(0x8e8e93ff)),
                task_checkbox_bg: Hsla::from(rgba(0xffffffff)),
                task_checkbox_checked_bg: Hsla::from(rgba(0x007affff)),
                task_checkbox_check: Hsla::from(rgba(0xffffffff)),
                separator_color: Hsla::from(rgba(0xd1d1d6ff)),
                code_bg: Hsla::from(rgba(0xf5f5f5ff)),
                code_text: Hsla::from(rgba(0x000000ff)),
                code_language_input_bg: Hsla::from(rgba(0xffffffff)),
                code_language_input_border: Hsla::from(rgba(0xd1d1d6ff)),
                code_language_input_text: Hsla::from(rgba(0x000000ff)),
                code_language_input_placeholder: Hsla::from(rgba(0x6e6e73cc)),
                code_syntax_comment: Hsla::from(rgba(0x5d6c79ff)),
                code_syntax_keyword: Hsla::from(rgba(0xad3da4ff)),
                code_syntax_string: Hsla::from(rgba(0xd12f1bff)),
                code_syntax_number: Hsla::from(rgba(0x272ad8ff)),
                code_syntax_type: Hsla::from(rgba(0x0b4f79ff)),
                code_syntax_function: Hsla::from(rgba(0x326d74ff)),
                code_syntax_constant: Hsla::from(rgba(0x703daaff)),
                code_syntax_variable: Hsla::from(rgba(0x000000ff)),
                code_syntax_property: Hsla::from(rgba(0x0b4f79ff)),
                code_syntax_operator: Hsla::from(rgba(0xad3da4ff)),
                code_syntax_punctuation: Hsla::from(rgba(0x4a4a4fff)),
                table_border: Hsla::from(rgba(0xd1d1d6ff)),
                table_header_bg: Hsla::from(rgba(0xf5f5f5ff)),
                table_cell_bg: Hsla::from(rgba(0xffffffff)),
                table_cell_active_outline: Hsla::from(rgba(0x007affff)),
                table_axis_preview_bg: Hsla::from(rgba(0x007aff1f)),
                table_axis_selected_bg: Hsla::from(rgba(0x007aff3d)),
                table_append_button_bg: Hsla::from(rgba(0xf2f2f7ff)),
                table_append_button_hover: Hsla::from(rgba(0xe5e5eaff)),
                table_append_button_text: Hsla::from(rgba(0x49494fff)),
                image_placeholder_bg: Hsla::from(rgba(0xf5f5f5ff)),
                image_placeholder_border: Hsla::from(rgba(0xd1d1d6ff)),
                image_placeholder_text: Hsla::from(rgba(0x515154ff)),
                image_caption_text: Hsla::from(rgba(0x6e6e73ff)),
                scrollbar_thumb: Hsla::from(rgba(0x8e8e93b8)),
                cursor: Hsla::from(rgba(0x000000ff)),
                selection: Hsla::from(rgba(0x007aff2e)),
                dialog_backdrop: Hsla::from(rgba(0x1d1d1f66)),
                dialog_surface: Hsla::from(rgba(0xffffffff)),
                dialog_border: Hsla::from(rgba(0xd1d1d6ff)),
                dialog_title: Hsla::from(rgba(0x000000ff)),
                dialog_body: Hsla::from(rgba(0x3a3a3cff)),
                dialog_muted: Hsla::from(rgba(0x6e6e73ff)),
                dialog_primary_button_bg: Hsla::from(rgba(0x007affff)),
                dialog_primary_button_hover: Hsla::from(rgba(0x006ee6ff)),
                dialog_primary_button_text: Hsla::from(rgba(0xffffffff)),
                dialog_secondary_button_bg: Hsla::from(rgba(0xf2f2f7ff)),
                dialog_secondary_button_hover: Hsla::from(rgba(0xe5e5eaff)),
                dialog_secondary_button_text: Hsla::from(rgba(0x000000ff)),
                dialog_danger_button_bg: Hsla::from(rgba(0xdc2626ff)),
                dialog_danger_button_hover: Hsla::from(rgba(0xb91c1cff)),
                dialog_danger_button_text: Hsla::from(rgba(0xffffffff)),
                status_bar_background: Hsla::from(rgba(0xf5f5f5ff)),
                status_bar_text: Hsla::from(rgba(0x49494fff)),
                status_bar_text_dim: Hsla::from(rgba(0x77777eff)),
                status_bar_button_hover: Hsla::from(rgba(0xe5e5eaff)),
                chrome_background: Hsla::from(rgba(0xf5f5f5ff)),
                chrome_hover: Hsla::from(rgba(0xe5e5eaff)),
                sidebar_background: Hsla::from(rgba(0xf8fafaff)),
                tab_strip_background: Hsla::from(rgba(0xf2f2f2ff)),
                tab_active_background: Hsla::from(rgba(0xffffffff)),
            },
            dimensions: base.dimensions,
            typography: base.typography,
            placeholders: base.placeholders,
        }
    }

    /// Returns JetBrains Fleet's default dark palette.
    ///
    /// Fleet's official product image uses `#18191b` for the editor and panels,
    /// `#090909` for outer chrome, and a cyan/purple/gold syntax vocabulary.
    pub fn fleet_dark() -> Self {
        let mut theme = Self::xcode_dark();
        theme.name = "Fleet Dark".into();
        let colors = &mut theme.colors;
        colors.editor_background = Hsla::from(rgba(0x18191bff));
        colors.source_mode_block_bg = Hsla::from(rgba(0x101112ff));
        colors.comment_bg = Hsla::from(rgba(0x726cf926));
        colors.text_default = Hsla::from(rgba(0xe0e1e4ff));
        colors.text_link = Hsla::from(rgba(0x87c3ffff));
        colors.text_placeholder = Hsla::from(rgba(0x898e94cc));
        colors.text_h1 = Hsla::from(rgba(0xe0e1e4ff));
        colors.text_h2 = Hsla::from(rgba(0xe0e1e4ff));
        colors.text_h3 = Hsla::from(rgba(0xe0e1e4ff));
        colors.text_h4 = Hsla::from(rgba(0xe0e1e4ff));
        colors.text_h5 = Hsla::from(rgba(0xc7c8cbff));
        colors.text_h6 = Hsla::from(rgba(0xaeafb2ff));
        colors.border_h1 = Hsla::from(rgba(0x363739ff));
        colors.border_h2 = Hsla::from(rgba(0x28292bff));
        colors.text_quote = Hsla::from(rgba(0xaeafb2ff));
        colors.border_quote = Hsla::from(rgba(0x726cf9ff));
        colors.callout_note_bg = Hsla::from(rgba(0x3178c633));
        colors.callout_note_border = Hsla::from(rgba(0x87c3ffff));
        colors.callout_tip_bg = Hsla::from(rgba(0x82d2ce26));
        colors.callout_tip_border = Hsla::from(rgba(0x82d2ceff));
        colors.callout_important_bg = Hsla::from(rgba(0xaf9cff26));
        colors.callout_important_border = Hsla::from(rgba(0xaf9cffff));
        colors.callout_warning_bg = Hsla::from(rgba(0xebc88d26));
        colors.callout_warning_border = Hsla::from(rgba(0xebc88dff));
        colors.callout_caution_bg = Hsla::from(rgba(0xb82d461f));
        colors.callout_caution_border = Hsla::from(rgba(0xb82d46ff));
        colors.footnote_bg = Hsla::from(rgba(0x202123ff));
        colors.footnote_border = Hsla::from(rgba(0x363739ff));
        colors.footnote_badge_bg = Hsla::from(rgba(0x363739ff));
        colors.footnote_badge_text = Hsla::from(rgba(0xc7c8cbff));
        colors.footnote_backref = Hsla::from(rgba(0x87c3ffff));
        colors.task_checkbox_border = Hsla::from(rgba(0x6e747bff));
        colors.task_checkbox_checked_bg = Hsla::from(rgba(0x726cf9ff));
        colors.task_checkbox_check = Hsla::from(rgba(0xffffffff));
        colors.separator_color = Hsla::from(rgba(0x363739ff));
        colors.code_bg = Hsla::from(rgba(0x101112ff));
        colors.code_text = Hsla::from(rgba(0xe0e1e4ff));
        colors.code_language_input_bg = Hsla::from(rgba(0x202123ff));
        colors.code_language_input_border = Hsla::from(rgba(0x363739ff));
        colors.code_language_input_text = Hsla::from(rgba(0xe0e1e4ff));
        colors.code_language_input_placeholder = Hsla::from(rgba(0x898e94cc));
        colors.code_syntax_comment = Hsla::from(rgba(0x6e747bff));
        colors.code_syntax_keyword = Hsla::from(rgba(0x82d2ceff));
        colors.code_syntax_string = Hsla::from(rgba(0xaf9cffff));
        colors.code_syntax_number = Hsla::from(rgba(0xebc88dff));
        colors.code_syntax_type = Hsla::from(rgba(0x87c3ffff));
        colors.code_syntax_function = Hsla::from(rgba(0xebc88dff));
        colors.code_syntax_constant = Hsla::from(rgba(0xaf9cffff));
        colors.code_syntax_variable = Hsla::from(rgba(0xe0e1e4ff));
        colors.code_syntax_property = Hsla::from(rgba(0x82d2ceff));
        colors.code_syntax_operator = Hsla::from(rgba(0xc7c8cbff));
        colors.code_syntax_punctuation = Hsla::from(rgba(0xc7c8cbff));
        colors.table_border = Hsla::from(rgba(0x363739ff));
        colors.table_header_bg = Hsla::from(rgba(0x202123ff));
        colors.table_cell_bg = Hsla::from(rgba(0x18191bff));
        colors.table_cell_active_outline = Hsla::from(rgba(0x726cf9ff));
        colors.table_axis_preview_bg = Hsla::from(rgba(0x726cf933));
        colors.table_axis_selected_bg = Hsla::from(rgba(0x726cf955));
        colors.table_append_button_bg = Hsla::from(rgba(0x28292bff));
        colors.table_append_button_hover = Hsla::from(rgba(0x363739ff));
        colors.table_append_button_text = Hsla::from(rgba(0xe0e1e4ff));
        colors.image_placeholder_bg = Hsla::from(rgba(0x202123ff));
        colors.image_placeholder_border = Hsla::from(rgba(0x363739ff));
        colors.image_placeholder_text = Hsla::from(rgba(0xc7c8cbff));
        colors.image_caption_text = Hsla::from(rgba(0x898e94ff));
        colors.scrollbar_thumb = Hsla::from(rgba(0x6e747bb8));
        colors.cursor = Hsla::from(rgba(0xe0e1e4ff));
        colors.selection = Hsla::from(rgba(0x152a44ff));
        colors.dialog_backdrop = Hsla::from(rgba(0x000000b8));
        colors.dialog_surface = Hsla::from(rgba(0x202123ff));
        colors.dialog_border = Hsla::from(rgba(0x363739ff));
        colors.dialog_title = Hsla::from(rgba(0xe0e1e4ff));
        colors.dialog_body = Hsla::from(rgba(0xc7c8cbff));
        colors.dialog_muted = Hsla::from(rgba(0x898e94ff));
        colors.dialog_primary_button_bg = Hsla::from(rgba(0x726cf9ff));
        colors.dialog_primary_button_hover = Hsla::from(rgba(0x827cffff));
        colors.dialog_primary_button_text = Hsla::from(rgba(0xffffffff));
        colors.dialog_secondary_button_bg = Hsla::from(rgba(0x28292bff));
        colors.dialog_secondary_button_hover = Hsla::from(rgba(0x363739ff));
        colors.dialog_secondary_button_text = Hsla::from(rgba(0xe0e1e4ff));
        colors.status_bar_background = Hsla::from(rgba(0x090909ff));
        colors.status_bar_text = Hsla::from(rgba(0xc7c8cbcc));
        colors.status_bar_text_dim = Hsla::from(rgba(0x898e94ff));
        colors.status_bar_button_hover = Hsla::from(rgba(0x28292bff));
        colors.chrome_background = Hsla::from(rgba(0x090909ff));
        colors.chrome_hover = Hsla::from(rgba(0x28292bff));
        colors.sidebar_background = Hsla::from(rgba(0x18191bff));
        colors.tab_strip_background = Hsla::from(rgba(0x18191bff));
        colors.tab_active_background = Hsla::from(rgba(0x202123ff));
        theme
    }

    /// Returns JetBrains Fleet's default light palette.
    pub fn fleet_light() -> Self {
        let mut theme = Self::xcode_light();
        theme.name = "Fleet Light".into();
        let colors = &mut theme.colors;
        colors.editor_background = Hsla::from(rgba(0xffffffff));
        colors.source_mode_block_bg = Hsla::from(rgba(0xf2f2f2ff));
        colors.comment_bg = Hsla::from(rgba(0x726cf91f));
        colors.text_default = Hsla::from(rgba(0x181818ff));
        colors.text_link = Hsla::from(rgba(0x1565c8ff));
        colors.text_placeholder = Hsla::from(rgba(0x8b8b8bcc));
        colors.text_h1 = Hsla::from(rgba(0x181818ff));
        colors.text_h2 = Hsla::from(rgba(0x181818ff));
        colors.text_h3 = Hsla::from(rgba(0x181818ff));
        colors.text_h4 = Hsla::from(rgba(0x181818ff));
        colors.text_h5 = Hsla::from(rgba(0x4f4f4fff));
        colors.text_h6 = Hsla::from(rgba(0x767676ff));
        colors.border_h1 = Hsla::from(rgba(0xe2e2e2ff));
        colors.border_h2 = Hsla::from(rgba(0xf2f2f2ff));
        colors.text_quote = Hsla::from(rgba(0x4f4f4fff));
        colors.border_quote = Hsla::from(rgba(0x726cf9ff));
        colors.callout_note_bg = Hsla::from(rgba(0x1565c81a));
        colors.callout_note_border = Hsla::from(rgba(0x1565c8ff));
        colors.callout_tip_bg = Hsla::from(rgba(0x07805f1a));
        colors.callout_tip_border = Hsla::from(rgba(0x07805fff));
        colors.callout_important_bg = Hsla::from(rgba(0xa842eb1a));
        colors.callout_important_border = Hsla::from(rgba(0xa842ebff));
        colors.callout_warning_bg = Hsla::from(rgba(0xf8ab171a));
        colors.callout_warning_border = Hsla::from(rgba(0xf8ab17ff));
        colors.callout_caution_bg = Hsla::from(rgba(0xb82d461a));
        colors.callout_caution_border = Hsla::from(rgba(0xb82d46ff));
        colors.footnote_bg = Hsla::from(rgba(0xf2f2f2ff));
        colors.footnote_border = Hsla::from(rgba(0xe2e2e2ff));
        colors.footnote_badge_bg = Hsla::from(rgba(0xe2e2e2ff));
        colors.footnote_badge_text = Hsla::from(rgba(0x4f4f4fff));
        colors.footnote_backref = Hsla::from(rgba(0x1565c8ff));
        colors.task_checkbox_border = Hsla::from(rgba(0x8b8b8bff));
        colors.task_checkbox_checked_bg = Hsla::from(rgba(0x726cf9ff));
        colors.task_checkbox_check = Hsla::from(rgba(0xffffffff));
        colors.separator_color = Hsla::from(rgba(0xe2e2e2ff));
        colors.code_bg = Hsla::from(rgba(0xf2f2f2ff));
        colors.code_text = Hsla::from(rgba(0x181818ff));
        colors.code_language_input_bg = Hsla::from(rgba(0xffffffff));
        colors.code_language_input_border = Hsla::from(rgba(0xe2e2e2ff));
        colors.code_language_input_text = Hsla::from(rgba(0x181818ff));
        colors.code_language_input_placeholder = Hsla::from(rgba(0x8b8b8bcc));
        colors.code_syntax_comment = Hsla::from(rgba(0x767676ff));
        colors.code_syntax_keyword = Hsla::from(rgba(0x07805fff));
        colors.code_syntax_string = Hsla::from(rgba(0xa842ebff));
        colors.code_syntax_number = Hsla::from(rgba(0xf8ab17ff));
        colors.code_syntax_type = Hsla::from(rgba(0x1565c8ff));
        colors.code_syntax_function = Hsla::from(rgba(0x07805fff));
        colors.code_syntax_constant = Hsla::from(rgba(0xa842ebff));
        colors.code_syntax_variable = Hsla::from(rgba(0x181818ff));
        colors.code_syntax_property = Hsla::from(rgba(0x07805fff));
        colors.code_syntax_operator = Hsla::from(rgba(0x181818ff));
        colors.code_syntax_punctuation = Hsla::from(rgba(0x4f4f4fff));
        colors.table_border = Hsla::from(rgba(0xe2e2e2ff));
        colors.table_header_bg = Hsla::from(rgba(0xf2f2f2ff));
        colors.table_cell_bg = Hsla::from(rgba(0xffffffff));
        colors.table_cell_active_outline = Hsla::from(rgba(0x726cf9ff));
        colors.table_axis_preview_bg = Hsla::from(rgba(0x726cf91f));
        colors.table_axis_selected_bg = Hsla::from(rgba(0x726cf93d));
        colors.table_append_button_bg = Hsla::from(rgba(0xf2f2f2ff));
        colors.table_append_button_hover = Hsla::from(rgba(0xe2e2e2ff));
        colors.table_append_button_text = Hsla::from(rgba(0x181818ff));
        colors.image_placeholder_bg = Hsla::from(rgba(0xf2f2f2ff));
        colors.image_placeholder_border = Hsla::from(rgba(0xe2e2e2ff));
        colors.image_placeholder_text = Hsla::from(rgba(0x4f4f4fff));
        colors.image_caption_text = Hsla::from(rgba(0x8b8b8bff));
        colors.scrollbar_thumb = Hsla::from(rgba(0x8b8b8bb8));
        colors.cursor = Hsla::from(rgba(0x181818ff));
        colors.selection = Hsla::from(rgba(0xd8e8faff));
        colors.dialog_backdrop = Hsla::from(rgba(0x18181866));
        colors.dialog_surface = Hsla::from(rgba(0xffffffff));
        colors.dialog_border = Hsla::from(rgba(0xe2e2e2ff));
        colors.dialog_title = Hsla::from(rgba(0x181818ff));
        colors.dialog_body = Hsla::from(rgba(0x4f4f4fff));
        colors.dialog_muted = Hsla::from(rgba(0x8b8b8bff));
        colors.dialog_primary_button_bg = Hsla::from(rgba(0x726cf9ff));
        colors.dialog_primary_button_hover = Hsla::from(rgba(0x625ce9ff));
        colors.dialog_primary_button_text = Hsla::from(rgba(0xffffffff));
        colors.dialog_secondary_button_bg = Hsla::from(rgba(0xf2f2f2ff));
        colors.dialog_secondary_button_hover = Hsla::from(rgba(0xe2e2e2ff));
        colors.dialog_secondary_button_text = Hsla::from(rgba(0x181818ff));
        colors.status_bar_background = Hsla::from(rgba(0xf2f2f2ff));
        colors.status_bar_text = Hsla::from(rgba(0x4f4f4fcc));
        colors.status_bar_text_dim = Hsla::from(rgba(0x8b8b8bff));
        colors.status_bar_button_hover = Hsla::from(rgba(0xe2e2e2ff));
        colors.chrome_background = Hsla::from(rgba(0xf2f2f2ff));
        colors.chrome_hover = Hsla::from(rgba(0xe2e2e2ff));
        colors.sidebar_background = Hsla::from(rgba(0xffffffff));
        colors.tab_strip_background = Hsla::from(rgba(0xf2f2f2ff));
        colors.tab_active_background = Hsla::from(rgba(0xffffffff));
        theme
    }

    /// Returns Obsidian 1.12's default dark palette.
    pub fn obsidian_dark() -> Self {
        let mut theme = Self::xcode_dark();
        theme.name = "Obsidian Dark".into();
        let colors = &mut theme.colors;
        // 直接映射官方 app.css 的 base、accent 与 code token，避免混入第三方主题色。
        colors.editor_background = Hsla::from(rgba(0x1e1e1eff));
        colors.source_mode_block_bg = Hsla::from(rgba(0x242424ff));
        colors.comment_bg = Hsla::from(rgba(0x8a5cf526));
        colors.text_default = Hsla::from(rgba(0xdadadaff));
        colors.text_link = Hsla::from(rgba(0xa68af9ff));
        colors.text_placeholder = Hsla::from(rgba(0x999999cc));
        colors.text_h1 = Hsla::from(rgba(0xdadadaff));
        colors.text_h2 = Hsla::from(rgba(0xdadadaff));
        colors.text_h3 = Hsla::from(rgba(0xdadadaff));
        colors.text_h4 = Hsla::from(rgba(0xdadadaff));
        colors.text_h5 = Hsla::from(rgba(0xb3b3b3ff));
        colors.text_h6 = Hsla::from(rgba(0xb3b3b3ff));
        colors.border_h1 = Hsla::from(rgba(0x363636ff));
        colors.border_h2 = Hsla::from(rgba(0x363636ff));
        colors.text_quote = Hsla::from(rgba(0xb3b3b3ff));
        colors.border_quote = Hsla::from(rgba(0x8a5cf5ff));
        colors.callout_note_bg = Hsla::from(rgba(0x027aff1f));
        colors.callout_note_border = Hsla::from(rgba(0x027affff));
        colors.callout_tip_bg = Hsla::from(rgba(0x44cf6e1f));
        colors.callout_tip_border = Hsla::from(rgba(0x44cf6eff));
        colors.callout_important_bg = Hsla::from(rgba(0xa882ff1f));
        colors.callout_important_border = Hsla::from(rgba(0xa882ffff));
        colors.callout_warning_bg = Hsla::from(rgba(0xe9973f1f));
        colors.callout_warning_border = Hsla::from(rgba(0xe9973fff));
        colors.callout_caution_bg = Hsla::from(rgba(0xfb464c1f));
        colors.callout_caution_border = Hsla::from(rgba(0xfb464cff));
        colors.footnote_bg = Hsla::from(rgba(0x242424ff));
        colors.footnote_border = Hsla::from(rgba(0x363636ff));
        colors.footnote_badge_bg = Hsla::from(rgba(0x363636ff));
        colors.footnote_badge_text = Hsla::from(rgba(0xdadadaff));
        colors.footnote_backref = Hsla::from(rgba(0xa68af9ff));
        colors.task_checkbox_border = Hsla::from(rgba(0x666666ff));
        colors.task_checkbox_checked_bg = Hsla::from(rgba(0x8a5cf5ff));
        colors.separator_color = Hsla::from(rgba(0x363636ff));
        colors.code_bg = Hsla::from(rgba(0x242424ff));
        colors.code_text = Hsla::from(rgba(0xdadadaff));
        colors.code_language_input_bg = Hsla::from(rgba(0x242424ff));
        colors.code_language_input_border = Hsla::from(rgba(0x363636ff));
        colors.code_language_input_text = Hsla::from(rgba(0xdadadaff));
        colors.code_language_input_placeholder = Hsla::from(rgba(0x999999cc));
        colors.code_syntax_comment = Hsla::from(rgba(0x666666ff));
        colors.code_syntax_keyword = Hsla::from(rgba(0xfa99cdff));
        colors.code_syntax_string = Hsla::from(rgba(0x44cf6eff));
        colors.code_syntax_number = Hsla::from(rgba(0xa882ffff));
        colors.code_syntax_type = Hsla::from(rgba(0x53dfddff));
        colors.code_syntax_function = Hsla::from(rgba(0xe0de71ff));
        colors.code_syntax_constant = Hsla::from(rgba(0xe9973fff));
        colors.code_syntax_variable = Hsla::from(rgba(0xdadadaff));
        colors.code_syntax_property = Hsla::from(rgba(0x53dfddff));
        colors.code_syntax_operator = Hsla::from(rgba(0xfb464cff));
        colors.code_syntax_punctuation = Hsla::from(rgba(0xb3b3b3ff));
        colors.table_border = Hsla::from(rgba(0x363636ff));
        colors.table_header_bg = Hsla::from(rgba(0x242424ff));
        colors.table_cell_bg = Hsla::from(rgba(0x1e1e1eff));
        colors.table_cell_active_outline = Hsla::from(rgba(0x8a5cf5ff));
        colors.table_axis_preview_bg = Hsla::from(rgba(0x8a5cf533));
        colors.table_axis_selected_bg = Hsla::from(rgba(0x8a5cf555));
        colors.table_append_button_bg = Hsla::from(rgba(0x363636ff));
        colors.table_append_button_hover = Hsla::from(rgba(0x3f3f3fff));
        colors.table_append_button_text = Hsla::from(rgba(0xdadadaff));
        colors.image_placeholder_bg = Hsla::from(rgba(0x262626ff));
        colors.image_placeholder_border = Hsla::from(rgba(0x363636ff));
        colors.image_placeholder_text = Hsla::from(rgba(0xb3b3b3ff));
        colors.image_caption_text = Hsla::from(rgba(0x999999ff));
        colors.scrollbar_thumb = Hsla::from(rgba(0x666666b8));
        colors.cursor = Hsla::from(rgba(0xdadadaff));
        colors.selection = Hsla::from(rgba(0x8a5cf554));
        colors.dialog_surface = Hsla::from(rgba(0x262626ff));
        colors.dialog_border = Hsla::from(rgba(0x363636ff));
        colors.dialog_title = Hsla::from(rgba(0xdadadaff));
        colors.dialog_body = Hsla::from(rgba(0xdadadaff));
        colors.dialog_muted = Hsla::from(rgba(0x999999ff));
        colors.dialog_primary_button_bg = Hsla::from(rgba(0x8a5cf5ff));
        colors.dialog_primary_button_hover = Hsla::from(rgba(0xa68af9ff));
        colors.dialog_secondary_button_bg = Hsla::from(rgba(0x363636ff));
        colors.dialog_secondary_button_hover = Hsla::from(rgba(0x3f3f3fff));
        colors.dialog_secondary_button_text = Hsla::from(rgba(0xdadadaff));
        colors.status_bar_background = Hsla::from(rgba(0x262626ff));
        colors.status_bar_text = Hsla::from(rgba(0xb3b3b3ff));
        colors.status_bar_text_dim = Hsla::from(rgba(0x999999ff));
        colors.status_bar_button_hover = Hsla::from(rgba(0x363636ff));
        colors.chrome_background = Hsla::from(rgba(0x1e1e1eff));
        colors.chrome_hover = Hsla::from(rgba(0x363636ff));
        colors.sidebar_background = Hsla::from(rgba(0x262626ff));
        colors.tab_strip_background = Hsla::from(rgba(0x262626ff));
        colors.tab_active_background = Hsla::from(rgba(0x1e1e1eff));
        theme
    }

    /// Returns Obsidian 1.12's default light palette.
    pub fn obsidian_light() -> Self {
        let mut theme = Self::xcode_light();
        theme.name = "Obsidian Light".into();
        let colors = &mut theme.colors;
        colors.editor_background = Hsla::from(rgba(0xffffffff));
        colors.source_mode_block_bg = Hsla::from(rgba(0xfafafaff));
        colors.comment_bg = Hsla::from(rgba(0x8a5cf533));
        colors.text_default = Hsla::from(rgba(0x222222ff));
        colors.text_link = Hsla::from(rgba(0x8a5cf5ff));
        colors.text_placeholder = Hsla::from(rgba(0x707070cc));
        colors.text_h1 = Hsla::from(rgba(0x222222ff));
        colors.text_h2 = Hsla::from(rgba(0x222222ff));
        colors.text_h3 = Hsla::from(rgba(0x222222ff));
        colors.text_h4 = Hsla::from(rgba(0x222222ff));
        colors.text_h5 = Hsla::from(rgba(0x5c5c5cff));
        colors.text_h6 = Hsla::from(rgba(0x5c5c5cff));
        colors.border_h1 = Hsla::from(rgba(0xe0e0e0ff));
        colors.border_h2 = Hsla::from(rgba(0xe0e0e0ff));
        colors.text_quote = Hsla::from(rgba(0x5c5c5cff));
        colors.border_quote = Hsla::from(rgba(0x8a5cf5ff));
        colors.callout_note_bg = Hsla::from(rgba(0x086ddd14));
        colors.callout_note_border = Hsla::from(rgba(0x086dddff));
        colors.callout_tip_bg = Hsla::from(rgba(0x08b94e14));
        colors.callout_tip_border = Hsla::from(rgba(0x08b94eff));
        colors.callout_important_bg = Hsla::from(rgba(0x7852ee14));
        colors.callout_important_border = Hsla::from(rgba(0x7852eeff));
        colors.callout_warning_bg = Hsla::from(rgba(0xec750014));
        colors.callout_warning_border = Hsla::from(rgba(0xec7500ff));
        colors.callout_caution_bg = Hsla::from(rgba(0xe9314714));
        colors.callout_caution_border = Hsla::from(rgba(0xe93147ff));
        colors.footnote_bg = Hsla::from(rgba(0xfafafaff));
        colors.footnote_border = Hsla::from(rgba(0xe0e0e0ff));
        colors.footnote_badge_bg = Hsla::from(rgba(0xe3e3e3ff));
        colors.footnote_badge_text = Hsla::from(rgba(0x5c5c5cff));
        colors.footnote_backref = Hsla::from(rgba(0x8a5cf5ff));
        colors.task_checkbox_border = Hsla::from(rgba(0xabababff));
        colors.task_checkbox_checked_bg = Hsla::from(rgba(0x9873f7ff));
        colors.separator_color = Hsla::from(rgba(0xe0e0e0ff));
        colors.code_bg = Hsla::from(rgba(0xfafafaff));
        colors.code_text = Hsla::from(rgba(0x222222ff));
        colors.code_language_input_bg = Hsla::from(rgba(0xffffffff));
        colors.code_language_input_border = Hsla::from(rgba(0xe0e0e0ff));
        colors.code_language_input_text = Hsla::from(rgba(0x222222ff));
        colors.code_language_input_placeholder = Hsla::from(rgba(0x707070cc));
        colors.code_syntax_comment = Hsla::from(rgba(0xabababff));
        colors.code_syntax_keyword = Hsla::from(rgba(0xd53984ff));
        colors.code_syntax_string = Hsla::from(rgba(0x08b94eff));
        colors.code_syntax_number = Hsla::from(rgba(0x7852eeff));
        colors.code_syntax_type = Hsla::from(rgba(0x00bfbcff));
        colors.code_syntax_function = Hsla::from(rgba(0xe0ac00ff));
        colors.code_syntax_constant = Hsla::from(rgba(0xec7500ff));
        colors.code_syntax_variable = Hsla::from(rgba(0x222222ff));
        colors.code_syntax_property = Hsla::from(rgba(0x00bfbcff));
        colors.code_syntax_operator = Hsla::from(rgba(0xe93147ff));
        colors.code_syntax_punctuation = Hsla::from(rgba(0x5c5c5cff));
        colors.table_border = Hsla::from(rgba(0xe0e0e0ff));
        colors.table_header_bg = Hsla::from(rgba(0xfafafaff));
        colors.table_cell_bg = Hsla::from(rgba(0xffffffff));
        colors.table_cell_active_outline = Hsla::from(rgba(0x9873f7ff));
        colors.table_axis_preview_bg = Hsla::from(rgba(0x8a5cf51f));
        colors.table_axis_selected_bg = Hsla::from(rgba(0x8a5cf53d));
        colors.table_append_button_bg = Hsla::from(rgba(0xf6f6f6ff));
        colors.table_append_button_hover = Hsla::from(rgba(0xe3e3e3ff));
        colors.table_append_button_text = Hsla::from(rgba(0x222222ff));
        colors.image_placeholder_bg = Hsla::from(rgba(0xf6f6f6ff));
        colors.image_placeholder_border = Hsla::from(rgba(0xe0e0e0ff));
        colors.image_placeholder_text = Hsla::from(rgba(0x5c5c5cff));
        colors.image_caption_text = Hsla::from(rgba(0x707070ff));
        colors.scrollbar_thumb = Hsla::from(rgba(0xabababb8));
        colors.cursor = Hsla::from(rgba(0x222222ff));
        colors.selection = Hsla::from(rgba(0x8a5cf533));
        colors.dialog_surface = Hsla::from(rgba(0xffffffff));
        colors.dialog_border = Hsla::from(rgba(0xe0e0e0ff));
        colors.dialog_title = Hsla::from(rgba(0x222222ff));
        colors.dialog_body = Hsla::from(rgba(0x222222ff));
        colors.dialog_muted = Hsla::from(rgba(0x707070ff));
        colors.dialog_primary_button_bg = Hsla::from(rgba(0x9873f7ff));
        colors.dialog_primary_button_hover = Hsla::from(rgba(0xa68af9ff));
        colors.dialog_secondary_button_bg = Hsla::from(rgba(0xfafafaff));
        colors.dialog_secondary_button_hover = Hsla::from(rgba(0xe3e3e3ff));
        colors.dialog_secondary_button_text = Hsla::from(rgba(0x222222ff));
        colors.status_bar_background = Hsla::from(rgba(0xf6f6f6ff));
        colors.status_bar_text = Hsla::from(rgba(0x5c5c5cff));
        colors.status_bar_text_dim = Hsla::from(rgba(0x707070ff));
        colors.status_bar_button_hover = Hsla::from(rgba(0xe3e3e3ff));
        colors.chrome_background = Hsla::from(rgba(0xffffffff));
        colors.chrome_hover = Hsla::from(rgba(0xfafafaff));
        colors.sidebar_background = Hsla::from(rgba(0xf6f6f6ff));
        colors.tab_strip_background = Hsla::from(rgba(0xf6f6f6ff));
        colors.tab_active_background = Hsla::from(rgba(0xffffffff));
        theme
    }

    /// Returns Claude's current official dark palette from claude.com.
    pub fn claude_dark() -> Self {
        let mut theme = Self::xcode_dark();
        theme.name = "Claude Dark".into();
        let colors = &mut theme.colors;
        // 直接映射 claude.com 的 theme-dark 与品牌色变量，锚点为 gray-950/050 和 clay-dark。
        colors.editor_background = Hsla::from(rgba(0x141413ff));
        colors.source_mode_block_bg = Hsla::from(rgba(0x1a1918ff));
        colors.comment_bg = Hsla::from(rgba(0xc4684926));
        colors.text_default = Hsla::from(rgba(0xfaf9f5ff));
        colors.text_link = Hsla::from(rgba(0xc46849ff));
        colors.text_placeholder = Hsla::from(rgba(0x87867fcc));
        colors.text_h1 = Hsla::from(rgba(0xfaf9f5ff));
        colors.text_h2 = Hsla::from(rgba(0xfaf9f5ff));
        colors.text_h3 = Hsla::from(rgba(0xfaf9f5ff));
        colors.text_h4 = Hsla::from(rgba(0xfaf9f5ff));
        colors.text_h5 = Hsla::from(rgba(0xb0aea5ff));
        colors.text_h6 = Hsla::from(rgba(0x87867fff));
        colors.border_h1 = Hsla::from(rgba(0x5e5d59ff));
        colors.border_h2 = Hsla::from(rgba(0x3d3d3aff));
        colors.text_quote = Hsla::from(rgba(0xb0aea5ff));
        colors.border_quote = Hsla::from(rgba(0xc46849ff));
        colors.callout_note_bg = Hsla::from(rgba(0x6a9bcc26));
        colors.callout_note_border = Hsla::from(rgba(0x6a9bccff));
        colors.callout_tip_bg = Hsla::from(rgba(0x788c5d26));
        colors.callout_tip_border = Hsla::from(rgba(0x788c5dff));
        colors.callout_important_bg = Hsla::from(rgba(0xc4668626));
        colors.callout_important_border = Hsla::from(rgba(0xc46686ff));
        colors.callout_warning_bg = Hsla::from(rgba(0xd9775726));
        colors.callout_warning_border = Hsla::from(rgba(0xd97757ff));
        colors.callout_caution_bg = Hsla::from(rgba(0xbf4d4326));
        colors.callout_caution_border = Hsla::from(rgba(0xbf4d43ff));
        colors.footnote_bg = Hsla::from(rgba(0x1a1918ff));
        colors.footnote_border = Hsla::from(rgba(0x3d3d3aff));
        colors.footnote_badge_bg = Hsla::from(rgba(0x262624ff));
        colors.footnote_badge_text = Hsla::from(rgba(0xb0aea5ff));
        colors.footnote_backref = Hsla::from(rgba(0xc46849ff));
        colors.task_checkbox_border = Hsla::from(rgba(0x5e5d59ff));
        colors.task_checkbox_bg = Hsla::from(rgba(0x000000ff));
        colors.task_checkbox_checked_bg = Hsla::from(rgba(0xc46849ff));
        colors.task_checkbox_check = Hsla::from(rgba(0xffffffff));
        colors.separator_color = Hsla::from(rgba(0x3d3d3aff));
        colors.code_bg = Hsla::from(rgba(0x1a1918ff));
        colors.code_text = Hsla::from(rgba(0xfaf9f5ff));
        colors.code_language_input_bg = Hsla::from(rgba(0x262624ff));
        colors.code_language_input_border = Hsla::from(rgba(0x3d3d3aff));
        colors.code_language_input_text = Hsla::from(rgba(0xfaf9f5ff));
        colors.code_language_input_placeholder = Hsla::from(rgba(0x87867fcc));
        colors.code_syntax_comment = Hsla::from(rgba(0x87867fff));
        colors.code_syntax_keyword = Hsla::from(rgba(0xc46686ff));
        colors.code_syntax_string = Hsla::from(rgba(0xbcd1caff));
        colors.code_syntax_number = Hsla::from(rgba(0xd97757ff));
        colors.code_syntax_type = Hsla::from(rgba(0x6a9bccff));
        colors.code_syntax_function = Hsla::from(rgba(0xe3daccff));
        colors.code_syntax_constant = Hsla::from(rgba(0xebceceff));
        colors.code_syntax_variable = Hsla::from(rgba(0xfaf9f5ff));
        colors.code_syntax_property = Hsla::from(rgba(0x6a9bccff));
        colors.code_syntax_operator = Hsla::from(rgba(0xc46849ff));
        colors.code_syntax_punctuation = Hsla::from(rgba(0xb0aea5ff));
        colors.table_border = Hsla::from(rgba(0x3d3d3aff));
        colors.table_header_bg = Hsla::from(rgba(0x1a1918ff));
        colors.table_cell_bg = Hsla::from(rgba(0x141413ff));
        colors.table_cell_active_outline = Hsla::from(rgba(0xc46849ff));
        colors.table_axis_preview_bg = Hsla::from(rgba(0xc4684933));
        colors.table_axis_selected_bg = Hsla::from(rgba(0xc4684955));
        colors.table_append_button_bg = Hsla::from(rgba(0x262624ff));
        colors.table_append_button_hover = Hsla::from(rgba(0x30302eff));
        colors.table_append_button_text = Hsla::from(rgba(0xfaf9f5ff));
        colors.image_placeholder_bg = Hsla::from(rgba(0x1a1918ff));
        colors.image_placeholder_border = Hsla::from(rgba(0x3d3d3aff));
        colors.image_placeholder_text = Hsla::from(rgba(0xb0aea5ff));
        colors.image_caption_text = Hsla::from(rgba(0x87867fff));
        colors.scrollbar_thumb = Hsla::from(rgba(0x5e5d59b8));
        colors.cursor = Hsla::from(rgba(0xfaf9f5ff));
        colors.selection = Hsla::from(rgba(0xc468494d));
        colors.dialog_backdrop = Hsla::from(rgba(0x000000b8));
        colors.dialog_surface = Hsla::from(rgba(0x1a1918ff));
        colors.dialog_border = Hsla::from(rgba(0x3d3d3aff));
        colors.dialog_title = Hsla::from(rgba(0xfaf9f5ff));
        colors.dialog_body = Hsla::from(rgba(0xb0aea5ff));
        colors.dialog_muted = Hsla::from(rgba(0x87867fff));
        colors.dialog_primary_button_bg = Hsla::from(rgba(0xc6613fff));
        colors.dialog_primary_button_hover = Hsla::from(rgba(0xd97757ff));
        colors.dialog_primary_button_text = Hsla::from(rgba(0xffffffff));
        colors.dialog_secondary_button_bg = Hsla::from(rgba(0x30302eff));
        colors.dialog_secondary_button_hover = Hsla::from(rgba(0x3d3d3aff));
        colors.dialog_secondary_button_text = Hsla::from(rgba(0xfaf9f5ff));
        colors.dialog_danger_button_bg = Hsla::from(rgba(0xbf4d43ff));
        colors.dialog_danger_button_hover = Hsla::from(rgba(0xa8433bff));
        colors.dialog_danger_button_text = Hsla::from(rgba(0xffffffff));
        colors.status_bar_background = Hsla::from(rgba(0x1a1918ff));
        colors.status_bar_text = Hsla::from(rgba(0xb0aea5ff));
        colors.status_bar_text_dim = Hsla::from(rgba(0x87867fff));
        colors.status_bar_button_hover = Hsla::from(rgba(0x30302eff));
        colors.chrome_background = Hsla::from(rgba(0x141413ff));
        colors.chrome_hover = Hsla::from(rgba(0x262624ff));
        colors.sidebar_background = Hsla::from(rgba(0x1a1918ff));
        colors.tab_strip_background = Hsla::from(rgba(0x1a1918ff));
        colors.tab_active_background = Hsla::from(rgba(0x141413ff));
        theme
    }

    /// Returns Claude's current official light palette from claude.com.
    pub fn claude_light() -> Self {
        let mut theme = Self::xcode_light();
        theme.name = "Claude Light".into();
        let colors = &mut theme.colors;
        colors.editor_background = Hsla::from(rgba(0xfaf9f5ff));
        colors.source_mode_block_bg = Hsla::from(rgba(0xf5f4edff));
        colors.comment_bg = Hsla::from(rgba(0xd9775726));
        colors.text_default = Hsla::from(rgba(0x141413ff));
        colors.text_link = Hsla::from(rgba(0xd97757ff));
        colors.text_placeholder = Hsla::from(rgba(0x5e5d59cc));
        colors.text_h1 = Hsla::from(rgba(0x141413ff));
        colors.text_h2 = Hsla::from(rgba(0x141413ff));
        colors.text_h3 = Hsla::from(rgba(0x141413ff));
        colors.text_h4 = Hsla::from(rgba(0x141413ff));
        colors.text_h5 = Hsla::from(rgba(0x30302eff));
        colors.text_h6 = Hsla::from(rgba(0x5e5d59ff));
        colors.border_h1 = Hsla::from(rgba(0xb0aea5ff));
        colors.border_h2 = Hsla::from(rgba(0xd1cfc5ff));
        colors.text_quote = Hsla::from(rgba(0x30302eff));
        colors.border_quote = Hsla::from(rgba(0xd97757ff));
        colors.callout_note_bg = Hsla::from(rgba(0x6a9bcc1f));
        colors.callout_note_border = Hsla::from(rgba(0x6a9bccff));
        colors.callout_tip_bg = Hsla::from(rgba(0x788c5d1f));
        colors.callout_tip_border = Hsla::from(rgba(0x788c5dff));
        colors.callout_important_bg = Hsla::from(rgba(0xc466861f));
        colors.callout_important_border = Hsla::from(rgba(0xc46686ff));
        colors.callout_warning_bg = Hsla::from(rgba(0xd977571f));
        colors.callout_warning_border = Hsla::from(rgba(0xd97757ff));
        colors.callout_caution_bg = Hsla::from(rgba(0xbf4d431f));
        colors.callout_caution_border = Hsla::from(rgba(0xbf4d43ff));
        colors.footnote_bg = Hsla::from(rgba(0xf5f4edff));
        colors.footnote_border = Hsla::from(rgba(0xd1cfc5ff));
        colors.footnote_badge_bg = Hsla::from(rgba(0xe8e6dcff));
        colors.footnote_badge_text = Hsla::from(rgba(0x30302eff));
        colors.footnote_backref = Hsla::from(rgba(0xd97757ff));
        colors.task_checkbox_border = Hsla::from(rgba(0xb0aea5ff));
        colors.task_checkbox_bg = Hsla::from(rgba(0xffffffff));
        colors.task_checkbox_checked_bg = Hsla::from(rgba(0xd97757ff));
        colors.task_checkbox_check = Hsla::from(rgba(0xffffffff));
        colors.separator_color = Hsla::from(rgba(0xd1cfc5ff));
        colors.code_bg = Hsla::from(rgba(0xf5f4edff));
        colors.code_text = Hsla::from(rgba(0x141413ff));
        colors.code_language_input_bg = Hsla::from(rgba(0xffffffff));
        colors.code_language_input_border = Hsla::from(rgba(0xd1cfc5ff));
        colors.code_language_input_text = Hsla::from(rgba(0x141413ff));
        colors.code_language_input_placeholder = Hsla::from(rgba(0x5e5d59cc));
        colors.code_syntax_comment = Hsla::from(rgba(0x5e5d59ff));
        colors.code_syntax_keyword = Hsla::from(rgba(0xc46686ff));
        colors.code_syntax_string = Hsla::from(rgba(0x788c5dff));
        colors.code_syntax_number = Hsla::from(rgba(0xc46849ff));
        colors.code_syntax_type = Hsla::from(rgba(0x476f98ff));
        colors.code_syntax_function = Hsla::from(rgba(0x9a593dff));
        colors.code_syntax_constant = Hsla::from(rgba(0xc46686ff));
        colors.code_syntax_variable = Hsla::from(rgba(0x141413ff));
        colors.code_syntax_property = Hsla::from(rgba(0x476f98ff));
        colors.code_syntax_operator = Hsla::from(rgba(0xc46849ff));
        colors.code_syntax_punctuation = Hsla::from(rgba(0x30302eff));
        colors.table_border = Hsla::from(rgba(0xd1cfc5ff));
        colors.table_header_bg = Hsla::from(rgba(0xf5f4edff));
        colors.table_cell_bg = Hsla::from(rgba(0xfaf9f5ff));
        colors.table_cell_active_outline = Hsla::from(rgba(0xd97757ff));
        colors.table_axis_preview_bg = Hsla::from(rgba(0xd977571f));
        colors.table_axis_selected_bg = Hsla::from(rgba(0xd977573d));
        colors.table_append_button_bg = Hsla::from(rgba(0xf0eee6ff));
        colors.table_append_button_hover = Hsla::from(rgba(0xe8e6dcff));
        colors.table_append_button_text = Hsla::from(rgba(0x141413ff));
        colors.image_placeholder_bg = Hsla::from(rgba(0xf5f4edff));
        colors.image_placeholder_border = Hsla::from(rgba(0xd1cfc5ff));
        colors.image_placeholder_text = Hsla::from(rgba(0x30302eff));
        colors.image_caption_text = Hsla::from(rgba(0x5e5d59ff));
        colors.scrollbar_thumb = Hsla::from(rgba(0xb0aea5b8));
        colors.cursor = Hsla::from(rgba(0x141413ff));
        colors.selection = Hsla::from(rgba(0xd9775733));
        colors.dialog_backdrop = Hsla::from(rgba(0x14141366));
        colors.dialog_surface = Hsla::from(rgba(0xfaf9f5ff));
        colors.dialog_border = Hsla::from(rgba(0xd1cfc5ff));
        colors.dialog_title = Hsla::from(rgba(0x141413ff));
        colors.dialog_body = Hsla::from(rgba(0x30302eff));
        colors.dialog_muted = Hsla::from(rgba(0x5e5d59ff));
        colors.dialog_primary_button_bg = Hsla::from(rgba(0xc6613fff));
        colors.dialog_primary_button_hover = Hsla::from(rgba(0xd97757ff));
        colors.dialog_primary_button_text = Hsla::from(rgba(0xffffffff));
        colors.dialog_secondary_button_bg = Hsla::from(rgba(0xe8e6dcff));
        colors.dialog_secondary_button_hover = Hsla::from(rgba(0xdedcd1ff));
        colors.dialog_secondary_button_text = Hsla::from(rgba(0x141413ff));
        colors.dialog_danger_button_bg = Hsla::from(rgba(0xbf4d43ff));
        colors.dialog_danger_button_hover = Hsla::from(rgba(0xa8433bff));
        colors.dialog_danger_button_text = Hsla::from(rgba(0xffffffff));
        colors.status_bar_background = Hsla::from(rgba(0xf5f4edff));
        colors.status_bar_text = Hsla::from(rgba(0x30302eff));
        colors.status_bar_text_dim = Hsla::from(rgba(0x5e5d59ff));
        colors.status_bar_button_hover = Hsla::from(rgba(0xe8e6dcff));
        colors.chrome_background = Hsla::from(rgba(0xfaf9f5ff));
        colors.chrome_hover = Hsla::from(rgba(0xf0eee6ff));
        colors.sidebar_background = Hsla::from(rgba(0xf5f4edff));
        colors.tab_strip_background = Hsla::from(rgba(0xf5f4edff));
        colors.tab_active_background = Hsla::from(rgba(0xfaf9f5ff));
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
