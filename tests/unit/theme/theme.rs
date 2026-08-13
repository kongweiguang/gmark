// @author kongweiguang

use super::{Theme, ThemeAppearance, ThemeManager, ThemePalette, resolved_theme_id};
use gpui::{WindowAppearance, rgba};

#[test]
fn built_in_themes_have_stable_names_and_distinct_surfaces() {
    let xcode_dark = Theme::xcode_dark();
    let xcode_light = Theme::xcode_light();
    let fleet_dark = Theme::fleet_dark();
    let fleet_light = Theme::fleet_light();
    let obsidian_dark = Theme::obsidian_dark();
    let obsidian_light = Theme::obsidian_light();
    let claude_dark = Theme::claude_dark();
    let claude_light = Theme::claude_light();

    assert_eq!(xcode_dark.name, "Xcode Dark");
    assert_eq!(xcode_light.name, "Xcode Light");
    assert_eq!(fleet_dark.name, "Fleet Dark");
    assert_eq!(fleet_light.name, "Fleet Light");
    assert_eq!(obsidian_dark.name, "Obsidian Dark");
    assert_eq!(obsidian_light.name, "Obsidian Light");
    assert_eq!(claude_dark.name, "Claude Dark");
    assert_eq!(claude_light.name, "Claude Light");
    assert_ne!(
        xcode_dark.colors.editor_background,
        xcode_light.colors.editor_background
    );
    assert_ne!(
        xcode_dark.colors.editor_background,
        fleet_dark.colors.editor_background
    );
    assert_ne!(
        xcode_light.colors.chrome_background,
        fleet_light.colors.chrome_background
    );
    assert_ne!(
        xcode_dark.colors.code_syntax_keyword,
        fleet_dark.colors.code_syntax_keyword
    );
    assert_ne!(
        xcode_dark.colors.editor_background,
        obsidian_dark.colors.editor_background
    );
    assert_ne!(
        xcode_light.colors.chrome_background,
        obsidian_light.colors.chrome_background
    );
    assert_ne!(
        obsidian_dark.colors.editor_background,
        claude_dark.colors.editor_background
    );
    assert_ne!(
        obsidian_light.colors.editor_background,
        claude_light.colors.editor_background
    );
    assert_ne!(
        xcode_dark.colors.workbench.accent,
        fleet_dark.colors.workbench.accent
    );
    assert_ne!(
        fleet_light.colors.workbench.accent,
        obsidian_light.colors.workbench.accent
    );
    assert_ne!(
        obsidian_dark.colors.workbench.accent,
        claude_dark.colors.workbench.accent
    );
}

/// 浅色主题必须把标签栏底与编辑器表面分层，避免未选中区域和活动内容融合后失去当前边界。
#[test]
fn light_builtin_theme_tab_strip_and_editor_surfaces_remain_distinct() {
    let themes = [
        Theme::xcode_light(),
        Theme::fleet_light(),
        Theme::obsidian_light(),
        Theme::claude_light(),
    ];

    for theme in themes {
        assert_ne!(
            theme.colors.tab_strip_background, theme.colors.workbench.editor_surface,
            "{}: light tab strip and editor surfaces must remain distinct",
            theme.name
        );
    }
}

#[test]
fn built_in_code_themes_keep_operators_and_punctuation_quiet() {
    let themes = [
        Theme::xcode_dark(),
        Theme::xcode_light(),
        Theme::fleet_dark(),
        Theme::fleet_light(),
        Theme::obsidian_dark(),
        Theme::obsidian_light(),
        Theme::claude_dark(),
        Theme::claude_light(),
    ];

    for theme in themes {
        assert_eq!(
            theme.colors.code_syntax_operator, theme.colors.code_syntax_punctuation,
            "operators and punctuation should share a quiet neutral role"
        );
        assert_ne!(
            theme.colors.code_syntax_comment, theme.colors.code_syntax_variable,
            "comments should remain visually secondary to ordinary code"
        );
    }
}

#[test]
fn workbench_materials_respect_transparency_and_contrast_preferences() {
    use super::workbench::{ResolvedVisualPreferences, SurfaceKind};

    let theme = Theme::xcode_dark();
    let normal = theme
        .colors
        .workbench
        .material(SurfaceKind::Glass, ResolvedVisualPreferences::default());
    assert!(normal.is_translucent);

    let accessible = theme.colors.workbench.material(
        SurfaceKind::Glass,
        ResolvedVisualPreferences {
            reduced_transparency: true,
            high_contrast: true,
            ..ResolvedVisualPreferences::default()
        },
    );
    assert!(!accessible.is_translucent);
    assert_eq!(
        accessible.background,
        theme.colors.workbench.elevated_surface
    );
    assert_eq!(accessible.border, theme.colors.workbench.border_strong);
}

#[test]
fn legacy_serialized_theme_derives_workbench_tokens_without_a_new_required_field() {
    let theme = Theme::xcode_dark();
    let mut value = serde_json::to_value(&theme).expect("built-in theme should serialize");
    value
        .get_mut("colors")
        .and_then(serde_json::Value::as_object_mut)
        .expect("serialized theme colors should be an object")
        .remove("workbench");

    let restored: Theme =
        serde_json::from_value(value).expect("legacy theme without workbench should deserialize");
    assert_eq!(
        restored.colors.workbench.editor_surface,
        restored.colors.editor_background
    );
    assert_eq!(
        restored.colors.workbench.accent,
        restored.colors.dialog_primary_button_bg
    );
    assert_eq!(
        restored.colors.workbench.overlay_scrim,
        restored.colors.dialog_backdrop
    );
}

#[test]
fn built_in_themes_keep_the_official_default_palette_anchors() {
    let xcode_dark = Theme::xcode_dark();
    let xcode_light = Theme::xcode_light();
    let fleet_dark = Theme::fleet_dark();
    let fleet_light = Theme::fleet_light();
    let obsidian_dark = Theme::obsidian_dark();
    let obsidian_light = Theme::obsidian_light();
    let claude_dark = Theme::claude_dark();
    let claude_light = Theme::claude_light();

    // Xcode 27 官方深浅色产品图与默认源码主题。
    assert_eq!(xcode_dark.colors.editor_background, rgba(0x292a30ff).into());
    assert_eq!(
        xcode_dark.colors.code_syntax_keyword,
        rgba(0xff7ab2ff).into()
    );
    assert_eq!(
        xcode_light.colors.editor_background,
        rgba(0xffffffff).into()
    );
    assert_eq!(
        xcode_light.colors.code_syntax_keyword,
        rgba(0xad3da4ff).into()
    );

    // Fleet 1.48 官方产品图及主题编辑器中 Fleet Dark/Light 的稳定锚点。
    assert_eq!(fleet_dark.colors.editor_background, rgba(0x18191bff).into());
    assert_eq!(fleet_dark.colors.chrome_background, rgba(0x090909ff).into());
    assert_eq!(
        fleet_dark.colors.code_syntax_keyword,
        rgba(0x82d2ceff).into()
    );
    assert_eq!(
        fleet_light.colors.editor_background,
        rgba(0xffffffff).into()
    );
    assert_eq!(
        fleet_light.colors.chrome_background,
        rgba(0xf2f2f2ff).into()
    );
    assert_eq!(
        fleet_light.colors.code_syntax_keyword,
        rgba(0x07805fff).into()
    );

    // Obsidian 1.12.7 官方 app.css 的 base 与 code token。
    assert_eq!(
        obsidian_dark.colors.editor_background,
        rgba(0x1e1e1eff).into()
    );
    assert_eq!(obsidian_dark.colors.text_default, rgba(0xdadadaff).into());
    assert_eq!(
        obsidian_dark.colors.code_syntax_keyword,
        rgba(0xfa99cdff).into()
    );
    assert_eq!(
        obsidian_light.colors.editor_background,
        rgba(0xffffffff).into()
    );
    assert_eq!(obsidian_light.colors.text_default, rgba(0x222222ff).into());
    assert_eq!(
        obsidian_light.colors.code_syntax_keyword,
        rgba(0xd53984ff).into()
    );

    // claude.com 当前官网 CSS 的 theme-main/theme-dark 与 clay 品牌变量。
    assert_eq!(
        claude_dark.colors.editor_background,
        rgba(0x141413ff).into()
    );
    assert_eq!(claude_dark.colors.text_default, rgba(0xfaf9f5ff).into());
    assert_eq!(claude_dark.colors.text_link, rgba(0xc46849ff).into());
    assert_eq!(
        claude_light.colors.editor_background,
        rgba(0xfaf9f5ff).into()
    );
    assert_eq!(claude_light.colors.text_default, rgba(0x141413ff).into());
    assert_eq!(claude_light.colors.text_link, rgba(0xd97757ff).into());
}

#[test]
fn all_user_combinations_resolve_to_the_eight_concrete_ids() {
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
            ThemePalette::Fleet,
            WindowAppearance::Light,
            "fleet-dark",
        ),
        (
            ThemeAppearance::Light,
            ThemePalette::Fleet,
            WindowAppearance::Dark,
            "fleet-light",
        ),
        (
            ThemeAppearance::System,
            ThemePalette::Fleet,
            WindowAppearance::Dark,
            "fleet-dark",
        ),
        (
            ThemeAppearance::System,
            ThemePalette::Fleet,
            WindowAppearance::Light,
            "fleet-light",
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
        (
            ThemeAppearance::Dark,
            ThemePalette::Claude,
            WindowAppearance::Light,
            "claude-dark",
        ),
        (
            ThemeAppearance::Light,
            ThemePalette::Claude,
            WindowAppearance::Dark,
            "claude-light",
        ),
        (
            ThemeAppearance::System,
            ThemePalette::Claude,
            WindowAppearance::Dark,
            "claude-dark",
        ),
        (
            ThemeAppearance::System,
            ThemePalette::Claude,
            WindowAppearance::Light,
            "claude-light",
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
    assert_eq!(ThemePalette::parse("fleet"), Some(ThemePalette::Fleet));
    assert_eq!(ThemePalette::Fleet.as_str(), "fleet");
    assert_eq!(ThemePalette::parse("jetbrains"), None);
    assert_eq!(
        ThemePalette::parse("obsidian"),
        Some(ThemePalette::Obsidian)
    );
    assert_eq!(ThemePalette::parse("claude"), Some(ThemePalette::Claude));
    assert_eq!(ThemePalette::Claude.as_str(), "claude");
    assert_eq!(ThemePalette::parse("custom:paper"), None);
}

#[test]
fn manager_preserves_palette_when_appearance_changes() {
    let mut manager = ThemeManager::default();
    assert!(manager.set_theme_preference(
        ThemeAppearance::Dark,
        ThemePalette::Fleet,
        WindowAppearance::Light,
    ));
    assert_eq!(manager.current_theme_id(), "fleet-dark");
    assert_eq!(manager.selected_appearance(), ThemeAppearance::Dark);
    assert_eq!(manager.selected_palette(), ThemePalette::Fleet);

    assert!(manager.set_theme_preference(
        ThemeAppearance::Light,
        ThemePalette::Fleet,
        WindowAppearance::Light,
    ));
    assert_eq!(manager.current_theme_id(), "fleet-light");
    assert_eq!(manager.selected_palette(), ThemePalette::Fleet);
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
        ThemePalette::Fleet,
        WindowAppearance::Dark,
    ));
    assert_eq!(manager.current_theme_id(), "fleet-light");
    assert_eq!(manager.selected_appearance(), ThemeAppearance::Light);
    assert_eq!(manager.selected_palette(), ThemePalette::Fleet);
}

#[test]
fn system_updates_only_when_system_appearance_is_selected() {
    let mut manager = ThemeManager::default();
    manager.set_theme_preference(
        ThemeAppearance::System,
        ThemePalette::Fleet,
        WindowAppearance::Dark,
    );
    assert_eq!(manager.current_theme_id(), "fleet-dark");
    assert!(manager.update_system_appearance(WindowAppearance::Light));
    assert_eq!(manager.current_theme_id(), "fleet-light");

    manager.set_theme_preference(
        ThemeAppearance::Dark,
        ThemePalette::Fleet,
        WindowAppearance::Light,
    );
    assert!(!manager.update_system_appearance(WindowAppearance::Dark));
    assert_eq!(manager.current_theme_id(), "fleet-dark");
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
    assert_eq!(manager.current().colors.cursor, rgba(0x000000ff).into());

    assert!(!manager.set_theme_preference(
        ThemeAppearance::Light,
        ThemePalette::Xcode,
        WindowAppearance::Dark,
    ));
    assert_eq!(manager.current().typography.text_size, 20.0);
    assert_eq!(manager.current().dimensions.centered_max_width, 1_400.0);
}
