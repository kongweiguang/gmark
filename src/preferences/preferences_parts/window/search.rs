// @author kongweiguang

use super::*;

impl PreferencesWindow {
    pub(super) fn preference_search_results(
        &self,
        strings: &crate::i18n::I18nStrings,
        cx: &App,
    ) -> Vec<PreferenceSearchItem> {
        let query = self.search_query(cx);
        if query.is_empty() {
            return Vec::new();
        }
        let tokens = query.split_whitespace().collect::<Vec<_>>();
        self.preference_search_items(strings)
            .into_iter()
            .filter(|item| {
                let searchable = format!("{} {}", item.category, item.label).to_lowercase();
                tokens.iter().all(|token| searchable.contains(token))
            })
            .collect()
    }

    pub(super) fn clear_search(&mut self, cx: &mut Context<Self>) {
        let input = self.search_input.clone();
        input.update(cx, |input, cx| {
            let len = input.visible_len();
            input.replace_text_in_visible_range(0..len, "", None, false, cx);
        });
        self.search_selected = 0;
    }

    pub(super) fn clear_search_from_button(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_search(cx);
        self.search_input.read(cx).focus_handle.focus(window);
    }

    pub(super) fn clear_search_from_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            self.clear_search(cx);
            self.search_input.read(cx).focus_handle.focus(window);
            cx.stop_propagation();
        }
    }

    pub(super) fn open_search_result(
        &mut self,
        nav: PreferencesNav,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_nav(nav, cx);
        self.clear_search(cx);
        self.focus_handle.focus(window);
    }

    pub(super) fn select_nav(&mut self, nav: PreferencesNav, cx: &mut Context<Self>) {
        self.nav = nav;
        self.close_all_dropdowns();
        self.recording_shortcut = None;
        self.shortcut_error = None;
        cx.notify();
    }

    pub(super) fn has_unsaved_changes(&self) -> bool {
        self.startup_open != self.saved_startup_open
            || self.auto_save != self.saved_auto_save
            || self.spell_check != self.saved_spell_check
            || self.auto_pair_brackets != self.saved_auto_pair_brackets
            || self.auto_pair_markdown != self.saved_auto_pair_markdown
            || self.editor_font_size != self.saved_editor_font_size
            || self.editor_line_height_percent != self.saved_editor_line_height_percent
            || self.editor_content_width != self.saved_editor_content_width
            || self.editor_font_family != self.saved_editor_font_family
            || self.workspace_sidebar_position != self.saved_workspace_sidebar_position
            || self.show_tab_bar_actions != self.saved_show_tab_bar_actions
            || self.selected_theme_id != self.saved_theme_id
            || self.selected_language_id != self.saved_language_id
            || self.image_paste_behavior != self.saved_image_paste_behavior
            || normalize_shortcut_config(&self.keybindings)
                != normalize_shortcut_config(&self.saved_keybindings)
            || self.document_loading != self.saved_document_loading
            || self.status_bar_enabled != self.saved_status_bar_enabled
            || self.status_bar_show_word_count != self.saved_status_bar_show_word_count
            || self.status_bar_show_cursor_position != self.saved_status_bar_show_cursor_position
            || self.status_bar_show_sidebar_toggle != self.saved_status_bar_show_sidebar_toggle
            || self.status_bar_show_mode_switch != self.saved_status_bar_show_mode_switch
    }

    pub(super) fn set_nav_file(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.select_nav(PreferencesNav::File, cx);
        self.clear_search(cx);
    }

    pub(super) fn set_nav_theme(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.select_nav(PreferencesNav::Theme, cx);
        self.clear_search(cx);
    }

    pub(super) fn set_nav_editor(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_nav(PreferencesNav::Editor, cx);
        self.clear_search(cx);
    }

    pub(super) fn set_nav_image(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.select_nav(PreferencesNav::Image, cx);
        self.clear_search(cx);
    }

    pub(super) fn set_nav_shortcuts(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_nav(PreferencesNav::Shortcuts, cx);
        self.clear_search(cx);
    }

    pub(super) fn set_nav_status_bar(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_nav(PreferencesNav::StatusBar, cx);
        self.clear_search(cx);
    }

    pub(super) fn on_nav_key_down(
        &mut self,
        nav: PreferencesNav,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = nav.index();
        let target = match event.keystroke.key.as_str() {
            "up" | "left" => {
                Some((current + PreferencesNav::ORDER.len() - 1) % PreferencesNav::ORDER.len())
            }
            "down" | "right" => Some((current + 1) % PreferencesNav::ORDER.len()),
            "home" => Some(0),
            "end" => Some(PreferencesNav::ORDER.len() - 1),
            "enter" | "space" => Some(current),
            _ => None,
        };
        let Some(target) = target else {
            return;
        };
        let nav = PreferencesNav::ORDER[target];
        self.select_nav(nav, cx);
        self.clear_search(cx);
        self.nav_focus_handles[target].focus(window);
        cx.stop_propagation();
    }

    pub(super) fn close_all_dropdowns(&mut self) {
        self.startup_dropdown_open = false;
        self.auto_save_dropdown_open = false;
        self.document_loading_dropdown_open = false;
        self.theme_dropdown_open = false;
        self.language_dropdown_open = false;
        self.image_dropdown_open = false;
        self.font_dropdown_open = false;
    }

    pub(super) fn dropdown_is_open(&self, dropdown: PreferencesDropdown) -> bool {
        match dropdown {
            PreferencesDropdown::Startup => self.startup_dropdown_open,
            PreferencesDropdown::AutoSave => self.auto_save_dropdown_open,
            PreferencesDropdown::DocumentLoadingPreset => self.document_loading_dropdown_open,
            PreferencesDropdown::Theme => self.theme_dropdown_open,
            PreferencesDropdown::Language => self.language_dropdown_open,
            PreferencesDropdown::Image => self.image_dropdown_open,
            PreferencesDropdown::Font => self.font_dropdown_open,
        }
    }

    pub(super) fn set_dropdown_open(&mut self, dropdown: PreferencesDropdown, open: bool) {
        self.close_all_dropdowns();
        if open {
            match dropdown {
                PreferencesDropdown::Startup => self.startup_dropdown_open = true,
                PreferencesDropdown::AutoSave => self.auto_save_dropdown_open = true,
                PreferencesDropdown::DocumentLoadingPreset => {
                    self.document_loading_dropdown_open = true
                }
                PreferencesDropdown::Theme => self.theme_dropdown_open = true,
                PreferencesDropdown::Language => self.language_dropdown_open = true,
                PreferencesDropdown::Image => self.image_dropdown_open = true,
                PreferencesDropdown::Font => self.font_dropdown_open = true,
            }
            let selected = self.dropdown_current_index(dropdown);
            self.dropdown_selected_indices[dropdown.index()] = selected;
        }
    }

    pub(super) fn dropdown_option_count(&self, dropdown: PreferencesDropdown) -> usize {
        match dropdown {
            PreferencesDropdown::Startup | PreferencesDropdown::AutoSave => 2,
            PreferencesDropdown::DocumentLoadingPreset => 3,
            PreferencesDropdown::Theme => self.theme_options.len(),
            PreferencesDropdown::Language => self.language_options.len(),
            PreferencesDropdown::Image => 4,
            PreferencesDropdown::Font => self.font_options.len(),
        }
    }

    pub(super) fn dropdown_current_index(&self, dropdown: PreferencesDropdown) -> usize {
        match dropdown {
            PreferencesDropdown::Startup => match self.startup_open {
                StartupOpenPreference::NewFile => 0,
                StartupOpenPreference::LastOpenedFile => 1,
            },
            PreferencesDropdown::AutoSave => match self.auto_save {
                AutoSavePreference::Off => 0,
                AutoSavePreference::AfterDelay => 1,
            },
            PreferencesDropdown::DocumentLoadingPreset => match self.document_loading.preset {
                DocumentLoadingPreset::Balanced => 0,
                DocumentLoadingPreset::LowMemory => 1,
                DocumentLoadingPreset::HighPerformance => 2,
            },
            PreferencesDropdown::Theme => self
                .theme_options
                .iter()
                .position(|entry| entry.id == self.selected_theme_id)
                .unwrap_or(0),
            PreferencesDropdown::Language => self
                .language_options
                .iter()
                .position(|entry| entry.id == self.selected_language_id)
                .unwrap_or(0),
            PreferencesDropdown::Image => match self.image_paste_behavior {
                ImagePasteBehavior::None => 0,
                ImagePasteBehavior::CopyToDocumentFolder => 1,
                ImagePasteBehavior::CopyToAssetsFolder => 2,
                ImagePasteBehavior::CopyToNamedAssetsFolder => 3,
            },
            PreferencesDropdown::Font => self
                .font_options
                .iter()
                .position(|font| font == &self.editor_font_family)
                .unwrap_or(0),
        }
    }

    pub(super) fn commit_dropdown_selection(
        &mut self,
        dropdown: PreferencesDropdown,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        match dropdown {
            PreferencesDropdown::Startup => {
                self.startup_open = [
                    StartupOpenPreference::NewFile,
                    StartupOpenPreference::LastOpenedFile,
                ][index.min(1)];
                self.close_all_dropdowns();
                cx.notify();
            }
            PreferencesDropdown::AutoSave => {
                self.auto_save =
                    [AutoSavePreference::Off, AutoSavePreference::AfterDelay][index.min(1)];
                self.close_all_dropdowns();
                cx.notify();
            }
            PreferencesDropdown::DocumentLoadingPreset => {
                self.document_loading.preset = [
                    DocumentLoadingPreset::Balanced,
                    DocumentLoadingPreset::LowMemory,
                    DocumentLoadingPreset::HighPerformance,
                ][index.min(2)];
                // 选择预设表示回到预设基线；高级阈值之后可再逐项覆盖。
                self.document_loading.max_resident_mib = None;
                self.document_loading.max_resident_lines = None;
                self.document_loading.max_structural_units = None;
                self.close_all_dropdowns();
                cx.notify();
            }
            PreferencesDropdown::Theme => {
                if let Some(theme_id) = self.theme_options.get(index).map(|entry| entry.id.clone())
                {
                    self.preview_theme(theme_id, cx);
                }
            }
            PreferencesDropdown::Language => {
                if let Some(language_id) = self
                    .language_options
                    .get(index)
                    .map(|entry| entry.id.clone())
                {
                    self.selected_language_id = language_id;
                    self.close_all_dropdowns();
                    cx.notify();
                }
            }
            PreferencesDropdown::Image => {
                self.image_paste_behavior = [
                    ImagePasteBehavior::None,
                    ImagePasteBehavior::CopyToDocumentFolder,
                    ImagePasteBehavior::CopyToAssetsFolder,
                    ImagePasteBehavior::CopyToNamedAssetsFolder,
                ][index.min(3)];
                self.close_all_dropdowns();
                cx.notify();
            }
            PreferencesDropdown::Font => {
                if let Some(font) = self.font_options.get(index).cloned() {
                    self.preview_editor_font_family(font, cx);
                }
            }
        }
    }

    pub(super) fn on_dropdown_click(
        &mut self,
        dropdown: PreferencesDropdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dropdown_focus_handles[dropdown.index()].focus(window);
        let open = !self.dropdown_is_open(dropdown);
        self.set_dropdown_open(dropdown, open);
        cx.notify();
    }

    pub(super) fn on_dropdown_key_down(
        &mut self,
        dropdown: PreferencesDropdown,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let open = self.dropdown_is_open(dropdown);
        if key == "escape" && open {
            self.close_all_dropdowns();
            self.dropdown_focus_handles[dropdown.index()].focus(window);
            cx.notify();
            cx.stop_propagation();
            return;
        }

        if matches!(key, "enter" | "space") {
            if open {
                let selected = self.dropdown_selected_indices[dropdown.index()];
                self.commit_dropdown_selection(dropdown, selected, cx);
            } else {
                self.set_dropdown_open(dropdown, true);
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }

        if !matches!(key, "up" | "down" | "home" | "end") {
            return;
        }
        let count = self.dropdown_option_count(dropdown);
        if count == 0 {
            return;
        }
        if !open {
            self.set_dropdown_open(dropdown, true);
        } else {
            let current = self.dropdown_selected_indices[dropdown.index()].min(count - 1);
            self.dropdown_selected_indices[dropdown.index()] = match key {
                "up" => (current + count - 1) % count,
                "down" => (current + 1) % count,
                "home" => 0,
                "end" => count - 1,
                _ => current,
            };
        }
        cx.notify();
        cx.stop_propagation();
    }

    pub(super) fn toggle_preference_switch(
        &mut self,
        preference: PreferencesSwitch,
        cx: &mut Context<Self>,
    ) {
        match preference {
            PreferencesSwitch::SpellCheck => self.spell_check = !self.spell_check,
            PreferencesSwitch::AutoPairBrackets => {
                self.auto_pair_brackets = !self.auto_pair_brackets
            }
            PreferencesSwitch::AutoPairMarkdown => {
                self.auto_pair_markdown = !self.auto_pair_markdown
            }
            PreferencesSwitch::WorkspaceSidebarRight => {
                self.workspace_sidebar_position =
                    if self.workspace_sidebar_position == WorkspaceSidebarPosition::Right {
                        WorkspaceSidebarPosition::Left
                    } else {
                        WorkspaceSidebarPosition::Right
                    };
            }
            PreferencesSwitch::ShowTabBarActions => {
                self.show_tab_bar_actions = !self.show_tab_bar_actions
            }
            PreferencesSwitch::StatusBarEnabled => {
                self.status_bar_enabled = !self.status_bar_enabled
            }
            PreferencesSwitch::StatusBarWordCount => {
                self.status_bar_show_word_count = !self.status_bar_show_word_count
            }
            PreferencesSwitch::StatusBarCursorPosition => {
                self.status_bar_show_cursor_position = !self.status_bar_show_cursor_position
            }
            PreferencesSwitch::StatusBarSidebarToggle => {
                self.status_bar_show_sidebar_toggle = !self.status_bar_show_sidebar_toggle
            }
            PreferencesSwitch::StatusBarModeSwitch => {
                self.status_bar_show_mode_switch = !self.status_bar_show_mode_switch
            }
        }
        cx.notify();
    }

    pub(super) fn preference_switch(
        &self,
        preference: PreferencesSwitch,
        checked: bool,
        cx: &mut Context<Self>,
    ) -> Switch {
        let focus_handle = self.switch_focus_handles[preference.index()].clone();
        let pointer_focus_handle = focus_handle.clone();
        Switch::new(preference.id())
            .debug_selector(preference.id())
            .checked(checked)
            .focus_handle(focus_handle)
            .on_click(cx.listener(move |this, _, window, cx| {
                pointer_focus_handle.focus(window);
                this.toggle_preference_switch(preference, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.toggle_preference_switch(preference, cx);
                    cx.stop_propagation();
                }
            }))
    }

    pub(super) fn preview_theme(&mut self, theme_id: String, cx: &mut Context<Self>) {
        let appearance = cx.window_appearance();
        let changed = cx.update_global::<ThemeManager, _>(|theme_manager, _cx| {
            let changed = theme_manager.set_theme_preference(&theme_id, appearance);
            if changed {
                theme_manager
                    .set_editor_typography(self.editor_font_size, self.editor_line_height_percent);
                theme_manager.set_editor_content_width(self.editor_content_width);
            }
            changed
        });
        if changed {
            self.selected_theme_id = theme_id;
            self.theme_dropdown_open = false;
            cx.refresh_windows();
            cx.notify();
        }
    }

    pub(super) fn preview_editor_typography(&mut self, cx: &mut Context<Self>) {
        cx.update_global::<ThemeManager, _>(|theme_manager, _cx| {
            theme_manager
                .set_editor_typography(self.editor_font_size, self.editor_line_height_percent);
            theme_manager.set_editor_content_width(self.editor_content_width);
        });
        cx.refresh_windows();
        cx.notify();
    }

    pub(super) fn preview_editor_font_family(
        &mut self,
        font_family: String,
        cx: &mut Context<Self>,
    ) {
        self.editor_font_family = normalize_editor_font_family(&font_family);
        let font_family = self.editor_font_family.clone();
        cx.update_global::<EditorSettings, _>(|settings, _cx| {
            settings.editor_font_family = font_family;
        });
        self.close_all_dropdowns();
        cx.refresh_windows();
        cx.notify();
    }

    pub(super) fn activate_stepper(
        &mut self,
        control: PreferencesStepperControl,
        cx: &mut Context<Self>,
    ) {
        match control {
            PreferencesStepperControl::FontSizeDecrease => {
                self.editor_font_size = self
                    .editor_font_size
                    .saturating_sub(1)
                    .max(MIN_EDITOR_FONT_SIZE);
            }
            PreferencesStepperControl::FontSizeIncrease => {
                self.editor_font_size = self
                    .editor_font_size
                    .saturating_add(1)
                    .min(MAX_EDITOR_FONT_SIZE);
            }
            PreferencesStepperControl::LineHeightDecrease => {
                self.editor_line_height_percent = self
                    .editor_line_height_percent
                    .saturating_sub(EDITOR_LINE_HEIGHT_STEP)
                    .max(MIN_EDITOR_LINE_HEIGHT_PERCENT);
            }
            PreferencesStepperControl::LineHeightIncrease => {
                self.editor_line_height_percent = self
                    .editor_line_height_percent
                    .saturating_add(EDITOR_LINE_HEIGHT_STEP)
                    .min(MAX_EDITOR_LINE_HEIGHT_PERCENT);
            }
            PreferencesStepperControl::ContentWidthDecrease => {
                self.editor_content_width = self
                    .editor_content_width
                    .saturating_sub(EDITOR_CONTENT_WIDTH_STEP)
                    .max(MIN_EDITOR_CONTENT_WIDTH);
            }
            PreferencesStepperControl::ContentWidthIncrease => {
                self.editor_content_width = self
                    .editor_content_width
                    .saturating_add(EDITOR_CONTENT_WIDTH_STEP)
                    .min(MAX_EDITOR_CONTENT_WIDTH);
            }
            PreferencesStepperControl::ResidentMibDecrease => {
                let value = self.document_loading.effective_max_resident_mib();
                self.document_loading.max_resident_mib = Some(value.saturating_sub(1).max(1));
            }
            PreferencesStepperControl::ResidentMibIncrease => {
                let value = self.document_loading.effective_max_resident_mib();
                self.document_loading.max_resident_mib = Some(value.saturating_add(1).min(1_024));
            }
            PreferencesStepperControl::ResidentLinesDecrease => {
                let value = self.document_loading.effective_max_resident_lines();
                self.document_loading.max_resident_lines =
                    Some(value.saturating_sub(10_000).max(1_000));
            }
            PreferencesStepperControl::ResidentLinesIncrease => {
                let value = self.document_loading.effective_max_resident_lines();
                self.document_loading.max_resident_lines =
                    Some(value.saturating_add(10_000).min(10_000_000));
            }
            PreferencesStepperControl::StructuralUnitsDecrease => {
                let value = self.document_loading.effective_max_structural_units();
                self.document_loading.max_structural_units =
                    Some(value.saturating_sub(50_000).max(10_000));
            }
            PreferencesStepperControl::StructuralUnitsIncrease => {
                let value = self.document_loading.effective_max_structural_units();
                self.document_loading.max_structural_units =
                    Some(value.saturating_add(50_000).min(50_000_000));
            }
        }
        if matches!(
            control,
            PreferencesStepperControl::FontSizeDecrease
                | PreferencesStepperControl::FontSizeIncrease
                | PreferencesStepperControl::LineHeightDecrease
                | PreferencesStepperControl::LineHeightIncrease
                | PreferencesStepperControl::ContentWidthDecrease
                | PreferencesStepperControl::ContentWidthIncrease
        ) {
            self.preview_editor_typography(cx);
        } else {
            cx.notify();
        }
    }

    /// 主题预览属于可丢弃的窗口草稿；任何未保存关闭都必须恢复打开窗口时的基线。
    pub(super) fn restore_saved_theme(&mut self, cx: &mut Context<Self>) {
        if self.selected_theme_id == self.saved_theme_id
            && self.editor_font_size == self.saved_editor_font_size
            && self.editor_line_height_percent == self.saved_editor_line_height_percent
            && self.editor_content_width == self.saved_editor_content_width
            && self.editor_font_family == self.saved_editor_font_family
        {
            return;
        }
        let saved_theme_id = self.saved_theme_id.clone();
        let appearance = cx.window_appearance();
        let restored = cx.update_global::<ThemeManager, _>(|theme_manager, _cx| {
            let restored = theme_manager.set_theme_preference(&saved_theme_id, appearance);
            if restored {
                theme_manager.set_editor_typography(
                    self.saved_editor_font_size,
                    self.saved_editor_line_height_percent,
                );
                theme_manager.set_editor_content_width(self.saved_editor_content_width);
            }
            restored
        });
        if !restored {
            let _ = cx.update_global::<ThemeManager, _>(|theme_manager, _cx| {
                let changed = theme_manager.set_theme_by_id(DEFAULT_THEME_ID);
                theme_manager.set_editor_typography(
                    self.saved_editor_font_size,
                    self.saved_editor_line_height_percent,
                );
                theme_manager.set_editor_content_width(self.saved_editor_content_width);
                changed
            });
        }
        self.selected_theme_id = self.saved_theme_id.clone();
        self.editor_font_size = self.saved_editor_font_size;
        self.editor_line_height_percent = self.saved_editor_line_height_percent;
        self.editor_content_width = self.saved_editor_content_width;
        self.editor_font_family = self.saved_editor_font_family.clone();
        let restored_font = self.saved_editor_font_family.clone();
        cx.update_global::<EditorSettings, _>(|settings, _cx| {
            settings.editor_font_family = restored_font;
        });
        cx.refresh_windows();
        cx.notify();
    }

    pub(super) fn cancel(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.restore_saved_theme(cx);
        window.remove_window();
    }

    pub(super) fn on_titlebar_close(
        &mut self,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.standard_click() {
            self.restore_saved_theme(cx);
            window.remove_window();
        }
    }

    pub(super) fn save(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_unsaved_changes() {
            return;
        }

        let preferences = match save_preferences_from_window(
            self.startup_open,
            self.auto_save,
            self.spell_check,
            self.auto_pair_brackets,
            self.auto_pair_markdown,
            self.editor_font_size,
            self.editor_line_height_percent,
            self.editor_content_width,
            &self.editor_font_family,
            self.workspace_sidebar_position,
            self.show_tab_bar_actions,
            &self.selected_theme_id,
            &self.selected_language_id,
            self.image_paste_behavior,
            self.keybindings.clone(),
            &self.document_loading,
            &StatusBarPreferences {
                enabled: self.status_bar_enabled,
                show_word_count: self.status_bar_show_word_count,
                show_cursor_position: self.status_bar_show_cursor_position,
                show_sidebar_toggle: self.status_bar_show_sidebar_toggle,
                show_mode_switch: self.status_bar_show_mode_switch,
                custom_buttons: self.status_bar_custom_buttons.clone(),
            },
        ) {
            Ok(preferences) => preferences,
            Err(err) => {
                let strings = cx.global::<I18nManager>().strings().clone();
                let ok = strings.info_dialog_ok;
                let buttons = [ok.as_str()];
                let _ = window.prompt(
                    PromptLevel::Critical,
                    &strings.preferences_save_failed_title,
                    Some(&err.to_string()),
                    &buttons,
                    cx,
                );
                return;
            }
        };

        self.apply_saved_preferences(preferences, window, cx);
    }

    pub(super) fn apply_saved_preferences(
        &mut self,
        preferences: AppPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let appearance = window.appearance();
        cx.update_global::<ThemeManager, _>(|theme_manager, _cx| {
            if !theme_manager.set_theme_preference(&preferences.default_theme_id, appearance) {
                let _ = theme_manager.set_theme_by_id(DEFAULT_THEME_ID);
            }
            theme_manager.set_editor_typography(
                preferences.editor_font_size,
                preferences.editor_line_height_percent,
            );
            theme_manager.set_editor_content_width(preferences.editor_content_width);
        });
        cx.update_global::<I18nManager, _>(|i18n_manager, _cx| {
            let _ = i18n_manager.set_language_by_id(&preferences.default_language_id);
        });
        cx.clear_key_bindings();
        install_keybindings(cx, &preferences.keybindings);
        crate::app_menu::install_menus(cx);
        cx.update_global::<EditorSettings, _>(|settings, _cx| {
            settings.auto_save = preferences.auto_save;
            settings.spell_check = preferences.spell_check;
            settings.auto_pair_brackets = preferences.auto_pair_brackets;
            settings.auto_pair_markdown = preferences.auto_pair_markdown;
            settings.editor_font_family = preferences.editor_font_family.clone();
            settings.workspace_sidebar_position = preferences.workspace_sidebar_position;
            settings.show_tab_bar_actions = preferences.show_tab_bar_actions;
            settings.status_bar_settings.status_bar_enabled = preferences.status_bar.enabled;
            settings.status_bar_settings.status_bar_show_word_count =
                preferences.status_bar.show_word_count;
            settings.status_bar_settings.status_bar_show_cursor_position =
                preferences.status_bar.show_cursor_position;
            settings.status_bar_settings.status_bar_show_sidebar_toggle =
                preferences.status_bar.show_sidebar_toggle;
            settings.status_bar_settings.status_bar_show_mode_switch =
                preferences.status_bar.show_mode_switch;
            settings.status_bar_settings.custom_buttons = preferences.status_bar.custom_buttons;
        });
        cx.refresh_windows();
        window.activate_window();
        self.focus_handle.focus(window);
        self.saved_startup_open = self.startup_open;
        self.saved_auto_save = self.auto_save;
        self.saved_spell_check = self.spell_check;
        self.saved_auto_pair_brackets = self.auto_pair_brackets;
        self.saved_auto_pair_markdown = self.auto_pair_markdown;
        self.saved_editor_font_size = self.editor_font_size;
        self.saved_editor_line_height_percent = self.editor_line_height_percent;
        self.saved_editor_content_width = self.editor_content_width;
        self.saved_editor_font_family = self.editor_font_family.clone();
        self.saved_workspace_sidebar_position = self.workspace_sidebar_position;
        self.saved_show_tab_bar_actions = self.show_tab_bar_actions;
        self.saved_theme_id = self.selected_theme_id.clone();
        self.saved_language_id = self.selected_language_id.clone();
        self.saved_image_paste_behavior = self.image_paste_behavior;
        self.saved_keybindings = normalize_shortcut_config(&self.keybindings);
        self.saved_document_loading = self.document_loading.clone();
        self.saved_status_bar_enabled = self.status_bar_enabled;
        self.saved_status_bar_show_word_count = self.status_bar_show_word_count;
        self.saved_status_bar_show_cursor_position = self.status_bar_show_cursor_position;
        self.saved_status_bar_show_sidebar_toggle = self.status_bar_show_sidebar_toggle;
        self.saved_status_bar_show_mode_switch = self.status_bar_show_mode_switch;
        cx.notify();
    }

    pub(super) fn nav_button(
        &self,
        id: &'static str,
        label: String,
        icon: &'static str,
        nav: PreferencesNav,
        selected: bool,
        theme: &Theme,
        on_click: fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let t = &theme.typography;
        let focus_handle = self.nav_focus_handles[nav.index()].clone();
        let pointer_focus_handle = focus_handle.clone();
        div()
            .h(px(36.0))
            .w_full()
            .tab_index(0)
            .track_focus(&focus_handle)
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .rounded(px(8.0))
            .border(px(1.0))
            .border_color(hsla(0.0, 0.0, 0.0, 0.0))
            .cursor_pointer()
            .text_size(px(t.dialog_body_size))
            .font_weight(t.dialog_button_weight.to_font_weight())
            .text_color(if selected {
                c.text_default
            } else {
                c.dialog_muted
            })
            .bg(if selected {
                c.text_link.opacity(0.14)
            } else {
                hsla(0.0, 0.0, 0.0, 0.0)
            })
            .hover(move |this| {
                this.bg(if selected {
                    c.selection
                } else {
                    c.chrome_hover
                })
            })
            .focus(move |this| this.bg(c.chrome_hover).border_color(c.text_link))
            .id(id)
            .debug_selector(move || id.to_owned())
            .child(
                svg()
                    .path(icon)
                    .size(px(16.0))
                    .flex_shrink_0()
                    .text_color(if selected {
                        c.text_default
                    } else {
                        c.dialog_muted
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .on_click(cx.listener(move |this, event, window, cx| {
                pointer_focus_handle.focus(window);
                on_click(this, event, window, cx);
            }))
            .on_key_down(cx.listener(move |this, event, window, cx| {
                this.on_nav_key_down(nav, event, window, cx);
            }))
    }
}
