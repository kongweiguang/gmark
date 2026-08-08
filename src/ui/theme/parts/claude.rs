// @author kongweiguang

//! Built-in palette constructors.

use super::*;

impl Theme {
    /// Returns Claude's current official dark palette from claude.com.
    pub fn claude_dark() -> Self {
        let mut theme = Self::xcode_dark();
        theme.name = "Claude Dark".into();
        let colors = &mut theme.colors;
        colors.workbench = WorkbenchThemeTokens::claude_dark();
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
        colors.code_syntax_comment = Hsla::from(rgba(0x918f87ff));
        colors.code_syntax_keyword = Hsla::from(rgba(0xc46686ff));
        colors.code_syntax_string = Hsla::from(rgba(0xbcd1caff));
        colors.code_syntax_number = Hsla::from(rgba(0xd97757ff));
        colors.code_syntax_type = Hsla::from(rgba(0x6a9bccff));
        colors.code_syntax_function = Hsla::from(rgba(0xe3daccff));
        colors.code_syntax_constant = Hsla::from(rgba(0xebceceff));
        colors.code_syntax_variable = Hsla::from(rgba(0xfaf9f5ff));
        colors.code_syntax_property = Hsla::from(rgba(0x6a9bccff));
        colors.code_syntax_operator = Hsla::from(rgba(0xb0aea5ff));
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
        colors.workbench = WorkbenchThemeTokens::claude_light();
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
        colors.code_syntax_operator = Hsla::from(rgba(0x30302eff));
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
