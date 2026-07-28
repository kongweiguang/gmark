// @author kongweiguang

use super::*;

impl PreferencesWindow {
    pub(super) fn dropdown_button(
        &self,
        id: &'static str,
        label: String,
        dropdown: PreferencesDropdown,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let focus_handle = self.dropdown_focus_handles[dropdown.index()].clone();
        let pointer_focus_handle = focus_handle.clone();
        div()
            .w(px(280.0))
            .h(px(32.0))
            .tab_index(0)
            .track_focus(&focus_handle)
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(d.menu_item_radius))
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .bg(c.dialog_surface)
            .hover(|this| this.bg(c.chrome_hover))
            .focus(move |this| this.border_color(c.text_link))
            .cursor_pointer()
            .text_size(px(t.dialog_body_size))
            .text_color(c.dialog_body)
            .id(id)
            .debug_selector(move || id.to_owned())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .child(
                svg()
                    .path(CHEVRON_DOWN_ICON)
                    .size(px(14.0))
                    .text_color(c.dialog_body),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                pointer_focus_handle.focus(window);
                this.on_dropdown_click(dropdown, window, cx);
            }))
            .on_key_down(cx.listener(move |this, event, window, cx| {
                this.on_dropdown_key_down(dropdown, event, window, cx);
            }))
    }

    /// 下拉列表是独立浮层，不能参与设置行布局，否则左侧标签会随列表高度跳动。
    pub(super) fn dropdown_list(theme: &Theme) -> Div {
        let c = &theme.colors;
        let d = &theme.dimensions;
        div()
            .absolute()
            .occlude()
            .top(px(36.0))
            .w(px(280.0))
            .p(px(4.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .rounded(px(10.0))
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .bg(c.dialog_surface)
            .shadow_lg()
    }

    pub(super) fn dropdown_item(
        id: impl Into<ElementId>,
        label: String,
        selected: bool,
        highlighted: bool,
        theme: &Theme,
        on_click: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        div()
            .w_full()
            .min_h(px(30.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(d.menu_item_radius))
            .cursor_pointer()
            .bg(if highlighted {
                c.text_link.opacity(0.14)
            } else {
                hsla(0.0, 0.0, 0.0, 0.0)
            })
            .hover(|this| this.bg(c.dialog_secondary_button_hover))
            .text_size(px(t.dialog_body_size))
            .text_color(c.dialog_body)
            .id(id)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .child(
                div()
                    .size(px(16.0))
                    .flex_shrink_0()
                    .children(selected.then(|| {
                        svg()
                            .path(CHECK_ICON)
                            .size(px(14.0))
                            .text_color(c.dialog_body)
                    })),
            )
            .on_click(cx.listener(on_click))
    }

    pub(super) fn labeled_row(&self, label: &str, control: impl IntoElement, theme: &Theme) -> Div {
        let c = &theme.colors;
        let t = &theme.typography;
        div()
            .w_full()
            .max_w(px(PREFERENCES_FORM_WIDTH))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(20.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(t.dialog_body_size))
                    .font_weight(t.dialog_button_weight.to_font_weight())
                    .text_color(c.dialog_title)
                    .child(SharedString::from(label.to_string())),
            )
            .child(control)
    }

    fn theme_appearance_option(
        &self,
        id: &'static str,
        index: usize,
        label: SharedString,
        option: ThemeAppearance,
        selected: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let focus_handle = self.theme_appearance_focus_handles[index].clone();
        let pointer_focus_handle = focus_handle.clone();
        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .flex_1()
            .min_w(px(0.0))
            .h(px(34.0))
            .tab_index(0)
            .track_focus(&focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .px(px(8.0))
            .rounded(px(d.menu_item_radius))
            .border(px(d.dialog_border_width))
            .border_color(if selected {
                c.text_link
            } else {
                c.dialog_border
            })
            .bg(if selected {
                c.text_link.opacity(0.16)
            } else {
                c.dialog_surface
            })
            .hover(|this| this.bg(c.chrome_hover))
            .focus(|this| this.border_color(c.text_link))
            .cursor_pointer()
            .text_size(px(t.dialog_body_size))
            .text_color(if selected { c.text_link } else { c.dialog_body })
            .child(
                div()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                pointer_focus_handle.focus(window);
                this.preview_theme_appearance(option, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    focus_handle.focus(window);
                    this.preview_theme_appearance(option, cx);
                    cx.stop_propagation();
                }
            }))
    }

    fn theme_palette_option(
        &self,
        id: &'static str,
        index: usize,
        label: SharedString,
        option: ThemePalette,
        selected: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let focus_handle = self.theme_palette_focus_handles[index].clone();
        let pointer_focus_handle = focus_handle.clone();
        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .flex_1()
            .min_w(px(0.0))
            .h(px(34.0))
            .tab_index(0)
            .track_focus(&focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .px(px(8.0))
            .rounded(px(d.menu_item_radius))
            .border(px(d.dialog_border_width))
            .border_color(if selected {
                c.text_link
            } else {
                c.dialog_border
            })
            .bg(if selected {
                c.text_link.opacity(0.16)
            } else {
                c.dialog_surface
            })
            .hover(|this| this.bg(c.chrome_hover))
            .focus(|this| this.border_color(c.text_link))
            .cursor_pointer()
            .text_size(px(t.dialog_body_size))
            .text_color(if selected { c.text_link } else { c.dialog_body })
            .child(
                div()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                pointer_focus_handle.focus(window);
                this.preview_theme_palette(option, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    focus_handle.focus(window);
                    this.preview_theme_palette(option, cx);
                    cx.stop_propagation();
                }
            }))
    }

    pub(super) fn numeric_stepper(
        &self,
        id: &'static str,
        input: PreferencesNumericInput,
        decrease: PreferencesStepperControl,
        increase: PreferencesStepperControl,
        unit: &'static str,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let input_id = input.input_id();
        let input_is_valid = self.numeric_input_is_valid(input, cx);
        let input_focus_handle = self.numeric_inputs[input.index()]
            .read(cx)
            .focus_handle
            .clone();
        let button =
            |control: PreferencesStepperControl, icon: &'static str, cx: &mut Context<Self>| {
                let focus_handle = self.stepper_focus_handles[control.index()].clone();
                let pointer_focus_handle = focus_handle.clone();
                div()
                    .id(control.id())
                    .debug_selector(move || control.id().to_owned())
                    .size(px(32.0))
                    .tab_index(0)
                    .track_focus(&focus_handle)
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(d.menu_item_radius))
                    .border(px(d.dialog_border_width))
                    .border_color(c.dialog_border)
                    .bg(c.dialog_secondary_button_bg)
                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                    .focus(move |this| this.border_color(c.text_link))
                    .cursor_pointer()
                    .text_color(c.dialog_secondary_button_text)
                    .child(
                        svg()
                            .path(icon)
                            .size(px(14.0))
                            .text_color(c.dialog_secondary_button_text),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        pointer_focus_handle.focus(window);
                        this.activate_stepper(control, cx);
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.activate_stepper(control, cx);
                            cx.stop_propagation();
                        }
                    }))
            };

        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .w(px(160.0))
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(button(decrease, MINUS_ICON, cx))
            .child(
                div()
                    .id(input_id)
                    .debug_selector(move || input_id.to_owned())
                    .flex_1()
                    .h_full()
                    .min_w(px(0.0))
                    .relative()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .rounded(px(d.menu_item_radius))
                    .border(px(d.dialog_border_width))
                    .border_color(if input_is_valid {
                        c.dialog_border
                    } else {
                        c.dialog_danger_button_bg
                    })
                    .bg(c.dialog_surface)
                    .px(px(7.0))
                    .cursor(CursorStyle::IBeam)
                    .text_size(px(t.dialog_body_size))
                    .text_color(c.dialog_title)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .child(self.numeric_inputs[input.index()].clone()),
                    )
                    .children((!unit.is_empty()).then(|| {
                        div()
                            .flex_shrink_0()
                            .pl(px(3.0))
                            .text_color(c.dialog_muted)
                            .child(unit)
                    }))
                    .on_click(move |_, window, _| input_focus_handle.focus(window)),
            )
            .child(button(increase, PLUS_ICON, cx))
    }

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

    pub(super) fn render_startup_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let auto_check_label = if cx
            .global::<I18nManager>()
            .current_language_id()
            .starts_with("zh")
        {
            "自动检查软件更新"
        } else {
            "Automatically check for updates"
        };
        let selected = match self.startup_open {
            StartupOpenPreference::NewFile => strings.preferences_startup_new_file.clone(),
            StartupOpenPreference::LastOpenedFile => {
                strings.preferences_startup_last_opened_file.clone()
            }
        };
        let dropdown = div()
            .relative()
            .w(px(280.0))
            .h(px(32.0))
            .flex_shrink_0()
            .child(self.dropdown_button(
                "preferences-startup-dropdown",
                selected,
                PreferencesDropdown::Startup,
                theme,
                cx,
            ));
        let startup_list = if self.startup_dropdown_open {
            let new_file_label = strings.preferences_startup_new_file.clone();
            let last_file_label = strings.preferences_startup_last_opened_file.clone();
            let list = Self::dropdown_list(theme)
                .right_0()
                .child(Self::dropdown_item(
                    "preferences-startup-new-file",
                    new_file_label,
                    self.startup_open == StartupOpenPreference::NewFile,
                    self.dropdown_selected_indices[PreferencesDropdown::Startup.index()] == 0,
                    theme,
                    |this, _, _, cx| {
                        this.commit_dropdown_selection(PreferencesDropdown::Startup, 0, cx);
                    },
                    cx,
                ))
                .child(Self::dropdown_item(
                    "preferences-startup-last-opened-file",
                    last_file_label,
                    self.startup_open == StartupOpenPreference::LastOpenedFile,
                    self.dropdown_selected_indices[PreferencesDropdown::Startup.index()] == 1,
                    theme,
                    |this, _, _, cx| {
                        this.commit_dropdown_selection(PreferencesDropdown::Startup, 1, cx);
                    },
                    cx,
                ));
            Some(list)
        } else {
            None
        };
        let auto_save_label = match self.auto_save {
            AutoSavePreference::Off => strings.preferences_auto_save_off.clone(),
            AutoSavePreference::AfterDelay => strings.preferences_auto_save_after_delay.clone(),
        };
        let auto_save_dropdown = div()
            .relative()
            .w(px(280.0))
            .h(px(32.0))
            .flex_shrink_0()
            .child(self.dropdown_button(
                "preferences-auto-save-dropdown",
                auto_save_label,
                PreferencesDropdown::AutoSave,
                theme,
                cx,
            ));
        let auto_save_list = if self.auto_save_dropdown_open {
            let mut list = Self::dropdown_list(theme).top(px(88.0)).right_0();
            for (index, option) in [AutoSavePreference::Off, AutoSavePreference::AfterDelay]
                .into_iter()
                .enumerate()
            {
                let label = match option {
                    AutoSavePreference::Off => strings.preferences_auto_save_off.clone(),
                    AutoSavePreference::AfterDelay => {
                        strings.preferences_auto_save_after_delay.clone()
                    }
                };
                list = list.child(Self::dropdown_item(
                    ("preferences-auto-save-option", index),
                    label,
                    self.auto_save == option,
                    self.dropdown_selected_indices[PreferencesDropdown::AutoSave.index()] == index,
                    theme,
                    move |this, _, _, cx| {
                        this.commit_dropdown_selection(PreferencesDropdown::AutoSave, index, cx);
                    },
                    cx,
                ));
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
            .gap(px(20.0))
            .child(
                self.labeled_row(&strings.preferences_startup_option, dropdown, theme)
                    .debug_selector(|| "preferences-startup-row".to_owned()),
            )
            .child(
                self.labeled_row(
                    &strings.preferences_auto_save_option,
                    auto_save_dropdown,
                    theme,
                )
                .debug_selector(|| "preferences-auto-save-row".to_owned()),
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
                            .child(auto_check_label),
                    )
                    .child(self.preference_switch(
                        PreferencesSwitch::AutoCheckUpdates,
                        self.auto_check_updates,
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
                            .child(strings.preferences_spell_check.clone()),
                    )
                    .child(self.preference_switch(
                        PreferencesSwitch::SpellCheck,
                        self.spell_check,
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .pt(px(8.0))
                    .border_t(px(theme.dimensions.dialog_border_width))
                    .border_color(theme.colors.dialog_border)
                    .text_size(px(theme.typography.dialog_title_size))
                    .font_weight(theme.typography.dialog_title_weight.to_font_weight())
                    .text_color(theme.colors.dialog_title)
                    .child(strings.preferences_document_loading.clone()),
            )
            .child(self.labeled_row(
                &strings.preferences_document_max_resident_mib,
                self.numeric_stepper(
                    "preferences-document-resident-mib",
                    PreferencesNumericInput::ResidentMib,
                    PreferencesStepperControl::ResidentMibDecrease,
                    PreferencesStepperControl::ResidentMibIncrease,
                    "MiB",
                    theme,
                    cx,
                ),
                theme,
            ))
            .children(self.document_loading.has_invalid_override().then(|| {
                div()
                    .text_size(px(theme.typography.dialog_body_size))
                    .text_color(theme.colors.dialog_danger_button_bg)
                    .child(strings.preferences_document_loading_invalid.clone())
            }))
            .child(
                div()
                    .text_size(px(theme.typography.dialog_body_size))
                    .text_color(theme.colors.dialog_muted)
                    .child(strings.preferences_document_loading_next_open.clone()),
            )
            // 浮层最后绘制，确保不会被后续设置行覆盖。
            .children(startup_list)
            .children(auto_save_list)
    }

    pub(super) fn render_theme_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let appearance_label = strings.preferences_theme_appearance.clone();
        let palette_label = strings.preferences_palette.clone();
        let dark_label = strings.preferences_theme_dark.clone();
        let light_label = strings.preferences_theme_light.clone();
        let system_label = strings.preferences_follow_system_theme.clone();
        let xcode_label = "Xcode";
        let jetbrains_label = "JetBrains";
        let obsidian_label = "Obsidian";
        let appearance_control = div().w(px(280.0)).flex().gap(px(4.0));
        let appearance_dark = self.theme_appearance_option(
            "preferences-theme-appearance-dark",
            0,
            dark_label.into(),
            ThemeAppearance::Dark,
            self.theme_appearance == ThemeAppearance::Dark,
            theme,
            cx,
        );
        let appearance_light = self.theme_appearance_option(
            "preferences-theme-appearance-light",
            1,
            light_label.into(),
            ThemeAppearance::Light,
            self.theme_appearance == ThemeAppearance::Light,
            theme,
            cx,
        );
        let appearance_system = self.theme_appearance_option(
            "preferences-theme-appearance-system",
            2,
            system_label.into(),
            ThemeAppearance::System,
            self.theme_appearance == ThemeAppearance::System,
            theme,
            cx,
        );
        let appearance_control =
            appearance_control.children([appearance_dark, appearance_light, appearance_system]);
        let palette_control = div().w(px(280.0)).flex().gap(px(4.0));
        let palette_xcode = self.theme_palette_option(
            "preferences-theme-palette-xcode",
            0,
            xcode_label.into(),
            ThemePalette::Xcode,
            self.theme_palette == ThemePalette::Xcode,
            theme,
            cx,
        );
        let palette_jetbrains = self.theme_palette_option(
            "preferences-theme-palette-jetbrains",
            1,
            jetbrains_label.into(),
            ThemePalette::JetBrains,
            self.theme_palette == ThemePalette::JetBrains,
            theme,
            cx,
        );
        let palette_obsidian = self.theme_palette_option(
            "preferences-theme-palette-obsidian",
            2,
            obsidian_label.into(),
            ThemePalette::Obsidian,
            self.theme_palette == ThemePalette::Obsidian,
            theme,
            cx,
        );
        let palette_control =
            palette_control.children([palette_xcode, palette_jetbrains, palette_obsidian]);
        let language_dropdown = div()
            .relative()
            .w(px(280.0))
            .h(px(32.0))
            .flex_shrink_0()
            .child(
                self.dropdown_button(
                    "preferences-language-dropdown",
                    self.language_options
                        .iter()
                        .find(|entry| entry.id == self.selected_language_id)
                        .map(|entry| entry.name.clone())
                        .unwrap_or_else(|| self.selected_language_id.clone()),
                    PreferencesDropdown::Language,
                    theme,
                    cx,
                ),
            );
        let language_list = if self.language_dropdown_open {
            let mut list = Self::dropdown_list(theme)
                .top(px(80.0))
                .right_0()
                .id("preferences-language-dropdown-list")
                .max_h(px(240.0))
                .overflow_y_scroll();
            for (index, entry) in self.language_options.clone().into_iter().enumerate() {
                let selected = entry.id == self.selected_language_id;
                let highlighted =
                    self.dropdown_selected_indices[PreferencesDropdown::Language.index()] == index;
                let language_id = entry.id.clone();
                list = list.child(Self::dropdown_item(
                    ("preferences-language-option", index),
                    entry.name,
                    selected,
                    highlighted,
                    theme,
                    move |this, _, _, cx| {
                        this.selected_language_id = language_id.clone();
                        this.close_all_dropdowns();
                        cx.notify();
                    },
                    cx,
                ));
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
            .gap(px(12.0))
            .child(self.labeled_row(&appearance_label, appearance_control, theme))
            .child(self.labeled_row(&palette_label, palette_control, theme))
            .child(self.labeled_row(&strings.menu_language, language_dropdown, theme))
            .children(language_list)
    }
}
