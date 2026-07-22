// @author kongweiguang

use super::*;

impl PreferencesWindow {
    pub(super) fn image_paste_behavior_label(
        behavior: ImagePasteBehavior,
        strings: &crate::i18n::I18nStrings,
    ) -> String {
        match behavior {
            ImagePasteBehavior::None => strings.preferences_image_paste_none.clone(),
            ImagePasteBehavior::CopyToDocumentFolder => strings
                .preferences_image_paste_copy_to_document_folder
                .clone(),
            ImagePasteBehavior::CopyToAssetsFolder => strings
                .preferences_image_paste_copy_to_assets_folder
                .clone(),
            ImagePasteBehavior::CopyToNamedAssetsFolder => strings
                .preferences_image_paste_copy_to_named_assets_folder
                .clone(),
        }
    }

    pub(super) fn render_image_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let options = [
            ImagePasteBehavior::None,
            ImagePasteBehavior::CopyToDocumentFolder,
            ImagePasteBehavior::CopyToAssetsFolder,
            ImagePasteBehavior::CopyToNamedAssetsFolder,
        ];
        let mut dropdown = div()
            .relative()
            .w(px(280.0))
            .h(px(32.0))
            .flex_shrink_0()
            .child(self.dropdown_button(
                "preferences-image-dropdown",
                Self::image_paste_behavior_label(self.image_paste_behavior, strings),
                PreferencesDropdown::Image,
                theme,
                cx,
            ));
        if self.image_dropdown_open {
            let mut list = Self::dropdown_list(theme).left_0();
            for (index, behavior) in options.into_iter().enumerate() {
                let selected = behavior == self.image_paste_behavior;
                let label = Self::image_paste_behavior_label(behavior, strings);
                list = list.child(Self::dropdown_item(
                    ("preferences-image-option", index),
                    label,
                    selected,
                    self.dropdown_selected_indices[PreferencesDropdown::Image.index()] == index,
                    theme,
                    move |this, _, _, cx| {
                        this.commit_dropdown_selection(PreferencesDropdown::Image, index, cx);
                    },
                    cx,
                ));
            }
            dropdown = dropdown.child(list);
        }
        self.labeled_row(&strings.preferences_image_insert_behavior, dropdown, theme)
    }

    pub(super) fn shortcut_category_label(
        category: ShortcutCategory,
        strings: &crate::i18n::I18nStrings,
    ) -> String {
        match category {
            ShortcutCategory::File => strings.preferences_shortcuts_group_file.clone(),
            ShortcutCategory::Edit => strings.preferences_shortcuts_group_edit.clone(),
            ShortcutCategory::Navigation => strings.preferences_shortcuts_group_navigation.clone(),
            ShortcutCategory::Formatting => strings.preferences_shortcuts_group_formatting.clone(),
            ShortcutCategory::Block => strings.preferences_shortcuts_group_block.clone(),
            ShortcutCategory::Other => strings.preferences_shortcuts_group_other.clone(),
        }
    }

    pub(super) fn shortcut_command_label(
        command: ShortcutCommand,
        strings: &crate::i18n::I18nStrings,
    ) -> String {
        match command {
            ShortcutCommand::Newline => strings.preferences_shortcut_newline.clone(),
            ShortcutCommand::DeleteBack => strings.preferences_shortcut_delete_back.clone(),
            ShortcutCommand::Delete => strings.preferences_shortcut_delete.clone(),
            ShortcutCommand::WordDeleteBack => {
                strings.preferences_shortcut_word_delete_back.clone()
            }
            ShortcutCommand::WordDeleteForward => {
                strings.preferences_shortcut_word_delete_forward.clone()
            }
            ShortcutCommand::FocusPrev => strings.preferences_shortcut_focus_prev.clone(),
            ShortcutCommand::FocusNext => strings.preferences_shortcut_focus_next.clone(),
            ShortcutCommand::MoveLeft => strings.preferences_shortcut_move_left.clone(),
            ShortcutCommand::MoveRight => strings.preferences_shortcut_move_right.clone(),
            ShortcutCommand::WordMoveLeft => strings.preferences_shortcut_word_move_left.clone(),
            ShortcutCommand::WordMoveRight => strings.preferences_shortcut_word_move_right.clone(),
            ShortcutCommand::Home => strings.preferences_shortcut_home.clone(),
            ShortcutCommand::End => strings.preferences_shortcut_end.clone(),
            ShortcutCommand::BlockUp => strings.preferences_shortcut_block_up.clone(),
            ShortcutCommand::BlockDown => strings.preferences_shortcut_block_down.clone(),
            ShortcutCommand::PageUp => strings.preferences_shortcut_page_up.clone(),
            ShortcutCommand::PageDown => strings.preferences_shortcut_page_down.clone(),
            ShortcutCommand::JumpToTop => strings.preferences_shortcut_jump_to_top.clone(),
            ShortcutCommand::JumpToBottom => strings.preferences_shortcut_jump_to_bottom.clone(),
            ShortcutCommand::SelectLeft => strings.preferences_shortcut_select_left.clone(),
            ShortcutCommand::SelectRight => strings.preferences_shortcut_select_right.clone(),
            ShortcutCommand::WordSelectLeft => {
                strings.preferences_shortcut_word_select_left.clone()
            }
            ShortcutCommand::WordSelectRight => {
                strings.preferences_shortcut_word_select_right.clone()
            }
            ShortcutCommand::SelectHome => strings.preferences_shortcut_select_home.clone(),
            ShortcutCommand::SelectEnd => strings.preferences_shortcut_select_end.clone(),
            ShortcutCommand::SelectAll => strings.preferences_shortcut_select_all.clone(),
            ShortcutCommand::Copy => strings.preferences_shortcut_copy.clone(),
            ShortcutCommand::CopyAsMarkdown => {
                strings.preferences_shortcut_copy_as_markdown.clone()
            }
            ShortcutCommand::Cut => strings.preferences_shortcut_cut.clone(),
            ShortcutCommand::Paste => strings.preferences_shortcut_paste.clone(),
            ShortcutCommand::PasteAsPlainText => {
                strings.preferences_shortcut_paste_as_plain_text.clone()
            }
            ShortcutCommand::Undo => strings.preferences_shortcut_undo.clone(),
            ShortcutCommand::Redo => strings.preferences_shortcut_redo.clone(),
            ShortcutCommand::BoldSelection => strings.preferences_shortcut_bold_selection.clone(),
            ShortcutCommand::ItalicSelection => {
                strings.preferences_shortcut_italic_selection.clone()
            }
            ShortcutCommand::UnderlineSelection => {
                strings.preferences_shortcut_underline_selection.clone()
            }
            ShortcutCommand::StrikethroughSelection => {
                strings.preferences_shortcut_strikethrough_selection.clone()
            }
            ShortcutCommand::CodeSelection => strings.preferences_shortcut_code_selection.clone(),
            ShortcutCommand::LinkSelection => strings.preferences_shortcut_link_selection.clone(),
            ShortcutCommand::HighlightSelection => strings.slash_commands["highlight"].clone(),
            ShortcutCommand::SuperscriptSelection => strings.slash_commands["superscript"].clone(),
            ShortcutCommand::SubscriptSelection => strings.slash_commands["subscript"].clone(),
            ShortcutCommand::InlineMathSelection => strings.slash_commands["inline_math"].clone(),
            ShortcutCommand::SetParagraph => strings
                .slash_commands
                .get("paragraph")
                .cloned()
                .unwrap_or_else(|| "Paragraph".to_owned()),
            ShortcutCommand::SetHeading1 => strings.slash_commands["heading_1"].clone(),
            ShortcutCommand::SetHeading2 => strings.slash_commands["heading_2"].clone(),
            ShortcutCommand::SetHeading3 => strings.slash_commands["heading_3"].clone(),
            ShortcutCommand::SetHeading4 => strings.slash_commands["heading_4"].clone(),
            ShortcutCommand::SetHeading5 => strings.slash_commands["heading_5"].clone(),
            ShortcutCommand::SetHeading6 => strings.slash_commands["heading_6"].clone(),
            ShortcutCommand::IndentBlock => strings.preferences_shortcut_indent_block.clone(),
            ShortcutCommand::OutdentBlock => strings.preferences_shortcut_outdent_block.clone(),
            ShortcutCommand::ExitCodeBlock => strings.preferences_shortcut_exit_code_block.clone(),
            ShortcutCommand::SaveDocument => strings.preferences_shortcut_save_document.clone(),
            ShortcutCommand::SaveDocumentAs => {
                strings.preferences_shortcut_save_document_as.clone()
            }
            ShortcutCommand::NewWindow => strings.preferences_shortcut_new_window.clone(),
            ShortcutCommand::NewTab => strings.preferences_shortcut_new_tab.clone(),
            ShortcutCommand::OpenFile => strings.preferences_shortcut_open_file.clone(),
            ShortcutCommand::OpenFolder => strings.preferences_shortcut_open_folder.clone(),
            ShortcutCommand::OpenPreferences => strings.menu_preferences.clone(),
            ShortcutCommand::QuitApplication => {
                strings.preferences_shortcut_quit_application.clone()
            }
            ShortcutCommand::CloseWindow => strings.preferences_shortcut_close_window.clone(),
            ShortcutCommand::CloseTab => strings.preferences_shortcut_close_tab.clone(),
            ShortcutCommand::ReopenClosedTab => {
                strings.preferences_shortcut_reopen_closed_tab.clone()
            }
            ShortcutCommand::PreviousTab => strings.preferences_shortcut_previous_tab.clone(),
            ShortcutCommand::NextTab => strings.preferences_shortcut_next_tab.clone(),
            ShortcutCommand::DismissTransientUi => {
                strings.preferences_shortcut_dismiss_transient_ui.clone()
            }
            ShortcutCommand::ToggleViewMode => {
                strings.preferences_shortcut_toggle_view_mode.clone()
            }
            ShortcutCommand::ToggleWorkspace => {
                strings.preferences_shortcut_toggle_workspace.clone()
            }
            ShortcutCommand::QuickOpen => strings.preferences_shortcut_quick_open.clone(),
            ShortcutCommand::CommandPalette => strings.preferences_shortcut_command_palette.clone(),
            ShortcutCommand::GoToLine => strings.preferences_shortcut_go_to_line.clone(),
            ShortcutCommand::FindInDocument => {
                strings.preferences_shortcut_find_in_document.clone()
            }
            ShortcutCommand::ReplaceInDocument => {
                strings.preferences_shortcut_replace_in_document.clone()
            }
            ShortcutCommand::FindNext => strings.preferences_shortcut_find_next.clone(),
            ShortcutCommand::FindPrevious => strings.preferences_shortcut_find_previous.clone(),
            ShortcutCommand::ToggleFocusMode => {
                strings.preferences_shortcut_toggle_focus_mode.clone()
            }
            ShortcutCommand::ToggleTypewriterMode => {
                strings.preferences_shortcut_toggle_typewriter_mode.clone()
            }
        }
    }

    pub(super) fn format_template(template: &str, key: &str, value: &str) -> String {
        template.replace(key, value)
    }

    pub(super) fn begin_recording_shortcut(
        &mut self,
        command: ShortcutCommand,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.recording_shortcut = Some(command);
        self.shortcut_error = None;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(super) fn reset_shortcut(
        &mut self,
        command: ShortcutCommand,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(definition) = shortcut_definitions()
            .iter()
            .find(|definition| definition.command == command)
        {
            self.keybindings.remove(definition.id);
        }
        if self.recording_shortcut == Some(command) {
            self.recording_shortcut = None;
        }
        self.shortcut_error = None;
        cx.notify();
    }

    pub(super) fn capture_shortcut_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.recording_shortcut.is_none() {
            let search_focused = self.search_input.read(cx).focus_handle.is_focused(window);
            let query = self.search_query(cx);
            if !search_focused || query.is_empty() {
                return;
            }

            let results = {
                let strings = cx.global::<I18nManager>().strings();
                self.preference_search_results(strings, cx)
            };
            match event.keystroke.key.as_str() {
                "up" => {
                    self.search_selected = self.search_selected.saturating_sub(1);
                }
                "down" => {
                    self.search_selected =
                        (self.search_selected + 1).min(results.len().saturating_sub(1));
                }
                "enter" => {
                    if let Some(result) = results.get(self.search_selected) {
                        self.open_search_result(result.nav, window, cx);
                    }
                }
                "escape" => self.clear_search(cx),
                _ => return,
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }

        let Some(command) = self.recording_shortcut else {
            return;
        };
        cx.stop_propagation();
        if event.is_held {
            return;
        }

        let key = event.keystroke.unparse();
        if key == "escape" {
            self.recording_shortcut = None;
            self.shortcut_error = None;
            cx.notify();
            return;
        }

        let Some(keys) = normalize_shortcut_keys(std::slice::from_ref(&key)) else {
            let strings = cx.global::<I18nManager>().strings();
            self.shortcut_error = Some(Self::format_template(
                &strings.preferences_shortcut_invalid_template,
                "{shortcut}",
                &key,
            ));
            cx.notify();
            return;
        };

        if let Some(conflict) = shortcut_conflict_for(command, &keys, &self.keybindings) {
            let strings = cx.global::<I18nManager>().strings();
            let label = Self::shortcut_command_label(conflict.command, strings);
            self.shortcut_error = Some(Self::format_template(
                &strings.preferences_shortcut_conflict_template,
                "{command}",
                &label,
            ));
            cx.notify();
            return;
        }

        if let Some(definition) = shortcut_definitions()
            .iter()
            .find(|definition| definition.command == command)
        {
            let defaults = definition
                .default_keys
                .iter()
                .map(|key| key.to_string())
                .collect::<Vec<_>>();
            if keys == defaults {
                self.keybindings.remove(definition.id);
            } else {
                self.keybindings.insert(definition.id.to_string(), keys);
            }
        }
        self.recording_shortcut = None;
        self.shortcut_error = None;
        cx.notify();
    }

    pub(super) fn shortcut_chip(label: &str, theme: &Theme) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        div()
            .min_w(px(58.0))
            .h(px(24.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px((d.menu_item_radius - 1.0).max(0.0)))
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .bg(c.code_bg)
            .text_size(px((t.dialog_body_size - 1.0).max(10.0)))
            .text_color(c.code_text)
            .child(SharedString::from(label.to_string()))
    }

    pub(super) fn shortcut_action_button(
        id: impl Into<ElementId>,
        label: String,
        theme: &Theme,
        on_click: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        div()
            .id(id)
            .h(px(28.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px((d.dialog_radius - 5.0).max(0.0)))
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .bg(c.dialog_secondary_button_bg)
            .hover(|this| this.bg(c.dialog_secondary_button_hover))
            .cursor_pointer()
            .text_size(px((t.dialog_button_size - 1.0).max(10.0)))
            .font_weight(t.dialog_button_weight.to_font_weight())
            .text_color(c.dialog_secondary_button_text)
            .child(label)
            .on_click(cx.listener(on_click))
    }

    pub(super) fn render_shortcut_row(
        &self,
        definition: ShortcutDefinition,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let is_recording = self.recording_shortcut == Some(definition.command);
        let keys = resolved_shortcut_keys(&self.keybindings, definition.command);
        let label = Self::shortcut_command_label(definition.command, strings);
        let command = definition.command;

        let mut chips = div().flex().flex_wrap().gap(px(6.0));
        if is_recording {
            chips = chips.child(Self::shortcut_chip(
                &strings.preferences_shortcut_recording,
                theme,
            ));
        } else {
            for key in keys {
                chips = chips.child(Self::shortcut_chip(&key, theme));
            }
        }

        div()
            .w_full()
            .min_h(px(42.0))
            .px(px(10.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(d.menu_item_radius))
            .bg(c.dialog_surface)
            .child(
                div()
                    .min_w(px(144.0))
                    .text_size(px(t.dialog_body_size))
                    .text_color(c.dialog_body)
                    .child(label),
            )
            .child(div().flex_1().child(chips))
            .child(
                div()
                    .flex()
                    .gap(px(6.0))
                    .child(Self::shortcut_action_button(
                        ("preferences-shortcut-record", definition.command as u32),
                        strings.preferences_shortcut_record.clone(),
                        theme,
                        move |this, event, window, cx| {
                            this.begin_recording_shortcut(command, event, window, cx)
                        },
                        cx,
                    ))
                    .child(Self::shortcut_action_button(
                        ("preferences-shortcut-reset", definition.command as u32),
                        strings.preferences_shortcut_reset.clone(),
                        theme,
                        move |this, event, window, cx| {
                            this.reset_shortcut(command, event, window, cx)
                        },
                        cx,
                    )),
            )
    }

    pub(super) fn render_shortcuts_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let mut content = div()
            .id("preferences-shortcuts-scroll")
            .debug_selector(|| "preferences-shortcuts-scroll".to_owned())
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(18.0))
            .pr(px(4.0));

        let categories = [
            ShortcutCategory::File,
            ShortcutCategory::Edit,
            ShortcutCategory::Navigation,
            ShortcutCategory::Formatting,
            ShortcutCategory::Block,
            ShortcutCategory::Other,
        ];

        for category in categories {
            let mut group = div().w_full().flex().flex_col().gap(px(8.0)).child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(t.dialog_body_size))
                            .font_weight(t.dialog_button_weight.to_font_weight())
                            .text_color(c.dialog_title)
                            .child(Self::shortcut_category_label(category, strings)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h(px(d.dialog_border_width.max(1.0)))
                            .bg(c.dialog_border),
                    ),
            );
            for definition in shortcut_definitions()
                .iter()
                .copied()
                .filter(|definition| definition.category == category)
            {
                group = group.child(self.render_shortcut_row(definition, theme, strings, cx));
            }
            content = content.child(group);
        }

        let mut page = div()
            .w_full()
            .h_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.0));
        if let Some(error) = &self.shortcut_error {
            page = page.child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .text_size(px(t.dialog_body_size))
                    .text_color(c.dialog_danger_button_bg)
                    .child(error.clone()),
            );
        }
        page.child(content)
    }

    pub(super) fn render_search_results(
        &self,
        results: &[PreferenceSearchItem],
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let t = &theme.typography;
        let mut list = div()
            .id("preferences-search-results")
            .debug_selector(|| "preferences-search-results".to_owned())
            .w_full()
            .max_w(px(PREFERENCES_FORM_WIDTH))
            .flex()
            .flex_col()
            .gap(px(2.0));

        if results.is_empty() {
            return list.child(
                div()
                    .id("preferences-search-no-results")
                    .debug_selector(|| "preferences-search-no-results".to_owned())
                    .w_full()
                    .py(px(18.0))
                    .text_size(px(t.dialog_body_size))
                    .text_color(c.dialog_muted)
                    .child(strings.preferences_search_no_results.clone()),
            );
        }

        for (index, result) in results.iter().enumerate() {
            let nav = result.nav;
            let category = result.category.clone();
            let label = result.label.clone();
            list = list.child(
                div()
                    .id(("preferences-search-result", index))
                    .debug_selector(move || format!("preferences-search-result-{index}"))
                    .w_full()
                    .h(px(40.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(5.0))
                    .bg(if index == self.search_selected {
                        c.selection
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .child(
                        div()
                            .w(px(104.0))
                            .flex_shrink_0()
                            .overflow_hidden()
                            .truncate()
                            .text_size(px((t.dialog_body_size - 1.0).max(10.0)))
                            .text_color(c.dialog_muted)
                            .child(category),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .truncate()
                            .text_size(px(t.dialog_body_size))
                            .text_color(c.dialog_body)
                            .child(label),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_search_result(nav, window, cx);
                    })),
            );
        }
        list
    }

    pub(super) fn render_status_bar_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = &theme.colors;
        let t = &theme.typography;

        let switch_row =
            |label: &str, preference: PreferencesSwitch, checked: bool, cx: &mut Context<Self>| {
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(t.dialog_body_size))
                            .text_color(c.dialog_body)
                            .child(SharedString::from(label.to_string())),
                    )
                    .child(self.preference_switch(preference, checked, cx))
            };

        let items = div()
            .id("preferences-status-bar-options")
            .debug_selector(|| "preferences-status-bar-options".to_owned())
            .w_full()
            .max_w(px(PREFERENCES_FORM_WIDTH))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(switch_row(
                &strings.preferences_status_bar_enabled,
                PreferencesSwitch::StatusBarEnabled,
                self.status_bar_enabled,
                cx,
            ))
            .child(switch_row(
                &strings.preferences_status_bar_show_word_count,
                PreferencesSwitch::StatusBarWordCount,
                self.status_bar_show_word_count,
                cx,
            ))
            .child(switch_row(
                &strings.preferences_status_bar_show_cursor_position,
                PreferencesSwitch::StatusBarCursorPosition,
                self.status_bar_show_cursor_position,
                cx,
            ))
            .child(switch_row(
                &strings.preferences_status_bar_show_sidebar_toggle,
                PreferencesSwitch::StatusBarSidebarToggle,
                self.status_bar_show_sidebar_toggle,
                cx,
            ))
            .child(switch_row(
                &strings.preferences_status_bar_show_mode_switch,
                PreferencesSwitch::StatusBarModeSwitch,
                self.status_bar_show_mode_switch,
                cx,
            ));

        div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .child(items)
    }
}
