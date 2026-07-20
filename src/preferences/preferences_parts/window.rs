// @author kongweiguang

use super::*;

impl PreferencesNav {
    const ORDER: [Self; 6] = [
        Self::File,
        Self::Editor,
        Self::Theme,
        Self::Image,
        Self::Shortcuts,
        Self::StatusBar,
    ];

    fn index(self) -> usize {
        Self::ORDER
            .iter()
            .position(|candidate| *candidate == self)
            .expect("preferences navigation is part of the fixed order")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PreferencesDropdown {
    Startup,
    AutoSave,
    Theme,
    Language,
    Image,
    Font,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PreferencesSwitch {
    SpellCheck,
    AutoPairBrackets,
    AutoPairMarkdown,
    WorkspaceSidebarRight,
    ShowTabBarActions,
    StatusBarEnabled,
    StatusBarWordCount,
    StatusBarCursorPosition,
    StatusBarSidebarToggle,
    StatusBarModeSwitch,
}

impl PreferencesSwitch {
    const COUNT: usize = 10;

    fn index(self) -> usize {
        match self {
            Self::SpellCheck => 0,
            Self::AutoPairBrackets => 1,
            Self::AutoPairMarkdown => 2,
            Self::WorkspaceSidebarRight => 3,
            Self::ShowTabBarActions => 4,
            Self::StatusBarEnabled => 5,
            Self::StatusBarWordCount => 6,
            Self::StatusBarCursorPosition => 7,
            Self::StatusBarSidebarToggle => 8,
            Self::StatusBarModeSwitch => 9,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::SpellCheck => "preferences-spell-check",
            Self::AutoPairBrackets => "preferences-auto-pair-brackets",
            Self::AutoPairMarkdown => "preferences-auto-pair-markdown",
            Self::WorkspaceSidebarRight => "preferences-workspace-sidebar-right",
            Self::ShowTabBarActions => "preferences-show-tab-bar-actions",
            Self::StatusBarEnabled => "preferences-status-bar-enabled",
            Self::StatusBarWordCount => "preferences-status-bar-word-count",
            Self::StatusBarCursorPosition => "preferences-status-bar-cursor-position",
            Self::StatusBarSidebarToggle => "preferences-status-bar-sidebar-toggle",
            Self::StatusBarModeSwitch => "preferences-status-bar-mode-switch",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreferencesStepperControl {
    FontSizeDecrease,
    FontSizeIncrease,
    LineHeightDecrease,
    LineHeightIncrease,
    ContentWidthDecrease,
    ContentWidthIncrease,
}

impl PreferencesStepperControl {
    const COUNT: usize = 6;

    fn index(self) -> usize {
        match self {
            Self::FontSizeDecrease => 0,
            Self::FontSizeIncrease => 1,
            Self::LineHeightDecrease => 2,
            Self::LineHeightIncrease => 3,
            Self::ContentWidthDecrease => 4,
            Self::ContentWidthIncrease => 5,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::FontSizeDecrease => "preferences-editor-font-size-decrease",
            Self::FontSizeIncrease => "preferences-editor-font-size-increase",
            Self::LineHeightDecrease => "preferences-editor-line-height-decrease",
            Self::LineHeightIncrease => "preferences-editor-line-height-increase",
            Self::ContentWidthDecrease => "preferences-editor-content-width-decrease",
            Self::ContentWidthIncrease => "preferences-editor-content-width-increase",
        }
    }
}

impl PreferencesDropdown {
    const COUNT: usize = 6;

    fn index(self) -> usize {
        match self {
            Self::Startup => 0,
            Self::AutoSave => 1,
            Self::Theme => 2,
            Self::Language => 3,
            Self::Image => 4,
            Self::Font => 5,
        }
    }
}

#[derive(Clone)]
struct PreferenceSearchItem {
    nav: PreferencesNav,
    category: String,
    label: String,
}

/// Independent preferences window view.
pub(crate) struct PreferencesWindow {
    nav: PreferencesNav,
    startup_open: StartupOpenPreference,
    auto_save: AutoSavePreference,
    spell_check: bool,
    auto_pair_brackets: bool,
    auto_pair_markdown: bool,
    editor_font_size: u8,
    editor_line_height_percent: u16,
    editor_content_width: u16,
    editor_font_family: String,
    workspace_sidebar_position: WorkspaceSidebarPosition,
    show_tab_bar_actions: bool,
    selected_theme_id: String,
    selected_language_id: String,
    image_paste_behavior: ImagePasteBehavior,
    keybindings: BTreeMap<String, Vec<String>>,
    saved_startup_open: StartupOpenPreference,
    saved_auto_save: AutoSavePreference,
    saved_spell_check: bool,
    saved_auto_pair_brackets: bool,
    saved_auto_pair_markdown: bool,
    saved_editor_font_size: u8,
    saved_editor_line_height_percent: u16,
    saved_editor_content_width: u16,
    saved_editor_font_family: String,
    saved_workspace_sidebar_position: WorkspaceSidebarPosition,
    saved_show_tab_bar_actions: bool,
    saved_theme_id: String,
    saved_language_id: String,
    saved_image_paste_behavior: ImagePasteBehavior,
    saved_keybindings: BTreeMap<String, Vec<String>>,
    theme_options: Vec<ThemeCatalogEntry>,
    language_options: Vec<LanguageCatalogEntry>,
    font_options: Vec<String>,
    focus_handle: FocusHandle,
    nav_focus_handles: [FocusHandle; 6],
    dropdown_focus_handles: [FocusHandle; PreferencesDropdown::COUNT],
    dropdown_selected_indices: [usize; PreferencesDropdown::COUNT],
    switch_focus_handles: [FocusHandle; PreferencesSwitch::COUNT],
    stepper_focus_handles: [FocusHandle; PreferencesStepperControl::COUNT],
    search_input: Entity<Block>,
    search_selected: usize,
    startup_dropdown_open: bool,
    auto_save_dropdown_open: bool,
    theme_dropdown_open: bool,
    language_dropdown_open: bool,
    image_dropdown_open: bool,
    font_dropdown_open: bool,
    recording_shortcut: Option<ShortcutCommand>,
    shortcut_error: Option<String>,
    status_bar_enabled: bool,
    status_bar_show_word_count: bool,
    status_bar_show_cursor_position: bool,
    status_bar_show_sidebar_toggle: bool,
    status_bar_show_mode_switch: bool,
    status_bar_custom_buttons: Vec<StatusBarButton>,
    saved_status_bar_enabled: bool,
    saved_status_bar_show_word_count: bool,
    saved_status_bar_show_cursor_position: bool,
    saved_status_bar_show_sidebar_toggle: bool,
    saved_status_bar_show_mode_switch: bool,
}

impl PreferencesWindow {}

#[path = "window/constructor.rs"]
mod constructor;
#[path = "window/controls.rs"]
mod controls;
#[path = "window/labels.rs"]
mod labels;
#[path = "window/search.rs"]
mod search;

impl Render for PreferencesWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<ThemeManager>().current().clone();
        let strings = cx.global::<I18nManager>().strings().clone();
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let can_save = self.has_unsaved_changes();
        let search_query = self.search_query(cx);
        let clear_search_tooltip: SharedString = strings.ui_clear_search.clone().into();
        let search_results = self.preference_search_results(&strings, cx);
        let window_title =
            SharedString::from(format!("gmark - {}", strings.preferences_window_title));
        window.set_window_title(window_title.as_ref());
        let titlebar_height = custom_titlebar_height(window, d);

        let content = div()
            .id("preferences-content")
            .debug_selector(|| "preferences-content".to_owned())
            .size_full()
            .pt(px(titlebar_height))
            .flex()
            .key_context("Preferences")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::capture_shortcut_key))
            .bg(c.editor_background)
            .text_color(c.dialog_body)
            .child(
                div()
                    .id("preferences-navigation")
                    .debug_selector(|| "preferences-navigation".to_owned())
                    .w(px(PREFERENCES_NAV_WIDTH))
                    .h_full()
                    .px(px(12.0))
                    .pt(px(16.0))
                    .flex_shrink_0()
                    .flex()
                    .items_start()
                    .bg(c.sidebar_background)
                    .border_r(px(d.dialog_border_width))
                    .border_color(c.dialog_border)
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .id("preferences-search-input")
                                    .debug_selector(|| "preferences-search-input".to_owned())
                                    .w_full()
                                    .h(px(34.0))
                                    .px(px(8.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(7.0))
                                    .rounded(px(6.0))
                                    .border(px(d.dialog_border_width))
                                    .border_color(c.dialog_border)
                                    .bg(c.dialog_surface)
                                    .child(
                                        div()
                                            .id("preferences-search-icon")
                                            .debug_selector(|| "preferences-search-icon".to_owned())
                                            .size(px(16.0))
                                            .flex_shrink_0()
                                            .text_color(c.dialog_muted)
                                            .child(
                                                svg()
                                                    .path(SEARCH_ICON)
                                                    .size(px(16.0))
                                                    .text_color(c.dialog_muted),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .child(self.search_input.clone()),
                                    )
                                    .child(
                                        div()
                                            .w(px(24.0))
                                            .h(px(24.0))
                                            .flex_shrink_0()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .children((!search_query.is_empty()).then(|| {
                                                div()
                                                    .id("preferences-search-clear")
                                                    .debug_selector(|| {
                                                        "preferences-search-clear".to_owned()
                                                    })
                                                    .size(px(24.0))
                                                    .tab_index(0)
                                                    .border(px(1.0))
                                                    .border_color(hsla(0.0, 0.0, 0.0, 0.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded(px(4.0))
                                                    .text_color(c.dialog_muted)
                                                    .cursor_pointer()
                                                    .hover(|this| this.bg(c.chrome_hover))
                                                    .focus(|style| {
                                                        style
                                                            .bg(c.chrome_hover)
                                                            .border_color(c.text_link)
                                                    })
                                                    .tooltip(move |_window, cx| {
                                                        crate::ui::ui_tooltip(
                                                            clear_search_tooltip.clone(),
                                                            cx,
                                                        )
                                                    })
                                                    .child(
                                                        svg()
                                                            .path(CLOSE_ICON)
                                                            .size(px(14.0))
                                                            .text_color(c.dialog_muted),
                                                    )
                                                    .on_click(
                                                        cx.listener(Self::clear_search_from_button),
                                                    )
                                                    .on_key_down(
                                                        cx.listener(Self::clear_search_from_key),
                                                    )
                                            })),
                                    ),
                            )
                            .child(self.nav_button(
                                "preferences-nav-file",
                                strings.preferences_nav_file.clone(),
                                "icon/ui/files.svg",
                                PreferencesNav::File,
                                self.nav == PreferencesNav::File,
                                &theme,
                                Self::set_nav_file,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-editor",
                                strings.preferences_nav_editor.clone(),
                                "icon/ui/type.svg",
                                PreferencesNav::Editor,
                                self.nav == PreferencesNav::Editor,
                                &theme,
                                Self::set_nav_editor,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-theme",
                                strings.preferences_nav_theme.clone(),
                                "icon/ui/palette.svg",
                                PreferencesNav::Theme,
                                self.nav == PreferencesNav::Theme,
                                &theme,
                                Self::set_nav_theme,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-image",
                                strings.preferences_nav_image.clone(),
                                "icon/ui/image.svg",
                                PreferencesNav::Image,
                                self.nav == PreferencesNav::Image,
                                &theme,
                                Self::set_nav_image,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-shortcuts",
                                strings.preferences_nav_shortcuts.clone(),
                                "icon/ui/keyboard.svg",
                                PreferencesNav::Shortcuts,
                                self.nav == PreferencesNav::Shortcuts,
                                &theme,
                                Self::set_nav_shortcuts,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-status-bar",
                                strings.preferences_nav_status_bar.clone(),
                                "icon/ui/panel-bottom.svg",
                                PreferencesNav::StatusBar,
                                self.nav == PreferencesNav::StatusBar,
                                &theme,
                                Self::set_nav_status_bar,
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .id("preferences-main")
                    .debug_selector(|| "preferences-main".to_owned())
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .p(px(d.dialog_padding))
                    .flex()
                    .flex_col()
                    .gap(px(d.dialog_gap))
                    .child(
                        div()
                            .id("preferences-page-area")
                            .debug_selector(|| "preferences-page-area".to_owned())
                            .w_full()
                            .flex_1()
                            .min_h(px(0.0))
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(d.dialog_gap))
                            .child(
                                div()
                                    .id("preferences-page-title")
                                    .debug_selector(|| "preferences-page-title".to_owned())
                                    .w_full()
                                    .max_w(px(PREFERENCES_FORM_WIDTH))
                                    .flex_shrink_0()
                                    .text_size(px(t.dialog_title_size))
                                    .font_weight(t.dialog_title_weight.to_font_weight())
                                    .text_color(c.dialog_title)
                                    .child(if search_query.is_empty() {
                                        match self.nav {
                                            PreferencesNav::File => {
                                                strings.preferences_nav_file.clone()
                                            }
                                            PreferencesNav::Editor => {
                                                strings.preferences_nav_editor.clone()
                                            }
                                            PreferencesNav::Theme => {
                                                strings.preferences_nav_theme.clone()
                                            }
                                            PreferencesNav::Image => {
                                                strings.preferences_nav_image.clone()
                                            }
                                            PreferencesNav::Shortcuts => {
                                                strings.preferences_nav_shortcuts.clone()
                                            }
                                            PreferencesNav::StatusBar => {
                                                strings.preferences_nav_status_bar.clone()
                                            }
                                        }
                                    } else {
                                        strings.preferences_search_results.clone()
                                    }),
                            )
                            .child(
                                div()
                                    .id("preferences-page-scroll")
                                    .debug_selector(|| "preferences-page-scroll".to_owned())
                                    .w_full()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .overflow_y_scroll()
                                    .flex()
                                    .flex_col()
                                    .items_start()
                                    .child(if !search_query.is_empty() {
                                        div()
                                            .w_full()
                                            .pt(px(8.0))
                                            .child(self.render_search_results(
                                                &search_results,
                                                &theme,
                                                &strings,
                                                cx,
                                            ))
                                            .into_any_element()
                                    } else {
                                        match self.nav {
                                            PreferencesNav::File => div()
                                                .w_full()
                                                .flex_1()
                                                .min_h(px(0.0))
                                                .flex()
                                                .items_start()
                                                .pt(px(12.0))
                                                .child(
                                                    self.render_startup_page(&theme, &strings, cx),
                                                )
                                                .into_any_element(),
                                            PreferencesNav::Editor => div()
                                                .w_full()
                                                .flex_1()
                                                .min_h(px(0.0))
                                                .flex()
                                                .items_start()
                                                .pt(px(12.0))
                                                .child(
                                                    self.render_editor_page(&theme, &strings, cx),
                                                )
                                                .into_any_element(),
                                            PreferencesNav::Theme => div()
                                                .w_full()
                                                .flex_1()
                                                .min_h(px(0.0))
                                                .flex()
                                                .items_start()
                                                .pt(px(12.0))
                                                .child(self.render_theme_page(&theme, &strings, cx))
                                                .into_any_element(),
                                            PreferencesNav::Image => div()
                                                .w_full()
                                                .flex_1()
                                                .min_h(px(0.0))
                                                .flex()
                                                .items_start()
                                                .pt(px(12.0))
                                                .child(self.render_image_page(&theme, &strings, cx))
                                                .into_any_element(),
                                            PreferencesNav::Shortcuts => {
                                                div()
                                                    .w_full()
                                                    .flex_1()
                                                    .min_h(px(0.0))
                                                    .child(self.render_shortcuts_page(
                                                        &theme, &strings, cx,
                                                    ))
                                                    .into_any_element()
                                            }
                                            PreferencesNav::StatusBar => {
                                                div()
                                                    .w_full()
                                                    .flex_1()
                                                    .min_h(px(0.0))
                                                    .flex()
                                                    .items_start()
                                                    .pt(px(12.0))
                                                    .child(self.render_status_bar_page(
                                                        &theme, &strings, cx,
                                                    ))
                                                    .into_any_element()
                                            }
                                        }
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id("preferences-actions")
                            .debug_selector(|| "preferences-actions".to_owned())
                            .w_full()
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .gap(px(d.dialog_button_gap))
                            .pt(px(12.0))
                            .border_t(px(d.dialog_border_width))
                            .border_color(c.dialog_border)
                            .child(
                                div()
                                    .id("preferences-cancel")
                                    .debug_selector(|| "preferences-cancel".to_owned())
                                    .h(px(d.dialog_button_height))
                                    .px(px(d.dialog_button_padding_x))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                    .border(px(d.dialog_border_width))
                                    .border_color(c.dialog_border)
                                    .bg(c.dialog_secondary_button_bg)
                                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                                    .cursor_pointer()
                                    .text_size(px(t.dialog_button_size))
                                    .font_weight(t.dialog_button_weight.to_font_weight())
                                    .text_color(c.dialog_secondary_button_text)
                                    .child(strings.preferences_cancel.clone())
                                    .on_click(cx.listener(Self::cancel)),
                            )
                            .child(
                                div()
                                    .id("preferences-save")
                                    .debug_selector(|| "preferences-save".to_owned())
                                    .h(px(d.dialog_button_height))
                                    .px(px(d.dialog_button_padding_x))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                    .border(px(if can_save { 0.0 } else { d.dialog_border_width }))
                                    .border_color(c.dialog_border)
                                    .bg(if can_save {
                                        c.dialog_primary_button_bg
                                    } else {
                                        c.dialog_secondary_button_bg
                                    })
                                    .hover(move |this| {
                                        if can_save {
                                            this.bg(c.dialog_primary_button_hover)
                                        } else {
                                            this.bg(c.dialog_secondary_button_bg)
                                        }
                                    })
                                    .when(can_save, |this| this.cursor_pointer())
                                    .text_size(px(t.dialog_button_size))
                                    .font_weight(t.dialog_button_weight.to_font_weight())
                                    .text_color(if can_save {
                                        c.dialog_primary_button_text
                                    } else {
                                        c.dialog_secondary_button_text
                                    })
                                    .child(strings.preferences_save.clone())
                                    .on_click(cx.listener(Self::save)),
                            ),
                    ),
            );

        let root = div()
            .size_full()
            .relative()
            .bg(c.editor_background)
            .child(content);

        if let Some(titlebar) = render_custom_titlebar(
            "preferences-titlebar",
            Some(window_title),
            Some("icon/gmark-icon.svg"),
            &theme,
            window,
            cx,
            Self::on_titlebar_close,
        ) {
            root.child(titlebar)
        } else {
            root
        }
    }
}

pub(super) fn open_preferences_window_with_state(
    cx: &mut App,
    preferences: AppPreferences,
    theme_options: Vec<ThemeCatalogEntry>,
    title: String,
) -> WindowHandle<PreferencesWindow> {
    let bounds = Bounds::centered(None, size(px(860.0), px(620.0)), cx);
    let window_title = SharedString::from(format!("gmark - {title}"));
    let handle = cx
        .open_window(
            gmark_window_options(window_title, bounds),
            move |_window, cx| {
                cx.new(move |cx| PreferencesWindow::new(preferences, theme_options, cx))
            },
        )
        .expect("preferences window should open");

    handle
        .update(cx, |view, window, cx| {
            let preferences = cx.entity().downgrade();
            window.on_window_should_close(cx, move |_window, cx| {
                let _ = preferences.update(cx, |view, cx| view.restore_saved_theme(cx));
                true
            });
            window.activate_window();
            view.focus_handle.focus(window);
        })
        .expect("newly opened preferences window should be updateable");

    handle
}

pub(crate) fn open_preferences_window(cx: &mut App) -> WindowHandle<PreferencesWindow> {
    let preferences = match read_app_preferences() {
        Ok(preferences) => preferences,
        Err(err) => {
            eprintln!("failed to read app preferences: {err}");
            AppPreferences::default()
        }
    };
    let mut theme_options = cx.global::<ThemeManager>().available_themes().to_vec();
    theme_options.insert(
        0,
        ThemeCatalogEntry {
            id: SYSTEM_THEME_ID.into(),
            name: cx
                .global::<I18nManager>()
                .strings()
                .preferences_follow_system_theme
                .clone(),
        },
    );
    let title = cx
        .global::<I18nManager>()
        .strings()
        .preferences_window_title
        .clone();
    open_preferences_window_with_state(cx, preferences, theme_options, title)
}

#[cfg(test)]
#[path = "../../../tests/unit/config/preferences.rs"]
mod tests;
