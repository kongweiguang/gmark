// @author kongweiguang

#[gpui::test]
async fn tab_context_menu_renders_stable_commands(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let (editor, visual) = cx
        .add_window_view(|_window, cx| super::Editor::from_markdown(cx, "first".to_owned(), None));
    visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
    editor.update(visual, |editor, cx| {
        add_inactive_tab(editor, "second", "second.md");
        editor.tabs.context_menu = Some(super::TabContextMenu {
            index: 0,
            position: gpui::point(gpui::px(710.0), gpui::px(510.0)),
        });
        cx.notify();
    });
    visual.update(|window, cx| window.draw(cx).clear());
    let strip = visual.debug_bounds("document-tab-strip").unwrap();
    for (tab_selector, close_selector) in [
        ("document-tab-0", "document-tab-close-0"),
        ("document-tab-1", "document-tab-close-1"),
    ] {
        let tab = visual.debug_bounds(tab_selector).unwrap();
        let close = visual.debug_bounds(close_selector).unwrap();
        assert!(tab.left() >= strip.left());
        assert!(tab.right() <= strip.right());
        assert!(f32::from(tab.size.width) <= super::TAB_MAX_WIDTH);
        assert!(close.left() >= tab.left());
        assert!(close.right() <= tab.right());
    }
    for index in 0..2 {
        let leading = visual
            .debug_bounds(match index {
                0 => "document-tab-leading-0",
                _ => "document-tab-leading-1",
            })
            .unwrap();
        let title = visual
            .debug_bounds(match index {
                0 => "document-tab-title-0",
                _ => "document-tab-title-1",
            })
            .unwrap();
        let close = visual
            .debug_bounds(match index {
                0 => "document-tab-close-0",
                _ => "document-tab-close-1",
            })
            .unwrap();
        assert_eq!(leading.size, size(px(16.0), px(16.0)));
        assert!(title.left() > leading.right());
        assert!(title.right() <= close.left());
    }
    let close = visual.debug_bounds("document-tab-close-0").unwrap();
    let dirty = visual.debug_bounds("document-tab-dirty-0").unwrap();
    let close_icon = visual.debug_bounds("document-tab-close-icon-0").unwrap();
    assert_eq!(f32::from(close.size.width), 18.0);
    assert_eq!(f32::from(close.size.height), 18.0);
    assert!(dirty.left() >= close.left());
    assert!(dirty.right() <= close.right());
    assert!(close_icon.left() >= close.left());
    assert!(close_icon.right() <= close.right());
    let menu = visual.debug_bounds("tab-context-menu").unwrap();
    assert!(f32::from(menu.left()) >= 8.0);
    assert!(f32::from(menu.top()) >= 8.0);
    assert!(f32::from(menu.right()) <= 712.0);
    assert!(f32::from(menu.bottom()) <= 512.0);
    assert!(visual.debug_bounds("tab-context-pin").is_some());
    assert!(visual.debug_bounds("tab-context-close").is_some());
    assert!(visual.debug_bounds("tab-context-close-others").is_some());
    for selector in [
        "tab-context-pin-icon",
        "tab-context-close-icon",
        "tab-context-close-others-icon",
    ] {
        let icon = visual.debug_bounds(selector).unwrap();
        assert_eq!(f32::from(icon.size.width), 18.0, "{selector}");
        assert_eq!(f32::from(icon.size.height), 18.0, "{selector}");
    }
    visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
}

#[gpui::test]
async fn tab_icon_controls_show_compact_native_tooltips(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let (_editor, visual) = cx
        .add_window_view(|_window, cx| super::Editor::from_markdown(cx, "first".to_owned(), None));
    visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
    visual.update(|window, cx| window.draw(cx).clear());
    let new_tab = visual.debug_bounds("document-new-tab").unwrap();
    assert_eq!(new_tab.size.width, gpui::px(28.0));
    assert_eq!(new_tab.size.height, gpui::px(28.0));

    visual.simulate_mouse_move(new_tab.center(), None, gpui::Modifiers::default());
    visual.executor().advance_clock(Duration::from_millis(520));
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
    let tooltip = visual.debug_bounds("ui-tooltip").unwrap();
    assert!(tooltip.size.width <= gpui::px(280.0));
    assert!(tooltip.size.height <= gpui::px(32.0));
    assert!(tooltip.left() >= gpui::px(0.0));
    assert!(tooltip.top() >= gpui::px(0.0));
    assert!(tooltip.right() <= gpui::px(720.0));
    assert!(tooltip.bottom() <= gpui::px(520.0));
    visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
}

#[gpui::test]
async fn tab_context_menu_keyboard_skips_disabled_close_others(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        super::Editor::from_markdown(cx, "only tab".to_owned(), None)
    });
    let key = |name: &str| gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse(name).expect("valid menu key"),
        is_held: false,
    };
    editor.update_in(visual, |editor, window, cx| {
        editor.tabs.context_menu = Some(super::TabContextMenu {
            index: 0,
            position: gpui::point(gpui::px(40.0), gpui::px(40.0)),
        });
        assert!(editor.handle_context_menu_key(&key("down"), window, cx));
        assert_eq!(editor.context_menu_keyboard_item, Some(0));
        editor.context_menu_keyboard_item = Some(1);
        assert!(editor.handle_context_menu_key(&key("down"), window, cx));
        assert_eq!(
            editor.context_menu_keyboard_item,
            Some(0),
            "Close Others is disabled for a single tab"
        );
        assert!(editor.handle_context_menu_key(&key("escape"), window, cx));
        assert!(editor.tabs.context_menu.is_none());
    });
}

#[gpui::test]
async fn tab_close_dialog_uses_standard_compact_layout(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let (editor, visual) = cx
        .add_window_view(|_window, cx| super::Editor::from_markdown(cx, "dirty".to_owned(), None));
    visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
    editor.update(visual, |editor, cx| {
        editor.tabs.show_close_dialog = true;
        cx.notify();
    });
    visual.update(|window, cx| window.draw(cx).clear());

    let overlay = visual.debug_bounds("tab-close-dialog-overlay").unwrap();
    let dialog = visual.debug_bounds("tab-close-dialog").unwrap();
    let title_icon = visual.debug_bounds("tab-close-title-icon").unwrap();
    let title_label = visual.debug_bounds("tab-close-title-label").unwrap();
    assert_eq!(title_icon.size, gpui::size(gpui::px(22.0), gpui::px(22.0)));
    assert!(title_icon.left() >= dialog.left());
    assert!(title_label.left() > title_icon.right());
    assert!(title_label.right() <= dialog.right());
    assert!(dialog.left() >= overlay.left());
    assert!(dialog.right() <= overlay.right());
    assert!(dialog.top() >= overlay.top());
    assert!(dialog.bottom() <= overlay.bottom());
    for selector in ["cancel-tab-close", "discard-tab-close", "save-tab-close"] {
        let action = visual.debug_bounds(selector).unwrap();
        assert!(action.left() >= dialog.left(), "{selector}");
        assert!(action.right() <= dialog.right(), "{selector}");
        assert!(f32::from(action.size.width) >= 72.0, "{selector}");
        assert_eq!(f32::from(action.size.height), 36.0, "{selector}");
    }
    visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
}
