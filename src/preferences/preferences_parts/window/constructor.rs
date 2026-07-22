// @author kongweiguang

use super::*;

impl PreferencesWindow {
    pub(super) fn new(
        preferences: AppPreferences,
        theme_options: Vec<ThemeCatalogEntry>,
        cx: &mut Context<Self>,
    ) -> Self {
        let selected_theme_id = if preferences.default_theme_id == SYSTEM_THEME_ID
            || theme_options
                .iter()
                .any(|entry| entry.id == preferences.default_theme_id)
        {
            preferences.default_theme_id
        } else {
            DEFAULT_THEME_ID.into()
        };
        let language_options = cx.global::<I18nManager>().available_languages().to_vec();
        let selected_language_id = if language_options
            .iter()
            .any(|entry| entry.id == preferences.default_language_id)
        {
            preferences.default_language_id.clone()
        } else {
            DEFAULT_LANGUAGE_ID.into()
        };
        let startup_open = preferences.startup_open;
        let auto_save = preferences.auto_save;
        let spell_check = preferences.spell_check;
        let auto_pair_brackets = preferences.auto_pair_brackets;
        let auto_pair_markdown = preferences.auto_pair_markdown;
        let editor_font_size = preferences.editor_font_size;
        let editor_line_height_percent = preferences.editor_line_height_percent;
        let editor_content_width = preferences.editor_content_width;
        let editor_font_family = preferences.editor_font_family;
        let workspace_sidebar_position = preferences.workspace_sidebar_position;
        let show_tab_bar_actions = preferences.show_tab_bar_actions;
        let image_paste_behavior = preferences.image_paste_behavior;
        let keybindings = preferences.keybindings;
        let document_loading = preferences.document_loading.clone();
        // 字体枚举成本较高，偏好设置窗口创建时只读取一次，渲染阶段复用稳定列表。
        let mut font_options = cx.text_system().all_font_names();
        font_options.retain(|font| !font.trim().is_empty());
        font_options.sort_by_key(|font| font.to_lowercase());
        font_options.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        if !editor_font_family.is_empty()
            && !font_options
                .iter()
                .any(|font| font.eq_ignore_ascii_case(&editor_font_family))
        {
            // 保留旧配置中的自定义字体，即使当前系统暂时没有枚举到它。
            font_options.insert(0, editor_font_family.clone());
        }
        // 空字符串是持久化契约中的“跟随系统字体”。
        font_options.insert(0, String::new());
        let search_placeholder = cx
            .global::<I18nManager>()
            .strings()
            .preferences_search_placeholder
            .clone();
        let search_input = cx.new(|cx| {
            let mut block = Block::with_record(cx, BlockRecord::paragraph(String::new()));
            block.set_compact_source_host();
            block.set_input_placeholder(search_placeholder);
            block
        });
        cx.subscribe(&search_input, Self::on_search_input_event)
            .detach();
        Self {
            nav: PreferencesNav::File,
            startup_open,
            auto_save,
            spell_check,
            auto_pair_brackets,
            auto_pair_markdown,
            editor_font_size,
            editor_line_height_percent,
            editor_content_width,
            editor_font_family: editor_font_family.clone(),
            workspace_sidebar_position,
            show_tab_bar_actions,
            selected_theme_id: selected_theme_id.clone(),
            selected_language_id: selected_language_id.clone(),
            image_paste_behavior,
            keybindings: keybindings.clone(),
            document_loading: document_loading.clone(),
            saved_startup_open: startup_open,
            saved_auto_save: auto_save,
            saved_spell_check: spell_check,
            saved_auto_pair_brackets: auto_pair_brackets,
            saved_auto_pair_markdown: auto_pair_markdown,
            saved_editor_font_size: editor_font_size,
            saved_editor_line_height_percent: editor_line_height_percent,
            saved_editor_content_width: editor_content_width,
            saved_editor_font_family: editor_font_family,
            saved_workspace_sidebar_position: workspace_sidebar_position,
            saved_show_tab_bar_actions: show_tab_bar_actions,
            saved_theme_id: selected_theme_id,
            saved_language_id: selected_language_id,
            saved_image_paste_behavior: image_paste_behavior,
            saved_keybindings: keybindings,
            saved_document_loading: document_loading,
            theme_options,
            language_options,
            font_options,
            focus_handle: cx.focus_handle(),
            nav_focus_handles: std::array::from_fn(|_| cx.focus_handle()),
            dropdown_focus_handles: std::array::from_fn(|_| cx.focus_handle()),
            dropdown_selected_indices: [0; PreferencesDropdown::COUNT],
            switch_focus_handles: std::array::from_fn(|_| cx.focus_handle()),
            stepper_focus_handles: std::array::from_fn(|_| cx.focus_handle()),
            search_input,
            search_selected: 0,
            startup_dropdown_open: false,
            auto_save_dropdown_open: false,
            document_loading_dropdown_open: false,
            theme_dropdown_open: false,
            language_dropdown_open: false,
            image_dropdown_open: false,
            font_dropdown_open: false,
            recording_shortcut: None,
            shortcut_error: None,
            status_bar_enabled: preferences.status_bar.enabled,
            status_bar_show_word_count: preferences.status_bar.show_word_count,
            status_bar_show_cursor_position: preferences.status_bar.show_cursor_position,
            status_bar_show_sidebar_toggle: preferences.status_bar.show_sidebar_toggle,
            status_bar_show_mode_switch: preferences.status_bar.show_mode_switch,
            status_bar_custom_buttons: preferences.status_bar.custom_buttons.clone(),
            saved_status_bar_enabled: preferences.status_bar.enabled,
            saved_status_bar_show_word_count: preferences.status_bar.show_word_count,
            saved_status_bar_show_cursor_position: preferences.status_bar.show_cursor_position,
            saved_status_bar_show_sidebar_toggle: preferences.status_bar.show_sidebar_toggle,
            saved_status_bar_show_mode_switch: preferences.status_bar.show_mode_switch,
        }
    }

    pub(super) fn selected_theme_name(&self) -> String {
        self.theme_options
            .iter()
            .find(|entry| entry.id == self.selected_theme_id)
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| "gmark".into())
    }

    pub(super) fn on_search_input_event(
        &mut self,
        _: Entity<Block>,
        event: &BlockEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, BlockEvent::Changed) {
            self.search_selected = 0;
            cx.notify();
        }
    }

    pub(super) fn search_query(&self, cx: &App) -> String {
        self.search_input
            .read(cx)
            .display_text()
            .trim()
            .to_lowercase()
    }

    pub(super) fn preference_search_items(
        &self,
        strings: &crate::i18n::I18nStrings,
    ) -> Vec<PreferenceSearchItem> {
        let mut items = vec![
            PreferenceSearchItem {
                nav: PreferencesNav::File,
                category: strings.preferences_nav_file.clone(),
                label: strings.preferences_startup_option.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::File,
                category: strings.preferences_nav_file.clone(),
                label: strings.preferences_auto_save_option.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::File,
                category: strings.preferences_nav_file.clone(),
                label: strings.preferences_spell_check.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::File,
                category: strings.preferences_nav_file.clone(),
                label: strings.preferences_document_loading.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::File,
                category: strings.preferences_nav_file.clone(),
                label: strings.preferences_document_loading_preset.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::File,
                category: strings.preferences_nav_file.clone(),
                label: strings.preferences_document_max_resident_mib.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::File,
                category: strings.preferences_nav_file.clone(),
                label: strings.preferences_document_max_resident_lines.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::File,
                category: strings.preferences_nav_file.clone(),
                label: strings.preferences_document_max_structural_units.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::Editor,
                category: strings.preferences_nav_editor.clone(),
                label: strings.preferences_editor_font_size.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::Editor,
                category: strings.preferences_nav_editor.clone(),
                label: strings.preferences_auto_pair_brackets.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::Editor,
                category: strings.preferences_nav_editor.clone(),
                label: strings.preferences_auto_pair_markdown.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::Editor,
                category: strings.preferences_nav_editor.clone(),
                label: strings.preferences_editor_line_height.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::Editor,
                category: strings.preferences_nav_editor.clone(),
                label: strings.preferences_editor_content_width.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::Editor,
                category: strings.preferences_nav_editor.clone(),
                label: strings.preferences_editor_font_family.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::Editor,
                category: strings.preferences_nav_editor.clone(),
                label: strings.preferences_workspace_sidebar_right.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::Editor,
                category: strings.preferences_nav_editor.clone(),
                label: strings.preferences_show_tab_bar_actions.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::Theme,
                category: strings.preferences_nav_theme.clone(),
                label: strings.preferences_local_theme.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::Theme,
                category: strings.preferences_nav_theme.clone(),
                label: strings.menu_language.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::Image,
                category: strings.preferences_nav_image.clone(),
                label: strings.preferences_image_insert_behavior.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::StatusBar,
                category: strings.preferences_nav_status_bar.clone(),
                label: strings.preferences_status_bar_enabled.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::StatusBar,
                category: strings.preferences_nav_status_bar.clone(),
                label: strings.preferences_status_bar_show_word_count.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::StatusBar,
                category: strings.preferences_nav_status_bar.clone(),
                label: strings.preferences_status_bar_show_cursor_position.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::StatusBar,
                category: strings.preferences_nav_status_bar.clone(),
                label: strings.preferences_status_bar_show_sidebar_toggle.clone(),
            },
            PreferenceSearchItem {
                nav: PreferencesNav::StatusBar,
                category: strings.preferences_nav_status_bar.clone(),
                label: strings.preferences_status_bar_show_mode_switch.clone(),
            },
        ];
        items.extend(
            shortcut_definitions()
                .iter()
                .map(|definition| PreferenceSearchItem {
                    nav: PreferencesNav::Shortcuts,
                    category: strings.preferences_nav_shortcuts.clone(),
                    label: Self::shortcut_command_label(definition.command, strings),
                }),
        );
        items
    }
}
