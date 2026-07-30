// @author kongweiguang

//! Editor preference controls.

use super::*;

impl PreferencesWindow {
    pub(super) fn render_editor_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let selected_font = if self.editor_font_family.is_empty() {
            strings.preferences_editor_font_system_placeholder.clone()
        } else {
            self.editor_font_family.clone()
        };
        let chinese = cx
            .global::<I18nManager>()
            .current_language_id()
            .starts_with("zh");
        let code_folding_label = if chinese {
            "代码折叠"
        } else {
            "Code Folding"
        };
        let format_on_save_label = if chinese {
            "保存时格式化"
        } else {
            "Format on Save"
        };
        let font_dropdown = div()
            .relative()
            .w(px(280.0))
            .h(px(32.0))
            .flex_shrink_0()
            .child(self.dropdown_button(
                "preferences-editor-font-family",
                selected_font,
                PreferencesDropdown::Font,
                theme,
                cx,
            ));
        let font_list = if self.font_dropdown_open {
            let mut list = Self::dropdown_list(theme)
                .right_0()
                .id("preferences-editor-font-list")
                .debug_selector(|| "preferences-editor-font-list".to_owned())
                .max_h(px(260.0))
                .overflow_y_scroll();
            for (index, font) in self.font_options.iter().cloned().enumerate() {
                let label = if font.is_empty() {
                    strings.preferences_editor_font_system_placeholder.clone()
                } else {
                    font.clone()
                };
                list = list.child(
                    div()
                        .w_full()
                        .debug_selector(move || format!("preferences-editor-font-option-{index}"))
                        .child(Self::dropdown_item(
                            ("preferences-editor-font-option", index),
                            label,
                            self.editor_font_family == font,
                            self.dropdown_selected_indices[PreferencesDropdown::Font.index()]
                                == index,
                            theme,
                            move |this, _, _, cx| {
                                this.commit_dropdown_selection(
                                    PreferencesDropdown::Font,
                                    index,
                                    cx,
                                );
                            },
                            cx,
                        )),
                );
            }
            Some(list)
        } else {
            None
        };

        div()
            .relative()
            .w_full()
            .max_w(px(PREFERENCES_FORM_WIDTH))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(self.labeled_row(
                &strings.preferences_editor_font_family,
                font_dropdown,
                theme,
            ))
            .child(self.labeled_row(
                &strings.preferences_editor_font_size,
                self.numeric_stepper(
                    "preferences-editor-font-size",
                    PreferencesNumericInput::FontSize,
                    PreferencesStepperControl::FontSizeDecrease,
                    PreferencesStepperControl::FontSizeIncrease,
                    "px",
                    theme,
                    cx,
                ),
                theme,
            ))
            .child(self.labeled_row(
                &strings.preferences_editor_line_height,
                self.numeric_stepper(
                    "preferences-editor-line-height",
                    PreferencesNumericInput::LineHeight,
                    PreferencesStepperControl::LineHeightDecrease,
                    PreferencesStepperControl::LineHeightIncrease,
                    "%",
                    theme,
                    cx,
                ),
                theme,
            ))
            .child(self.labeled_row(
                &strings.preferences_editor_content_width,
                self.numeric_stepper(
                    "preferences-editor-content-width",
                    PreferencesNumericInput::ContentWidth,
                    PreferencesStepperControl::ContentWidthDecrease,
                    PreferencesStepperControl::ContentWidthIncrease,
                    "px",
                    theme,
                    cx,
                ),
                theme,
            ))
            .child(
                div()
                    .w_full()
                    .max_w(px(PREFERENCES_FORM_WIDTH))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(theme.typography.dialog_body_size))
                            .text_color(theme.colors.dialog_body)
                            .child(strings.preferences_auto_pair_brackets.clone()),
                    )
                    .child(self.preference_switch(
                        PreferencesSwitch::AutoPairBrackets,
                        self.auto_pair_brackets,
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(PREFERENCES_FORM_WIDTH))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(theme.typography.dialog_body_size))
                            .text_color(theme.colors.dialog_body)
                            .child(strings.preferences_auto_pair_markdown.clone()),
                    )
                    .child(self.preference_switch(
                        PreferencesSwitch::AutoPairMarkdown,
                        self.auto_pair_markdown,
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(PREFERENCES_FORM_WIDTH))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(theme.typography.dialog_body_size))
                            .text_color(theme.colors.dialog_body)
                            .child(code_folding_label),
                    )
                    .child(self.preference_switch(
                        PreferencesSwitch::CodeFolding,
                        self.code_folding,
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(PREFERENCES_FORM_WIDTH))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(theme.typography.dialog_body_size))
                            .text_color(theme.colors.dialog_body)
                            .child(format_on_save_label),
                    )
                    .child(self.preference_switch(
                        PreferencesSwitch::FormatOnSave,
                        self.format_on_save,
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(PREFERENCES_FORM_WIDTH))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(theme.typography.dialog_body_size))
                            .text_color(theme.colors.dialog_body)
                            .child(strings.preferences_show_tab_bar_actions.clone()),
                    )
                    .child(self.preference_switch(
                        PreferencesSwitch::ShowTabBarActions,
                        self.show_tab_bar_actions,
                        cx,
                    )),
            )
            // 字体菜单最后绘制，确保浮层覆盖后续数值设置行。
            .children(font_list)
    }
}
