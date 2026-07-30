// @author kongweiguang

#[gpui::test]
async fn split_workspace_uses_compact_overlay_at_two_x_scale(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    cx.update(|cx| {
        crate::config::EditorSettings::init(cx, true, crate::config::AutoSavePreference::Off, true);
        crate::config::EditorSettings::set_status_bar_preferences_for_test(
            cx,
            crate::preferences::StatusBarPreferences {
                custom_buttons: vec![crate::preferences::StatusBarButton {
                    id: "mode".into(),
                    label: "Mode".into(),
                    action_id: "toggle_view_mode".into(),
                }],
                ..crate::preferences::StatusBarPreferences::default()
            },
        );
    });
    let (editor, visual_cx) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "# Heading\n\nBody\n".to_owned(), None)
    });
    editor.update(visual_cx, |editor, cx| {
        editor.workspace.is_open = true;
        editor.set_view_mode(ViewMode::Split, cx);
    });

    for (viewport, compact) in [
        (size(px(1180.0), px(780.0)), false),
        (size(px(820.0), px(620.0)), true),
        (size(px(720.0), px(520.0)), true),
    ] {
        visual_cx.simulate_resize(viewport);
        redraw(visual_cx);
        if compact && !editor.read_with(visual_cx, |editor, _cx| editor.workspace.is_open) {
            editor.update(visual_cx, |editor, cx| {
                editor.workspace.is_open = true;
                cx.notify();
            });
            redraw(visual_cx);
        }
        visual_cx.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));

        let main = visual_cx.debug_bounds("editor-main-content").unwrap();
        let titlebar = visual_cx.debug_bounds("editor-titlebar");
        let status_bar = visual_cx.debug_bounds("status-bar").unwrap();
        let content = visual_cx.debug_bounds("editor-content").unwrap();
        let workspace = visual_cx.debug_bounds("workspace-panel").unwrap();
        let source = visual_cx.debug_bounds("editor-source-pane").unwrap();
        let preview = visual_cx.debug_bounds("split-preview-pane").unwrap();
        let mode_switch = visual_cx.debug_bounds("status-bar-mode-switch").unwrap();
        let line_ending_picker = visual_cx
            .debug_bounds("status-bar-line-ending-button")
            .unwrap();
        let document_sidebar_toggle = visual_cx
            .debug_bounds("status-bar-document-sidebar-toggle")
            .unwrap();
        let sidebar_toggle = visual_cx.debug_bounds("status-bar-sidebar-toggle").unwrap();
        let sidebar_open = editor.read_with(visual_cx, |editor, _cx| editor.workspace.is_open);
        assert!(visual_cx.debug_bounds("workspace-collapse").is_none());
        assert!(
            visual_cx
                .debug_bounds("document-tab-leading-tools")
                .is_none()
        );
        if let Some(titlebar) = titlebar {
            assert_eq!(f32::from(titlebar.size.height), 38.0);
            assert!(titlebar.bottom() <= content.top());
            assert!(titlebar.bottom() <= workspace.top());
            assert!(
                visual_cx
                    .debug_bounds("editor-titlebar-title-label")
                    .is_none()
            );
            assert!(
                visual_cx
                    .debug_bounds("editor-titlebar-leading-icon")
                    .is_none()
            );
        }
        assert_eq!(f32::from(status_bar.size.height), 24.0);
        assert_eq!(sidebar_toggle.left(), status_bar.left());
        assert_eq!(document_sidebar_toggle.right(), status_bar.right());
        assert!(mode_switch.right() <= document_sidebar_toggle.left());
        assert_eq!(mode_switch.size, size(px(24.0), px(24.0)));
        assert!(line_ending_picker.right() <= mode_switch.left());
        assert!(f32::from(mode_switch.left() - line_ending_picker.right()) <= 2.0);
        assert!(visual_cx.debug_bounds("status-bar-mode-menu").is_none());
        assert!(visual_cx.debug_bounds("status-bar-mode-Split").is_none());
        if sidebar_open {
            let sidebar_indicator = visual_cx
                .debug_bounds("status-bar-sidebar-indicator")
                .unwrap();
            assert!(sidebar_indicator.left() >= sidebar_toggle.left());
            assert!(sidebar_indicator.right() <= sidebar_toggle.right());
            assert!(sidebar_indicator.bottom() <= sidebar_toggle.bottom());
            assert!(
                f32::from(sidebar_toggle.bottom() - sidebar_indicator.bottom()) <= 1.0,
                "sidebar indicator must stay visible inside the status-bar border"
            );
        }
        if f32::from(viewport.width) >= 760.0 {
            for selector in [
                "status-bar-word-count",
                "status-bar-cursor",
                "status-bar-custom-button-mode",
            ] {
                let metadata = visual_cx.debug_bounds(selector).unwrap();
                assert!(
                    metadata.right() <= mode_switch.left(),
                    "{selector} must precede the mode switch"
                );
                assert!(
                    metadata.left() >= status_bar.center().x,
                    "{selector} must stay in the right status group"
                );
            }
        }
        assert!(sidebar_toggle.right() <= mode_switch.left());
        if f32::from(viewport.width) >= 900.0 {
            assert!(
                visual_cx
                    .debug_bounds("status-bar-format-overflow-button")
                    .is_none()
            );
        } else {
            assert!(
                visual_cx
                    .debug_bounds("status-bar-format-overflow-button")
                    .is_some()
            );
        }
        assert!(content.left() >= main.left());
        assert!(content.right() <= main.right());
        assert!(source.left() >= content.left());
        assert!(preview.right() <= content.right());
        assert!(source.right() <= preview.left());

        for selector in [
            "workspace-tab-files",
            "workspace-tab-search",
        ] {
            let control = visual_cx.debug_bounds(selector).unwrap();
            assert_eq!(f32::from(control.size.width), 32.0, "{selector}");
            assert_eq!(f32::from(control.size.height), 32.0, "{selector}");
            assert!(control.left() >= workspace.left(), "{selector}");
            assert!(control.right() <= workspace.right(), "{selector}");
        }
        for selector in [
            "status-bar-sidebar-toggle",
            "status-bar-mode-switch",
            "status-bar-document-sidebar-toggle",
        ] {
            let control = visual_cx.debug_bounds(selector).unwrap();
            assert_eq!(f32::from(control.size.width), 24.0, "{selector}");
            assert_eq!(f32::from(control.size.height), 24.0, "{selector}");
            assert!(f32::from(control.left()) >= 0.0, "{selector}");
            assert!(
                f32::from(control.right()) <= f32::from(viewport.width),
                "{selector}"
            );
        }
        let overlay = visual_cx.debug_bounds("compact-workspace-overlay");
        if compact {
            let overlay = overlay.expect("compact workspace should render as overlay");
            assert_eq!(f32::from(overlay.size.width), 280.0);
            assert_eq!(content.left(), main.left());
            assert_eq!(content.right(), main.right());
            assert!(workspace.left() >= overlay.left());
            assert!(workspace.right() <= overlay.right());
            assert!(overlay.right() <= main.right());
        } else {
            assert!(
                overlay.is_none(),
                "stale compact overlay at viewport={viewport:?}, main={main:?}, overlay={overlay:?}"
            );
            assert!(workspace.right() <= content.left());
            assert_eq!(f32::from(workspace.size.width), 248.0);
        }
    }

    editor.update(visual_cx, |editor, cx| {
        editor.set_status_sidebar_tooltip_hover(true, cx);
    });
    visual_cx
        .executor()
        .advance_clock(Duration::from_millis(499));
    visual_cx.run_until_parked();
    redraw(visual_cx);
    assert!(
        visual_cx
            .debug_bounds("status-bar-sidebar-tooltip")
            .is_none()
    );
    visual_cx.executor().advance_clock(Duration::from_millis(1));
    visual_cx.run_until_parked();
    redraw(visual_cx);
    let status_tooltip = visual_cx
        .debug_bounds("status-bar-sidebar-tooltip")
        .unwrap();
    let main = visual_cx.debug_bounds("editor-main-content").unwrap();
    assert!(status_tooltip.left() >= main.left());
    assert!(status_tooltip.right() <= main.right());
    editor.update(visual_cx, |editor, cx| {
        editor.set_status_sidebar_tooltip_hover(false, cx);
    });

    editor.update(visual_cx, |editor, cx| {
        editor.set_status_mode_tooltip_hover(ViewMode::Split, true, cx);
    });
    visual_cx
        .executor()
        .advance_clock(Duration::from_millis(500));
    visual_cx.run_until_parked();
    redraw(visual_cx);
    let mode_switch = visual_cx.debug_bounds("status-bar-mode-switch").unwrap();
    let mode_tooltip = visual_cx.debug_bounds("status-bar-mode-tooltip").unwrap();
    let status_bar = visual_cx.debug_bounds("status-bar").unwrap();
    assert!(mode_tooltip.left() <= mode_switch.center().x);
    assert!(mode_tooltip.right() >= mode_switch.center().x);
    assert!(mode_tooltip.right() <= status_bar.right());
    assert!(mode_tooltip.bottom() <= mode_switch.top());
    editor.update(visual_cx, |editor, cx| {
        editor.set_status_mode_tooltip_hover(ViewMode::Split, false, cx);
    });

    let content = visual_cx.debug_bounds("editor-content").unwrap();
    visual_cx.simulate_click(
        point(content.right() - px(12.0), content.center().y),
        Modifiers::default(),
    );
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, _cx| {
        assert!(!editor.workspace.is_open);
    });
    redraw(visual_cx);
    let overflow_button = visual_cx
        .debug_bounds("status-bar-format-overflow-button")
        .unwrap();
    visual_cx.simulate_click(overflow_button.center(), Modifiers::default());
    redraw(visual_cx);
    let popup = visual_cx
        .debug_bounds("status-bar-format-overflow")
        .unwrap();
    let overflow_indicator = visual_cx
        .debug_bounds("status-bar-format-overflow-indicator")
        .unwrap();
    assert!(overflow_indicator.left() >= overflow_button.left());
    assert!(overflow_indicator.right() <= overflow_button.right());
    assert_eq!(overflow_indicator.bottom(), overflow_button.bottom());
    let main = visual_cx.debug_bounds("editor-main-content").unwrap();
    assert!(popup.left() >= main.left());
    assert!(popup.right() <= main.right());
    assert!(popup.top() >= main.top());
    assert!(popup.bottom() <= main.bottom());
    assert!(
        visual_cx
            .debug_bounds("status-bar-overflow-encoding")
            .is_some()
    );
    assert!(
        visual_cx
            .debug_bounds("status-bar-overflow-line-ending")
            .is_none(),
        "line-ending picker stays beside the mode button instead of moving into overflow"
    );
    for selector in [
        "status-bar-word-count",
        "status-bar-cursor",
        "status-bar-custom-button-mode",
    ] {
        let item = visual_cx.debug_bounds(selector).unwrap();
        assert!(item.left() >= popup.left(), "{selector}");
        assert!(item.right() <= popup.right(), "{selector}");
        assert!(item.top() >= popup.top(), "{selector}");
        assert!(item.bottom() <= popup.bottom(), "{selector}");
    }
    let custom = visual_cx
        .debug_bounds("status-bar-custom-button-mode")
        .unwrap();
    assert!(custom.left() >= popup.left());
    assert!(custom.right() <= popup.right());
    editor.update(visual_cx, |editor, cx| {
        editor.status_bar.format_overflow_open = false;
        cx.notify();
    });
    redraw(visual_cx);

    let revision = editor.read_with(visual_cx, |editor, _cx| editor.source_document.revision());
    let mode_button = visual_cx.debug_bounds("status-bar-mode-switch").unwrap();
    assert_eq!(mode_button.size, size(px(24.0), px(24.0)));
    visual_cx.simulate_click(mode_button.center(), Modifiers::default());
    redraw(visual_cx);
    let mode_menu = visual_cx.debug_bounds("status-bar-mode-menu").unwrap();
    assert_eq!(mode_menu.size.width, px(120.0));
    let source_mode = visual_cx.debug_bounds("status-bar-mode-Source").unwrap();
    assert!(source_mode.left() >= mode_menu.left());
    assert!(source_mode.right() <= mode_menu.right());
    visual_cx.simulate_click(source_mode.center(), Modifiers::default());
    visual_cx.run_until_parked();
    redraw(visual_cx);
    editor.update(visual_cx, |editor, _cx| {
        assert_eq!(editor.view_mode, ViewMode::Source);
        assert_eq!(editor.source_document.revision(), revision);
        assert!(!editor.status_bar.mode_menu_open);
    });
    visual_cx.simulate_click(mode_button.center(), Modifiers::default());
    redraw(visual_cx);
    let source_indicator = visual_cx
        .debug_bounds("status-bar-mode-Source-indicator")
        .unwrap();
    assert_eq!(source_indicator.size, size(px(14.0), px(14.0)));

    let source = editor.read_with(visual_cx, |editor, _cx| editor.source_document.text());
    let dirty = editor.read_with(visual_cx, |editor, _cx| editor.document_dirty);
    editor.update_in(visual_cx, |editor, window, _cx| {
        let handle = &editor
            .status_bar
            .mode_focus_handles
            .as_ref()
            .expect("status mode focus handles")[3];
        handle.focus(window);
        assert!(handle.is_focused(window));
    });
    redraw(visual_cx);
    let focused_preview = visual_cx.debug_bounds("status-bar-mode-Preview").unwrap();
    assert_eq!(f32::from(focused_preview.size.height), 30.0);
    visual_cx.simulate_keystrokes("space");
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, _cx| {
        assert_eq!(editor.view_mode, ViewMode::Preview);
        assert_eq!(editor.source_document.text(), source);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });

    visual_cx.simulate_click(mode_button.center(), Modifiers::default());
    redraw(visual_cx);
    editor.update_in(visual_cx, |editor, window, _cx| {
        editor
            .status_bar
            .mode_focus_handles
            .as_ref()
            .expect("status mode focus handles")[0]
            .focus(window);
    });
    visual_cx.simulate_keystrokes("enter");
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, _cx| {
        assert_eq!(editor.view_mode, ViewMode::Rendered);
        assert_eq!(editor.source_document.text(), source);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });

    editor.update_in(visual_cx, |editor, window, _cx| {
        let handle = editor
            .status_bar
            .sidebar_focus_handle
            .as_ref()
            .expect("status sidebar focus");
        handle.focus(window);
        assert!(handle.is_focused(window));
    });
    visual_cx.simulate_keystrokes("space");
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, _cx| {
        assert!(editor.workspace.is_open);
        assert_eq!(editor.source_document.text(), source);
        assert_eq!(editor.source_document.revision(), revision);
    });
    redraw(visual_cx);
    editor.update_in(visual_cx, |editor, window, _cx| {
        editor
            .status_bar
            .sidebar_focus_handle
            .as_ref()
            .expect("status sidebar focus")
            .focus(window);
    });
    visual_cx.simulate_keystrokes("enter");
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, _cx| {
        assert!(!editor.workspace.is_open);
        assert_eq!(editor.document_dirty, dirty);
    });

    redraw(visual_cx);
    editor.update_in(visual_cx, |editor, window, _cx| {
        let handle = editor
            .status_bar
            .overflow_focus_handle
            .as_ref()
            .expect("status overflow focus");
        handle.focus(window);
        assert!(handle.is_focused(window));
    });
    visual_cx.simulate_keystrokes("enter");
    visual_cx.run_until_parked();
    redraw(visual_cx);
    assert!(
        visual_cx
            .debug_bounds("status-bar-format-overflow")
            .is_some()
    );
    let overflow_button = visual_cx
        .debug_bounds("status-bar-format-overflow-button")
        .unwrap();
    assert_eq!(f32::from(overflow_button.size.height), 24.0);
    assert!(f32::from(overflow_button.size.width) >= 28.0);
    visual_cx.simulate_keystrokes("escape");
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, _cx| {
        assert!(!editor.status_bar.format_overflow_open);
        assert_eq!(editor.source_document.text(), source);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });
}
