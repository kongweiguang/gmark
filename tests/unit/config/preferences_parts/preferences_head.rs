// @author kongweiguang

use super::{
    AppPreferences, AutoSavePreference, EditorSettings, ImagePasteBehavior, PreferencesDropdown,
    PreferencesNav, PreferencesSwitch, StartupOpenPreference, StatusBarButton,
    StatusBarPreferences, WorkspaceSidebarPosition,
    load_or_create_app_preferences_with_dirs_and_locales, open_preferences_window_with_state,
    read_app_preferences_with_dirs, save_app_preferences_with_dirs,
    save_preferences_from_window_with_dirs, theme_option_icon,
};
use crate::config::GmarkConfigDirs;
use crate::i18n::I18nManager;
use crate::theme::{ThemeCatalogEntry, ThemeManager};
use gpui::{KeyDownEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext, px, size};
use std::collections::BTreeMap;

fn init_preferences_test_app(cx: &mut TestAppContext) {
    cx.update(|cx| {
        I18nManager::init_with_language_id(cx, "en-US");
        ThemeManager::init_with_theme_id(cx, "gmark");
        crate::components::init(cx);
        EditorSettings::init(cx, true, AutoSavePreference::Off, true);
    });
}

fn default_theme_options() -> Vec<ThemeCatalogEntry> {
    vec![
        ThemeCatalogEntry {
            id: "system".into(),
            name: "Follow System".into(),
        },
        ThemeCatalogEntry {
            id: "gmark".into(),
            name: "gmark".into(),
        },
        ThemeCatalogEntry {
            id: "gmark-light".into(),
            name: "gmark Light".into(),
        },
    ]
}

#[test]
fn theme_options_use_semantic_local_icons() {
    assert_eq!(theme_option_icon("system"), "icon/ui/monitor.svg");
    assert_eq!(theme_option_icon("gmark"), "icon/ui/moon.svg");
    assert_eq!(theme_option_icon("gmark-light"), "icon/ui/sun.svg");
    assert_eq!(theme_option_icon("custom:paper"), "icon/ui/palette.svg");
}

#[test]
fn missing_preferences_file_returns_defaults() {
    let root = std::env::temp_dir().join(format!(
        "gmark-preferences-missing-{}",
        uuid::Uuid::new_v4()
    ));
    let dirs = GmarkConfigDirs::from_root(&root);
    let preferences =
        read_app_preferences_with_dirs(&dirs).expect("missing preferences should load");
    assert_eq!(preferences, AppPreferences::default());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn partial_or_invalid_preferences_fall_back_by_field() {
    let root = std::env::temp_dir().join(format!(
        "gmark-preferences-partial-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("temp root should exist");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::write(
        dirs.app_config_file(),
        r#"
                [startup]
                open = "not-valid"

                [theme]
                default_theme_id = "gmark-light"
            "#,
    )
    .expect("preferences should be written");

    let preferences =
        read_app_preferences_with_dirs(&dirs).expect("partial preferences should load");
    assert_eq!(preferences.startup_open, StartupOpenPreference::NewFile);
    assert_eq!(preferences.default_language_id, "en-US");
    assert_eq!(preferences.default_theme_id, "gmark-light");
    assert_eq!(preferences.image_paste_behavior, ImagePasteBehavior::None);
    assert!(preferences.auto_pair_brackets);
    assert!(preferences.auto_pair_markdown);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn invalid_image_paste_behavior_falls_back_to_none() {
    let root = std::env::temp_dir().join(format!(
        "gmark-preferences-image-invalid-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("temp root should exist");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::write(
        dirs.app_config_file(),
        r#"
                [editor]
                image_paste_behavior = "somewhere-dangerous"
            "#,
    )
    .expect("preferences should be written");

    let preferences = read_app_preferences_with_dirs(&dirs).expect("preferences should load");
    assert_eq!(preferences.image_paste_behavior, ImagePasteBehavior::None);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn unknown_auto_save_value_falls_back_to_off() {
    let root = std::env::temp_dir().join(format!(
        "gmark-preferences-auto-save-invalid-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("temp root should exist");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::write(
        dirs.app_config_file(),
        r#"
                [editor]
                auto_save = "always_without_conflict_checks"
            "#,
    )
    .expect("preferences should be written");

    let preferences = read_app_preferences_with_dirs(&dirs).expect("preferences should load");
    assert_eq!(preferences.auto_save, AutoSavePreference::Off);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn editor_typography_preferences_are_bounded_and_quantized() {
    let root = std::env::temp_dir().join(format!(
        "gmark-preferences-typography-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("temp root should exist");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::write(
        dirs.app_config_file(),
        r#"
                [editor]
                font_size = 99
                line_height_percent = 163
                content_width = 901
                font_family = " Georgia "
            "#,
    )
    .expect("preferences should be written");

    let preferences = read_app_preferences_with_dirs(&dirs).expect("preferences should load");
    assert_eq!(preferences.editor_font_size, 24);
    assert_eq!(preferences.editor_line_height_percent, 165);
    assert_eq!(preferences.editor_content_width, 920);
    assert_eq!(preferences.editor_font_family, "Georgia");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn damaged_preferences_file_returns_defaults() {
    let root = std::env::temp_dir().join(format!(
        "gmark-preferences-damaged-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("temp root should exist");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::write(dirs.app_config_file(), "not = [valid").expect("preferences should be written");

    let preferences =
        read_app_preferences_with_dirs(&dirs).expect("damaged preferences should load");
    assert_eq!(preferences, AppPreferences::default());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn saves_and_reads_preferences() {
    let root =
        std::env::temp_dir().join(format!("gmark-preferences-save-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);
    let preferences = AppPreferences {
        startup_open: StartupOpenPreference::LastOpenedFile,
        default_language_id: "zh-CN".into(),
        default_theme_id: "gmark-light".into(),
        show_table_headers: false,
        image_paste_behavior: ImagePasteBehavior::CopyToAssetsFolder,
        auto_save: AutoSavePreference::AfterDelay,
        spell_check: false,
        auto_pair_brackets: false,
        auto_pair_markdown: true,
        editor_font_size: 19,
        editor_line_height_percent: 170,
        editor_content_width: 920,
        editor_font_family: "Georgia".into(),
        workspace_sidebar_position: WorkspaceSidebarPosition::Right,
        show_tab_bar_actions: true,
        recent_editing_commands: Vec::new(),
        keybindings: BTreeMap::new(),
        status_bar: StatusBarPreferences::default(),
    };

    save_app_preferences_with_dirs(&preferences, &dirs)
        .expect("preferences should save to config.toml");
    let loaded = read_app_preferences_with_dirs(&dirs).expect("preferences should read back");
    assert_eq!(loaded, preferences);

    let text = std::fs::read_to_string(dirs.app_config_file()).expect("config.toml should exist");
    assert!(text.contains("open = \"last_opened_file\""));
    assert!(text.contains("default_language_id = \"zh-CN\""));
    assert!(text.contains("default_theme_id = \"gmark-light\""));
    assert!(text.contains("show_table_headers = false"));
    assert!(text.contains("image_paste_behavior = \"copy_to_assets_folder\""));
    assert!(text.contains("auto_save = \"after_delay\""));
    assert!(text.contains("auto_pair_brackets = false"));
    assert!(text.contains("auto_pair_markdown = true"));
    assert!(text.contains("font_size = 19"));
    assert!(text.contains("line_height_percent = 170"));
    assert!(text.contains("content_width = 920"));
    assert!(text.contains("font_family = \"Georgia\""));
    assert!(text.contains("workspace_sidebar_position = \"right\""));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn missing_preferences_file_is_created_with_detected_language() {
    let root =
        std::env::temp_dir().join(format!("gmark-preferences-create-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);
    let preferences = load_or_create_app_preferences_with_dirs_and_locales(&dirs, ["zh-HK"])
        .expect("preferences should be created");
    assert_eq!(preferences.default_language_id, "zh-CN");
    assert!(dirs.app_config_file().exists());
    let text = std::fs::read_to_string(dirs.app_config_file()).expect("config.toml should exist");
    assert!(text.contains("[language]"));
    assert!(text.contains("default_language_id = \"zh-CN\""));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn legacy_preferences_are_normalized_with_language() {
    let root =
        std::env::temp_dir().join(format!("gmark-preferences-legacy-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp root should exist");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::write(
        dirs.app_config_file(),
        r#"
                [startup]
                open = "last_opened_file"

                [theme]
                default_theme_id = "gmark-light"
            "#,
    )
    .expect("legacy preferences should be written");

    let preferences = load_or_create_app_preferences_with_dirs_and_locales(&dirs, ["en-GB"])
        .expect("legacy preferences should normalize");
    assert_eq!(
        preferences.startup_open,
        StartupOpenPreference::LastOpenedFile
    );
    assert_eq!(preferences.default_language_id, "en-US");
    assert_eq!(preferences.default_theme_id, "gmark-light");
    let text = std::fs::read_to_string(dirs.app_config_file()).expect("config.toml should exist");
    assert!(text.contains("[language]"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn saving_preferences_window_persists_selected_language() {
    let root =
        std::env::temp_dir().join(format!("gmark-preferences-window-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);
    let preferences = AppPreferences {
        startup_open: StartupOpenPreference::NewFile,
        default_language_id: "zh-CN".into(),
        default_theme_id: "gmark".into(),
        show_table_headers: true,
        image_paste_behavior: ImagePasteBehavior::None,
        auto_save: AutoSavePreference::Off,
        spell_check: true,
        auto_pair_brackets: true,
        auto_pair_markdown: true,
        editor_font_size: 17,
        editor_line_height_percent: 160,
        editor_content_width: 880,
        editor_font_family: String::new(),
        workspace_sidebar_position: WorkspaceSidebarPosition::Left,
        show_tab_bar_actions: false,
        recent_editing_commands: Vec::new(),
        keybindings: BTreeMap::new(),
        status_bar: StatusBarPreferences::default(),
    };
    save_app_preferences_with_dirs(&preferences, &dirs)
        .expect("preferences should save to config.toml");

    let status_bar = StatusBarPreferences {
        custom_buttons: vec![StatusBarButton {
            id: "publish".into(),
            label: "Publish".into(),
            action_id: "publish_document".into(),
        }],
        ..StatusBarPreferences::default()
    };
    let saved = save_preferences_from_window_with_dirs(
        StartupOpenPreference::LastOpenedFile,
        AutoSavePreference::AfterDelay,
        false,
        false,
        false,
        18,
        175,
        960,
        "Georgia",
        WorkspaceSidebarPosition::Right,
        true,
        "gmark-light",
        "en-US",
        ImagePasteBehavior::CopyToNamedAssetsFolder,
        BTreeMap::from([("save_document".to_string(), vec!["ctrl-alt-s".to_string()])]),
        &status_bar,
        &dirs,
    )
    .expect("window preferences should save");
    assert_eq!(saved.default_language_id, "en-US");
    assert_eq!(saved.startup_open, StartupOpenPreference::LastOpenedFile);
    assert_eq!(saved.default_theme_id, "gmark-light");
    assert_eq!(saved.auto_save, AutoSavePreference::AfterDelay);
    assert!(!saved.spell_check);
    assert!(!saved.auto_pair_brackets);
    assert!(!saved.auto_pair_markdown);
    assert_eq!(saved.editor_font_size, 18);
    assert_eq!(saved.editor_line_height_percent, 175);
    assert_eq!(saved.editor_content_width, 960);
    assert_eq!(saved.editor_font_family, "Georgia");
    assert_eq!(
        saved.workspace_sidebar_position,
        WorkspaceSidebarPosition::Right
    );
    assert!(saved.show_tab_bar_actions);
    assert_eq!(
        saved.image_paste_behavior,
        ImagePasteBehavior::CopyToNamedAssetsFolder
    );
    assert_eq!(
        saved.keybindings.get("save_document"),
        Some(&vec!["ctrl-alt-s".to_string()])
    );
    assert_eq!(saved.status_bar.custom_buttons, status_bar.custom_buttons);
    let _ = std::fs::remove_dir_all(root);
}

#[gpui::test]
async fn preferences_window_activates_and_focuses_on_open(cx: &mut TestAppContext) {
    init_preferences_test_app(cx);

    let handle = cx.update(|cx| {
        open_preferences_window_with_state(
            cx,
            AppPreferences::default(),
            default_theme_options(),
            "Preferences".into(),
        )
    });
    cx.run_until_parked();

    let active_window = cx.update(|cx| cx.active_window().expect("window should be active"));
    assert_eq!(active_window.window_id(), handle.window_id());
    assert!(
        handle
            .update(cx, |preferences, window, _cx| preferences
                .focus_handle
                .is_focused(window))
            .expect("preferences window should be updateable")
    );
    assert!(
        !handle
            .update(cx, |preferences, _window, _cx| preferences
                .has_unsaved_changes())
            .expect("preferences window should be updateable")
    );
}

#[gpui::test]
async fn preferences_pages_keep_actions_visible_at_two_x_scale(cx: &mut TestAppContext) {
    init_preferences_test_app(cx);
    let handle = cx.update(|cx| {
        open_preferences_window_with_state(
            cx,
            AppPreferences::default(),
            default_theme_options(),
            "Preferences".into(),
        )
    });
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    for viewport in [size(px(720.0), px(520.0)), size(px(980.0), px(700.0))] {
        visual.simulate_resize(viewport);
        for nav in [
            PreferencesNav::File,
            PreferencesNav::Editor,
            PreferencesNav::Theme,
            PreferencesNav::Image,
            PreferencesNav::Shortcuts,
            PreferencesNav::StatusBar,
        ] {
            handle
                .update(&mut visual, |preferences, _window, cx| {
                    preferences.nav = nav;
                    cx.notify();
                })
                .unwrap();
            visual.update(|window, cx| {
                assert_eq!(window.scale_factor(), 2.0);
                window.draw(cx).clear();
            });
            visual.run_until_parked();

            let content = visual.debug_bounds("preferences-content").unwrap();
            if let Some(titlebar) = visual.debug_bounds("preferences-titlebar") {
                let title_label = visual
                    .debug_bounds("preferences-titlebar-title-label")
                    .unwrap();
                assert!(title_label.left() >= titlebar.left());
                assert!(title_label.right() <= titlebar.right());
                if cfg!(target_os = "macos") {
                    assert!(
                        visual
                            .debug_bounds("preferences-titlebar-leading-icon")
                            .is_none()
                    );
                    assert!(
                        (f32::from(title_label.center().x) - f32::from(titlebar.center().x)).abs()
                            <= 1.0
                    );
                } else {
                    let leading_icon = visual
                        .debug_bounds("preferences-titlebar-leading-icon")
                        .unwrap();
                    assert_eq!(leading_icon.size, size(px(20.0), px(20.0)));
                    assert!(leading_icon.left() >= titlebar.left());
                    assert!(title_label.left() > leading_icon.right());
                }
            }
            let navigation = visual.debug_bounds("preferences-navigation").unwrap();
            let search = visual.debug_bounds("preferences-search-input").unwrap();
            let search_icon = visual.debug_bounds("preferences-search-icon").unwrap();
            let main = visual.debug_bounds("preferences-main").unwrap();
            let title = visual.debug_bounds("preferences-page-title").unwrap();
            let page = visual.debug_bounds("preferences-page-scroll").unwrap();
            let actions = visual.debug_bounds("preferences-actions").unwrap();
            let cancel = visual.debug_bounds("preferences-cancel").unwrap();
            let save = visual.debug_bounds("preferences-save").unwrap();
            assert_eq!(f32::from(navigation.size.width), 200.0);
            assert_eq!(f32::from(search.size.height), 34.0);
            assert_eq!(navigation.right(), main.left());
            assert!(search.left() >= navigation.left());
            assert!(search.right() <= navigation.right());
            assert!(search_icon.left() >= search.left());
            assert!(search_icon.right() <= search.right());
            for selector in [
                "preferences-nav-file",
                "preferences-nav-editor",
                "preferences-nav-theme",
                "preferences-nav-image",
                "preferences-nav-shortcuts",
                "preferences-nav-status-bar",
            ] {
                let row = visual.debug_bounds(selector).unwrap();
                assert_eq!(f32::from(row.size.height), 36.0, "{selector}");
                assert!(row.left() >= navigation.left(), "{selector}");
                assert!(row.right() <= navigation.right(), "{selector}");
            }
            for (name, bounds) in [
                ("navigation", navigation),
                ("main", main),
                ("title", title),
                ("page", page),
                ("actions", actions),
            ] {
                assert!(bounds.left() >= content.left(), "{name} escaped left");
                assert!(bounds.right() <= content.right(), "{name} escaped right");
                assert!(bounds.top() >= content.top(), "{name} escaped top");
                assert!(bounds.bottom() <= content.bottom(), "{name} escaped bottom");
            }
            assert!(title.left() >= main.left());
            assert!(title.right() <= main.right());
            let page_control_selector = match nav {
                PreferencesNav::File => "preferences-startup-dropdown",
                PreferencesNav::Editor => "preferences-editor-font-size",
                PreferencesNav::Theme => "preferences-theme-dropdown",
                PreferencesNav::Image => "preferences-image-dropdown",
                PreferencesNav::Shortcuts => "preferences-shortcuts-scroll",
                PreferencesNav::StatusBar => "preferences-status-bar-options",
            };
            let page_control = visual
                .debug_bounds(page_control_selector)
                .unwrap_or_else(|| {
                    panic!(
                        "missing {page_control_selector} for {nav:?} at {}x{}",
                        viewport.width, viewport.height
                    )
                });
            assert!(page_control.left() >= page.left());
            assert!(page_control.right() <= page.right());
            for (name, bounds) in [("cancel", cancel), ("save", save)] {
                assert!(bounds.left() >= actions.left(), "{name} escaped actions");
                assert!(bounds.right() <= actions.right(), "{name} escaped actions");
                assert!(bounds.top() >= actions.top(), "{name} escaped actions");
                assert!(
                    bounds.bottom() <= actions.bottom(),
                    "{name} escaped actions"
                );
            }
        }
    }
}

#[gpui::test]
async fn language_preference_is_editable_from_the_theme_page(cx: &mut TestAppContext) {
    init_preferences_test_app(cx);
    let handle = cx.update(|cx| {
        open_preferences_window_with_state(
            cx,
            AppPreferences::default(),
            default_theme_options(),
            "Preferences".into(),
        )
    });
    cx.run_until_parked();

    handle
        .update(cx, |preferences, _window, cx| {
            assert_eq!(preferences.selected_language_id, "en-US");
            preferences.commit_dropdown_selection(PreferencesDropdown::Language, 0, cx);
            assert_eq!(preferences.selected_language_id, "zh-CN");
            assert!(preferences.has_unsaved_changes());
            assert!(!preferences.language_dropdown_open);
        })
        .expect("preferences window should be updateable");
}

#[gpui::test]
async fn preferences_navigation_supports_directional_and_activation_keys(cx: &mut TestAppContext) {
    init_preferences_test_app(cx);
    let handle = cx.update(|cx| {
        open_preferences_window_with_state(
            cx,
            AppPreferences::default(),
            default_theme_options(),
            "Preferences".into(),
        )
    });
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(size(px(720.0), px(520.0)));
    visual.update(|window, cx| window.draw(cx).clear());

    handle
        .update(&mut visual, |preferences, window, _cx| {
            preferences.nav_focus_handles[0].focus(window);
            assert!(preferences.nav_focus_handles[0].is_focused(window));
        })
        .unwrap();
    visual.simulate_keystrokes("right");
    visual.run_until_parked();
    handle
        .update(&mut visual, |preferences, window, _cx| {
            assert_eq!(preferences.nav, PreferencesNav::Editor);
            assert!(preferences.nav_focus_handles[1].is_focused(window));
        })
        .unwrap();

    visual.update(|window, cx| window.draw(cx).clear());
    visual.simulate_keystrokes("end");
    visual.run_until_parked();
    handle
        .update(&mut visual, |preferences, window, _cx| {
            assert_eq!(preferences.nav, PreferencesNav::StatusBar);
            assert!(preferences.nav_focus_handles[5].is_focused(window));
        })
        .unwrap();

    visual.update(|window, cx| window.draw(cx).clear());
    visual.simulate_keystrokes("home");
    visual.run_until_parked();
    handle
        .update(&mut visual, |preferences, window, _cx| {
            assert_eq!(preferences.nav, PreferencesNav::File);
            assert!(preferences.nav_focus_handles[0].is_focused(window));
            assert!(!preferences.has_unsaved_changes());
        })
        .unwrap();

    handle
        .update(&mut visual, |preferences, window, _cx| {
            preferences.nav_focus_handles[2].focus(window);
        })
        .unwrap();
    visual.simulate_keystrokes("space");
    visual.run_until_parked();
    handle
        .update(&mut visual, |preferences, window, _cx| {
            assert_eq!(preferences.nav, PreferencesNav::Theme);
            assert!(preferences.nav_focus_handles[2].is_focused(window));
            assert!(!preferences.has_unsaved_changes());
        })
        .unwrap();

    visual.simulate_resize(size(px(980.0), px(700.0)));
    visual.update(|window, cx| {
        assert_eq!(window.scale_factor(), 2.0);
        window.draw(cx).clear();
    });
    let navigation = visual.debug_bounds("preferences-navigation").unwrap();
    for selector in [
        "preferences-nav-file",
        "preferences-nav-editor",
        "preferences-nav-theme",
        "preferences-nav-image",
        "preferences-nav-shortcuts",
        "preferences-nav-status-bar",
    ] {
        let row = visual.debug_bounds(selector).unwrap();
        assert_eq!(f32::from(row.size.height), 36.0, "{selector}");
        assert!(row.left() >= navigation.left(), "{selector}");
        assert!(row.right() <= navigation.right(), "{selector}");
    }
}

#[gpui::test]
async fn preferences_dropdowns_support_keyboard_navigation_and_commit(cx: &mut TestAppContext) {
    init_preferences_test_app(cx);
    let handle = cx.update(|cx| {
        open_preferences_window_with_state(
            cx,
            AppPreferences::default(),
            default_theme_options(),
            "Preferences".into(),
        )
    });
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(size(px(720.0), px(520.0)));
    visual.update(|window, cx| window.draw(cx).clear());

    handle
        .update(&mut visual, |preferences, window, _cx| {
            preferences.dropdown_focus_handles[PreferencesDropdown::Startup.index()].focus(window);
        })
        .unwrap();
    visual.simulate_keystrokes("enter down enter");
    visual.run_until_parked();
    handle
        .update(&mut visual, |preferences, window, _cx| {
            assert_eq!(
                preferences.startup_open,
                StartupOpenPreference::LastOpenedFile
            );
            assert!(!preferences.startup_dropdown_open);
            assert!(preferences.dropdown_focus_handles[0].is_focused(window));
            preferences.dropdown_focus_handles[PreferencesDropdown::AutoSave.index()].focus(window);
        })
        .unwrap();

    visual.simulate_keystrokes("space end enter");
    visual.run_until_parked();
    handle
        .update(&mut visual, |preferences, window, _cx| {
            assert_eq!(preferences.auto_save, AutoSavePreference::AfterDelay);
            assert!(!preferences.auto_save_dropdown_open);
            preferences.select_nav(PreferencesNav::Theme, _cx);
            preferences.dropdown_focus_handles[PreferencesDropdown::Theme.index()].focus(window);
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());

    visual.simulate_keystrokes("enter down escape");
    visual.run_until_parked();
    handle
        .update(&mut visual, |preferences, window, cx| {
            assert_eq!(preferences.selected_theme_id, "gmark");
            assert!(!preferences.theme_dropdown_open);
            assert_eq!(cx.global::<ThemeManager>().current_theme_id(), "gmark");
            assert!(preferences.dropdown_focus_handles[2].is_focused(window));
        })
        .unwrap();

    visual.simulate_keystrokes("enter down enter");
    visual.run_until_parked();
    handle
        .update(&mut visual, |preferences, window, cx| {
            assert_eq!(preferences.selected_theme_id, "gmark-light");
            assert_eq!(
                cx.global::<ThemeManager>().current_theme_id(),
                "gmark-light"
            );
            preferences.select_nav(PreferencesNav::Image, cx);
            preferences.dropdown_focus_handles[PreferencesDropdown::Image.index()].focus(window);
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());

    visual.simulate_keystrokes("down end enter");
    visual.run_until_parked();
    handle
        .update(&mut visual, |preferences, _window, cx| {
            assert_eq!(
                preferences.image_paste_behavior,
                ImagePasteBehavior::CopyToNamedAssetsFolder
            );
            preferences.set_dropdown_open(PreferencesDropdown::Image, true);
            preferences.select_nav(PreferencesNav::File, cx);
            assert!(!preferences.image_dropdown_open);
        })
        .unwrap();

    for viewport in [size(px(720.0), px(520.0)), size(px(980.0), px(700.0))] {
        visual.simulate_resize(viewport);
        handle
            .update(&mut visual, |preferences, _window, cx| {
                preferences.set_dropdown_open(PreferencesDropdown::Startup, false);
                cx.notify();
            })
            .unwrap();
        visual.update(|window, cx| window.draw(cx).clear());
        let startup_row_before = visual.debug_bounds("preferences-startup-row").unwrap();
        handle
            .update(&mut visual, |preferences, _window, cx| {
                preferences.set_dropdown_open(PreferencesDropdown::Startup, true);
                cx.notify();
            })
            .unwrap();
        visual.update(|window, cx| {
            assert_eq!(window.scale_factor(), 2.0);
            window.draw(cx).clear();
        });
        visual.run_until_parked();
        let page = visual.debug_bounds("preferences-page-scroll").unwrap();
        let selector = "preferences-startup-dropdown";
        let bounds = visual
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("missing {selector}"));
        let startup_row_after = visual.debug_bounds("preferences-startup-row").unwrap();
        assert_eq!(startup_row_after, startup_row_before);
        assert!(bounds.left() >= page.left(), "{selector} escaped left");
        assert!(bounds.right() <= page.right(), "{selector} escaped right");
        assert!(bounds.top() >= page.top(), "{selector} escaped top");
        assert!(
            bounds.bottom() <= page.bottom(),
            "{selector} escaped bottom"
        );
    }
}
