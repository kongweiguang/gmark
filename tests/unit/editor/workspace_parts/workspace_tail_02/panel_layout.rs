// @author kongweiguang

    #[gpui::test]
    async fn workspace_visibility_separates_docked_preference_from_compact_overlay(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "# Heading".to_owned(), None)
        });

        editor.update(visual, |editor, _cx| {
            editor.sync_workspace_visibility_for_viewport(1180.0);
            assert!(editor.workspace.is_open);
            assert!(editor.workspace_docked_open_preference());

            editor.sync_workspace_visibility_for_viewport(720.0);
            assert!(!editor.workspace.is_open);
            editor.workspace.is_open = true;
            editor.sync_workspace_visibility_for_viewport(720.0);
            assert!(
                editor.workspace.is_open,
                "compact overlay must remain user-controlled"
            );

            editor.sync_workspace_visibility_for_viewport(1180.0);
            assert!(editor.workspace.is_open);
            editor.restore_workspace_docked_open_preference(Some(false));
            assert!(!editor.workspace.is_open);

            editor.sync_workspace_visibility_for_viewport(720.0);
            editor.workspace.is_open = true;
            editor.sync_workspace_visibility_for_viewport(1180.0);
            assert!(!editor.workspace.is_open);
        });
    }

    #[gpui::test]
    async fn document_sidebar_starts_collapsed_and_markdown_outline_moves_right(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "# Root\n\n## Child".to_owned(), None)
        });
        visual.simulate_resize(size(px(1180.0), px(780.0)));
        visual.update(|window, cx| window.draw(cx).clear());

        assert!(visual.debug_bounds("status-bar-document-sidebar-toggle").is_some());
        assert!(visual.debug_bounds("document-sidebar-panel").is_none());
        assert!(visual.debug_bounds("workspace-tab-outline").is_none());

        editor.update(visual, |editor, cx| {
            assert!(!editor.workspace.document_sidebar_open);
            editor.workspace.document_sidebar_open = true;
            editor.sync_workspace_outline(cx);
            cx.notify();
        });
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear());

        let main = visual.debug_bounds("editor-main-content").unwrap();
        let panel = visual.debug_bounds("document-sidebar-panel").unwrap();
        let header = visual.debug_bounds("document-sidebar-header").unwrap();
        let tabs = visual.debug_bounds("document-tab-strip").unwrap();
        assert_eq!(f32::from(header.size.height), 36.0);
        assert_eq!(header.top(), tabs.top());
        assert_eq!(header.size.height, tabs.size.height);
        assert!(panel.right() <= main.right());
        assert!(panel.left() > main.left());
        assert!(
            visual
                .debug_bounds("document-sidebar-markdown-outline")
                .is_some()
        );
        let resize_handle = visual.debug_bounds("document-sidebar-resize-handle").unwrap();
        assert!(resize_handle.left() <= panel.left());
        visual.simulate_click(resize_handle.center(), Modifiers::default());
        visual.simulate_keystrokes("left");
        visual.update(|window, cx| window.draw(cx).clear());
        let resized = visual.debug_bounds("document-sidebar-panel").unwrap();
        assert_eq!(f32::from(resized.size.width), f32::from(panel.size.width) + 4.0);
        editor.update_in(visual, |editor, window, cx| {
            editor.ensure_document_sidebar_focus_handle(cx).focus(window);
            assert!(!editor.handle_document_sidebar_key(&key_event("escape"), window, cx));
            assert!(editor.workspace.document_sidebar_open);
        });

        visual.simulate_resize(size(px(720.0), px(520.0)));
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update_in(visual, |editor, window, cx| {
            editor.workspace.is_open = true;
            editor.workspace.document_sidebar_open = true;
            editor.ensure_document_sidebar_focus_handle(cx).focus(window);
            assert!(editor.handle_document_sidebar_key(&key_event("escape"), window, cx));
            editor.workspace.document_sidebar_open = true;
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let workspace_overlay = visual.debug_bounds("compact-workspace-overlay").unwrap();
        let document_overlay = visual
            .debug_bounds("compact-document-sidebar-overlay")
            .unwrap();
        assert!(workspace_overlay.right() <= document_overlay.left());
        assert!(f32::from(workspace_overlay.size.width) <= 360.0);
        assert!(f32::from(document_overlay.size.width) <= 360.0);
    }

    #[gpui::test]
    async fn workspace_resize_handle_previews_clamps_and_stays_out_of_compact_overlay(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "# Resize".to_owned(), None)
        });
        visual.simulate_resize(size(px(1180.0), px(780.0)));
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            editor.restore_workspace_panel_width(Some(248.0));
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());

        for width in [200.0, 248.0, 360.0] {
            editor.update(visual, |editor, cx| {
                editor.restore_workspace_panel_width(Some(width));
                cx.notify();
            });
            visual.update(|window, cx| window.draw(cx).clear());
            assert_eq!(
                f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
                width
            );
            assert_workspace_header_layout(visual);
        }
        editor.update(visual, |editor, cx| {
            editor.restore_workspace_panel_width(Some(248.0));
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());

        let initial = visual.debug_bounds("workspace-panel").unwrap();
        let handle = visual.debug_bounds("workspace-resize-handle").unwrap();
        let line = visual.debug_bounds("workspace-resize-line").unwrap();
        assert_eq!(f32::from(initial.size.width), 248.0);
        assert_eq!(f32::from(handle.size.width), WORKSPACE_RESIZE_HIT_WIDTH);
        assert_eq!(f32::from(line.size.width), 1.0);
        assert!((f32::from(initial.right() - line.center().x)).abs() <= 1.0);
        assert!(line.left() >= handle.left());
        assert!(line.right() <= handle.right());

        let source = editor.read_with(visual, |editor, _cx| editor.source_document.text());
        let revision = editor.read_with(visual, |editor, _cx| editor.source_document.revision());
        let dirty = editor.read_with(visual, |editor, _cx| editor.document_dirty);
        visual.simulate_click(handle.center(), Modifiers::default());
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update_in(visual, |editor, window, _cx| {
            assert!(
                editor
                    .workspace
                    .resize_focus_handle
                    .as_ref()
                    .is_some_and(|handle| handle.is_focused(window))
            );
        });
        visual.simulate_keystrokes("right shift-right");
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.workspace_panel_width(), Some(268.0));
        });
        visual.simulate_keystrokes("home");
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            WORKSPACE_PANEL_MIN_WIDTH
        );
        visual.simulate_keystrokes("end");
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            WORKSPACE_PANEL_MAX_WIDTH
        );
        visual.simulate_keystrokes("enter");
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, cx| {
            assert_eq!(editor.workspace_panel_width(), None);
            assert_eq!(
                editor
                    .workspace_session_snapshot_result(cx)
                    .expect("canonical workspace session snapshot")
                    .workspace_panel_width,
                None
            );
            assert_eq!(editor.source_document.text(), source);
            assert_eq!(editor.source_document.revision(), revision);
            assert_eq!(editor.document_dirty, dirty);
        });
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            248.0
        );

        visual.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_move(
            point(handle.center().x + px(80.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            328.0
        );
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.workspace_panel_width(), Some(328.0));
            assert!(editor.workspace.resize_session.is_some());
        });

        visual.simulate_mouse_move(
            point(handle.center().x + px(400.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.simulate_mouse_up(
            point(handle.center().x + px(400.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            WORKSPACE_PANEL_MAX_WIDTH
        );
        editor.update(visual, |editor, _cx| {
            assert_eq!(
                editor.workspace_panel_width(),
                Some(WORKSPACE_PANEL_MAX_WIDTH)
            );
            assert!(editor.workspace.resize_session.is_none());
        });

        visual.simulate_resize(size(px(720.0), px(520.0)));
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            WORKSPACE_COMPACT_OVERLAY_WIDTH
        );
        assert_workspace_header_layout(visual);
        // GPUI test inspector 会保留已卸载分支的旧 bounds；用事件无副作用证明 overlay
        // 当前树没有可交互 resize handle。
        visual.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_move(
            point(handle.center().x - px(100.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.simulate_mouse_up(
            point(handle.center().x - px(100.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        editor.update(visual, |editor, _cx| {
            assert_eq!(
                editor.workspace_panel_width(),
                Some(WORKSPACE_PANEL_MAX_WIDTH)
            );
            assert!(editor.workspace.resize_session.is_none());
        });
    }

    #[gpui::test]
    async fn workspace_docks_overlays_and_resizes_from_the_physical_right_edge(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "# Right sidebar".to_owned(), None)
        });
        visual.simulate_resize(size(px(1180.0), px(780.0)));
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            editor.restore_workspace_panel_width(Some(248.0));
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());

        let main = visual.debug_bounds("editor-main-content").unwrap();
        let content = visual.debug_bounds("editor-content").unwrap();
        let panel = visual.debug_bounds("workspace-panel").unwrap();
        let handle = visual.debug_bounds("workspace-resize-handle").unwrap();
        let line = visual.debug_bounds("workspace-resize-line").unwrap();
        assert_eq!(panel.left(), main.left());
        assert!(content.left() >= panel.right());
        assert!((f32::from(panel.right() - line.center().x)).abs() <= 1.0);
        assert!(handle.left() <= panel.right());
        assert!(handle.right() >= panel.right());
        assert!(visual.debug_bounds("document-tab-leading-tools").is_none());
        assert!(visual.debug_bounds("document-tab-trailing-tools").is_some());
        assert!(visual.debug_bounds("document-toolbar-action-0").is_some());
        assert!(visual.debug_bounds("document-toolbar-action-1").is_none());

        visual.simulate_click(handle.center(), Modifiers::default());
        visual.simulate_keystrokes("right");
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.workspace_panel_width(), Some(252.0));
        });
        visual.simulate_keystrokes("left");
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.workspace_panel_width(), Some(248.0));
        });

        let handle = visual.debug_bounds("workspace-resize-handle").unwrap();
        visual.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_move(
            point(handle.center().x + px(40.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.simulate_mouse_up(
            point(handle.center().x + px(40.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            288.0
        );

        visual.simulate_resize(size(px(720.0), px(520.0)));
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let main = visual.debug_bounds("editor-main-content").unwrap();
        let content = visual.debug_bounds("editor-content").unwrap();
        let overlay = visual.debug_bounds("compact-workspace-overlay").unwrap();
        let panel = visual.debug_bounds("workspace-panel").unwrap();
        assert_eq!(overlay.left(), main.left());
        assert_eq!(panel.left(), overlay.left());
        assert_eq!(content.left(), main.left());
        assert_eq!(content.right(), main.right());
        assert_eq!(f32::from(panel.size.width), WORKSPACE_COMPACT_OVERLAY_WIDTH);
    }
