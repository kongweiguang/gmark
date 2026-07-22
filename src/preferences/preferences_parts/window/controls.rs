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

    pub(super) fn theme_dropdown_button(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let dropdown = PreferencesDropdown::Theme;
        let focus_handle = self.dropdown_focus_handles[dropdown.index()].clone();
        let pointer_focus_handle = focus_handle.clone();
        div()
            .id("preferences-theme-dropdown")
            .debug_selector(|| "preferences-theme-dropdown".to_owned())
            .w_full()
            .h(px(32.0))
            .tab_index(0)
            .track_focus(&focus_handle)
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(d.menu_item_radius))
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .bg(c.dialog_surface)
            .hover(|this| this.bg(c.chrome_hover))
            .focus(move |this| this.border_color(c.text_link))
            .cursor_pointer()
            .text_size(px(t.dialog_body_size))
            .text_color(c.dialog_body)
            .child(
                div()
                    .id("preferences-theme-selected-icon")
                    .debug_selector(|| "preferences-theme-selected-icon".to_owned())
                    .size(px(16.0))
                    .flex_shrink_0()
                    .text_color(c.dialog_muted)
                    .child(
                        svg()
                            .path(theme_option_icon(&self.selected_theme_id))
                            .size(px(16.0))
                            .text_color(c.dialog_muted),
                    ),
            )
            .child(
                div()
                    .id("preferences-theme-swatch")
                    .debug_selector(|| "preferences-theme-swatch".to_owned())
                    .w(px(42.0))
                    .h(px(16.0))
                    .flex_shrink_0()
                    .flex()
                    .overflow_hidden()
                    .rounded(px(4.0))
                    .border(px(d.dialog_border_width))
                    .border_color(c.dialog_border)
                    .child(div().flex_1().h_full().bg(c.editor_background))
                    .child(div().flex_1().h_full().bg(c.sidebar_background))
                    .child(div().flex_1().h_full().bg(c.selection))
                    .child(div().flex_1().h_full().bg(c.text_link)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(self.selected_theme_name()),
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

    pub(super) fn theme_dropdown_item(
        index: usize,
        entry: ThemeCatalogEntry,
        selected: bool,
        highlighted: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let icon = theme_option_icon(&entry.id);
        let theme_id = entry.id.clone();
        div()
            .id(("preferences-theme-option", index))
            .debug_selector(move || format!("preferences-theme-option-{index}"))
            .w(px(280.0))
            .min_h(px(30.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(8.0))
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
            .child(
                div()
                    .id(("preferences-theme-option-icon", index))
                    .debug_selector(move || format!("preferences-theme-option-icon-{index}"))
                    .size(px(16.0))
                    .flex_shrink_0()
                    .text_color(c.dialog_muted)
                    .child(svg().path(icon).size(px(16.0)).text_color(c.dialog_muted)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(entry.name),
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
            .on_click(cx.listener(move |this, _, _, cx| this.preview_theme(theme_id.clone(), cx)))
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

    pub(super) fn numeric_stepper(
        &self,
        id: &'static str,
        value: String,
        decrease: PreferencesStepperControl,
        increase: PreferencesStepperControl,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
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
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(d.menu_item_radius))
                    .border(px(d.dialog_border_width))
                    .border_color(c.dialog_border)
                    .bg(c.dialog_surface)
                    .text_size(px(t.dialog_body_size))
                    .text_color(c.dialog_title)
                    .child(value),
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
            .gap(px(20.0))
            .child(self.labeled_row(
                &strings.preferences_editor_font_family,
                font_dropdown,
                theme,
            ))
            .child(self.labeled_row(
                &strings.preferences_editor_font_size,
                self.numeric_stepper(
                    "preferences-editor-font-size",
                    format!("{} px", self.editor_font_size),
                    PreferencesStepperControl::FontSizeDecrease,
                    PreferencesStepperControl::FontSizeIncrease,
                    theme,
                    cx,
                ),
                theme,
            ))
            .child(self.labeled_row(
                &strings.preferences_editor_line_height,
                self.numeric_stepper(
                    "preferences-editor-line-height",
                    format!("{}%", self.editor_line_height_percent),
                    PreferencesStepperControl::LineHeightDecrease,
                    PreferencesStepperControl::LineHeightIncrease,
                    theme,
                    cx,
                ),
                theme,
            ))
            .child(self.labeled_row(
                &strings.preferences_editor_content_width,
                self.numeric_stepper(
                    "preferences-editor-content-width",
                    format!("{} px", self.editor_content_width),
                    PreferencesStepperControl::ContentWidthDecrease,
                    PreferencesStepperControl::ContentWidthIncrease,
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
                            .child(strings.preferences_workspace_sidebar_right.clone()),
                    )
                    .child(self.preference_switch(
                        PreferencesSwitch::WorkspaceSidebarRight,
                        self.workspace_sidebar_position == WorkspaceSidebarPosition::Right,
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
        let loading_preset_label = match self.document_loading.preset {
            DocumentLoadingPreset::Balanced => {
                strings.preferences_document_loading_balanced.clone()
            }
            DocumentLoadingPreset::LowMemory => {
                strings.preferences_document_loading_low_memory.clone()
            }
            DocumentLoadingPreset::HighPerformance => strings
                .preferences_document_loading_high_performance
                .clone(),
        };
        let loading_dropdown = div()
            .relative()
            .w(px(280.0))
            .h(px(32.0))
            .flex_shrink_0()
            .child(self.dropdown_button(
                "preferences-document-loading-dropdown",
                loading_preset_label,
                PreferencesDropdown::DocumentLoadingPreset,
                theme,
                cx,
            ));
        let loading_list = if self.document_loading_dropdown_open {
            let mut list = Self::dropdown_list(theme).top(px(238.0)).right_0();
            for (index, preset) in [
                DocumentLoadingPreset::Balanced,
                DocumentLoadingPreset::LowMemory,
                DocumentLoadingPreset::HighPerformance,
            ]
            .into_iter()
            .enumerate()
            {
                let label = match preset {
                    DocumentLoadingPreset::Balanced => {
                        strings.preferences_document_loading_balanced.clone()
                    }
                    DocumentLoadingPreset::LowMemory => {
                        strings.preferences_document_loading_low_memory.clone()
                    }
                    DocumentLoadingPreset::HighPerformance => strings
                        .preferences_document_loading_high_performance
                        .clone(),
                };
                list = list.child(Self::dropdown_item(
                    ("preferences-document-loading-option", index),
                    label,
                    self.document_loading.preset == preset,
                    self.dropdown_selected_indices
                        [PreferencesDropdown::DocumentLoadingPreset.index()]
                        == index,
                    theme,
                    move |this, _, _, cx| {
                        this.commit_dropdown_selection(
                            PreferencesDropdown::DocumentLoadingPreset,
                            index,
                            cx,
                        );
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
                &strings.preferences_document_loading_preset,
                loading_dropdown,
                theme,
            ))
            .child(self.labeled_row(
                &strings.preferences_document_max_resident_mib,
                self.numeric_stepper(
                    "preferences-document-resident-mib",
                    format!("{} MiB", self.document_loading.effective_max_resident_mib()),
                    PreferencesStepperControl::ResidentMibDecrease,
                    PreferencesStepperControl::ResidentMibIncrease,
                    theme,
                    cx,
                ),
                theme,
            ))
            .child(
                self.labeled_row(
                    &strings.preferences_document_max_resident_lines,
                    self.numeric_stepper(
                        "preferences-document-resident-lines",
                        self.document_loading
                            .effective_max_resident_lines()
                            .to_string(),
                        PreferencesStepperControl::ResidentLinesDecrease,
                        PreferencesStepperControl::ResidentLinesIncrease,
                        theme,
                        cx,
                    ),
                    theme,
                ),
            )
            .child(
                self.labeled_row(
                    &strings.preferences_document_max_structural_units,
                    self.numeric_stepper(
                        "preferences-document-structural-units",
                        self.document_loading
                            .effective_max_structural_units()
                            .to_string(),
                        PreferencesStepperControl::StructuralUnitsDecrease,
                        PreferencesStepperControl::StructuralUnitsIncrease,
                        theme,
                        cx,
                    ),
                    theme,
                ),
            )
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
            .children(loading_list)
    }

    pub(super) fn render_theme_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let dropdown = div()
            .relative()
            .w(px(280.0))
            .h(px(32.0))
            .flex_shrink_0()
            .child(self.theme_dropdown_button(theme, cx));
        let theme_list = if self.theme_dropdown_open {
            let mut list = Self::dropdown_list(theme)
                .right_0()
                .id("preferences-theme-dropdown-list")
                .max_h(px(240.0))
                .overflow_y_scroll();

            for (index, entry) in self.theme_options.clone().into_iter().enumerate() {
                let selected = entry.id == self.selected_theme_id;
                let highlighted =
                    self.dropdown_selected_indices[PreferencesDropdown::Theme.index()] == index;
                list = list.child(Self::theme_dropdown_item(
                    index,
                    entry,
                    selected,
                    highlighted,
                    theme,
                    cx,
                ));
            }
            Some(list)
        } else {
            None
        };
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
            .child(self.labeled_row(&strings.preferences_local_theme, dropdown, theme))
            .child(self.labeled_row(&strings.menu_language, language_dropdown, theme))
            // 浮层最后绘制，避免后续设置行截获菜单区域的绘制与点击。
            .children(theme_list)
            .children(language_list)
    }
}
