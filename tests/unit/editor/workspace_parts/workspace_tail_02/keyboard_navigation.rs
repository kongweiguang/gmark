// @author kongweiguang

    #[gpui::test]
    async fn workspace_empty_state_keeps_primary_action_minimal_in_compact_panel(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "# Untitled".to_owned(), None)
        });
        visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            editor.workspace.active_tab = WorkspaceTab::Files;
            editor.workspace.root = None;
            editor.workspace.file_tree = None;
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());

        let panel = visual.debug_bounds("workspace-panel").unwrap();
        assert_workspace_header_layout(visual);
        let files = visual.debug_bounds("workspace-files-empty").unwrap();
        let action = visual.debug_bounds("workspace-empty-open-folder").unwrap();
        let action_icon = visual
            .debug_bounds("workspace-empty-open-folder-icon")
            .unwrap();
        for (name, bounds) in [("files", files), ("action", action)] {
            assert!(bounds.left() >= panel.left(), "{name}");
            assert!(bounds.right() <= panel.right(), "{name}");
            assert!(bounds.top() >= panel.top(), "{name}");
            assert!(bounds.bottom() <= panel.bottom(), "{name}");
        }
        assert!(visual.debug_bounds("workspace-files-empty-icon").is_none());
        assert!(
            visual
                .debug_bounds("workspace-files-empty-icon-svg")
                .is_none()
        );
        assert_eq!(f32::from(action.size.height), 30.0);
        assert_eq!(action_icon.size, size(px(14.0), px(14.0)));
        assert!(action_icon.left() >= action.left());
        assert!(action_icon.right() <= action.right());

        editor.update(visual, |editor, cx| {
            editor.workspace.active_tab = WorkspaceTab::Outline;
            editor.workspace.outline_tree.clear();
            editor.workspace.outline_running = false;
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let outline = visual.debug_bounds("workspace-outline-empty").unwrap();
        let outline_icon = visual.debug_bounds("workspace-outline-empty-icon").unwrap();
        assert!(outline.left() >= panel.left());
        assert!(outline.right() <= panel.right());
        assert_eq!(f32::from(outline_icon.size.width), 32.0);

        editor.update(visual, |editor, cx| {
            editor.workspace.active_tab = WorkspaceTab::Search;
            editor.ensure_workspace_search_input(cx);
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let search_field = visual.debug_bounds("workspace-search-input").unwrap();
        let search_icon = visual
            .debug_bounds("workspace-search-input-icon")
            .unwrap();
        assert!(search_field.left() >= panel.left());
        assert!(search_field.right() <= panel.right());
        assert!(search_icon.left() >= search_field.left());
        assert!(search_icon.right() <= search_field.right());
        assert!(visual.debug_bounds("workspace-search-status").is_none());
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));

        editor.update(visual, |editor, cx| {
            editor.workspace.search_running = true;
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        assert!(
            visual
                .debug_bounds("workspace-search-running-icon")
                .is_some()
        );
        editor.update(visual, |editor, cx| {
            editor.workspace.search_running = false;
            editor.workspace.search_error = Some("Invalid expression".to_owned());
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        assert!(visual.debug_bounds("workspace-search-error-icon").is_some());
    }

    #[gpui::test]
    async fn workspace_status_bar_search_toggle_opens_and_cancels(cx: &mut gpui::TestAppContext) {
        init_workspace_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "# Heading\n\nBody".to_owned(), None)
        });
        visual.simulate_resize(size(px(720.0), px(520.0)));
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            editor.workspace.active_tab = WorkspaceTab::Files;
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());

        visual.update(|window, cx| window.draw(cx).clear());
        assert_workspace_header_layout(visual);
        let search_toggle = visual
            .debug_bounds("status-bar-search-toggle")
            .expect("status bar owns workspace search activation");
        visual.simulate_click(search_toggle.center(), Modifiers::default());
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.workspace.active_tab, WorkspaceTab::Search);
            assert!(editor.workspace.search_input.is_some());
        });

        // Clicking the active search action cancels search but leaves the
        // workspace drawer open so the Files tree is restored in place.
        visual.update(|window, cx| window.draw(cx).clear());
        let search_toggle = visual
            .debug_bounds("status-bar-search-toggle")
            .expect("status bar search action remains available while searching");
        visual.simulate_click(search_toggle.center(), Modifiers::default());
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            assert!(editor.workspace.is_open);
            assert_eq!(editor.workspace.active_tab, WorkspaceTab::Files);
        });

        // Opening search again from Files focuses the editor input; Escape
        // follows the same cancellation path as the status-bar toggle.
        visual.update(|window, cx| window.draw(cx).clear());
        let search_toggle = visual
            .debug_bounds("status-bar-search-toggle")
            .expect("status bar search action remains available in Files");
        visual.simulate_click(search_toggle.center(), Modifiers::default());
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.workspace.active_tab, WorkspaceTab::Search);
        });
        // Closing the drawer while Search is active preserves that view;
        // invoking the status-bar Search action while closed must reopen
        // Search rather than interpreting the click as cancellation.
        visual.update(|window, cx| window.draw(cx).clear());
        let files_toggle = visual
            .debug_bounds("status-bar-sidebar-toggle")
            .expect("status bar Files action remains available");
        visual.simulate_click(files_toggle.center(), Modifiers::default());
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| assert!(!editor.workspace.is_open));

        visual.update(|window, cx| window.draw(cx).clear());
        let search_toggle = visual
            .debug_bounds("status-bar-search-toggle")
            .expect("status bar Search action remains available while drawer is closed");
        visual.simulate_click(search_toggle.center(), Modifiers::default());
        visual.run_until_parked();
        editor.update_in(visual, |editor, window, cx| {
            assert!(editor.workspace.is_open);
            assert_eq!(editor.workspace.active_tab, WorkspaceTab::Search);
            assert!(editor.handle_workspace_key(&key_event("escape"), window, cx));
        });
        editor.update(visual, |editor, _cx| {
            assert!(editor.workspace.is_open);
            assert_eq!(editor.workspace.active_tab, WorkspaceTab::Files);
        });

        visual.simulate_resize(size(px(1180.0), px(780.0)));
        visual.update(|window, cx| window.draw(cx).clear());
        assert_workspace_header_layout(visual);
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
        editor.update(visual, |editor, _cx| {
            assert!(editor.workspace.is_open);
            assert_eq!(editor.workspace.active_tab, WorkspaceTab::Files);
        });
    }

    /// Keeps keyboard selection aligned with the scanner's canonical Windows
    /// paths instead of comparing an 8.3 alias with a long-path tree node.
    #[gpui::test]
    async fn workspace_files_keyboard_navigation_expands_selects_and_returns_focus(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root =
            std::env::temp_dir().join(format!("gmark-workspace-keyboard-{}", uuid::Uuid::new_v4()));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let current = root.join("current.md");
        let child = nested.join("child.md");
        fs::write(&current, "current").unwrap();
        fs::write(&child, "child").unwrap();
        let tree = scan_workspace_dir(&root).unwrap();
        let canonical_root = dunce::canonicalize(&root).unwrap();
        let canonical_nested = canonical_root.join("nested");
        let canonical_child = canonical_nested.join("child.md");
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "current".to_owned(), None)
        });

        visual.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.workspace.is_open = true;
                editor.workspace.active_tab = WorkspaceTab::Files;
                editor.workspace.root = Some(canonical_root.clone());
                editor.workspace.explicit_root = Some(canonical_root.clone());
                editor.workspace.file_tree = Some(tree.clone());
                editor.workspace.expanded.clear();
                editor.workspace.selected = None;
                editor.workspace.keyboard_zone = WorkspaceKeyboardZone::Tabs;
                editor.ensure_workspace_focus_handle(cx).focus(window);
            });
        });
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update_in(visual, |editor, window, cx| {
            editor.pending_focus = None;
            window.activate_window();
            editor.ensure_workspace_focus_handle(cx).focus(window);
            assert!(
                editor
                    .workspace
                    .focus_handle
                    .as_ref()
                    .is_some_and(|focus| focus.is_focused(window))
            );
        });
        for key in ["down", "right", "down", "right", "down"] {
            editor.update_in(visual, |editor, window, cx| {
                assert!(editor.handle_workspace_key(&key_event(key), window, cx));
            });
        }
        editor.update(visual, |editor, _cx| {
            assert_eq!(
                editor.workspace.selected,
                Some(WorkspaceSelection::File(canonical_child.clone()))
            );
            assert_eq!(editor.workspace.keyboard_zone, WorkspaceKeyboardZone::Body);
        });

        editor.update_in(visual, |editor, window, cx| {
            assert!(editor.handle_workspace_key(&key_event("left"), window, cx));
        });
        editor.update(visual, |editor, _cx| {
            assert_eq!(
                editor.workspace.selected,
                Some(WorkspaceSelection::File(canonical_nested.clone()))
            );
        });
        editor.update_in(visual, |editor, window, cx| {
            assert!(editor.handle_workspace_key(&key_event("escape"), window, cx));
        });
        editor.update(visual, |editor, _cx| assert!(!editor.workspace.is_open));

        let _ = fs::remove_dir_all(root);
    }
