// @author kongweiguang

use super::{
    INTEGRATED_MENU_LEFT, MENU_ICON_SLOT, MENU_SHORTCUT_SLOT, NoRecentFiles,
    RenderedRowSpacingInfo, callout_row_top_gap, clamped_floating_panel_origin,
    clamped_split_pane_ratio, compact_menu_panel_height, editor_tab_strip_insets,
    editor_text_font_for_family, floating_submenu_x, import_menu_split_index,
    in_window_menu_chrome_layout, menu_bar_button_width, menu_icon_slot,
    menu_items_visual_height_with_gaps, menu_panel_left, menu_panel_width_for_labels,
    menu_shortcut_slot, menu_shortcut_text, owned_menu_item_labels, rendered_row_top_gap,
    scrollable_import_menu_scroll_height, submenu_bridge_geometry,
    supports_in_window_menu_for_target_os, tibetan_font_fallbacks_for_target_os,
    top_level_menu_button_width, visible_menu_button_count,
};
use crate::components::{AddLanguageConfig, AddThemeConfig, SaveDocument};
use crate::config::WorkspaceSidebarPosition;
use crate::theme::Theme;
use gpui::{
    Context, InteractiveElement, IntoElement, KeyBinding, OwnedMenu, OwnedMenuItem, ParentElement,
    Render, Styled, Window, div, hsla,
};
use uuid::Uuid;

#[test]
fn split_pane_ratio_keeps_both_panes_above_minimum_width() {
    assert_eq!(clamped_split_pane_ratio(0.1, 1_000.0), 0.3);
    assert_eq!(clamped_split_pane_ratio(0.9, 1_000.0), 0.7);
    assert_eq!(clamped_split_pane_ratio(0.2, 700.0), 0.4);
    assert_eq!(clamped_split_pane_ratio(0.8, 700.0), 0.6);
    assert_eq!(clamped_split_pane_ratio(0.1, 500.0), 0.5);
}

#[test]
fn docked_workspace_only_insets_the_editor_tab_strip() {
    assert_eq!(
        editor_tab_strip_insets(WorkspaceSidebarPosition::Left, 248.0),
        (248.0, 0.0)
    );
    assert_eq!(
        editor_tab_strip_insets(WorkspaceSidebarPosition::Right, 248.0),
        (0.0, 248.0)
    );
    assert_eq!(
        editor_tab_strip_insets(WorkspaceSidebarPosition::Left, 0.0),
        (0.0, 0.0)
    );
}

#[test]
fn in_window_menu_keeps_navigation_visible_when_launcher_is_closed() {
    assert_eq!(visible_menu_button_count(false, 7), 7);
    assert_eq!(visible_menu_button_count(true, 7), 7);
    assert_eq!(visible_menu_button_count(false, 0), 0);
}

struct MenuIconSlotTestView;

impl Render for MenuIconSlotTestView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::default_theme();
        div()
            .flex()
            .items_center()
            .child(
                menu_icon_slot(None, hsla(0.0, 0.0, 1.0, 1.0))
                    .debug_selector(|| "menu-icon-slot-test".to_owned()),
            )
            .child(
                menu_shortcut_slot("ctrl-alt-s".to_owned(), &theme)
                    .debug_selector(|| "menu-shortcut-slot-test".to_owned()),
            )
    }
}

#[gpui::test]
async fn fallback_menu_icon_slot_has_stable_two_x_bounds(cx: &mut gpui::TestAppContext) {
    let (_view, visual) = cx.add_window_view(|_window, _cx| MenuIconSlotTestView);
    visual.update(|window, cx| window.draw(cx).clear());
    let slot = visual.debug_bounds("menu-icon-slot-test").unwrap();
    assert_eq!(f32::from(slot.size.width), MENU_ICON_SLOT);
    assert_eq!(f32::from(slot.size.height), MENU_ICON_SLOT);
    let shortcut = visual.debug_bounds("menu-shortcut-slot-test").unwrap();
    assert!(f32::from(shortcut.size.width) >= MENU_SHORTCUT_SLOT);
    assert!(shortcut.left() >= slot.right());
    visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
}

#[gpui::test]
async fn fallback_menu_shortcut_uses_highest_precedence_window_keymap(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("ctrl-alt-s", SaveDocument, None)]);
    });
    let (_view, visual) = cx.add_window_view(|_window, _cx| MenuIconSlotTestView);
    visual.update(|window, _cx| {
        assert_eq!(
            menu_shortcut_text(window, &SaveDocument).as_deref(),
            Some("ctrl-alt-S")
        );
        assert_eq!(menu_shortcut_text(window, &NoRecentFiles), None);
    });
}

fn disabled_menu_action(name: &str) -> OwnedMenuItem {
    OwnedMenuItem::Action {
        name: name.into(),
        action: Box::new(NoRecentFiles),
        os_action: None,
    }
}

fn add_theme_menu_action() -> OwnedMenuItem {
    OwnedMenuItem::Action {
        name: "Add Theme Config".into(),
        action: Box::new(AddThemeConfig),
        os_action: None,
    }
}

fn add_language_menu_action() -> OwnedMenuItem {
    OwnedMenuItem::Action {
        name: "Add Language Config".into(),
        action: Box::new(AddLanguageConfig),
        os_action: None,
    }
}

#[test]
fn contiguous_quote_rows_collapse_inter_row_gap() {
    let group = Uuid::new_v4();
    let gap = rendered_row_top_gap(
        Some(RenderedRowSpacingInfo {
            quote_group_anchor: Some(group),
            ..RenderedRowSpacingInfo::default()
        }),
        RenderedRowSpacingInfo {
            quote_group_anchor: Some(group),
            ..RenderedRowSpacingInfo::default()
        },
        4.0,
    );
    assert_eq!(gap, 0.0);
}

#[test]
fn editor_text_font_keeps_system_ui_as_primary_family() {
    assert_eq!(
        editor_text_font_for_family("").family.to_string(),
        ".SystemUIFont"
    );
    assert_eq!(
        editor_text_font_for_family("Georgia").family.to_string(),
        "Georgia"
    );
}

#[test]
fn tibetan_font_fallbacks_prioritize_platform_defaults() {
    assert_eq!(
        tibetan_font_fallbacks_for_target_os("windows")
            .first()
            .map(String::as_str),
        Some("Microsoft Himalaya")
    );
    assert_eq!(
        tibetan_font_fallbacks_for_target_os("macos")
            .first()
            .map(String::as_str),
        Some("Kailasa")
    );
    assert_eq!(
        tibetan_font_fallbacks_for_target_os("linux")
            .first()
            .map(String::as_str),
        Some("Noto Serif Tibetan")
    );
    assert_eq!(
        tibetan_font_fallbacks_for_target_os("unknown")
            .first()
            .map(String::as_str),
        Some("Noto Serif Tibetan")
    );
}

#[test]
fn nested_quote_separator_row_keeps_outer_group_gap_collapsed() {
    let group = Uuid::new_v4();
    let gap = rendered_row_top_gap(
        Some(RenderedRowSpacingInfo {
            quote_group_anchor: Some(group),
            ..RenderedRowSpacingInfo::default()
        }),
        RenderedRowSpacingInfo {
            quote_group_anchor: Some(group),
            ..RenderedRowSpacingInfo::default()
        },
        4.0,
    );
    assert_eq!(gap, 0.0);
}

#[test]
fn distinct_quote_groups_keep_default_gap() {
    let gap = rendered_row_top_gap(
        Some(RenderedRowSpacingInfo {
            quote_group_anchor: Some(Uuid::new_v4()),
            ..RenderedRowSpacingInfo::default()
        }),
        RenderedRowSpacingInfo {
            quote_group_anchor: Some(Uuid::new_v4()),
            ..RenderedRowSpacingInfo::default()
        },
        4.0,
    );
    assert_eq!(gap, 4.0);
}

#[test]
fn non_quote_rows_keep_default_gap() {
    let gap = rendered_row_top_gap(
        Some(RenderedRowSpacingInfo {
            quote_group_anchor: None,
            ..RenderedRowSpacingInfo::default()
        }),
        RenderedRowSpacingInfo {
            quote_group_anchor: Some(Uuid::new_v4()),
            ..RenderedRowSpacingInfo::default()
        },
        4.0,
    );
    assert_eq!(gap, 4.0);
}

#[test]
fn callout_inner_spacing_uses_header_and_body_tokens() {
    let theme = Theme::default_theme();
    let dimensions = &theme.dimensions;

    let header_gap = callout_row_top_gap(
        Some(RenderedRowSpacingInfo {
            is_callout_header: true,
            ..RenderedRowSpacingInfo::default()
        }),
        RenderedRowSpacingInfo::default(),
        dimensions,
    );
    let body_gap = callout_row_top_gap(
        Some(RenderedRowSpacingInfo {
            is_callout_header: false,
            ..RenderedRowSpacingInfo::default()
        }),
        RenderedRowSpacingInfo::default(),
        dimensions,
    );

    assert_eq!(header_gap, dimensions.callout_header_margin_bottom);
    assert_eq!(body_gap, dimensions.callout_body_gap);
}

#[test]
fn nested_quote_rows_inside_callout_collapse_body_gap() {
    let theme = Theme::default_theme();
    let dimensions = &theme.dimensions;
    let group = Uuid::new_v4();

    let gap = callout_row_top_gap(
        Some(RenderedRowSpacingInfo {
            is_callout_header: false,
            visible_quote_group_anchor: Some(group),
            ..RenderedRowSpacingInfo::default()
        }),
        RenderedRowSpacingInfo {
            visible_quote_group_anchor: Some(group),
            ..RenderedRowSpacingInfo::default()
        },
        dimensions,
    );

    assert_eq!(gap, 0.0);
}

#[test]
fn menu_button_width_expands_for_long_ascii_labels() {
    let theme = Theme::default_theme();
    let dimensions = &theme.dimensions;

    assert_eq!(
        menu_bar_button_width("文件", dimensions),
        dimensions.menu_bar_button_width
    );
    assert!(menu_bar_button_width("Language", dimensions) > dimensions.menu_bar_button_width);
    assert_eq!(
        top_level_menu_button_width(0, "gmark", dimensions),
        dimensions.status_bar_height
    );
}

#[test]
fn in_window_menu_is_enabled_for_every_target_except_macos() {
    for target_os in [
        "windows",
        "linux",
        "freebsd",
        "openbsd",
        "netbsd",
        "dragonfly",
        "solaris",
        "illumos",
        "android",
        "unknown",
    ] {
        assert!(
            supports_in_window_menu_for_target_os(target_os),
            "{target_os} should use the in-window fallback menu"
        );
    }
    assert!(!supports_in_window_menu_for_target_os("macos"));
}

#[test]
fn windows_menu_reuses_the_client_titlebar_row() {
    let theme = Theme::default_theme();
    let dimensions = &theme.dimensions;
    let titlebar_height = 38.0;

    let layout = in_window_menu_chrome_layout("windows", true, false, titlebar_height, dimensions);

    assert!(layout.integrated);
    assert_eq!(layout.content_height, 0.0);
    assert_eq!(layout.bar_top, 0.0);
    assert_eq!(layout.bar_height, titlebar_height);
    assert_eq!(layout.origin_x, INTEGRATED_MENU_LEFT);
    assert_eq!(
        layout.panel_top_offset + dimensions.menu_panel_top,
        titlebar_height
    );
}

#[test]
fn linux_server_decorations_keep_a_separate_menu_row() {
    let theme = Theme::default_theme();
    let dimensions = &theme.dimensions;

    let layout = in_window_menu_chrome_layout("linux", true, false, 0.0, dimensions);

    assert!(!layout.integrated);
    assert_eq!(layout.content_height, dimensions.menu_bar_height);
    assert_eq!(layout.bar_top, 0.0);
    assert_eq!(layout.bar_height, dimensions.menu_bar_height);
    assert_eq!(layout.origin_x, 0.0);
    assert_eq!(layout.panel_top_offset, 0.0);
}

#[test]
fn macos_and_focus_mode_do_not_allocate_an_in_window_menu_row() {
    let theme = Theme::default_theme();
    let dimensions = &theme.dimensions;

    let macos = in_window_menu_chrome_layout("macos", true, false, 38.0, dimensions);
    let focus = in_window_menu_chrome_layout("windows", true, true, 38.0, dimensions);

    for layout in [macos, focus] {
        assert!(!layout.integrated);
        assert_eq!(layout.content_height, 0.0);
        assert_eq!(layout.bar_height, 0.0);
        assert_eq!(layout.origin_x, 0.0);
    }
}

#[test]
fn menu_panel_left_uses_accumulated_dynamic_button_widths() {
    let theme = Theme::default_theme();
    let dimensions = &theme.dimensions;
    let labels = vec![
        "File".to_string(),
        "Language".to_string(),
        "Theme".to_string(),
        "Help".to_string(),
    ];

    let left = menu_panel_left(2, &labels, dimensions);
    let expected = dimensions.menu_bar_padding_x
        + menu_bar_button_width("File", dimensions)
        + dimensions.menu_bar_gap
        + menu_bar_button_width("Language", dimensions)
        + dimensions.menu_bar_gap;
    let old_fixed_left = dimensions.menu_bar_padding_x
        + 2.0 * (dimensions.menu_bar_button_width + dimensions.menu_bar_gap);

    assert_eq!(left, expected);
    assert!(left > old_fixed_left);
}

#[test]
fn menu_panel_width_expands_for_long_recent_paths() {
    let theme = Theme::default_theme();
    let dimensions = &theme.dimensions;
    let short_labels = vec!["Save".to_string()];
    let long_labels = vec![r"C:\Users\someone\Documents\Very Long Folder\notes.md".to_string()];

    assert_eq!(
        menu_panel_width_for_labels(&short_labels, dimensions),
        dimensions.menu_panel_width
    );
    assert!(menu_panel_width_for_labels(&long_labels, dimensions) > dimensions.menu_panel_width);
}

#[test]
fn floating_context_menu_clamps_and_flips_submenu_inside_minimum_viewport() {
    let theme = Theme::default_theme();
    let dimensions = &theme.dimensions;
    let viewport = gpui::size(gpui::px(720.0), gpui::px(520.0));
    let panel_height = compact_menu_panel_height(5, 2, dimensions);
    let origin = clamped_floating_panel_origin(
        gpui::point(gpui::px(710.0), gpui::px(510.0)),
        220.0,
        panel_height,
        viewport,
    );
    assert_eq!(f32::from(origin.x), 492.0);
    assert!(f32::from(origin.y) >= 8.0);
    assert!(f32::from(origin.y) + panel_height <= 512.0);

    let submenu_x = floating_submenu_x(origin.x, 220.0, 148.0, 2.0, viewport.width);
    assert_eq!(f32::from(submenu_x), 342.0);
    assert!(submenu_x + gpui::px(148.0) <= origin.x);
}

#[test]
fn import_menu_split_detects_theme_and_language_import_tails() {
    let theme_items = vec![
        disabled_menu_action("gmark"),
        OwnedMenuItem::Separator,
        add_theme_menu_action(),
    ];
    let language_items = vec![
        disabled_menu_action("English"),
        OwnedMenuItem::Separator,
        add_language_menu_action(),
    ];
    let regular_items = vec![
        disabled_menu_action("Open"),
        OwnedMenuItem::Separator,
        disabled_menu_action("Save"),
    ];
    let malformed_import_items = vec![disabled_menu_action("gmark"), add_theme_menu_action()];

    assert_eq!(import_menu_split_index(&theme_items), Some(1));
    assert_eq!(import_menu_split_index(&language_items), Some(1));
    assert_eq!(import_menu_split_index(&regular_items), None);
    assert_eq!(import_menu_split_index(&malformed_import_items), None);
}

#[test]
fn scrollable_import_menu_height_caps_visible_items_and_clamps_to_viewport() {
    let theme = Theme::default_theme();
    let dimensions = &theme.dimensions;
    let scroll_items = (0..20)
        .map(|index| disabled_menu_action(&format!("Custom Theme {index}")))
        .collect::<Vec<_>>();
    let footer_items = vec![OwnedMenuItem::Separator, add_theme_menu_action()];
    let expected_large_height = menu_items_visual_height_with_gaps(&scroll_items[..12], dimensions);
    let full_scroll_content_height = menu_items_visual_height_with_gaps(&scroll_items, dimensions);
    let footer_height = menu_items_visual_height_with_gaps(&footer_items, dimensions);

    let large_height =
        scrollable_import_menu_scroll_height(&scroll_items, &footer_items, 2000.0, 0.0, dimensions);
    let small_height =
        scrollable_import_menu_scroll_height(&scroll_items, &footer_items, 180.0, 0.0, dimensions);

    assert!((large_height - expected_large_height).abs() < f32::EPSILON);
    assert!(full_scroll_content_height > large_height);
    assert!(large_height < expected_large_height + footer_height);
    assert!(small_height < large_height);
    assert!(small_height >= dimensions.menu_item_height);
}

#[test]
fn submenu_bridge_spans_parent_child_menu_gap() {
    let theme = Theme::default_theme();
    let dimensions = &theme.dimensions;
    let labels = vec!["File".to_string()];
    let items = vec![
        OwnedMenuItem::Separator,
        OwnedMenuItem::Submenu(OwnedMenu {
            name: "Recent".into(),
            items: vec![OwnedMenuItem::Action {
                name: r"C:\Users\someone\Documents\notes.md".into(),
                action: Box::new(NoRecentFiles),
                os_action: None,
            }],
        }),
    ];
    let submenu_labels = match &items[1] {
        OwnedMenuItem::Submenu(submenu) => owned_menu_item_labels(&submenu.items),
        _ => Vec::new(),
    };

    let bridge = submenu_bridge_geometry(0, &labels, &items, 1, &submenu_labels, dimensions)
        .expect("submenu bridge geometry should be available");
    let submenu_width = menu_panel_width_for_labels(&submenu_labels, dimensions);

    assert_eq!(
        bridge.left,
        dimensions.menu_bar_padding_x + dimensions.menu_panel_width
    );
    assert_eq!(bridge.width, dimensions.menu_panel_gap + submenu_width);
    assert!(bridge.height > dimensions.menu_item_height);
    let item_top = dimensions.menu_panel_top
        + dimensions.menu_panel_padding
        + dimensions.menu_separator_height
        + dimensions.menu_separator_margin_y * 2.0
        + dimensions.menu_panel_gap;
    assert!(bridge.top < item_top);
    assert!(bridge.top >= dimensions.menu_panel_top);
}

#[test]
fn submenu_bridge_uses_dynamic_main_menu_width() {
    let theme = Theme::default_theme();
    let dimensions = &theme.dimensions;
    let labels = vec!["File".to_string()];
    let items = vec![OwnedMenuItem::Submenu(OwnedMenu {
        name: "Open Recently Used Markdown File".into(),
        items: vec![OwnedMenuItem::Action {
            name: r"C:\Users\someone\Documents\Very Long Folder\notes.md".into(),
            action: Box::new(NoRecentFiles),
            os_action: None,
        }],
    })];
    let submenu_labels = match &items[0] {
        OwnedMenuItem::Submenu(submenu) => owned_menu_item_labels(&submenu.items),
        _ => Vec::new(),
    };

    let bridge = submenu_bridge_geometry(0, &labels, &items, 0, &submenu_labels, dimensions)
        .expect("submenu bridge geometry should be available");

    assert!(bridge.left > dimensions.menu_bar_padding_x + dimensions.menu_panel_width);
    assert!(bridge.width > dimensions.menu_panel_gap + dimensions.menu_panel_width);
}
