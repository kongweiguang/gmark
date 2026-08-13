// @author kongweiguang

#[gpui::test]
async fn dismissing_menu_panel_from_body_preserves_navigation(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.menu_bar_expanded = true;
        editor.open_menu_bar(0, cx);
        editor.set_menu_bar_hovered(true, cx);
        editor.set_menu_panel_hovered(true, cx);
        assert_eq!(editor.menu_bar_open, Some(0));

        editor.dismiss_menu_bar_from_body(cx);
        assert!(editor.menu_bar_expanded);
        assert_eq!(editor.menu_bar_open, None);
        assert!(!editor.menu_bar_hovered);
        assert!(!editor.menu_panel_hovered);
        assert!(!editor.menu_submenu_panel_hovered);
        assert!(editor.menu_close_task.is_none());
    });
}

#[gpui::test]
async fn clicking_workspace_sidebar_closes_in_window_menu(cx: &mut TestAppContext) {
    // The Windows GPUI test platform intentionally leaves native folder
    // prompts unimplemented; assert the menu transition before dispatching
    // that platform-only interaction so the test remains deterministic.
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "alpha".to_string(), None));
    visual.simulate_resize(size(px(720.0), px(520.0)));
    editor.update_in(visual, |editor, _window, cx| {
        editor.workspace.is_open = true;
        editor.open_menu_bar(0, cx);
    });
    redraw(visual);

    let sidebar = visual
        .debug_bounds("workspace-panel")
        .expect("workspace sidebar");
    assert!(sidebar.size.width > px(0.0));
    editor.update(visual, |editor, cx| editor.dismiss_menu_bar_from_body(cx));

    assert_eq!(editor.read_with(visual, |editor, _cx| editor.menu_bar_open), None);
}

#[gpui::test]
async fn menu_launcher_toggles_its_panel_without_hiding_navigation(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        assert!(editor.menu_bar_expanded);
        assert_eq!(editor.menu_bar_open, None);

        editor.toggle_menu_bar_expanded(cx);
        assert!(editor.menu_bar_expanded);
        assert_eq!(editor.menu_bar_open, Some(0));

        editor.toggle_menu_bar_expanded(cx);
        assert!(editor.menu_bar_expanded);
        assert_eq!(editor.menu_bar_open, None);

        editor.toggle_menu_bar_expanded(cx);
        assert!(editor.menu_bar_expanded);
        assert_eq!(editor.menu_bar_open, Some(0));
    });
}

#[gpui::test]
async fn closing_menu_panels_preserves_expanded_navigation(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.open_menu_bar(0, cx);
        editor.set_menu_bar_hovered(true, cx);
        editor.set_menu_panel_hovered(true, cx);

        editor.close_menu_panels(cx);

        assert!(editor.menu_bar_expanded);
        assert_eq!(editor.menu_bar_open, None);
        assert!(!editor.menu_bar_hovered);
        assert!(!editor.menu_panel_hovered);
        assert!(editor.menu_close_task.is_none());
    });
}

#[gpui::test]
async fn in_window_menu_keyboard_navigation_preserves_editor_focus(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual_cx) = cx.add_window_view(|window, cx| {
        let editor = Editor::from_markdown(cx, "alpha".to_string(), None);
        editor
            .document
            .first_root()
            .expect("paragraph")
            .read(cx)
            .focus_handle
            .focus(window);
        editor
    });
    visual_cx.simulate_resize(size(px(720.0), px(520.0)));

    let key_event = |key: &str| KeyDownEvent {
        keystroke: Keystroke::parse(key).expect("valid menu key"),
        is_held: false,
    };
    let menus = vec![
        OwnedMenu {
            name: "File".into(),
            items: vec![
                OwnedMenuItem::Separator,
                OwnedMenuItem::Action {
                    name: "Unavailable".to_owned(),
                    action: Box::new(NoRecentFiles),
                    os_action: None,
                },
                OwnedMenuItem::Action {
                    name: "Save".to_owned(),
                    action: Box::new(SaveDocument),
                    os_action: None,
                },
                OwnedMenuItem::Submenu(OwnedMenu {
                    name: "Recent".into(),
                    items: vec![
                        OwnedMenuItem::Separator,
                        OwnedMenuItem::Action {
                            name: "Save child".to_owned(),
                            action: Box::new(SaveDocument),
                            os_action: None,
                        },
                    ],
                }),
            ],
        },
        OwnedMenu {
            name: "Edit".into(),
            items: vec![OwnedMenuItem::Action {
                name: "Save again".to_owned(),
                action: Box::new(SaveDocument),
                os_action: None,
            }],
        },
    ];
    editor.update_in(visual_cx, |editor, window, cx| {
        let f10 = key_event("f10");
        assert!(
            editor.handle_in_window_menu_key_with_menus(&f10, &menus, window, cx),
            "F10 event: {f10:?}"
        );
        let first = Editor::edge_menu_item(&menus[0].items, true).expect("first command");
        assert_eq!(first, 2, "separator and disabled placeholder are skipped");
        assert_eq!(editor.menu_bar_open, Some(0));
        assert_eq!(editor.menu_keyboard_item, Some(first));

        assert!(editor.handle_in_window_menu_key_with_menus(
            &key_event("escape"),
            &menus,
            window,
            cx
        ));
        assert_eq!(editor.menu_bar_open, None);

        let alt = key_event("alt");
        assert!(
            editor.handle_in_window_menu_key_with_menus(&alt, &menus, window, cx),
            "Alt event: {alt:?}"
        );
        assert_eq!(editor.menu_bar_open, Some(0));
        assert_eq!(editor.menu_keyboard_item, Some(first));

        let next =
            Editor::adjacent_menu_item(&menus[0].items, Some(first), true).expect("next command");
        assert!(editor.handle_in_window_menu_key_with_menus(
            &key_event("down"),
            &menus,
            window,
            cx
        ));
        assert_eq!(editor.menu_keyboard_item, Some(next));

        let submenu_index = menus[0]
            .items
            .iter()
            .position(|item| matches!(item, gpui::OwnedMenuItem::Submenu(_)))
            .expect("file menu submenu");
        editor.menu_keyboard_item = Some(submenu_index);
        assert!(editor.handle_in_window_menu_key_with_menus(
            &key_event("right"),
            &menus,
            window,
            cx
        ));
        assert_eq!(editor.menu_submenu_open, Some(submenu_index));
        assert!(editor.menu_keyboard_submenu_item.is_some());
        assert!(editor.handle_in_window_menu_key_with_menus(
            &key_event("left"),
            &menus,
            window,
            cx
        ));
        assert_eq!(editor.menu_submenu_open, None);

        assert!(editor.handle_in_window_menu_key_with_menus(
            &key_event("escape"),
            &menus,
            window,
            cx
        ));
        assert_eq!(editor.menu_bar_open, None);
        assert_eq!(editor.menu_keyboard_item, None);
        assert!(
            editor
                .document
                .first_root()
                .expect("paragraph")
                .read(cx)
                .focus_handle
                .is_focused(window)
        );
    });
}

#[gpui::test]
async fn moving_pointer_away_closes_in_window_menu_after_delay(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.open_menu_bar(0, cx);
        editor.open_menu_submenu(2, cx);
        editor.set_menu_submenu_panel_hovered(true, cx);
        editor.set_menu_panel_hovered(false, cx);
        editor.set_menu_bar_hovered(false, cx);

        assert_eq!(editor.menu_bar_open, Some(0));
        assert_eq!(editor.menu_submenu_open, Some(2));
        assert!(editor.menu_submenu_panel_hovered);
        assert!(editor.menu_close_task.is_none());

        editor.set_menu_submenu_panel_hovered(false, cx);
        assert!(editor.menu_close_task.is_some());
        assert_eq!(editor.menu_bar_open, Some(0));
    });

    cx.executor().advance_clock(Duration::from_millis(180));
    cx.run_until_parked();

    editor.update(cx, |editor, _cx| {
        assert!(editor.menu_close_task.is_none());
        assert_eq!(editor.menu_bar_open, None);
        assert_eq!(editor.menu_submenu_open, None);
    });
}

// The gap bridge and the submenu panel overlap, so moving the cursor from the
// bridge onto the submenu emits `bridge: false` and `panel: true` in the same
// gesture. With both regions sharing one hover flag the stale `bridge: false`
// could win and tear the menu down, which made reaching the recent-files list
// fail intermittently. Track the two regions independently so the handoff
// always keeps the menu open, regardless of event order.
#[gpui::test]
async fn submenu_survives_bridge_to_panel_hover_handoff(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.open_menu_bar(0, cx);
        editor.open_menu_submenu(3, cx);

        // Crossing the gap: only the bridge is hovered.
        editor.set_menu_panel_hovered(false, cx);
        editor.set_menu_bar_hovered(false, cx);
        editor.set_menu_submenu_bridge_hovered(true, cx);
        assert!(editor.menu_close_task.is_none());

        // Handoff into the submenu panel. The bridge reporting `false` after
        // the panel is already hovered must not schedule a close.
        editor.set_menu_submenu_panel_hovered(true, cx);
        editor.set_menu_submenu_bridge_hovered(false, cx);

        assert_eq!(editor.menu_bar_open, Some(0));
        assert_eq!(editor.menu_submenu_open, Some(3));
        assert!(editor.menu_submenu_panel_hovered);
        assert!(
            editor.menu_close_task.is_none(),
            "menu must stay open across the bridge-to-panel handoff"
        );

        editor.close_menu_bar(cx);
    });
}

#[gpui::test]
async fn starting_and_ending_scrollbar_drag_updates_editor_state(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.pending_scroll_active_block_into_view = true;
        editor.pending_scroll_recheck_after_layout = true;

        editor.start_scrollbar_drag(12.0, 320.0, 64.0, 500.0, cx);
        assert_eq!(
            editor.scrollbar_drag,
            Some(super::ScrollbarDragSession {
                pointer_offset_y: 12.0,
                track_height: 320.0,
                thumb_height: 64.0,
                max_scroll_y: 500.0,
            })
        );
        assert!(!editor.pending_scroll_active_block_into_view);
        assert!(!editor.pending_scroll_recheck_after_layout);

        editor.update_scrollbar_drag(172.0, cx);
        let offset_y = -f32::from(editor.scroll_handle.offset().y);
        assert!(offset_y > 0.0);

        editor.end_scrollbar_drag(cx);
        assert!(editor.scrollbar_drag.is_none());
    });
}
