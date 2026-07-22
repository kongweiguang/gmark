// @author kongweiguang

    #[test]
    fn outline_tree_skips_headings_inside_fenced_code() {
        let outline = build_outline_tree(
            "# Root\n\n```md\n# ignored\n```\n\n## Child\n\n### Grandchild\n\n# Next",
        );

        assert_eq!(outline.len(), 2);
        assert_eq!(outline[0].label, "Root");
        assert_eq!(outline[0].children[0].label, "Child");
        assert_eq!(outline[0].children[0].children[0].label, "Grandchild");
        assert_eq!(outline[1].label, "Next");
    }

    #[test]
    fn outline_expansion_state_is_not_auto_populated_and_prunes_stale_ids() {
        let outline = build_outline_tree("# Root\n\n## Child\n\n# Next");
        let mut fresh = WorkspaceState::default();
        prune_outline_state(&mut fresh, &outline);
        assert!(fresh.expanded.is_empty());

        let mut existing = WorkspaceState::default();
        existing.expanded.insert("outline:0".to_string());
        existing.expanded.insert("outline:999".to_string());
        existing
            .expanded
            .insert("workspace-dir:C:/docs".to_string());
        existing.selected = Some(WorkspaceSelection::Outline("outline:999".to_string()));

        prune_outline_state(&mut existing, &outline);

        assert!(existing.expanded.contains("outline:0"));
        assert!(existing.expanded.contains("workspace-dir:C:/docs"));
        assert!(!existing.expanded.contains("outline:999"));
        assert_eq!(existing.selected, None);
    }

    #[test]
    fn workspace_panel_width_uses_ratio_with_bounds() {
        assert!(workspace_uses_overlay(899.0));
        assert!(!workspace_uses_overlay(900.0));
        assert_eq!(workspace_panel_width_for_viewport(720.0, None), 280.0);
        assert_eq!(workspace_panel_width_for_viewport(1000.0, None), 248.0);
        assert_eq!(workspace_panel_width_for_viewport(2000.0, None), 300.0);
        assert_eq!(workspace_panel_width_for_viewport(4000.0, None), 360.0);
        assert_eq!(
            workspace_panel_width_for_viewport(1000.0, Some(200.0)),
            200.0
        );
        assert_eq!(
            workspace_panel_width_for_viewport(1000.0, Some(320.0)),
            320.0
        );
        assert_eq!(
            workspace_panel_width_for_viewport(1000.0, Some(900.0)),
            360.0
        );
        assert_eq!(
            workspace_panel_width_for_viewport(720.0, Some(320.0)),
            280.0
        );
    }

    #[gpui::test]
    async fn workspace_visibility_separates_docked_preference_from_compact_overlay(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "# Heading".to_owned(), None)
        });

        editor.update(visual, |editor, _cx| {
            editor.sync_workspace_visibility_for_viewport(1180.0);
            assert!(editor.workspace.is_open);
            assert!(editor.workspace_docked_open_preference());

            editor.sync_workspace_visibility_for_viewport(720.0);
            assert!(!editor.workspace.is_open);
            editor.workspace.is_open = true;
            editor.sync_workspace_visibility_for_viewport(720.0);
            assert!(
                editor.workspace.is_open,
                "compact overlay must remain user-controlled"
            );

            editor.sync_workspace_visibility_for_viewport(1180.0);
            assert!(editor.workspace.is_open);
            editor.restore_workspace_docked_open_preference(Some(false));
            assert!(!editor.workspace.is_open);

            editor.sync_workspace_visibility_for_viewport(720.0);
            editor.workspace.is_open = true;
            editor.sync_workspace_visibility_for_viewport(1180.0);
            assert!(!editor.workspace.is_open);
        });
    }

    #[gpui::test]
    async fn workspace_resize_handle_previews_clamps_and_stays_out_of_compact_overlay(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "# Resize".to_owned(), None)
        });
        visual.simulate_resize(size(px(1180.0), px(780.0)));
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            editor.restore_workspace_panel_width(Some(248.0));
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());

        for width in [200.0, 248.0, 360.0] {
            editor.update(visual, |editor, cx| {
                editor.restore_workspace_panel_width(Some(width));
                cx.notify();
            });
            visual.update(|window, cx| window.draw(cx).clear());
            assert_eq!(
                f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
                width
            );
            assert_workspace_header_layout(visual);
        }
        editor.update(visual, |editor, cx| {
            editor.restore_workspace_panel_width(Some(248.0));
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());

        let initial = visual.debug_bounds("workspace-panel").unwrap();
        let handle = visual.debug_bounds("workspace-resize-handle").unwrap();
        let line = visual.debug_bounds("workspace-resize-line").unwrap();
        assert_eq!(f32::from(initial.size.width), 248.0);
        assert_eq!(f32::from(handle.size.width), WORKSPACE_RESIZE_HIT_WIDTH);
        assert_eq!(f32::from(line.size.width), 1.0);
        assert!((f32::from(initial.right() - line.center().x)).abs() <= 1.0);
        assert!(line.left() >= handle.left());
        assert!(line.right() <= handle.right());

        let source = editor.read_with(visual, |editor, _cx| editor.source_document.text());
        let revision = editor.read_with(visual, |editor, _cx| editor.source_document.revision());
        let dirty = editor.read_with(visual, |editor, _cx| editor.document_dirty);
        visual.simulate_click(handle.center(), Modifiers::default());
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update_in(visual, |editor, window, _cx| {
            assert!(
                editor
                    .workspace
                    .resize_focus_handle
                    .as_ref()
                    .is_some_and(|handle| handle.is_focused(window))
            );
        });
        visual.simulate_keystrokes("right shift-right");
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.workspace_panel_width(), Some(268.0));
        });
        visual.simulate_keystrokes("home");
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            WORKSPACE_PANEL_MIN_WIDTH
        );
        visual.simulate_keystrokes("end");
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            WORKSPACE_PANEL_MAX_WIDTH
        );
        visual.simulate_keystrokes("enter");
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, cx| {
            assert_eq!(editor.workspace_panel_width(), None);
            assert_eq!(
                editor.workspace_session_snapshot(cx).workspace_panel_width,
                None
            );
            assert_eq!(editor.source_document.text(), source);
            assert_eq!(editor.source_document.revision(), revision);
            assert_eq!(editor.document_dirty, dirty);
        });
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            248.0
        );

        visual.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_move(
            point(handle.center().x + px(80.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            328.0
        );
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.workspace_panel_width(), Some(328.0));
            assert!(editor.workspace.resize_session.is_some());
        });

        visual.simulate_mouse_move(
            point(handle.center().x + px(400.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.simulate_mouse_up(
            point(handle.center().x + px(400.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            WORKSPACE_PANEL_MAX_WIDTH
        );
        editor.update(visual, |editor, _cx| {
            assert_eq!(
                editor.workspace_panel_width(),
                Some(WORKSPACE_PANEL_MAX_WIDTH)
            );
            assert!(editor.workspace.resize_session.is_none());
        });

        visual.simulate_resize(size(px(720.0), px(520.0)));
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            WORKSPACE_COMPACT_OVERLAY_WIDTH
        );
        assert_workspace_header_layout(visual);
        // GPUI test inspector 会保留已卸载分支的旧 bounds；用事件无副作用证明 overlay
        // 当前树没有可交互 resize handle。
        visual.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_move(
            point(handle.center().x - px(100.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.simulate_mouse_up(
            point(handle.center().x - px(100.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        editor.update(visual, |editor, _cx| {
            assert_eq!(
                editor.workspace_panel_width(),
                Some(WORKSPACE_PANEL_MAX_WIDTH)
            );
            assert!(editor.workspace.resize_session.is_none());
        });
    }

    #[gpui::test]
    async fn right_workspace_docks_overlays_and_resizes_from_the_physical_left_edge(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        cx.update(|cx| {
            crate::config::EditorSettings::init(
                cx,
                true,
                crate::config::AutoSavePreference::Off,
                true,
            );
            crate::config::EditorSettings::set_workspace_sidebar_position_for_test(
                cx,
                crate::config::WorkspaceSidebarPosition::Right,
            );
        });
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "# Right sidebar".to_owned(), None)
        });
        visual.simulate_resize(size(px(1180.0), px(780.0)));
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            editor.restore_workspace_panel_width(Some(248.0));
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());

        let main = visual.debug_bounds("editor-main-content").unwrap();
        let content = visual.debug_bounds("editor-content").unwrap();
        let panel = visual.debug_bounds("workspace-panel").unwrap();
        let handle = visual.debug_bounds("workspace-resize-handle").unwrap();
        let line = visual.debug_bounds("workspace-resize-line").unwrap();
        assert_eq!(panel.right(), main.right());
        assert!(content.right() <= panel.left());
        assert!((f32::from(panel.left() - line.center().x)).abs() <= 1.0);
        assert!(handle.left() <= panel.left());
        assert!(handle.right() >= panel.left());
        assert!(visual.debug_bounds("document-tab-leading-tools").is_none());
        assert!(visual.debug_bounds("document-tab-trailing-tools").is_none());

        visual.simulate_click(handle.center(), Modifiers::default());
        visual.simulate_keystrokes("left");
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.workspace_panel_width(), Some(252.0));
        });
        visual.simulate_keystrokes("right");
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.workspace_panel_width(), Some(248.0));
        });

        let handle = visual.debug_bounds("workspace-resize-handle").unwrap();
        visual.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_move(
            point(handle.center().x - px(40.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.simulate_mouse_up(
            point(handle.center().x - px(40.0), handle.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            f32::from(visual.debug_bounds("workspace-panel").unwrap().size.width),
            288.0
        );

        visual.simulate_resize(size(px(720.0), px(520.0)));
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let main = visual.debug_bounds("editor-main-content").unwrap();
        let content = visual.debug_bounds("editor-content").unwrap();
        let overlay = visual.debug_bounds("compact-workspace-overlay").unwrap();
        let panel = visual.debug_bounds("workspace-panel").unwrap();
        assert_eq!(overlay.right(), main.right());
        assert_eq!(panel.right(), overlay.right());
        assert_eq!(content.left(), main.left());
        assert_eq!(content.right(), main.right());
        assert_eq!(f32::from(panel.size.width), WORKSPACE_COMPACT_OVERLAY_WIDTH);
    }

    #[test]
    fn quick_open_ranking_prefers_file_name_and_is_stable_when_empty() {
        let root = PathBuf::from("C:/notes");
        let paths = vec![
            root.join("z/readme.md"),
            root.join("readme/archive.md"),
            root.join("a/alpha.md"),
        ];
        let ranked = rank_quick_open_paths(&root, paths.clone(), "rdm");
        assert_eq!(ranked[0].relative_path, "z/readme.md");

        let all = rank_quick_open_paths(&root, paths, "");
        assert_eq!(
            all.iter()
                .map(|result| result.relative_path.as_str())
                .collect::<Vec<_>>(),
            vec!["a/alpha.md", "readme/archive.md", "z/readme.md"]
        );
    }

    #[test]
    fn workspace_search_supports_case_word_regex_utf8_and_gitignore() {
        let root = std::env::temp_dir().join(format!("gmark-search-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join(".gitignore"), "ignored.md\n").unwrap();
        fs::write(
            root.join("notes.md"),
            "Alpha alphabet\n中文 alpha\nissue-42 and issue-7",
        )
        .unwrap();
        fs::write(root.join("nested").join("more.markdown"), "alpha").unwrap();
        fs::write(root.join("ignored.md"), "alpha").unwrap();
        fs::write(root.join("plain.txt"), "alpha").unwrap();
        fs::write(root.join("legacy.md"), [b'c', b'a', b'f', 0xe9]).unwrap();

        let insensitive = search_workspace(&root, "alpha", WorkspaceSearchOptions::default())
            .expect("plain search");
        assert_eq!(insensitive.len(), 4);
        assert!(
            insensitive
                .iter()
                .all(|result| result.path != root.join("ignored.md"))
        );
        assert!(insensitive.iter().any(|result| {
            result.relative_path == "notes.md" && result.line == 2 && result.column == 4
        }));

        let case_sensitive = search_workspace(
            &root,
            "Alpha",
            WorkspaceSearchOptions {
                case_sensitive: true,
                ..WorkspaceSearchOptions::default()
            },
        )
        .unwrap();
        assert_eq!(case_sensitive.len(), 1);

        let whole_word = search_workspace(
            &root,
            "alpha",
            WorkspaceSearchOptions {
                whole_word: true,
                ..WorkspaceSearchOptions::default()
            },
        )
        .unwrap();
        assert_eq!(whole_word.len(), 3);

        let regex = search_workspace(
            &root,
            r"issue-\d+",
            WorkspaceSearchOptions {
                regex: true,
                ..WorkspaceSearchOptions::default()
            },
        )
        .unwrap();
        assert_eq!(regex.len(), 2);
        assert!(
            search_workspace(
                &root,
                "(",
                WorkspaceSearchOptions {
                    regex: true,
                    ..WorkspaceSearchOptions::default()
                }
            )
            .is_err()
        );
        let legacy = search_workspace(&root, "café", WorkspaceSearchOptions::default()).unwrap();
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].relative_path, "legacy.md");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn workspace_search_enforces_result_and_file_size_budgets() {
        let root =
            std::env::temp_dir().join(format!("gmark-search-budget-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("many.md"), "hit\n".repeat(700)).unwrap();
        let oversized = fs::File::create(root.join("oversized.md")).unwrap();
        oversized.set_len(super::SEARCH_MAX_FILE_BYTES + 1).unwrap();

        let results = search_workspace(&root, "hit", WorkspaceSearchOptions::default()).unwrap();
        assert_eq!(results.len(), super::SEARCH_MAX_RESULTS);
        assert!(
            results
                .iter()
                .all(|result| result.path.ends_with("many.md"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[gpui::test]
    async fn workspace_search_tab_renders_input_controls_and_results(
        cx: &mut gpui::TestAppContext,
    ) {
        init_workspace_test_app(cx);
        let root = std::env::temp_dir().join(format!("gmark-search-ui-{}", uuid::Uuid::new_v4()));
        let nested = root.join("a-very-long-workspace-directory-name");
        fs::create_dir_all(&nested).unwrap();
        let path = nested.join("a-very-long-document-name-that-keeps-extension.markdown");
        fs::write(&path, "needle here").unwrap();
        let editor_path = path.clone();
        let workspace_root = root.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "needle here".to_owned(), Some(editor_path))
        });

        editor.update(visual, |editor, cx| {
            editor.workspace.is_open = true;
            editor.workspace.root = Some(workspace_root.clone());
            editor.workspace.explicit_root = Some(workspace_root.clone());
            editor.set_workspace_tab(super::WorkspaceTab::Search, cx);
            let input = editor.ensure_workspace_search_input(cx);
            let expected = cx
                .global::<crate::i18n::I18nManager>()
                .strings()
                .workspace_search_prompt
                .clone();
            let placeholder = input.read(cx).input_placeholder();
            assert_eq!(
                placeholder.as_ref().map(|placeholder| placeholder.as_ref()),
                Some(expected.as_str())
            );
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let search_field = visual.debug_bounds("workspace-search-input").unwrap();
        let search_icon = visual
            .debug_bounds("workspace-search-input-icon")
            .unwrap();
        assert!(search_icon.left() >= search_field.left());
        assert!(search_icon.right() <= search_field.right());
        assert!(visual.debug_bounds("workspace-search-status").is_none());

        editor.update(visual, |editor, cx| {
            let input = editor.ensure_workspace_search_input(cx);
            input.update(cx, |input, cx| {
                input.replace_text_in_visible_range(0..0, "needle", None, false, cx);
            });
            editor.schedule_workspace_search(cx);
        });
        visual.executor().advance_clock(super::SEARCH_DEBOUNCE);
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear());
        visual.run_until_parked();

        assert!(visual.debug_bounds("workspace-search-input").is_some());
        assert!(visual.debug_bounds("workspace-search-case").is_some());
        assert!(visual.debug_bounds("workspace-search-word").is_some());
        assert!(visual.debug_bounds("workspace-search-regex").is_some());
        for (control, icon) in [
            ("workspace-search-case", "workspace-search-case-icon"),
            ("workspace-search-word", "workspace-search-word-icon"),
            ("workspace-search-regex", "workspace-search-regex-icon"),
        ] {
            let control = visual.debug_bounds(control).unwrap();
            let icon = visual.debug_bounds(icon).unwrap();
            assert_eq!(icon.size, size(px(15.0), px(15.0)));
            assert!(icon.left() >= control.left());
            assert!(icon.right() <= control.right());
        }
        let result = visual.debug_bounds("workspace-search-result-0").unwrap();
        let path = visual
            .debug_bounds("workspace-search-result-path-0")
            .unwrap();
        let location = visual
            .debug_bounds("workspace-search-result-location-0")
            .unwrap();
        assert!(path.left() >= result.left());
        assert!(path.right() <= location.left());
        assert!(location.right() <= result.right());
        assert!(path.bottom() <= result.bottom());
        assert!(location.bottom() <= result.bottom());

        let _ = fs::remove_dir_all(root);
    }

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
    async fn workspace_header_tabs_support_keyboard_activation(cx: &mut gpui::TestAppContext) {
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

        editor.update_in(visual, |editor, window, _cx| {
            let handle = &editor
                .workspace
                .header_focus_handles
                .as_ref()
                .expect("header focus handles")[1];
            handle.focus(window);
            assert!(handle.is_focused(window));
        });
        visual.update(|window, cx| window.draw(cx).clear());
        assert_workspace_header_layout(visual);
        visual.simulate_keystrokes("enter");
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.workspace.active_tab, WorkspaceTab::Outline);
        });

        visual.simulate_resize(size(px(1180.0), px(780.0)));
        visual.update(|window, cx| window.draw(cx).clear());
        assert_workspace_header_layout(visual);
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
        editor.update(visual, |editor, _cx| {
            assert!(editor.workspace.is_open);
            assert_eq!(
                editor
                    .workspace
                    .header_focus_handles
                    .as_ref()
                    .unwrap()
                    .len(),
                3
            );
        });
    }

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
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "current".to_owned(), None)
        });

        visual.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.workspace.is_open = true;
                editor.workspace.active_tab = WorkspaceTab::Files;
                editor.workspace.root = Some(root.clone());
                editor.workspace.explicit_root = Some(root.clone());
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
                Some(WorkspaceSelection::File(child.clone()))
            );
            assert_eq!(editor.workspace.keyboard_zone, WorkspaceKeyboardZone::Body);
        });

        editor.update_in(visual, |editor, window, cx| {
            assert!(editor.handle_workspace_key(&key_event("left"), window, cx));
        });
        editor.update(visual, |editor, _cx| {
            assert_eq!(
                editor.workspace.selected,
                Some(WorkspaceSelection::File(nested.clone()))
            );
        });
        editor.update_in(visual, |editor, window, cx| {
            assert!(editor.handle_workspace_key(&key_event("escape"), window, cx));
        });
        editor.update(visual, |editor, _cx| assert!(!editor.workspace.is_open));

        let _ = fs::remove_dir_all(root);
    }
