// @author kongweiguang

use super::*;
use gpui::Tiling;

#[test]
fn titlebar_options_enable_transparency_on_mac_and_windows() {
    assert!(titlebar_options_for_target_os("windows", "gmark".into()).appears_transparent);
    assert!(titlebar_options_for_target_os("macos", "gmark".into()).appears_transparent);
    assert!(!titlebar_options_for_target_os("linux", "gmark".into()).appears_transparent);
}

#[test]
fn linux_and_freebsd_request_client_decorations() {
    assert_eq!(
        window_decorations_for_target_os("linux"),
        Some(WindowDecorations::Client)
    );
    assert_eq!(
        window_decorations_for_target_os("freebsd"),
        Some(WindowDecorations::Client)
    );
    assert_eq!(window_decorations_for_target_os("unknown"), None);
}

#[test]
fn custom_titlebar_height_respects_platform_and_decorations() {
    let dimensions = Theme::default_theme().dimensions;
    assert_eq!(
        custom_titlebar_height_for_target_os("windows", Decorations::Server, &dimensions),
        38.0
    );
    assert_eq!(
        custom_titlebar_height_for_target_os("windows", Decorations::Server, &dimensions),
        dimensions.menu_bar_height.max(TITLEBAR_MIN_HEIGHT)
    );
    assert_eq!(
        custom_titlebar_height_for_target_os(
            "linux",
            Decorations::Client {
                tiling: Tiling::default()
            },
            &dimensions,
        ),
        dimensions.menu_bar_height.max(TITLEBAR_MIN_HEIGHT)
    );
    assert_eq!(
        custom_titlebar_height_for_target_os("linux", Decorations::Server, &dimensions),
        0.0
    );
    assert_eq!(
        custom_titlebar_height_for_target_os("unknown", Decorations::Server, &dimensions),
        0.0
    );
}

#[test]
fn titlebar_drag_strategy_matches_platform_window_api() {
    assert_eq!(
        titlebar_drag_strategy_for_target_os("windows", Decorations::Server),
        TitlebarDragStrategy::PlatformHitTest
    );
    assert_eq!(
        titlebar_drag_strategy_for_target_os("macos", Decorations::Server),
        TitlebarDragStrategy::PlatformHitTest
    );
    assert_eq!(
        titlebar_drag_strategy_for_target_os(
            "linux",
            Decorations::Client {
                tiling: Tiling::default()
            },
        ),
        TitlebarDragStrategy::ExplicitMoveRequest
    );
    assert_eq!(
        titlebar_drag_strategy_for_target_os("linux", Decorations::Server),
        TitlebarDragStrategy::PlatformHitTest
    );
}

#[test]
fn custom_titlebar_background_uses_dedicated_chrome_token() {
    let theme = Theme::light_theme();
    assert_eq!(
        custom_titlebar_background(&theme),
        theme.colors.chrome_background
    );
}

#[test]
fn custom_titlebar_icon_color_contrasts_with_theme_surface() {
    assert_eq!(
        custom_titlebar_icon_color(&Theme::default_theme()),
        Hsla::from(rgba(0xf4f4f5ff))
    );
    assert_eq!(
        custom_titlebar_icon_color(&Theme::light_theme()),
        Hsla::from(rgba(0x18181bff))
    );
}

#[test]
fn titlebar_maximize_icon_tracks_window_state() {
    assert_eq!(titlebar_maximize_icon(false, false), TITLEBAR_MAXIMIZE_ICON);
    assert_eq!(titlebar_maximize_icon(true, false), TITLEBAR_RESTORE_ICON);
    assert_eq!(titlebar_maximize_icon(false, true), TITLEBAR_RESTORE_ICON);
}

#[test]
fn middle_ellipsis_is_unicode_safe_and_preserves_filename_suffix() {
    assert_eq!(middle_ellipsis("short.md", 20), "short.md");
    assert_eq!(middle_ellipsis("abcdefghij.markdown", 12), "abcdef…kdown");
    assert_eq!(
        middle_ellipsis("设计文档最终生产版本.markdown", 11),
        "设计文档最…kdown"
    );
    assert_eq!(middle_ellipsis("abc", 1), "…");
    assert_eq!(middle_ellipsis("abc", 0), "");
}

#[test]
fn restored_window_is_clamped_to_current_display() {
    let display = Bounds::new(point(px(100.0), px(50.0)), size(px(1920.0), px(1080.0)));
    let offscreen = Bounds::new(point(px(-5000.0), px(9000.0)), size(px(2600.0), px(200.0)));
    let restored = clamp_window_to_display(offscreen, display);
    assert_eq!(restored.origin, point(px(100.0), px(610.0)));
    assert_eq!(restored.size, size(px(1920.0), px(520.0)));
}

#[test]
fn restored_window_keeps_valid_position_and_size() {
    let display = Bounds::new(point(px(0.0), px(0.0)), size(px(2560.0), px(1440.0)));
    let saved = Bounds::new(point(px(240.0), px(120.0)), size(px(1180.0), px(780.0)));
    assert_eq!(clamp_window_to_display(saved, display), saved);
}
