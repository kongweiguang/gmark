// @author kongweiguang

/// 校验工作区确认弹框沿用标准操作区，避免删除操作出现额外底部空白。
fn assert_workspace_dialog_actions(
    visual: &mut gpui::VisualTestContext,
    panel_selector: &'static str,
    actions_selector: &'static str,
    button_selectors: &[&'static str],
) {
    let panel = visual.debug_bounds(panel_selector).unwrap();
    let actions = visual.debug_bounds(actions_selector).unwrap();
    assert!(actions.left() >= panel.left(), "{actions_selector} escaped left");
    assert!(actions.right() <= panel.right(), "{actions_selector} escaped right");
    assert!(actions.bottom() <= panel.bottom(), "{actions_selector} escaped bottom");

    let first = visual.debug_bounds(button_selectors[0]).unwrap();
    let top_gap = f32::from(first.top()) - f32::from(actions.top());
    let bottom_gap = f32::from(panel.bottom()) - f32::from(first.bottom());
    assert!(
        (top_gap - bottom_gap).abs() <= 2.0,
        "{panel_selector} action padding should be symmetric: top={top_gap}, bottom={bottom_gap}"
    );

    for selector in button_selectors {
        let button = visual.debug_bounds(selector).unwrap();
        assert_eq!(f32::from(button.size.height), 36.0, "{selector} height");
        assert!(button.left() >= panel.left(), "{selector} escaped left");
        assert!(button.right() <= panel.right(), "{selector} escaped right");
        assert!(button.top() >= actions.top(), "{selector} escaped action top");
        assert!(button.bottom() <= panel.bottom(), "{selector} escaped bottom");
        assert_eq!(button.size.height, first.size.height, "{selector} height mismatch");
    }
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
            let dialog = editor
                .workspace
                .operation_dialog
                .as_ref()
                .expect("delete dialog while planning");
            assert!(dialog.plan.is_none());
            assert!(dialog.running);
            assert!(editor.workspace.file_operation_task.is_some());
        });
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            let dialog = editor
                .workspace
                .operation_dialog
                .as_ref()
                .expect("delete dialog after planning");
            assert!(dialog.plan.is_some());
            assert!(!dialog.running);
            assert!(editor.workspace.file_operation_task.is_none());
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
        assert_workspace_dialog_actions(
            visual,
            "workspace-delete-dialog",
            "workspace-delete-actions",
            &["cancel-workspace-delete", "confirm-workspace-delete"],
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

    #[gpui::test]
    async fn workspace_files_blank_area_opens_root_context_menu(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root = std::env::temp_dir().join(format!(
            "gmark-workspace-blank-context-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("note.md"), "# note\n").unwrap();
        let tree = scan_workspace_dir(&root).unwrap();
        let root_id = tree.id.clone();
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "document".to_owned(), None)
        });
        visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            editor.workspace.active_tab = super::WorkspaceTab::Files;
            editor.workspace.root = Some(root.clone());
            editor.workspace.explicit_root = Some(root.clone());
            editor.workspace.file_tree = Some(tree.clone());
            editor.workspace.expanded.insert(root_id);
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let panel = visual.debug_bounds("workspace-panel").unwrap();
        let blank = gpui::point(
            panel.left() + gpui::px(20.0),
            panel.top() + gpui::px(120.0),
        );
        visual.simulate_mouse_down(
            blank,
            gpui::MouseButton::Right,
            gpui::Modifiers::default(),
        );
        visual.simulate_mouse_up(
            blank,
            gpui::MouseButton::Right,
            gpui::Modifiers::default(),
        );
        editor.update(visual, |editor, _cx| {
            assert!(matches!(
                editor.context_menu,
                Some(super::ContextMenuState::Workspace { ref path, .. }) if path == &root
            ));
        });

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn workspace_delete_confirm_click_closes_dialog_and_removes_tempfile(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root = std::env::temp_dir().join(format!(
            "gmark-workspace-delete-click-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("delete-me.txt");
        fs::write(&target, b"temporary test content").unwrap();
        let tree = scan_workspace_dir(&root).unwrap();
        let root_id = tree.id.clone();
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "document".to_owned(), None)
        });
        visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
        editor.update_in(visual, |editor, window, cx| {
            editor.workspace.is_open = true;
            editor.workspace.active_tab = super::WorkspaceTab::Files;
            editor.workspace.root = Some(root.clone());
            editor.workspace.explicit_root = Some(root.clone());
            editor.workspace.file_tree = Some(tree.clone());
            editor.workspace.expanded.insert(root_id);
            editor.context_menu = Some(super::ContextMenuState::Workspace {
                position: gpui::point(gpui::px(40.0), gpui::px(40.0)),
                path: target.clone(),
            });
            editor.on_workspace_delete_menu(&gpui::ClickEvent::default(), window, cx);
        });
        // 删除规划在后台完成；先推进 worker，再绘制可点击的确认按钮。
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear());
        let confirm = visual.debug_bounds("confirm-workspace-delete").unwrap();
        visual.simulate_click(confirm.center(), gpui::Modifiers::default());
        visual.run_until_parked();
        editor.read_with(visual, |editor, _| {
            assert!(editor.workspace.file_operation_task.is_none());
            assert!(editor.workspace.operation_dialog.is_none());
            assert!(editor.workspace.operation_error.is_none());
            let tree = editor.workspace.file_tree.as_ref().expect("workspace tree");
            assert!(tree.children.iter().all(|node| {
                !matches!(
                    &node.kind,
                    super::WorkspaceTreeKind::File(path) if path == &target
                )
            }));
        });
        // 原因：GPUI 0.2.2 的 debug_bounds 会保留已移除元素的历史记录，终态必须以模型和磁盘事实验收，避免把测试框架缓存误判成界面仍在等待输入。
        assert!(!target.exists());

        let _ = fs::remove_dir_all(root);
    }
