// @author kongweiguang

    #[gpui::test]
    async fn editor_capture_routes_workspace_tab_navigation(cx: &mut gpui::TestAppContext) {
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
            assert_eq!(editor.workspace.active_tab, WorkspaceTab::Outline);
        });
    }

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
        let last_path = root.join("note-47.md");
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "document".to_owned(), None)
        });
        editor.update(visual, |editor, _cx| {
            editor.workspace.is_open = true;
            editor.workspace.active_tab = WorkspaceTab::Files;
            editor.workspace.root = Some(root.clone());
            editor.workspace.explicit_root = Some(root.clone());
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

    #[gpui::test]
    async fn workspace_file_menu_and_review_dialog_have_stable_bounds(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root = std::env::temp_dir().join(format!("gmark-file-op-ui-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("old.md");
        let destination = root.join("new.md");
        fs::write(&source, "# old\n").unwrap();
        fs::write(root.join("index.md"), "[old](old.md)\n").unwrap();
        let plan =
            crate::editor::workspace_file_ops::plan_workspace_move(&root, &source, &destination)
                .unwrap();
        let editor_path = source.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "# old\n".to_owned(), Some(editor_path))
        });
        visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));

        let menu_source = source.clone();
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            editor.context_menu = Some(super::ContextMenuState::Workspace {
                position: gpui::point(gpui::px(710.0), gpui::px(510.0)),
                path: menu_source,
            });
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let menu = visual.debug_bounds("workspace-context-menu-panel").unwrap();
        assert!(f32::from(menu.left()) >= 8.0);
        assert!(f32::from(menu.top()) >= 8.0);
        assert!(f32::from(menu.right()) <= 712.0);
        assert!(f32::from(menu.bottom()) <= 512.0);
        assert!(visual.debug_bounds("workspace-context-new-file").is_some());
        assert!(
            visual
                .debug_bounds("workspace-context-new-folder")
                .is_some()
        );
        assert!(visual.debug_bounds("workspace-context-rename").is_some());
        assert!(visual.debug_bounds("workspace-context-move").is_some());
        assert!(visual.debug_bounds("workspace-context-undo").is_some());
        for selector in [
            "workspace-context-new-file-icon",
            "workspace-context-new-folder-icon",
            "workspace-context-rename-icon",
            "workspace-context-move-icon",
            "workspace-context-undo-icon",
        ] {
            let icon = visual.debug_bounds(selector).unwrap();
            assert_eq!(f32::from(icon.size.width), 18.0, "{selector}");
            assert_eq!(f32::from(icon.size.height), 18.0, "{selector}");
        }

        editor.update(visual, |editor, cx| {
            let input = cx.new(|cx| {
                let mut block = super::Block::with_record(
                    cx,
                    super::BlockRecord::paragraph("new.md".to_owned()),
                );
                block.set_source_raw_mode();
                block
            });
            editor.context_menu = None;
            editor.workspace.operation_dialog = Some(super::WorkspaceOperationDialog {
                kind: super::WorkspaceOperationKind::Rename,
                source: source.clone(),
                input,
                plan: Some(super::WorkspacePendingPlan::Move(plan.clone())),
                error: None,
                running: false,
            });
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        assert!(visual.debug_bounds("workspace-operation-dialog").is_some());
        assert!(
            visual
                .debug_bounds("workspace-operation-destination-input")
                .is_some()
        );
        assert!(visual.debug_bounds("workspace-operation-status").is_some());
        let status_icon = visual
            .debug_bounds("workspace-operation-status-ready-icon")
            .unwrap();
        let status_svg = visual
            .debug_bounds("workspace-operation-status-ready-icon-svg")
            .unwrap();
        assert_eq!(status_icon.size, size(px(18.0), px(18.0)));
        assert_eq!(status_svg.size, size(px(14.0), px(14.0)));
        editor.update(visual, |editor, cx| {
            let dialog = editor.workspace.operation_dialog.as_mut().unwrap();
            dialog.running = true;
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        assert!(
            visual
                .debug_bounds("workspace-operation-status-progress-icon")
                .is_some()
        );
        editor.update(visual, |editor, cx| {
            let dialog = editor.workspace.operation_dialog.as_mut().unwrap();
            dialog.running = false;
            dialog.error = Some("Destination changed".to_owned());
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        assert!(
            visual
                .debug_bounds("workspace-operation-status-error-icon")
                .is_some()
        );
        assert!(visual.debug_bounds("confirm-workspace-operation").is_some());
        assert!(visual.debug_bounds("cancel-workspace-operation").is_some());
        let overlay = visual
            .debug_bounds("workspace-operation-dialog-overlay")
            .unwrap();
        let dialog = visual.debug_bounds("workspace-operation-dialog").unwrap();
        let title_icon = visual
            .debug_bounds("workspace-operation-title-icon")
            .unwrap();
        let title_label = visual
            .debug_bounds("workspace-operation-title-label")
            .unwrap();
        let input = visual
            .debug_bounds("workspace-operation-destination-input")
            .unwrap();
        assert_eq!(title_icon.size, size(px(22.0), px(22.0)));
        assert!(title_icon.left() >= dialog.left());
        assert!(title_label.left() > title_icon.right());
        assert!(title_label.right() <= dialog.right());
        assert!(dialog.left() >= overlay.left());
        assert!(dialog.right() <= overlay.right());
        assert!(dialog.top() >= overlay.top());
        assert!(dialog.bottom() <= overlay.bottom());
        assert!(input.left() >= dialog.left());
        assert!(input.right() <= dialog.right());
        for selector in ["cancel-workspace-operation", "confirm-workspace-operation"] {
            let action = visual.debug_bounds(selector).unwrap();
            assert!(action.left() >= dialog.left(), "{selector}");
            assert!(action.right() <= dialog.right(), "{selector}");
            assert!(f32::from(action.size.width) >= 72.0, "{selector}");
            assert_eq!(f32::from(action.size.height), 36.0, "{selector}");
        }
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn workspace_context_menu_keyboard_skips_root_only_commands_and_opens_dialog(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root = std::env::temp_dir().join(format!(
            "gmark-workspace-context-keyboard-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let (editor, visual) =
            cx.add_window_view(|_window, cx| super::Editor::from_markdown(cx, String::new(), None));
        editor.update_in(visual, |editor, window, cx| {
            editor.workspace.root = Some(root.clone());
            editor.context_menu = Some(super::ContextMenuState::Workspace {
                position: gpui::point(gpui::px(40.0), gpui::px(40.0)),
                path: root.clone(),
            });
            assert!(editor.handle_context_menu_key(&key_event("down"), window, cx));
            assert_eq!(editor.context_menu_keyboard_item, Some(0));
            editor.context_menu_keyboard_item = Some(1);
            assert!(editor.handle_context_menu_key(&key_event("down"), window, cx));
            assert_eq!(
                editor.context_menu_keyboard_item,
                Some(0),
                "rename, move, and undo are unavailable for the root"
            );
            assert!(editor.handle_context_menu_key(&key_event("enter"), window, cx));
            assert!(editor.context_menu.is_none());
            assert_eq!(editor.context_menu_keyboard_item, None);
            let dialog = editor
                .workspace
                .operation_dialog
                .as_ref()
                .expect("new-file dialog");
            assert_eq!(dialog.kind, super::WorkspaceOperationKind::NewFile);
            assert!(dialog.input.read(cx).focus_handle.is_focused(window));
        });
        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn moving_open_workspace_file_updates_editor_path_and_preserves_source(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root =
            std::env::temp_dir().join(format!("gmark-open-file-move-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("old.md");
        let destination = root.join("new.md");
        fs::write(&source, "# old\n").unwrap();
        let plan =
            crate::editor::workspace_file_ops::plan_workspace_move(&root, &source, &destination)
                .unwrap();
        let editor_path = source.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "# old\n".to_owned(), Some(editor_path))
        });

        editor.update(visual, |editor, cx| {
            editor.execute_workspace_move_plan(plan.clone(), false, cx);
        });
        visual.run_until_parked();
        editor.update(visual, |editor, cx| {
            let canonical_destination = dunce::canonicalize(&destination).unwrap();
            assert_eq!(editor.file_path.as_ref(), Some(&canonical_destination));
            assert_eq!(editor.source_document.text(), "# old\n");
            assert!(!editor.document_dirty);
            assert!(editor.workspace.undo_file_operation.is_some());
            assert!(editor.workspace.file_operation_task.is_none());
            let _ = cx;
        });
        assert!(!source.exists());
        assert!(destination.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn dirty_open_file_blocks_workspace_move_before_disk_mutation(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root =
            std::env::temp_dir().join(format!("gmark-dirty-file-move-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("old.md");
        let destination = root.join("new.md");
        fs::write(&source, "# old\n").unwrap();
        let plan =
            crate::editor::workspace_file_ops::plan_workspace_move(&root, &source, &destination)
                .unwrap();
        let editor_path = source.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "# old\n".to_owned(), Some(editor_path))
        });

        editor.update(visual, |editor, cx| {
            editor.document_dirty = true;
            editor.execute_workspace_move_plan(plan.clone(), false, cx);
            assert!(editor.workspace.file_operation_task.is_none());
            assert!(editor.workspace.operation_error.is_some());
        });
        visual.run_until_parked();
        assert!(source.exists());
        assert!(!destination.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn creating_and_undoing_markdown_file_updates_open_document(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root =
            std::env::temp_dir().join(format!("gmark-create-file-ui-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let existing = root.join("existing.md");
        fs::write(&existing, "# existing\n").unwrap();
        let plan = crate::editor::workspace_file_ops::plan_workspace_create(
            &root,
            &root,
            "created.md",
            crate::editor::workspace_file_ops::WorkspaceCreateKind::MarkdownFile,
        )
        .unwrap();
        let created = plan.path.clone();
        let editor_path = existing.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "# existing\n".to_owned(), Some(editor_path))
        });

        editor.update(visual, |editor, cx| {
            editor.execute_workspace_create_plan(plan.clone(), false, cx);
        });
        visual.run_until_parked();
        editor.update(visual, |editor, cx| {
            assert_eq!(editor.file_path.as_ref(), Some(&created));
            assert_eq!(editor.source_document.text(), "");
            let Some(super::WorkspaceUndoOperation::Create(plan)) =
                editor.workspace.undo_file_operation.clone()
            else {
                panic!("missing create undo plan");
            };
            editor.execute_workspace_create_plan(plan, true, cx);
        });
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.file_path, None);
            assert!(editor.workspace.undo_file_operation.is_none());
        });
        assert!(!created.exists());
        assert!(existing.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn workspace_drop_prefills_review_dialog_without_moving_disk(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root = std::env::temp_dir().join(format!("gmark-drop-review-{}", uuid::Uuid::new_v4()));
        let target = root.join("archive");
        fs::create_dir_all(&target).unwrap();
        let source = root.join("note.md");
        fs::write(&source, "# note\n").unwrap();
        let editor_path = source.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "# note\n".to_owned(), Some(editor_path))
        });

        visual.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.workspace.root = Some(root.clone());
                editor.open_workspace_drop_move_dialog(source.clone(), target.clone(), window, cx);
                let dialog = editor.workspace.operation_dialog.as_ref().unwrap();
                assert_eq!(dialog.kind, super::WorkspaceOperationKind::Move);
                assert_eq!(dialog.input.read(cx).display_text(), "archive/note.md");
                assert!(dialog.plan.is_none());
            });
            window.draw(cx).clear();
        });
        assert!(visual.debug_bounds("workspace-operation-dialog").is_some());
        assert!(source.exists());
        assert!(!target.join("note.md").exists());

        let _ = fs::remove_dir_all(root);
    }

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
