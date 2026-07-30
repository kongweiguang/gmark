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
