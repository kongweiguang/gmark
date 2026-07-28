// @author kongweiguang

use super::{Theme, ThemeAppearance, ThemeManager, ThemePalette, resolved_theme_id};
use gpui::{WindowAppearance, rgba};

#[test]
fn built_in_themes_have_stable_names_and_distinct_surfaces() {
    let xcode_dark = Theme::xcode_dark();
    let xcode_light = Theme::xcode_light();
    let jetbrains_dark = Theme::jetbrains_dark();
    let jetbrains_light = Theme::jetbrains_light();
    let obsidian_dark = Theme::obsidian_dark();
    let obsidian_light = Theme::obsidian_light();

    assert_eq!(xcode_dark.name, "Xcode Dark");
    assert_eq!(xcode_light.name, "Xcode Light");
    assert_eq!(jetbrains_dark.name, "JetBrains Dark");
    assert_eq!(jetbrains_light.name, "JetBrains Light");
    assert_eq!(obsidian_dark.name, "Obsidian Dark");
    assert_eq!(obsidian_light.name, "Obsidian Light");
    assert_ne!(
        xcode_dark.colors.editor_background,
        xcode_light.colors.editor_background
    );
    assert_ne!(
        xcode_dark.colors.editor_background,
        jetbrains_dark.colors.editor_background
    );
    assert_ne!(
        xcode_light.colors.editor_background,
        jetbrains_light.colors.editor_background
    );
    assert_ne!(
        xcode_dark.colors.code_syntax_keyword,
        jetbrains_dark.colors.code_syntax_keyword
    );
    assert_ne!(
        xcode_dark.colors.editor_background,
        obsidian_dark.colors.editor_background
    );
    assert_ne!(
        xcode_light.colors.editor_background,
        obsidian_light.colors.editor_background
    );
}

#[test]
fn all_user_combinations_resolve_to_the_six_concrete_ids() {
    let cases = [
        (
            ThemeAppearance::Dark,
            ThemePalette::Xcode,
            WindowAppearance::Dark,
            "xcode-dark",
        ),
        (
            ThemeAppearance::Light,
            ThemePalette::Xcode,
            WindowAppearance::Dark,
            "xcode-light",
        ),
        (
            ThemeAppearance::System,
            ThemePalette::Xcode,
            WindowAppearance::Dark,
            "xcode-dark",
        ),
        (
            ThemeAppearance::System,
            ThemePalette::Xcode,
            WindowAppearance::Light,
            "xcode-light",
        ),
        (
            ThemeAppearance::Dark,
            ThemePalette::JetBrains,
            WindowAppearance::Light,
            "jetbrains-dark",
        ),
        (
            ThemeAppearance::Light,
            ThemePalette::JetBrains,
            WindowAppearance::Dark,
            "jetbrains-light",
        ),
        (
            ThemeAppearance::System,
            ThemePalette::JetBrains,
            WindowAppearance::Dark,
            "jetbrains-dark",
        ),
        (
            ThemeAppearance::System,
            ThemePalette::JetBrains,
            WindowAppearance::Light,
            "jetbrains-light",
        ),
        (
            ThemeAppearance::Dark,
            ThemePalette::Obsidian,
            WindowAppearance::Light,
            "obsidian-dark",
        ),
        (
            ThemeAppearance::Light,
            ThemePalette::Obsidian,
            WindowAppearance::Dark,
            "obsidian-light",
        ),
        (
            ThemeAppearance::System,
            ThemePalette::Obsidian,
            WindowAppearance::Dark,
            "obsidian-dark",
        ),
        (
            ThemeAppearance::System,
            ThemePalette::Obsidian,
            WindowAppearance::Light,
            "obsidian-light",
        ),
    ];

    for (appearance, palette, platform, expected) in cases {
        assert_eq!(resolved_theme_id(appearance, palette, platform), expected);
    }
}

#[test]
fn invalid_theme_values_are_not_accepted() {
    assert_eq!(ThemeAppearance::parse("dark"), Some(ThemeAppearance::Dark));
    assert_eq!(
        ThemeAppearance::parse("light"),
        Some(ThemeAppearance::Light)
    );
    assert_eq!(
        ThemeAppearance::parse("system"),
        Some(ThemeAppearance::System)
    );
    assert_eq!(ThemeAppearance::parse("gmark"), None);
    assert_eq!(ThemePalette::parse("xcode"), Some(ThemePalette::Xcode));
    assert_eq!(
        ThemePalette::parse("jetbrains"),
        Some(ThemePalette::JetBrains)
    );
    assert_eq!(
        ThemePalette::parse("obsidian"),
        Some(ThemePalette::Obsidian)
    );
    assert_eq!(ThemePalette::parse("custom:paper"), None);
}

#[test]
fn manager_preserves_palette_when_appearance_changes() {
    let mut manager = ThemeManager::default();
    assert!(manager.set_theme_preference(
        ThemeAppearance::Dark,
        ThemePalette::JetBrains,
        WindowAppearance::Light,
    ));
    assert_eq!(manager.current_theme_id(), "jetbrains-dark");
    assert_eq!(manager.selected_appearance(), ThemeAppearance::Dark);
    assert_eq!(manager.selected_palette(), ThemePalette::JetBrains);

    assert!(manager.set_theme_preference(
        ThemeAppearance::Light,
        ThemePalette::JetBrains,
        WindowAppearance::Light,
    ));
    assert_eq!(manager.current_theme_id(), "jetbrains-light");
    assert_eq!(manager.selected_palette(), ThemePalette::JetBrains);
}

#[test]
fn manager_preserves_appearance_when_palette_changes() {
    let mut manager = ThemeManager::default();
    manager.set_theme_preference(
        ThemeAppearance::Light,
        ThemePalette::Xcode,
        WindowAppearance::Dark,
    );

    assert!(manager.set_theme_preference(
        ThemeAppearance::Light,
        ThemePalette::JetBrains,
        WindowAppearance::Dark,
    ));
    assert_eq!(manager.current_theme_id(), "jetbrains-light");
    assert_eq!(manager.selected_appearance(), ThemeAppearance::Light);
    assert_eq!(manager.selected_palette(), ThemePalette::JetBrains);
}

#[test]
fn system_updates_only_when_system_appearance_is_selected() {
    let mut manager = ThemeManager::default();
    manager.set_theme_preference(
        ThemeAppearance::System,
        ThemePalette::JetBrains,
        WindowAppearance::Dark,
    );
    assert_eq!(manager.current_theme_id(), "jetbrains-dark");
    assert!(manager.update_system_appearance(WindowAppearance::Light));
    assert_eq!(manager.current_theme_id(), "jetbrains-light");

    manager.set_theme_preference(
        ThemeAppearance::Dark,
        ThemePalette::JetBrains,
        WindowAppearance::Light,
    );
    assert!(!manager.update_system_appearance(WindowAppearance::Dark));
    assert_eq!(manager.current_theme_id(), "jetbrains-dark");
}

#[test]
fn editor_overrides_survive_theme_changes() {
    let mut manager = ThemeManager::default();
    manager.set_editor_typography(20, 180);
    manager.set_editor_content_width(1_400);
    manager.set_theme_preference(
        ThemeAppearance::Light,
        ThemePalette::Xcode,
        WindowAppearance::Dark,
    );

    assert_eq!(manager.current().typography.text_size, 20.0);
    assert_eq!(manager.current().typography.text_line_height, 1.8);
    assert_eq!(manager.current().dimensions.centered_max_width, 1_400.0);
    assert_eq!(manager.current().colors.cursor, rgba(0x1d1d1fff).into());

    assert!(!manager.set_theme_preference(
        ThemeAppearance::Light,
        ThemePalette::Xcode,
        WindowAppearance::Dark,
    ));
    assert_eq!(manager.current().typography.text_size, 20.0);
    assert_eq!(manager.current().dimensions.centered_max_width, 1_400.0);
}
