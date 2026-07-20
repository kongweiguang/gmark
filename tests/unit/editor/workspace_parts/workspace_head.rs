// @author kongweiguang

    use super::{
        WORKSPACE_COMPACT_OVERLAY_WIDTH, WORKSPACE_PANEL_MAX_WIDTH, WORKSPACE_PANEL_MIN_WIDTH,
        WORKSPACE_RESIZE_HIT_WIDTH, WorkspaceKeyboardZone, WorkspaceSearchMatch,
        WorkspaceSearchOptions, WorkspaceSelection, WorkspaceState, WorkspaceTab,
        WorkspaceTreeKind, build_outline_tree, insert_workspace_directory, prune_outline_state,
        rank_quick_open_paths, scan_workspace_dir, search_workspace,
        workspace_panel_width_for_viewport, workspace_uses_overlay,
    };
    use gpui::{AppContext as _, KeyDownEvent, Keystroke, Modifiers, MouseButton, point, px, size};
    use std::fs;
    use std::path::PathBuf;

    fn init_workspace_test_app(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::i18n::I18nManager::init(cx);
            crate::theme::ThemeManager::init(cx);
            crate::components::init(cx);
        });
    }

    fn key_event(key: &str) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: Keystroke::parse(key).expect("valid workspace test keystroke"),
            is_held: false,
        }
    }

    fn assert_workspace_header_layout(visual: &mut gpui::VisualTestContext) {
        let panel = visual.debug_bounds("workspace-panel").unwrap();
        let header = visual.debug_bounds("workspace-panel-header").unwrap();
        assert_eq!(f32::from(header.size.height), 44.0);
        assert!(header.left() >= panel.left());
        assert!(header.right() <= panel.right());
        assert!(visual.debug_bounds("workspace-collapse").is_none());
        for (selector, icon_selector) in [
            ("workspace-tab-files", "workspace-tab-files-icon"),
            ("workspace-tab-outline", "workspace-tab-outline-icon"),
            ("workspace-tab-search", "workspace-tab-search-icon"),
        ] {
            let tab = visual.debug_bounds(selector).unwrap();
            let icon = visual.debug_bounds(icon_selector).unwrap();
            assert_eq!(tab.size, size(px(32.0), px(32.0)), "{selector}");
            assert_eq!(icon.size, size(px(16.0), px(16.0)), "{selector}");
            assert!(tab.left() >= header.left(), "{selector}");
            assert!(tab.right() <= header.right(), "{selector}");
            assert!(icon.left() >= tab.left(), "{selector}");
            assert!(icon.right() <= tab.right(), "{selector}");
            assert!(icon.top() >= tab.top(), "{selector}");
            assert!(icon.bottom() <= tab.bottom(), "{selector}");
        }
    }

    #[gpui::test]
    async fn workspace_tooltip_waits_and_cancels_on_pointer_leave(cx: &mut gpui::TestAppContext) {
        init_workspace_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "# Heading".to_owned(), None)
        });
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            editor.set_workspace_tooltip_hover("workspace-tab-files", true, cx);
            assert_eq!(editor.workspace.tooltip_visible, None);
        });

        visual
            .executor()
            .advance_clock(super::TOOLTIP_DELAY - std::time::Duration::from_millis(1));
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.workspace.tooltip_visible, None);
        });
        visual
            .executor()
            .advance_clock(std::time::Duration::from_millis(1));
        visual.run_until_parked();
        editor.update(visual, |editor, cx| {
            assert_eq!(
                editor.workspace.tooltip_visible,
                Some("workspace-tab-files")
            );
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let panel = visual.debug_bounds("workspace-panel").unwrap();
        let tooltip = visual.debug_bounds("workspace-tooltip").unwrap();
        assert!(tooltip.left() >= panel.left());
        assert!(tooltip.right() <= panel.right());
        editor.update(visual, |editor, cx| {
            editor.set_workspace_tooltip_hover("workspace-tab-files", false, cx);
            assert_eq!(editor.workspace.tooltip_visible, None);
        });
    }

    #[test]
    fn workspace_scan_keeps_dirs_and_md_files_only() {
        let root =
            std::env::temp_dir().join(format!("gmark-workspace-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("nested")).expect("create dirs");
        fs::write(root.join("a.md"), "a").expect("write md");
        fs::write(root.join("a.txt"), "ignored").expect("write txt");
        fs::write(root.join("ignored.md"), "ignored").expect("write ignored md");
        fs::write(root.join(".gitignore"), "ignored.md\n").expect("write gitignore");
        fs::write(root.join("nested").join("b.md"), "b").expect("write nested md");

        let tree = scan_workspace_dir(&root).expect("scan tree");
        let labels = tree
            .children
            .iter()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["nested", "a.md"]);
        assert!(matches!(
            tree.children[0].kind,
            WorkspaceTreeKind::Directory(_)
        ));
        assert!(matches!(
            tree.children[1].kind,
            WorkspaceTreeKind::MarkdownFile(_)
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicitly_created_empty_directory_can_be_merged_into_scanned_tree() {
        let root =
            std::env::temp_dir().join(format!("gmark-empty-dir-tree-{}", uuid::Uuid::new_v4()));
        let empty = root.join("empty");
        fs::create_dir_all(&empty).unwrap();
        fs::write(root.join("note.md"), "note").unwrap();
        let mut tree = scan_workspace_dir(&root).unwrap();
        assert!(tree.children.iter().all(|node| node.label != "empty"));

        insert_workspace_directory(&mut tree, &root, &empty);
        assert!(tree.children.iter().any(|node| {
            matches!(&node.kind, WorkspaceTreeKind::Directory(path) if path == &empty)
        }));

        let _ = fs::remove_dir_all(root);
    }
