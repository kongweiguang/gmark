// @author kongweiguang

    #[gpui::test]
    async fn quick_open_renders_background_index_results(cx: &mut gpui::TestAppContext) {
        init_workspace_test_app(cx);
        let root = std::env::temp_dir().join(format!("gmark-quick-open-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("nested")).unwrap();
        let current = root.join("current.md");
        fs::write(&current, "# current\n").unwrap();
        fs::write(root.join("nested/target.md"), "# target\n").unwrap();
        let tree = super::scan_workspace_dir(&root).unwrap();
        let editor_path = current.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "# current\n".to_owned(), Some(editor_path))
        });
        visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));

        visual.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.workspace.root = Some(root.clone());
                editor.workspace.explicit_root = Some(root.clone());
                editor.workspace.file_tree = Some(tree.clone());
                editor.on_quick_open_action(&crate::components::QuickOpen, window, cx);
                let input = editor.workspace.quick_open.as_ref().unwrap().input.clone();
                input.update(cx, |input, cx| {
                    input.replace_text_in_visible_range(0..0, "target", None, false, cx);
                });
            });
        });
        // 先让输入 Changed 订阅创建带新 query 的 debounce task，再推进虚拟时钟。
        visual.run_until_parked();
        editor.update(visual, |editor, cx| editor.schedule_quick_open(cx));
        visual.run_until_parked();
        visual.executor().advance_clock(super::QUICK_OPEN_DEBOUNCE);
        visual.run_until_parked();
        editor.update(visual, |editor, cx| {
            let state = editor.workspace.quick_open.as_ref().unwrap();
            assert_eq!(state.input.read(cx).display_text(), "target");
            assert!(!state.running);
            assert!(!state.results.is_empty());
        });
        visual.update(|window, cx| window.draw(cx).clear());

        assert!(visual.debug_bounds("quick-open-dialog").is_some());
        assert!(visual.debug_bounds("quick-open-input").is_some());
        assert!(visual.debug_bounds("quick-open-search-icon").is_some());
        assert!(visual.debug_bounds("quick-open-close").is_some());
        assert!(visual.debug_bounds("quick-open-results").is_some());
        assert!(visual.debug_bounds("quick-open-result-0").is_some());
        let dialog = visual.debug_bounds("quick-open-dialog").unwrap();
        assert!(f32::from(dialog.left()) >= 0.0);
        assert!(f32::from(dialog.right()) <= 720.0);
        assert!(f32::from(dialog.top()) >= 0.0);
        assert!(f32::from(dialog.bottom()) <= 520.0);
        let input = visual.debug_bounds("quick-open-input").unwrap();
        let search_icon = visual.debug_bounds("quick-open-search-icon").unwrap();
        let search_icon_svg = visual.debug_bounds("quick-open-search-icon-svg").unwrap();
        let close = visual.debug_bounds("quick-open-close").unwrap();
        let close_icon = visual.debug_bounds("quick-open-close-icon").unwrap();
        let result_icon = visual.debug_bounds("quick-open-result-icon-0").unwrap();
        assert_eq!(f32::from(input.size.height), 40.0);
        assert_eq!(f32::from(search_icon.size.width), 16.0);
        assert_eq!(f32::from(search_icon.size.height), 16.0);
        assert_eq!(search_icon_svg.size, size(px(16.0), px(16.0)));
        assert_eq!(f32::from(close.size.width), 28.0);
        assert_eq!(f32::from(close.size.height), 28.0);
        assert_eq!(close_icon.size, size(px(15.0), px(15.0)));
        assert_eq!(result_icon.size, size(px(16.0), px(16.0)));
        assert_eq!(
            f32::from(
                visual
                    .debug_bounds("quick-open-result-0")
                    .unwrap()
                    .size
                    .height
            ),
            34.0
        );
        assert!(input.left() >= dialog.left());
        assert!(input.right() <= dialog.right());
        assert!(close.left() >= dialog.left());
        assert!(close.right() <= dialog.right());
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
        editor.update(visual, |editor, _cx| {
            let state = editor.workspace.quick_open.as_ref().unwrap();
            assert_eq!(state.results[0].relative_path, "nested/target.md");
            assert!(!state.running);
        });

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn opening_single_file_does_not_infer_parent_workspace(cx: &mut gpui::TestAppContext) {
        init_workspace_test_app(cx);
        let root =
            std::env::temp_dir().join(format!("gmark-single-file-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("single.md");
        fs::write(&path, "# Single\n").unwrap();
        let editor_path = path.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "# Single\n".to_owned(), Some(editor_path))
        });

        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            editor.sync_workspace_after_document_path_change(cx);
            assert_eq!(editor.workspace_root_for_current_file(), None);
            assert_eq!(editor.workspace.root, None);
            assert!(editor.workspace.file_tree.is_none());
        });

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn explicit_workspace_root_survives_document_path_changes(cx: &mut gpui::TestAppContext) {
        init_workspace_test_app(cx);
        let base =
            std::env::temp_dir().join(format!("gmark-explicit-root-{}", uuid::Uuid::new_v4()));
        let document_root = base.join("document");
        let workspace_root = base.join("workspace");
        fs::create_dir_all(&document_root).unwrap();
        fs::create_dir_all(&workspace_root).unwrap();
        let document = document_root.join("document.md");
        let workspace_file = workspace_root.join("workspace.md");
        fs::write(&document, "# document\n").unwrap();
        fs::write(&workspace_file, "# workspace\n").unwrap();
        let canonical_workspace = dunce::canonicalize(&workspace_root).unwrap();
        let editor_path = document.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "# document\n".to_owned(), Some(editor_path))
        });

        editor.update(visual, |editor, cx| {
            editor.set_explicit_workspace_root(workspace_root.clone(), cx);
        });
        visual.run_until_parked();
        editor.update(visual, |editor, cx| {
            assert_eq!(editor.workspace.root.as_ref(), Some(&canonical_workspace));
            assert_eq!(
                editor.workspace.explicit_root.as_ref(),
                Some(&canonical_workspace)
            );
            let tree = editor.workspace.file_tree.as_ref().unwrap();
            let mut paths = Vec::new();
            super::collect_markdown_paths(tree, &mut paths);
            assert_eq!(paths, vec![canonical_workspace.join("workspace.md")]);
            let tree_before_switch = tree.clone();
            let generation_before_switch = editor.workspace.file_scan_generation;
            editor.replace_document_from_markdown(
                "# replacement\n".to_owned(),
                Some(document_root.join("replacement.md")),
                cx,
            );
            assert_eq!(
                editor.workspace_root_for_current_file(),
                Some(canonical_workspace.clone())
            );
            assert_eq!(editor.workspace.file_tree.as_ref(), Some(&tree_before_switch));
            assert_eq!(
                editor.workspace.file_scan_generation,
                generation_before_switch
            );
            assert!(!editor.workspace.file_scanning);
        });

        let _ = fs::remove_dir_all(base);
    }

    #[gpui::test]
    async fn outline_refresh_keeps_stale_tree_and_rejects_superseded_source(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "# Old".to_owned(), None)
        });
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            editor.workspace.active_tab = super::WorkspaceTab::Outline;
            editor.sync_workspace_outline(cx);
            if !editor.workspace.outline_running {
                assert_eq!(editor.workspace.outline_tree[0].label, "Old");
                assert_eq!(editor.workspace.outline_source.as_deref(), Some("# Old"));
            }
        });
        visual.run_until_parked();
        editor.update(visual, |editor, cx| {
            assert_eq!(editor.workspace.outline_tree[0].label, "Old");
            editor.replace_document_from_markdown("# Superseded".to_owned(), None, cx);
            editor.replace_document_from_markdown("# Final".to_owned(), None, cx);
            assert!(editor.workspace.outline_running);
            assert_eq!(editor.workspace.outline_tree[0].label, "Old");
        });
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            assert!(!editor.workspace.outline_running);
            assert_eq!(editor.workspace.outline_tree.len(), 1);
            assert_eq!(editor.workspace.outline_tree[0].label, "Final");
            assert_eq!(editor.workspace.outline_source.as_deref(), Some("# Final"));
        });

        editor.update(visual, |editor, cx| {
            editor.workspace.outline_running = true;
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let progress = visual.debug_bounds("workspace-outline-progress").unwrap();
        let icon = visual
            .debug_bounds("workspace-outline-progress-icon")
            .unwrap();
        let svg = visual
            .debug_bounds("workspace-outline-progress-icon-svg")
            .unwrap();
        assert_eq!(icon.size, size(px(18.0), px(18.0)));
        assert_eq!(svg.size, size(px(14.0), px(14.0)));
        assert!(icon.left() >= progress.left());
        assert!(icon.right() <= progress.right());
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
    }
