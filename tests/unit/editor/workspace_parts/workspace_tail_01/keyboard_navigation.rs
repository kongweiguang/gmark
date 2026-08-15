// @author kongweiguang

    #[gpui::test]
    async fn editor_capture_routes_legacy_workspace_tab_zone_to_visible_body(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "# document".to_owned(), None)
        });
        editor.update(visual, |editor, _cx| {
            editor.workspace.is_open = true;
            editor.workspace.active_tab = WorkspaceTab::Files;
            editor.workspace.keyboard_zone = WorkspaceKeyboardZone::Tabs;
        });
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update_in(visual, |editor, window, cx| {
            editor.pending_focus = None;
            editor.ensure_workspace_focus_handle(cx).focus(window);
            editor.on_editor_key_down_capture(&key_event("right"), window, cx);
        });
        editor.update(visual, |editor, _cx| {
            // The former top tab strip is no longer rendered. A restored focus
            // zone must fall back to the visible body instead of switching the
            // workspace tab through hidden controls.
            assert_eq!(editor.workspace.active_tab, WorkspaceTab::Files);
            assert_eq!(editor.workspace.keyboard_zone, WorkspaceKeyboardZone::Body);
        });
    }

    /// Uses the scanner's canonical path spelling so keyboard selection tests
    /// exercise the same identity that production tree nodes expose.
    #[gpui::test]
    async fn workspace_keyboard_keeps_long_tree_selection_in_scroll_viewport(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root =
            std::env::temp_dir().join(format!("gmark-workspace-scroll-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        for index in 0..48 {
            fs::write(root.join(format!("note-{index:02}.md")), "note").unwrap();
        }
        let tree = scan_workspace_dir(&root).unwrap();
        let root_id = tree.id.clone();
        let canonical_root = dunce::canonicalize(&root).unwrap();
        let last_path = canonical_root.join("note-47.md");
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "document".to_owned(), None)
        });
        editor.update(visual, |editor, _cx| {
            editor.workspace.is_open = true;
            editor.workspace.active_tab = WorkspaceTab::Files;
            editor.workspace.root = Some(canonical_root.clone());
            editor.workspace.explicit_root = Some(canonical_root.clone());
            editor.workspace.file_tree = Some(tree.clone());
            editor.workspace.expanded.insert(root_id.clone());
            editor.workspace.keyboard_zone = WorkspaceKeyboardZone::Body;
        });
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update_in(visual, |editor, window, cx| {
            editor.pending_focus = None;
            editor.ensure_workspace_focus_handle(cx).focus(window);
            assert!(editor.handle_workspace_key(&key_event("end"), window, cx));
        });
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, _cx| {
            assert_eq!(
                editor.workspace.selected,
                Some(WorkspaceSelection::File(last_path.clone()))
            );
            assert!(f32::from(editor.workspace.panel_scroll.offset().y) < 0.0);
        });

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn workspace_outline_keyboard_expands_and_activates_heading(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let source = "# Root\n\n## Child\n";
        let outline = build_outline_tree(source);
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, source.to_owned(), None)
        });
        editor.update(visual, |editor, _cx| {
            editor.workspace.is_open = true;
            editor.workspace.active_tab = WorkspaceTab::Outline;
            editor.workspace.outline_tree = outline.clone();
            editor.workspace.outline_source = Some(source.to_owned());
            editor.workspace.outline_revision =
                Some((editor.document_epoch, editor.source_document.revision()));
            editor.workspace.expanded.clear();
            editor.workspace.selected = None;
            editor.workspace.keyboard_zone = WorkspaceKeyboardZone::Tabs;
        });
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update_in(visual, |editor, window, cx| {
            editor.pending_focus = None;
            editor.ensure_workspace_focus_handle(cx).focus(window);
        });
        editor.update_in(visual, |editor, window, cx| {
            assert_eq!(editor.workspace.outline_tree.len(), 1);
            for key in ["down", "right", "down"] {
                assert!(editor.handle_workspace_key(&key_event(key), window, cx));
            }
            assert_eq!(
                editor.workspace.selected,
                Some(WorkspaceSelection::Outline("outline:2".to_owned()))
            );
            assert!(editor.handle_workspace_key(&key_event("enter"), window, cx));
        });
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.last_selection_snapshot.range(), 11..11);
        });
    }

    #[gpui::test]
    async fn workspace_search_keyboard_reaches_options_results_and_navigation(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root =
            std::env::temp_dir().join(format!("gmark-search-keyboard-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let current = root.join("current.md");
        fs::write(&current, "first\nsecond\n").unwrap();
        let editor_path = current.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "first\nsecond\n".to_owned(), Some(editor_path))
        });

        visual.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.workspace.is_open = true;
                editor.workspace.active_tab = WorkspaceTab::Search;
                editor.workspace.root = Some(root.clone());
                editor.workspace.explicit_root = Some(root.clone());
                editor.workspace.search_results = vec![WorkspaceSearchMatch {
                    path: current.clone(),
                    relative_path: "current.md".to_owned(),
                    line: 2,
                    column: 1,
                    preview: "second".to_owned(),
                }];
                let input = editor.ensure_workspace_search_input(cx);
                input.update(cx, |input, cx| {
                    input.replace_text_in_visible_range(0..0, "second", None, false, cx);
                });
                input.read(cx).focus_handle.focus(window);
            });
        });
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update_in(visual, |editor, window, cx| {
            editor.workspace.search_results = vec![WorkspaceSearchMatch {
                path: current.clone(),
                relative_path: "current.md".to_owned(),
                line: 2,
                column: 1,
                preview: "second".to_owned(),
            }];
            editor
                .ensure_workspace_search_input(cx)
                .read(cx)
                .focus_handle
                .focus(window);
        });
        editor.update_in(visual, |editor, window, cx| {
            editor.workspace.search_results = vec![WorkspaceSearchMatch {
                path: current.clone(),
                relative_path: "current.md".to_owned(),
                line: 2,
                column: 1,
                preview: "second".to_owned(),
            }];
            assert!(editor.handle_workspace_key(&key_event("tab"), window, cx));
            assert!(editor.handle_workspace_key(&key_event("space"), window, cx));
        });
        editor.update(visual, |editor, _cx| {
            assert!(editor.workspace.search_options.case_sensitive);
            assert_eq!(
                editor.workspace.keyboard_zone,
                WorkspaceKeyboardZone::SearchOptions
            );
        });
        editor.update_in(visual, |editor, window, cx| {
            editor.workspace.search_results = vec![WorkspaceSearchMatch {
                path: current.clone(),
                relative_path: "current.md".to_owned(),
                line: 2,
                column: 1,
                preview: "second".to_owned(),
            }];
            assert!(editor.handle_workspace_key(&key_event("tab"), window, cx));
            assert_eq!(
                editor.workspace.keyboard_zone,
                WorkspaceKeyboardZone::SearchResults
            );
            assert_eq!(editor.workspace.search_results[0].line, 2);
            assert!(editor.handle_workspace_key(&key_event("enter"), window, cx));
            assert_eq!(editor.last_selection_snapshot.range(), 6..6);
        });
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.last_selection_snapshot.range(), 6..6);
            assert_eq!(
                editor.workspace.keyboard_zone,
                WorkspaceKeyboardZone::SearchResults
            );
        });

        let _ = fs::remove_dir_all(root);
    }
