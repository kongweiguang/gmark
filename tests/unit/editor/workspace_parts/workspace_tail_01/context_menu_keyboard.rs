// @author kongweiguang

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
            assert_eq!(
                editor.context_menu_keyboard_item,
                Some(1),
                "opening the root is disabled, so reveal is the first command"
            );
            editor.context_menu_keyboard_item = Some(9);
            assert!(editor.handle_context_menu_key(&key_event("down"), window, cx));
            assert_eq!(
                editor.context_menu_keyboard_item,
                Some(1),
                "open, relative path, rename, move, undo, and delete are unavailable for the root"
            );
            editor.context_menu_keyboard_item = Some(4);
            assert!(editor.handle_context_menu_key(&key_event("enter"), window, cx));
            assert!(editor.context_menu.is_none());
            assert_eq!(editor.context_menu_keyboard_item, None);
            let dialog = editor
                .workspace
                .operation_dialog
                .as_ref()
                .expect("new-file dialog");
            assert_eq!(dialog.kind, super::WorkspaceOperationKind::NewFile);
            assert_eq!(dialog.input.read(cx).display_text(), "untitled.txt");
            assert!(dialog.input.read(cx).focus_handle.is_focused(window));
        });
        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn workspace_context_menu_copies_absolute_and_relative_paths(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root = std::env::temp_dir().join(format!(
            "gmark-workspace-copy-path-{}",
            uuid::Uuid::new_v4()
        ));
        let folder = root.join("notes");
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("daily.md");
        fs::write(&path, "# Daily\n").unwrap();
        let (editor, visual) =
            cx.add_window_view(|_window, cx| super::Editor::from_markdown(cx, String::new(), None));

        let relative_target = path.clone();
        editor.update_in(visual, |editor, window, cx| {
            editor.workspace.root = Some(root.clone());
            editor.context_menu = Some(super::ContextMenuState::Workspace {
                position: gpui::point(gpui::px(40.0), gpui::px(40.0)),
                path: relative_target,
            });
            editor.on_workspace_copy_relative_path_menu(
                &gpui::ClickEvent::default(),
                window,
                cx,
            );
        });
        visual.update(|_window, cx| {
            assert_eq!(
                cx.read_from_clipboard().and_then(|item| item.text()),
                Some("notes/daily.md".to_owned())
            );
        });

        let absolute_target = path.clone();
        editor.update_in(visual, |editor, window, cx| {
            editor.context_menu = Some(super::ContextMenuState::Workspace {
                position: gpui::point(gpui::px(40.0), gpui::px(40.0)),
                path: absolute_target,
            });
            editor.on_workspace_copy_path_menu(&gpui::ClickEvent::default(), window, cx);
        });
        visual.update(|_window, cx| {
            assert_eq!(
                cx.read_from_clipboard().and_then(|item| item.text()),
                Some(path.to_string_lossy().into_owned())
            );
        });

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn workspace_new_file_and_folder_confirm_once_and_keep_tree_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root = std::env::temp_dir().join(format!(
            "gmark-workspace-create-once-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let existing = root.join("existing.md");
        fs::write(&existing, "# existing\n").unwrap();
        let tree = scan_workspace_dir(&root).unwrap();
        let root_id = tree.id.clone();
        let (editor, visual) = cx.add_window_view({
            let existing = existing.clone();
            move |_window, cx| {
                super::Editor::from_markdown(cx, "# existing\n".to_owned(), Some(existing))
            }
        });
        visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));

        editor.update_in(visual, |editor, window, cx| {
            editor.workspace.is_open = true;
            editor.workspace.active_tab = super::WorkspaceTab::Files;
            editor.workspace.root = Some(root.clone());
            editor.workspace.explicit_root = Some(root.clone());
            editor.workspace.file_tree = Some(tree.clone());
            editor.workspace.expanded.insert(root_id.clone());
            editor.context_menu = Some(super::ContextMenuState::Workspace {
                position: gpui::point(gpui::px(40.0), gpui::px(40.0)),
                path: root.clone(),
            });
            editor.on_workspace_new_file_menu(&gpui::ClickEvent::default(), window, cx);
            let input = editor
                .workspace
                .operation_dialog
                .as_ref()
                .expect("new-file dialog")
                .input
                .clone();
            input.update(cx, |block, cx| {
                let len = block.display_text().len();
                block.replace_text_in_visible_range(0..len, "created.txt", None, false, cx);
            });
        });
        visual.update(|window, cx| window.draw(cx).clear());
        // Enter 必须走 BlockHostAction 携带的文本，避免在同一 GPUI 更新租约内
        // 回读输入 Block；真实 Windows 键盘事件曾在这里触发 double lease panic。
        visual.simulate_keystrokes("enter");
        visual.run_until_parked();
        let created_file = root.join("created.txt");
        editor.update(visual, |editor, _cx| {
            assert!(editor.workspace.operation_dialog.is_none());
            assert!(editor.workspace.file_operation_task.is_none());
            assert_eq!(
                editor.workspace.selected,
                Some(super::WorkspaceSelection::File(created_file.clone()))
            );
            assert_eq!(editor.file_path.as_ref(), Some(&created_file));
        });
        assert_eq!(fs::read(&created_file).unwrap(), Vec::<u8>::new());

        editor.update_in(visual, |editor, window, cx| {
            editor.context_menu = Some(super::ContextMenuState::Workspace {
                position: gpui::point(gpui::px(40.0), gpui::px(40.0)),
                path: root.clone(),
            });
            editor.on_workspace_new_folder_menu(&gpui::ClickEvent::default(), window, cx);
            let input = editor
                .workspace
                .operation_dialog
                .as_ref()
                .expect("new-folder dialog")
                .input
                .clone();
            input.update(cx, |block, cx| {
                let len = block.display_text().len();
                block.replace_text_in_visible_range(0..len, "new-folder", None, false, cx);
            });
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let confirm = visual.debug_bounds("confirm-workspace-operation").unwrap();
        visual.simulate_click(confirm.center(), gpui::Modifiers::default());
        visual.run_until_parked();
        let created_folder = root.join("new-folder");
        editor.update(visual, |editor, _cx| {
            assert!(editor.workspace.operation_dialog.is_none());
            assert!(editor.workspace.file_operation_task.is_none());
            assert_eq!(
                editor.workspace.selected,
                Some(super::WorkspaceSelection::File(created_folder.clone()))
            );
            assert!(
                editor
                    .workspace
                    .expanded
                    .contains(&created_folder.to_string_lossy().into_owned())
            );
        });
        assert!(created_folder.is_dir());

        let _ = fs::remove_dir_all(root);
    }
