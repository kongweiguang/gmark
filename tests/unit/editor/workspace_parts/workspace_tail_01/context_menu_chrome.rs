// @author kongweiguang

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
            editor.workspace.root = Some(root.clone());
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
        assert!(visual.debug_bounds("workspace-context-delete").is_some());
        assert!(visual.debug_bounds("workspace-context-undo").is_some());
        for selector in [
            "workspace-context-open-icon",
            "workspace-context-reveal-icon",
            "workspace-context-copy-path-icon",
            "workspace-context-copy-relative-path-icon",
            "workspace-context-new-file-icon",
            "workspace-context-new-folder-icon",
            "workspace-context-rename-icon",
            "workspace-context-move-icon",
            "workspace-context-refresh-icon",
            "workspace-context-undo-icon",
            "workspace-context-delete-icon",
        ] {
            let icon = visual.debug_bounds(selector).unwrap();
            assert_eq!(f32::from(icon.size.width), 18.0, "{selector}");
            assert_eq!(f32::from(icon.size.height), 18.0, "{selector}");
        }

        let delete_target = source.clone();
        editor.update_in(visual, |editor, window, cx| {
            editor.workspace.root = Some(root.clone());
            editor.context_menu = Some(super::ContextMenuState::Workspace {
                position: gpui::point(gpui::px(40.0), gpui::px(40.0)),
                path: delete_target,
            });
            editor.on_workspace_delete_menu(&gpui::ClickEvent::default(), window, cx);
            assert!(editor.workspace.operation_dialog.is_some());
        });
        visual.update(|window, cx| window.draw(cx).clear());
        assert!(visual.debug_bounds("workspace-delete-dialog").is_some());
        assert!(visual.debug_bounds("workspace-delete-target").is_some());
        assert!(visual.debug_bounds("workspace-delete-actions").is_some());
        assert!(visual.debug_bounds("cancel-workspace-delete").is_some());
        assert!(visual.debug_bounds("confirm-workspace-delete").is_some());
        let delete_dialog = visual.debug_bounds("workspace-delete-dialog").unwrap();
        let delete_overlay = visual
            .debug_bounds("workspace-delete-dialog-overlay")
            .unwrap();
        assert!(delete_dialog.left() >= delete_overlay.left());
        assert!(delete_dialog.right() <= delete_overlay.right());
        assert!(delete_dialog.top() >= delete_overlay.top());
        assert!(delete_dialog.bottom() <= delete_overlay.bottom());
        let delete_content = visual
            .debug_bounds("workspace-delete-dialog-content")
            .unwrap();
        let delete_target = visual.debug_bounds("workspace-delete-target").unwrap();
        let delete_actions = visual.debug_bounds("workspace-delete-actions").unwrap();
        assert!(
            delete_target.bottom() <= delete_content.bottom(),
            "target={delete_target:?} content={delete_content:?}"
        );
        assert!(
            delete_content.bottom() <= delete_actions.top(),
            "content={delete_content:?} actions={delete_actions:?}"
        );

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
