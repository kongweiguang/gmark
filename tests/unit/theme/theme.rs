// @author kongweiguang

use super::{SYSTEM_THEME_ID, Theme, ThemeManager, resolved_system_theme_id};
use crate::config::GmarkConfigDirs;
use gpui::{WindowAppearance, rgba};

#[test]
fn deserializes_legacy_block_focused_bg_key() {
    let default_json = Theme::default_theme()
        .to_json()
        .expect("default theme should serialize");
    let legacy_json = default_json.replace("source_mode_block_bg", "block_focused_bg");

    let theme = Theme::from_json(&legacy_json).expect("legacy theme should deserialize");
    assert!(theme.colors.source_mode_block_bg.a > 0.0);
}

#[test]
fn border_h2_falls_back_when_omitted() {
    let default_json = Theme::default_theme()
        .to_json()
        .expect("default theme should serialize");
    let parsed: serde_json::Value =
        serde_json::from_str(&default_json).expect("default theme json should parse");
    let mut object = parsed
        .as_object()
        .expect("theme should serialize to a json object")
        .clone();
    object
        .get_mut("colors")
        .and_then(|colors| colors.as_object_mut())
        .expect("theme should include colors")
        .remove("border_h2");
    let json = serde_json::to_string(&object).expect("theme json should serialize");

    let theme = Theme::from_json(&json).expect("theme without border_h2 should deserialize");
    assert_eq!(theme.colors.border_h2, rgba(0xe0e0e0cc).into());
}

#[test]
fn comment_background_falls_back_when_omitted() {
    let default_json = Theme::default_theme()
        .to_json()
        .expect("default theme should serialize");
    let parsed: serde_json::Value =
        serde_json::from_str(&default_json).expect("default theme json should parse");
    let mut object = parsed
        .as_object()
        .expect("theme should serialize to a json object")
        .clone();
    object
        .get_mut("colors")
        .and_then(|colors| colors.as_object_mut())
        .expect("theme should include colors")
        .remove("comment_bg");
    let json = serde_json::to_string(&object).expect("theme json should serialize");

    let theme = Theme::from_json(&json).expect("theme without comment_bg should deserialize");
    assert_eq!(theme.colors.comment_bg, rgba(0xfbbf2426).into());
}

#[test]
fn default_theme_json_omits_dialog_badge_and_strings_tokens() {
    let default_json = Theme::default_theme()
        .to_json()
        .expect("default theme should serialize");
    let parsed: serde_json::Value =
        serde_json::from_str(&default_json).expect("default theme json should parse");

    assert!(parsed.get("strings").is_none());

    let colors = parsed
        .get("colors")
        .and_then(|colors| colors.as_object())
        .expect("theme should include colors");
    assert!(!colors.contains_key(&format!("dialog_{}", "badge_bg")));
    assert!(!colors.contains_key(&format!("dialog_{}", "badge_text")));

    let dimensions = parsed
        .get("dimensions")
        .and_then(|dimensions| dimensions.as_object())
        .expect("theme should include dimensions");
    assert!(!dimensions.contains_key(&format!("dialog_{}", "badge_padding_x")));
    assert!(!dimensions.contains_key(&format!("dialog_{}", "badge_padding_y")));
}

#[test]
fn legacy_theme_json_with_strings_still_loads() {
    let default_json = Theme::default_theme()
        .to_json()
        .expect("default theme should serialize");
    let parsed: serde_json::Value =
        serde_json::from_str(&default_json).expect("default theme json should parse");
    let mut object = parsed
        .as_object()
        .expect("theme should serialize to a json object")
        .clone();
    object.insert(
        "strings".into(),
        serde_json::json!({
            "menu_file": "Legacy File",
            "menu_language": "Legacy Language"
        }),
    );
    let json = serde_json::to_string(&object).expect("theme json should serialize");

    Theme::from_json(&json).expect("legacy theme strings should be ignored safely");
}

#[test]
fn callout_dimensions_fall_back_when_omitted() {
    let default_json = Theme::default_theme()
        .to_json()
        .expect("default theme should serialize");
    let parsed: serde_json::Value =
        serde_json::from_str(&default_json).expect("default theme json should parse");
    let mut object = parsed
        .as_object()
        .expect("theme should serialize to a json object")
        .clone();
    let dimensions = object
        .get_mut("dimensions")
        .and_then(|dimensions| dimensions.as_object_mut())
        .expect("theme should include dimensions");
    dimensions.remove("callout_padding_x");
    dimensions.remove("callout_padding_y");
    dimensions.remove("callout_body_gap");
    dimensions.remove("callout_radius");
    dimensions.remove("callout_border_width");
    dimensions.remove("callout_header_gap");
    dimensions.remove("callout_header_margin_bottom");
    let json = serde_json::to_string(&object).expect("theme json should serialize");

    let theme = Theme::from_json(&json).expect("theme without callout dimensions should load");
    assert_eq!(theme.dimensions.callout_padding_x, 14.0);
    assert_eq!(theme.dimensions.callout_padding_y, 10.0);
    assert_eq!(theme.dimensions.callout_body_gap, 8.0);
    assert_eq!(theme.dimensions.callout_radius, 10.0);
    assert_eq!(theme.dimensions.callout_border_width, 4.0);
    assert_eq!(theme.dimensions.callout_header_gap, 6.0);
    assert_eq!(theme.dimensions.callout_header_margin_bottom, 6.0);
}

#[test]
fn footnote_tokens_fall_back_when_omitted() {
    let default_json = Theme::default_theme()
        .to_json()
        .expect("default theme should serialize");
    let parsed: serde_json::Value =
        serde_json::from_str(&default_json).expect("default theme json should parse");
    let mut object = parsed
        .as_object()
        .expect("theme should serialize to a json object")
        .clone();

    let colors = object
        .get_mut("colors")
        .and_then(|colors| colors.as_object_mut())
        .expect("theme should include colors");
    colors.remove("footnote_bg");
    colors.remove("footnote_border");
    colors.remove("footnote_badge_bg");
    colors.remove("footnote_badge_text");
    colors.remove("footnote_backref");

    let dimensions = object
        .get_mut("dimensions")
        .and_then(|dimensions| dimensions.as_object_mut())
        .expect("theme should include dimensions");
    dimensions.remove("footnote_padding_x");
    dimensions.remove("footnote_padding_y");
    dimensions.remove("footnote_radius");
    dimensions.remove("footnote_badge_padding_x");
    dimensions.remove("footnote_badge_padding_y");

    let json = serde_json::to_string(&object).expect("theme json should serialize");
    let theme = Theme::from_json(&json).expect("theme without footnote tokens should load");

    assert_eq!(theme.colors.footnote_bg, rgba(0x212124ff).into());
    assert_eq!(theme.colors.footnote_border, rgba(0x71717a52).into());
    assert_eq!(theme.colors.footnote_badge_bg, rgba(0xa1a1aa24).into());
    assert_eq!(theme.colors.footnote_badge_text, rgba(0xd4d4d8cc).into());
    assert_eq!(theme.colors.footnote_backref, rgba(0xa1a1aaff).into());
    assert_eq!(theme.dimensions.footnote_padding_x, 10.0);
    assert_eq!(theme.dimensions.footnote_padding_y, 6.0);
    assert_eq!(theme.dimensions.footnote_radius, 6.0);
    assert_eq!(theme.dimensions.footnote_badge_padding_x, 4.0);
    assert_eq!(theme.dimensions.footnote_badge_padding_y, 1.0);
}

#[test]
fn code_language_palette_tokens_fall_back_when_omitted() {
    let default_json = Theme::default_theme()
        .to_json()
        .expect("default theme should serialize");
    let parsed: serde_json::Value =
        serde_json::from_str(&default_json).expect("default theme json should parse");
    let mut object = parsed
        .as_object()
        .expect("theme should serialize to a json object")
        .clone();

    let colors = object
        .get_mut("colors")
        .and_then(|colors| colors.as_object_mut())
        .expect("theme should include colors");
    colors.remove("code_bg");
    colors.remove("code_language_input_bg");
    colors.remove("code_language_input_border");
    colors.remove("code_language_input_text");
    colors.remove("code_language_input_placeholder");

    let json = serde_json::to_string(&object).expect("theme json should serialize");
    let theme = Theme::from_json(&json).expect("theme without code language palette should load");

    assert_eq!(theme.colors.code_bg, rgba(0x111827ff).into());
    assert_eq!(theme.colors.code_language_input_bg, rgba(0x343941ff).into());
    assert_eq!(
        theme.colors.code_language_input_border,
        rgba(0x4b5563cc).into()
    );
    assert_eq!(
        theme.colors.code_language_input_text,
        rgba(0xe5e7ebff).into()
    );
    assert_eq!(
        theme.colors.code_language_input_placeholder,
        rgba(0x9ca3afcc).into()
    );
}

#[test]
fn important_callout_defaults_use_purple_palette() {
    let theme = Theme::default_theme();
    assert_eq!(theme.colors.callout_important_bg, rgba(0xa78bfa1f).into());
    assert_eq!(
        theme.colors.callout_important_border,
        rgba(0xa78bfaff).into()
    );
    assert_eq!(theme.dimensions.block_gap, 6.0);
    assert_eq!(theme.colors.footnote_bg, rgba(0x212124ff).into());
    assert_eq!(theme.dimensions.footnote_padding_x, 10.0);
    assert_eq!(theme.colors.code_bg, rgba(0x23272eff).into());
    assert_eq!(theme.colors.code_language_input_bg, rgba(0x343941ff).into());
    assert_eq!(
        theme.colors.code_language_input_border,
        rgba(0x4b5563cc).into()
    );
}

#[test]
fn light_theme_uses_light_palette_without_changing_layout_tokens() {
    let dark = Theme::default_theme();
    let light = Theme::light_theme();

    assert_eq!(light.name, "gmark Light");
    assert_eq!(light.colors.editor_background, rgba(0xffffffff).into());
    assert_eq!(light.colors.text_default, rgba(0x1d1d1fff).into());
    assert_eq!(light.colors.text_link, rgba(0x0a66c2ff).into());
    assert_eq!(light.colors.chrome_background, rgba(0xf6f6f7ff).into());
    assert_eq!(light.colors.sidebar_background, rgba(0xf5f5f7ff).into());
    assert_eq!(light.colors.tab_strip_background, rgba(0xf2f2f4ff).into());
    assert_eq!(light.colors.tab_active_background, rgba(0xffffffff).into());
    assert_ne!(light.colors.chrome_background, light.colors.dialog_surface);
    assert_eq!(light.colors.code_bg, rgba(0xf5f5f7ff).into());
    assert_eq!(
        light.colors.code_language_input_border,
        rgba(0xd2d2d7ff).into()
    );
    assert_eq!(
        light.colors.table_cell_active_outline,
        rgba(0x0a66c2ff).into()
    );
    assert_eq!(light.dimensions.block_gap, dark.dimensions.block_gap);
    assert_eq!(light.typography.text_size, dark.typography.text_size);
}

#[test]
fn legacy_theme_derives_missing_chrome_tokens_from_existing_surfaces() {
    let default_json = Theme::default_theme()
        .to_json()
        .expect("default theme should serialize");
    let mut value: serde_json::Value =
        serde_json::from_str(&default_json).expect("default theme json should parse");
    let colors = value
        .get_mut("colors")
        .and_then(|colors| colors.as_object_mut())
        .expect("theme should include colors");
    for token in [
        "chrome_background",
        "chrome_hover",
        "sidebar_background",
        "tab_strip_background",
        "tab_active_background",
    ] {
        colors.remove(token);
    }

    let theme = Theme::from_json(&value.to_string()).expect("legacy theme should load");
    assert_eq!(theme.colors.chrome_background, theme.colors.dialog_surface);
    assert_eq!(
        theme.colors.chrome_hover,
        theme.colors.dialog_secondary_button_hover
    );
    assert_eq!(theme.colors.sidebar_background, theme.colors.dialog_surface);
    assert_eq!(
        theme.colors.tab_strip_background,
        theme.colors.dialog_surface
    );
    assert_eq!(
        theme.colors.tab_active_background,
        theme.colors.editor_background
    );
}

#[test]
fn menu_dimension_tokens_fall_back_when_omitted() {
    let default_json = Theme::default_theme()
        .to_json()
        .expect("default theme should serialize");
    let parsed: serde_json::Value =
        serde_json::from_str(&default_json).expect("default theme json should parse");
    let mut object = parsed
        .as_object()
        .expect("theme should serialize to a json object")
        .clone();

    let dimensions = object
        .get_mut("dimensions")
        .and_then(|dimensions| dimensions.as_object_mut())
        .expect("theme should include dimensions");
    dimensions.remove("menu_bar_height");
    dimensions.remove("menu_item_height");
    dimensions.remove("context_menu_panel_width");
    dimensions.remove("table_insert_dialog_width");
    dimensions.remove("view_mode_toggle_min_width");
    dimensions.remove("view_mode_toggle_text_size");

    let json = serde_json::to_string(&object).expect("theme json should serialize");
    let theme = Theme::from_json(&json).expect("theme without menu tokens should load");

    assert_eq!(theme.dimensions.menu_bar_height, 32.0);
    assert_eq!(theme.dimensions.menu_item_height, 28.0);
    assert_eq!(theme.dimensions.context_menu_panel_width, 132.0);
    assert_eq!(theme.dimensions.table_insert_dialog_width, 380.0);
    assert_eq!(theme.dimensions.view_mode_toggle_min_width, 88.0);
    assert_eq!(theme.dimensions.view_mode_toggle_text_size, 11.0);
}

#[test]
fn imports_partial_jsonc_theme_and_persists_normalized_json() {
    let root = std::env::temp_dir().join(format!("gmark-theme-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let source = root.join("theme.jsonc");
    std::fs::write(
        &source,
        r#"{
                // Required metadata.
                "name": "Night Writer",
                "creator": "Ada",
                "description": "",
                "theme": {
                    "dimensions": {
                        "block_gap": 12.0,
                        "menu_text_size": null
                    },
                    "placeholders": {
                        "empty_editing": ""
                    }
                }
            }"#,
    )
    .expect("theme config should be written");

    let dirs = GmarkConfigDirs::from_root(&root);
    let mut manager = ThemeManager::default();
    let imported_id = manager
        .import_theme_config_with_dirs(&source, &dirs)
        .expect("theme config should import");

    assert_eq!(manager.current_theme_id(), imported_id);
    assert_eq!(manager.current().name, "Night Writer");
    assert_eq!(
        manager.current().colors.editor_background,
        Theme::default_theme().colors.editor_background
    );
    assert_eq!(manager.current().dimensions.block_gap, 12.0);
    assert_eq!(manager.current().dimensions.menu_text_size, 12.0);
    assert!(
        manager
            .available_themes()
            .iter()
            .any(|entry| { entry.id == imported_id && entry.name == "Night Writer - Ada" })
    );

    let normalized = std::fs::read_to_string(dirs.themes_dir().join("Night_Writer_Ada.json"))
        .expect("normalized theme config should exist");
    assert!(normalized.contains("\"name\": \"Night Writer\""));
    assert!(normalized.contains("\"creator\": \"Ada\""));
    assert!(normalized.contains("\"base_theme_id\": \"gmark\""));
    assert!(normalized.contains("\"block_gap\": 12.0"));
    assert!(!normalized.contains("menu_text_size"));
    assert!(!normalized.contains("empty_editing"));
    assert!(!normalized.contains("description"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn custom_theme_pack_can_inherit_light_base() {
    let value = serde_json::json!({
        "name": "Day Writer",
        "creator": "Ada",
        "base_theme_id": "gmark-light",
        "theme": {
            "dimensions": {
                "menu_panel_radius": 12.0
            },
            "colors": {
                "text_link": null
            }
        }
    });

    let (entry, normalized) = super::custom_theme_from_value(value).expect("theme should import");
    let light = Theme::light_theme();

    assert_eq!(entry.base_theme_id, "gmark-light");
    assert_eq!(
        entry.theme.colors.editor_background,
        light.colors.editor_background
    );
    assert_eq!(entry.theme.colors.text_default, light.colors.text_default);
    assert_eq!(entry.theme.colors.text_link, light.colors.text_link);
    assert_eq!(entry.theme.dimensions.menu_panel_radius, 12.0);
    assert_eq!(
        normalized
            .get("base_theme_id")
            .and_then(|value| value.as_str()),
        Some("gmark-light")
    );
    assert!(
        normalized
            .pointer("/theme/colors")
            .and_then(|value| value.as_object())
            .map(|colors| !colors.contains_key("text_link"))
            .unwrap_or(true)
    );
}

#[test]
fn invalid_custom_theme_base_falls_back_to_dark() {
    let value = serde_json::json!({
        "name": "Broken Base",
        "creator": "Ada",
        "base_theme_id": "missing",
        "theme": {
            "dimensions": {
                "block_gap": 10.0
            }
        }
    });

    let (entry, normalized) =
        super::custom_theme_from_value(value).expect("invalid base should not fail import");

    assert_eq!(entry.base_theme_id, "gmark");
    assert_eq!(
        entry.theme.colors.editor_background,
        Theme::default_theme().colors.editor_background
    );
    assert_eq!(
        normalized
            .get("base_theme_id")
            .and_then(|value| value.as_str()),
        Some("gmark")
    );
}

#[test]
fn importing_without_base_uses_current_builtin_theme_as_base() {
    let root = std::env::temp_dir().join(format!("gmark-light-theme-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let source = root.join("theme.jsonc");
    std::fs::write(
        &source,
        r#"{
                "name": "Light Radius",
                "creator": "Ada",
                "theme": {
                    "dimensions": {
                        "menu_panel_radius": 14.0
                    }
                }
            }"#,
    )
    .expect("theme config should be written");

    let dirs = GmarkConfigDirs::from_root(&root);
    let mut manager = ThemeManager::default();
    assert!(manager.set_theme_by_id("gmark-light"));
    let imported_id = manager
        .import_theme_config_with_dirs(&source, &dirs)
        .expect("theme config should import");

    assert_eq!(manager.current_theme_id(), imported_id);
    assert_eq!(
        manager.current().colors.editor_background,
        Theme::light_theme().colors.editor_background
    );
    assert_eq!(manager.current().dimensions.menu_panel_radius, 14.0);

    let normalized = std::fs::read_to_string(dirs.themes_dir().join("Light_Radius_Ada.json"))
        .expect("normalized theme config should exist");
    assert!(normalized.contains("\"base_theme_id\": \"gmark-light\""));

    let mut reloaded = ThemeManager::default();
    reloaded
        .load_custom_themes_from_dirs(&dirs)
        .expect("saved theme should reload");
    assert!(reloaded.set_theme_by_id(&imported_id));
    assert_eq!(
        reloaded.current().colors.editor_background,
        Theme::light_theme().colors.editor_background
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn theme_manager_switches_builtin_themes() {
    let mut manager = ThemeManager::default();
    assert_eq!(manager.current_theme_id(), "gmark");
    assert_eq!(manager.current().name, "gmark");
    assert_eq!(
        manager
            .available_themes()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        vec!["gmark", "gmark Light"]
    );

    assert!(manager.set_theme_by_id("gmark-light"));
    assert_eq!(manager.current_theme_id(), "gmark-light");
    assert_eq!(manager.current().name, "gmark Light");
    assert_eq!(
        manager.current().colors.editor_background,
        rgba(0xffffffff).into()
    );

    assert!(manager.set_theme_by_id("gmark"));
    assert_eq!(manager.current_theme_id(), "gmark");
    assert_eq!(manager.current().name, "gmark");
    assert!(!manager.set_theme_by_id("missing"));
}

#[test]
fn system_theme_resolves_all_platform_appearance_variants() {
    assert_eq!(
        resolved_system_theme_id(WindowAppearance::Light),
        "gmark-light"
    );
    assert_eq!(
        resolved_system_theme_id(WindowAppearance::VibrantLight),
        "gmark-light"
    );
    assert_eq!(resolved_system_theme_id(WindowAppearance::Dark), "gmark");
    assert_eq!(
        resolved_system_theme_id(WindowAppearance::VibrantDark),
        "gmark"
    );
}

#[test]
fn system_theme_keeps_mode_identity_and_editor_overrides() {
    let mut manager = ThemeManager::default();
    manager.set_editor_typography(20, 175);
    manager.set_editor_content_width(1040);

    assert!(manager.set_theme_preference(SYSTEM_THEME_ID, WindowAppearance::Light));
    assert_eq!(manager.selected_theme_id(), SYSTEM_THEME_ID);
    assert_eq!(manager.current_theme_id(), "gmark-light");
    assert_eq!(manager.current().typography.text_size, 20.0);
    assert_eq!(manager.current().typography.text_line_height, 1.75);
    assert_eq!(manager.current().dimensions.centered_max_width, 1040.0);

    assert!(manager.update_system_appearance(WindowAppearance::VibrantDark));
    assert_eq!(manager.selected_theme_id(), SYSTEM_THEME_ID);
    assert_eq!(manager.current_theme_id(), "gmark");
    assert_eq!(manager.current().typography.text_size, 20.0);
    assert_eq!(manager.current().typography.text_line_height, 1.75);
    assert_eq!(manager.current().dimensions.centered_max_width, 1040.0);
    assert!(!manager.update_system_appearance(WindowAppearance::Dark));
}

#[test]
fn editor_typography_override_scales_document_text_without_chrome() {
    let mut manager = ThemeManager::default();
    let base = manager.current().clone();
    manager.set_editor_typography(20, 175);

    let adjusted = manager.current();
    let scale = 20.0 / base.typography.text_size;
    assert_eq!(adjusted.typography.text_size, 20.0);
    assert_eq!(adjusted.typography.text_line_height, 1.75);
    assert!((adjusted.typography.h1_size - base.typography.h1_size * scale).abs() < 0.001);
    assert!((adjusted.typography.code_size - base.typography.code_size * scale).abs() < 0.001);
    assert_eq!(
        adjusted.typography.dialog_body_size,
        base.typography.dialog_body_size
    );
    assert_eq!(
        adjusted.dimensions.status_bar_text_size,
        base.dimensions.status_bar_text_size
    );
}
