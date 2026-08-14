// @author kongweiguang

    #[gpui::test]
    async fn workspace_delete_confirmation_blocks_dirty_open_file(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root = std::env::temp_dir().join(format!(
            "gmark-workspace-delete-dirty-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let alias = root.join("alias");
        fs::create_dir_all(&alias).unwrap();
        // Windows Runner 的临时目录可能使用 8.3 别名；显式保留一个跨平台路径别名，
        // 防止安全规范化后的删除路径绕过原始标签路径的脏状态检查。
        let path = alias.join("..").join("dirty.md");
        fs::write(&path, "# dirty\n").unwrap();
        let editor_path = path.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "# dirty\n".to_owned(), Some(editor_path))
        });

        editor.update_in(visual, |editor, window, cx| {
            editor.workspace.root = Some(root.clone());
            editor.set_document_dirty_for_test(true);
            editor.context_menu = Some(super::ContextMenuState::Workspace {
                position: gpui::point(gpui::px(40.0), gpui::px(40.0)),
                path: path.clone(),
            });
            editor.on_workspace_delete_menu(&gpui::ClickEvent::default(), window, cx);
            let dialog = editor
                .workspace
                .operation_dialog
                .as_ref()
                .expect("delete confirmation while planning");
            assert_eq!(dialog.kind, super::WorkspaceOperationKind::Delete);
            assert!(dialog.plan.is_none());
            assert!(dialog.running);
        });
        visual.run_until_parked();
        editor.update(visual, |editor, cx| {
            let dialog = editor
                .workspace
                .operation_dialog
                .as_ref()
                .expect("delete confirmation after planning");
            assert!(!dialog.running);
            assert!(dialog.plan.is_none());
            assert_eq!(
                dialog.error.as_deref(),
                Some(
                    cx.global::<crate::i18n::I18nManager>()
                        .strings()
                        .workspace_delete_dirty_error
                        .as_str()
                )
            );
            assert!(editor.workspace.file_operation_task.is_none());
        });

        assert!(path.exists());
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
            editor.set_document_dirty_for_test(true);
            editor.execute_workspace_move_plan(plan.clone(), false, cx);
            assert!(editor.workspace.file_operation_task.is_some());
        });
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            assert!(editor.workspace.file_operation_task.is_none());
            assert!(editor.workspace.operation_error.is_some());
        });
        assert!(source.exists());
        assert!(!destination.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn creating_arbitrary_file_opens_source_tab_and_can_be_undone(
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
            "created.java",
            crate::editor::workspace_file_ops::WorkspaceCreateKind::File,
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
            assert_eq!(editor.view_mode, super::ViewMode::Source);
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
            assert!(editor.document_host.is_none());
            assert!(editor.workspace.undo_file_operation.is_none());
        });
        assert!(!created.exists());
        assert!(existing.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn newly_created_empty_text_file_accepts_the_first_edit_without_external_conflict(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root = std::env::temp_dir().join(format!(
            "gmark-create-empty-edit-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let existing = root.join("existing.md");
        fs::write(&existing, "# existing\n").unwrap();
        let plan = crate::editor::workspace_file_ops::plan_workspace_create(
            &root,
            &root,
            "empty.txt",
            crate::editor::workspace_file_ops::WorkspaceCreateKind::File,
        )
        .unwrap();
        let created = plan.path.clone();
        let editor_path = existing.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "# existing\n".to_owned(), Some(editor_path))
        });

        editor.update(visual, |editor, cx| {
            editor.execute_workspace_create_plan(plan, false, cx);
        });
        visual.run_until_parked();
        let source_host = editor
            .read_with(visual, |editor, _cx| editor.document_host.clone())
            .expect("new text file source host");
        // 等待首轮外部变更检查完成再输入，覆盖真实用户在新建弹框结束后才开始编辑的时序。
        visual
            .executor()
            .advance_clock(std::time::Duration::from_millis(1_100));
        visual.run_until_parked();
        visual.update(|window, cx| {
            source_host.update(cx, |host, cx| host.begin_line_edit_for_test(0, window, cx));
        });
        let (_, row) = source_host
            .read_with(visual, |host, _cx| host.active_edit_for_test())
            .expect("empty file first source row");
        row.update(visual, |block, cx| {
            block.replace_text_in_visible_range(0..0, "hello", None, false, cx);
        });
        visual.run_until_parked();

        assert_eq!(
            source_host.read_with(visual, |host, _cx| host.source_text_for_test()),
            "hello"
        );
        assert!(editor.read_with(visual, |editor, _cx| editor.is_document_dirty()));
        assert_eq!(
            source_host.read_with(visual, |host, _cx| host.pending_external_change_for_test()),
            None
        );
        assert_eq!(fs::read(&created).unwrap(), Vec::<u8>::new());

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
