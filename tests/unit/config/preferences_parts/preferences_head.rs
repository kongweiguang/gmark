// @author kongweiguang

use super::{
    AppPreferences, AutoSavePreference, DocumentLoadingPreferences, EditorSettings,
    ImagePasteBehavior, PreferencesDropdown, PreferencesNav, PreferencesNumericInput,
    PreferencesStepperControl, PreferencesSwitch, ResourceInsertBehavior,
    StartupOpenPreference, StatusBarButton, StatusBarPreferences,
    load_or_create_app_preferences_with_dirs_and_locales, open_preferences_window_with_state,
    parse_numeric_input, read_app_preferences_with_dirs, save_app_preferences_with_dirs,
    save_preferences_from_window_with_dirs,
};
use crate::config::GmarkConfigDirs;
use crate::i18n::I18nManager;
use crate::theme::{ThemeAppearance, ThemeManager, ThemePalette};
use gpui::{KeyDownEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext, px, size};
use std::collections::BTreeMap;

fn init_preferences_test_app(cx: &mut TestAppContext) {
    cx.update(|cx| {
        I18nManager::init_with_language_id(cx, "en-US");
        ThemeManager::init_with_preference(cx, ThemeAppearance::System, ThemePalette::Xcode);
        crate::components::init(cx);
        EditorSettings::init(cx, true, AutoSavePreference::Off, true);
    });
}

#[test]
fn editor_preferences_match_product_defaults() {
    let preferences = AppPreferences::default();

    assert!(preferences.editor_font_family.is_empty());
    assert_eq!(preferences.editor_font_size, 16);
    assert_eq!(preferences.editor_line_height_percent, 160);
    assert_eq!(preferences.editor_content_width, 1200);
    assert!(preferences.auto_pair_brackets);
    assert!(preferences.auto_pair_markdown);
    assert!(!preferences.show_tab_bar_actions);
    assert_eq!(
        preferences.resource_insert_behavior(),
        ResourceInsertBehavior::None
    );
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
    assert_eq!(preferences.theme_appearance, ThemeAppearance::System);
    assert_eq!(preferences.theme_palette, ThemePalette::Xcode);
    assert_eq!(preferences.image_paste_behavior, ImagePasteBehavior::None);
    assert!(preferences.auto_pair_brackets);
    assert!(preferences.auto_pair_markdown);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn legacy_workspace_sidebar_position_is_ignored_and_not_written_back() {
    let root = std::env::temp_dir().join(format!(
        "gmark-preferences-sidebar-legacy-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("temp root should exist");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::write(
        dirs.app_config_file(),
        "[editor]\nworkspace_sidebar_position = \"right\"\n",
    )
    .expect("legacy preference should be written");

    let loaded = read_app_preferences_with_dirs(&dirs).expect("legacy preferences should load");
    assert_eq!(loaded, AppPreferences::default());
    save_app_preferences_with_dirs(&loaded, &dirs).expect("normalized preferences should save");
    let saved = std::fs::read_to_string(dirs.app_config_file()).expect("config should exist");
    assert!(!saved.contains("workspace_sidebar_position"));
    std::fs::remove_dir_all(root).expect("temp root should be removed");
}

#[test]
fn invalid_theme_pair_falls_back_to_system_xcode() {
    let root = std::env::temp_dir().join(format!(
        "gmark-preferences-invalid-theme-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("temp root should exist");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::write(
        dirs.app_config_file(),
        r#"
                [theme]
                appearance = "dark"
                palette = "not-a-palette"
            "#,
    )
    .expect("invalid theme preferences should be written");

    let preferences =
        read_app_preferences_with_dirs(&dirs).expect("invalid theme preferences should load");
    assert_eq!(preferences.theme_appearance, ThemeAppearance::System);
    assert_eq!(preferences.theme_palette, ThemePalette::Xcode);
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
fn resource_insert_behavior_migrates_legacy_key_and_new_key_wins() {
    let root = std::env::temp_dir().join(format!(
        "gmark-preferences-resource-migration-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("temp root should exist");
    let dirs = GmarkConfigDirs::from_root(&root);

    std::fs::write(
        dirs.app_config_file(),
        r#"
                [editor]
                image_paste_behavior = "copy_to_named_assets_folder"
            "#,
    )
    .expect("legacy preferences should be written");
    let migrated = load_or_create_app_preferences_with_dirs_and_locales(&dirs, ["en-US"])
        .expect("legacy preferences should migrate");
    assert_eq!(
        migrated.resource_insert_behavior(),
        ResourceInsertBehavior::CopyToNamedAssetsFolder
    );
    let normalized =
        std::fs::read_to_string(dirs.app_config_file()).expect("migrated config should exist");
    assert!(normalized.contains(
        "resource_insert_behavior = \"copy_to_named_assets_folder\""
    ));
    assert!(normalized.contains(
        "image_paste_behavior = \"copy_to_named_assets_folder\""
    ));

    std::fs::write(
        dirs.app_config_file(),
        r#"
                [editor]
                resource_insert_behavior = "copy_to_document_folder"
                image_paste_behavior = "copy_to_assets_folder"
            "#,
    )
    .expect("mixed-version preferences should be written");
    let preferred =
        read_app_preferences_with_dirs(&dirs).expect("mixed-version preferences should load");
    assert_eq!(
        preferred.resource_insert_behavior(),
        ResourceInsertBehavior::CopyToDocumentFolder
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn resource_insert_behavior_round_trips_all_modes_and_dual_writes() {
    let root = std::env::temp_dir().join(format!(
        "gmark-preferences-resource-round-trip-{}",
        uuid::Uuid::new_v4()
    ));
    let dirs = GmarkConfigDirs::from_root(&root);

    for behavior in [
        ResourceInsertBehavior::None,
        ResourceInsertBehavior::CopyToDocumentFolder,
        ResourceInsertBehavior::CopyToAssetsFolder,
        ResourceInsertBehavior::CopyToNamedAssetsFolder,
    ] {
        let preferences = AppPreferences {
            image_paste_behavior: behavior,
            ..AppPreferences::default()
        };
        save_app_preferences_with_dirs(&preferences, &dirs)
            .expect("resource preferences should save");

        let loaded =
            read_app_preferences_with_dirs(&dirs).expect("resource preferences should reload");
        assert_eq!(loaded.resource_insert_behavior(), behavior);

        let text =
            std::fs::read_to_string(dirs.app_config_file()).expect("config should exist");
        assert!(text.contains(&format!(
            "resource_insert_behavior = \"{}\"",
            behavior.as_str()
        )));
        assert!(text.contains(&format!(
            "image_paste_behavior = \"{}\"",
            behavior.as_str()
        )));
    }

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
        auto_check_updates: false,
        default_language_id: "zh-CN".into(),
        theme_appearance: ThemeAppearance::Light,
        theme_palette: ThemePalette::Xcode,
        show_table_headers: false,
        image_paste_behavior: ImagePasteBehavior::CopyToAssetsFolder,
        auto_save: AutoSavePreference::AfterDelay,
        spell_check: false,
        auto_pair_brackets: false,
        auto_pair_markdown: true,
        code_folding: true,
        format_on_save: false,
        editor_font_size: 19,
        editor_line_height_percent: 170,
        editor_content_width: 920,
        editor_font_family: "Georgia".into(),
        show_tab_bar_actions: true,
        recent_editing_commands: Vec::new(),
        keybindings: BTreeMap::new(),
        status_bar: StatusBarPreferences::default(),
        document_loading: DocumentLoadingPreferences {
            max_resident_mib: Some(32),
        },
    };

    save_app_preferences_with_dirs(&preferences, &dirs)
        .expect("preferences should save to config.toml");
    let loaded = read_app_preferences_with_dirs(&dirs).expect("preferences should read back");
    assert_eq!(loaded, preferences);

    let text = std::fs::read_to_string(dirs.app_config_file()).expect("config.toml should exist");
    assert!(text.contains("open = \"last_opened_file\""));
    assert!(text.contains("auto_check = false"));
    assert!(text.contains("default_language_id = \"zh-CN\""));
    assert!(text.contains("appearance = \"light\""));
    assert!(text.contains("palette = \"xcode\""));
    assert!(!text.contains("default_theme_id"));
    assert!(text.contains("show_table_headers = false"));
    assert!(text.contains("resource_insert_behavior = \"copy_to_assets_folder\""));
    assert!(text.contains("image_paste_behavior = \"copy_to_assets_folder\""));
    assert!(text.contains("auto_save = \"after_delay\""));
    assert!(text.contains("auto_pair_brackets = false"));
    assert!(text.contains("auto_pair_markdown = true"));
    assert!(text.contains("font_size = 19"));
    assert!(text.contains("line_height_percent = 170"));
    assert!(text.contains("content_width = 920"));
    assert!(text.contains("font_family = \"Georgia\""));
    assert!(!text.contains("workspace_sidebar_position"));
    assert!(text.contains("max_resident_mib = 32"));
    assert!(!text.contains("preset ="));
    assert!(!text.contains("max_resident_lines"));
    assert!(!text.contains("max_structural_units"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn loading_preferences_apply_valid_overrides_and_ignore_invalid_values() {
    let valid = DocumentLoadingPreferences {
        max_resident_mib: Some(32),
    }
    .policy()
    .effective_limits();
    assert_eq!(valid.max_resident_bytes, 32 * 1024 * 1024);

    let fallback = DocumentLoadingPreferences {
        max_resident_mib: Some(0),
    }
    .policy()
    .effective_limits();
    assert_eq!(fallback, gmark_document_core::DEFAULT_LOADING_LIMITS);

    let valid_preferences = DocumentLoadingPreferences {
        max_resident_mib: Some(24),
    };
    assert!(!valid_preferences.has_invalid_override());
    assert!(
        DocumentLoadingPreferences {
            max_resident_mib: Some(1_025),
        }
        .has_invalid_override()
    );
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
    assert_eq!(preferences.theme_appearance, ThemeAppearance::System);
    assert_eq!(preferences.theme_palette, ThemePalette::Xcode);
    let text = std::fs::read_to_string(dirs.app_config_file()).expect("config.toml should exist");
    assert!(text.contains("[language]"));
    assert!(text.contains("appearance = \"system\""));
    assert!(text.contains("palette = \"xcode\""));
    assert!(!text.contains("default_theme_id"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn saving_preferences_window_persists_selected_language() {
    let root =
        std::env::temp_dir().join(format!("gmark-preferences-window-{}", uuid::Uuid::new_v4()));
    let dirs = GmarkConfigDirs::from_root(&root);
    let preferences = AppPreferences {
        startup_open: StartupOpenPreference::NewFile,
        auto_check_updates: true,
        default_language_id: "zh-CN".into(),
        theme_appearance: ThemeAppearance::System,
        theme_palette: ThemePalette::Xcode,
        show_table_headers: true,
        image_paste_behavior: ImagePasteBehavior::None,
        auto_save: AutoSavePreference::Off,
        spell_check: true,
        auto_pair_brackets: true,
        auto_pair_markdown: true,
        code_folding: true,
        format_on_save: false,
        editor_font_size: 17,
        editor_line_height_percent: 160,
        editor_content_width: 880,
        editor_font_family: String::new(),
        show_tab_bar_actions: false,
        recent_editing_commands: Vec::new(),
        keybindings: BTreeMap::new(),
        status_bar: StatusBarPreferences::default(),
        document_loading: DocumentLoadingPreferences::default(),
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
    let document_loading = DocumentLoadingPreferences {
        max_resident_mib: Some(96),
    };
    let saved = save_preferences_from_window_with_dirs(
        StartupOpenPreference::LastOpenedFile,
        false,
        AutoSavePreference::AfterDelay,
        false,
        false,
        false,
        true,
        false,
        18,
        175,
        960,
        "Georgia",
        true,
        ThemeAppearance::Light,
        ThemePalette::JetBrains,
        "en-US",
        ImagePasteBehavior::CopyToNamedAssetsFolder,
        BTreeMap::from([("save_document".to_string(), vec!["ctrl-alt-s".to_string()])]),
        &document_loading,
        &status_bar,
        &dirs,
    )
    .expect("window preferences should save");
    assert_eq!(saved.default_language_id, "en-US");
    assert_eq!(saved.startup_open, StartupOpenPreference::LastOpenedFile);
    assert!(!saved.auto_check_updates);
    assert_eq!(saved.theme_appearance, ThemeAppearance::Light);
    assert_eq!(saved.theme_palette, ThemePalette::JetBrains);
    assert_eq!(saved.auto_save, AutoSavePreference::AfterDelay);
    assert!(!saved.spell_check);
    assert!(!saved.auto_pair_brackets);
    assert!(!saved.auto_pair_markdown);
    assert_eq!(saved.editor_font_size, 18);
    assert_eq!(saved.editor_line_height_percent, 175);
    assert_eq!(saved.editor_content_width, 960);
    assert_eq!(saved.editor_font_family, "Georgia");
    assert!(saved.show_tab_bar_actions);
    assert_eq!(saved.document_loading, document_loading);
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
        open_preferences_window_with_state(cx, AppPreferences::default(), "Preferences".into())
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
        open_preferences_window_with_state(cx, AppPreferences::default(), "Preferences".into())
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
                PreferencesNav::Theme => "preferences-theme-appearance-system",
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
        open_preferences_window_with_state(cx, AppPreferences::default(), "Preferences".into())
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
        open_preferences_window_with_state(cx, AppPreferences::default(), "Preferences".into())
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
        open_preferences_window_with_state(cx, AppPreferences::default(), "Preferences".into())
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
        .update(&mut visual, |preferences, window, cx| {
            assert_eq!(preferences.auto_save, AutoSavePreference::AfterDelay);
            assert!(!preferences.auto_save_dropdown_open);
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
