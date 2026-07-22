// @author kongweiguang

    #[gpui::test]
    async fn closing_only_tab_creates_fresh_untitled_and_can_reopen(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "only".to_owned(), Some(PathBuf::from("only.md")))
        });
        editor.update(visual, |editor, cx| {
            editor.request_close_tab_index(0, cx);
            assert_eq!(editor.tabs.records.len(), 1);
            assert_eq!(editor.source_document.text(), "");
            assert!(editor.file_path.is_none());
            assert!(!editor.document_dirty);
            assert_eq!(editor.tabs.closed.len(), 1);
        });
    }

    #[gpui::test]
    async fn pending_save_close_only_finishes_after_document_is_clean(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "dirty".to_owned(), None)
        });
        editor.update(visual, |editor, cx| {
            add_inactive_tab(editor, "survivor", "survivor.md");
            editor.set_document_dirty_for_test(true);
            editor.tabs.close_after_save = true;
            editor.finish_pending_tab_close_after_save(cx);
            assert_eq!(editor.tabs.records.len(), 2);

            editor.set_document_dirty_for_test(false);
            editor.finish_pending_tab_close_after_save(cx);
            assert_eq!(editor.tabs.records.len(), 1);
            assert_eq!(editor.source_document.text(), "survivor");
            assert!(!editor.tabs.close_after_save);
        });
    }

    #[gpui::test]
    async fn window_close_activates_background_dirty_tab(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "clean".to_owned(), None)
        });
        editor.update(visual, |editor, cx| {
            add_inactive_tab(editor, "dirty", "dirty.md");
            editor.tabs.records[1]
                .snapshot
                .as_mut()
                .unwrap()
                .document_dirty = true;

            assert!(editor.activate_dirty_tab_for_window_close(cx));
            assert_eq!(editor.tabs.active, 1);
            assert_eq!(editor.source_document.text(), "dirty");
            assert!(editor.document_dirty);
        });
    }

    #[gpui::test]
    async fn window_close_save_advances_to_next_dirty_tab(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "first dirty".to_owned(), None)
        });
        editor.update(visual, |editor, cx| {
            editor.set_document_dirty_for_test(true);
            add_inactive_tab(editor, "second dirty", "second.md");
            editor.tabs.records[1]
                .snapshot
                .as_mut()
                .unwrap()
                .document_dirty = true;

            assert!(!editor.prepare_window_close_save());
            assert!(editor.tabs.continue_window_close_after_save);
            editor.set_document_dirty_for_test(false);
            editor.continue_window_close_after_save(cx);

            assert_eq!(editor.tabs.active, 1);
            assert_eq!(editor.source_document.text(), "second dirty");
            assert!(editor.document_dirty);
            assert!(editor.show_unsaved_changes_dialog);
            assert!(!editor.tabs.continue_window_close_after_save);
        });
    }

    #[gpui::test]
    async fn pinning_and_reordering_preserve_active_snapshot_and_partitions(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "first".to_owned(), None)
        });
        editor.update(visual, |editor, cx| {
            add_inactive_tab(editor, "second", "second.md");
            add_inactive_tab(editor, "third", "third.md");
            let active_id = editor.tabs.records[0].id;

            assert!(editor.toggle_pin_tab(2, cx));
            assert!(editor.tabs.records[0].pinned);
            assert_eq!(editor.tabs.active, 1);
            assert_eq!(editor.tabs.records[1].id, active_id);

            assert!(editor.toggle_pin_tab(1, cx));
            assert_eq!(editor.pinned_tab_count(), 2);
            assert!(editor.reorder_tab(1, 0, cx));
            assert_eq!(editor.tabs.active, 0);
            assert_eq!(editor.tabs.records[0].id, active_id);

            // 未固定标签不能越过固定前缀，跨分区 drop 会被钳制到合法位置。
            assert!(!editor.reorder_tab(2, 0, cx));
            assert_eq!(editor.pinned_tab_count(), 2);
        });
    }

    #[gpui::test]
    async fn close_other_tabs_prompts_dirty_tabs_and_keeps_requested_tab(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "first".to_owned(), None)
        });
        let keep_id = editor.update(visual, |editor, cx| {
            add_inactive_tab(editor, "keep", "keep.md");
            add_inactive_tab(editor, "dirty", "dirty.md");
            editor.tabs.records[2]
                .snapshot
                .as_mut()
                .unwrap()
                .document_dirty = true;
            let keep_id = editor.tabs.records[1].id;
            editor.request_close_other_tabs(1, cx);
            assert_eq!(editor.tabs.records.len(), 2);
            assert!(editor.tabs.show_close_dialog);
            assert_eq!(editor.source_document.text(), "dirty");
            keep_id
        });
        visual.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.on_discard_tab_close(&gpui::ClickEvent::default(), window, cx);
                assert_eq!(editor.tabs.records.len(), 1);
                assert_eq!(editor.tabs.records[0].id, keep_id);
                assert_eq!(editor.source_document.text(), "keep");
                assert!(editor.tabs.close_others_keep.is_none());
            });
        });
    }

    #[gpui::test]
    async fn tab_context_menu_renders_stable_commands(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "first".to_owned(), None)
        });
        visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
        editor.update(visual, |editor, cx| {
            add_inactive_tab(editor, "second", "second.md");
            editor.tabs.context_menu = Some(super::TabContextMenu {
                index: 0,
                position: gpui::point(gpui::px(710.0), gpui::px(510.0)),
            });
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let strip = visual.debug_bounds("document-tab-strip").unwrap();
        for (tab_selector, close_selector) in [
            ("document-tab-0", "document-tab-close-0"),
            ("document-tab-1", "document-tab-close-1"),
        ] {
            let tab = visual.debug_bounds(tab_selector).unwrap();
            let close = visual.debug_bounds(close_selector).unwrap();
            assert!(tab.left() >= strip.left());
            assert!(tab.right() <= strip.right());
            assert!(f32::from(tab.size.width) <= super::TAB_MAX_WIDTH);
            assert!(close.left() >= tab.left());
            assert!(close.right() <= tab.right());
        }
        for index in 0..2 {
            let leading = visual
                .debug_bounds(match index {
                    0 => "document-tab-leading-0",
                    _ => "document-tab-leading-1",
                })
                .unwrap();
            let title = visual
                .debug_bounds(match index {
                    0 => "document-tab-title-0",
                    _ => "document-tab-title-1",
                })
                .unwrap();
            let close = visual
                .debug_bounds(match index {
                    0 => "document-tab-close-0",
                    _ => "document-tab-close-1",
                })
                .unwrap();
            assert_eq!(leading.size, size(px(16.0), px(16.0)));
            assert!(title.left() > leading.right());
            assert!(title.right() <= close.left());
        }
        let close = visual.debug_bounds("document-tab-close-0").unwrap();
        let dirty = visual.debug_bounds("document-tab-dirty-0").unwrap();
        let close_icon = visual.debug_bounds("document-tab-close-icon-0").unwrap();
        assert_eq!(f32::from(close.size.width), 18.0);
        assert_eq!(f32::from(close.size.height), 18.0);
        assert!(dirty.left() >= close.left());
        assert!(dirty.right() <= close.right());
        assert!(close_icon.left() >= close.left());
        assert!(close_icon.right() <= close.right());
        let menu = visual.debug_bounds("tab-context-menu").unwrap();
        assert!(f32::from(menu.left()) >= 8.0);
        assert!(f32::from(menu.top()) >= 8.0);
        assert!(f32::from(menu.right()) <= 712.0);
        assert!(f32::from(menu.bottom()) <= 512.0);
        assert!(visual.debug_bounds("tab-context-pin").is_some());
        assert!(visual.debug_bounds("tab-context-close").is_some());
        assert!(visual.debug_bounds("tab-context-close-others").is_some());
        for selector in [
            "tab-context-pin-icon",
            "tab-context-close-icon",
            "tab-context-close-others-icon",
        ] {
            let icon = visual.debug_bounds(selector).unwrap();
            assert_eq!(f32::from(icon.size.width), 18.0, "{selector}");
            assert_eq!(f32::from(icon.size.height), 18.0, "{selector}");
        }
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
    }

    #[gpui::test]
    async fn tab_icon_controls_show_compact_native_tooltips(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let (_editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "first".to_owned(), None)
        });
        visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
        visual.update(|window, cx| window.draw(cx).clear());
        let new_tab = visual.debug_bounds("document-new-tab").unwrap();
        assert_eq!(new_tab.size.width, gpui::px(28.0));
        assert_eq!(new_tab.size.height, gpui::px(28.0));

        visual.simulate_mouse_move(new_tab.center(), None, gpui::Modifiers::default());
        visual.executor().advance_clock(Duration::from_millis(520));
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear());
        let tooltip = visual.debug_bounds("ui-tooltip").unwrap();
        assert!(tooltip.size.width <= gpui::px(280.0));
        assert!(tooltip.size.height <= gpui::px(32.0));
        assert!(tooltip.left() >= gpui::px(0.0));
        assert!(tooltip.top() >= gpui::px(0.0));
        assert!(tooltip.right() <= gpui::px(720.0));
        assert!(tooltip.bottom() <= gpui::px(520.0));
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
    }

    #[gpui::test]
    async fn tab_context_menu_keyboard_skips_disabled_close_others(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "only tab".to_owned(), None)
        });
        let key = |name: &str| gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse(name).expect("valid menu key"),
            is_held: false,
        };
        editor.update_in(visual, |editor, window, cx| {
            editor.tabs.context_menu = Some(super::TabContextMenu {
                index: 0,
                position: gpui::point(gpui::px(40.0), gpui::px(40.0)),
            });
            assert!(editor.handle_context_menu_key(&key("down"), window, cx));
            assert_eq!(editor.context_menu_keyboard_item, Some(0));
            editor.context_menu_keyboard_item = Some(1);
            assert!(editor.handle_context_menu_key(&key("down"), window, cx));
            assert_eq!(
                editor.context_menu_keyboard_item,
                Some(0),
                "Close Others is disabled for a single tab"
            );
            assert!(editor.handle_context_menu_key(&key("escape"), window, cx));
            assert!(editor.tabs.context_menu.is_none());
        });
    }

    #[gpui::test]
    async fn tab_close_dialog_uses_standard_compact_layout(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "dirty".to_owned(), None)
        });
        visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
        editor.update(visual, |editor, cx| {
            editor.tabs.show_close_dialog = true;
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());

        let overlay = visual.debug_bounds("tab-close-dialog-overlay").unwrap();
        let dialog = visual.debug_bounds("tab-close-dialog").unwrap();
        let title_icon = visual.debug_bounds("tab-close-title-icon").unwrap();
        let title_label = visual.debug_bounds("tab-close-title-label").unwrap();
        assert_eq!(title_icon.size, gpui::size(gpui::px(22.0), gpui::px(22.0)));
        assert!(title_icon.left() >= dialog.left());
        assert!(title_label.left() > title_icon.right());
        assert!(title_label.right() <= dialog.right());
        assert!(dialog.left() >= overlay.left());
        assert!(dialog.right() <= overlay.right());
        assert!(dialog.top() >= overlay.top());
        assert!(dialog.bottom() <= overlay.bottom());
        for selector in ["cancel-tab-close", "discard-tab-close", "save-tab-close"] {
            let action = visual.debug_bounds(selector).unwrap();
            assert!(action.left() >= dialog.left(), "{selector}");
            assert!(action.right() <= dialog.right(), "{selector}");
            assert!(f32::from(action.size.width) >= 72.0, "{selector}");
            assert_eq!(f32::from(action.size.height), 36.0, "{selector}");
        }
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
    }

    #[gpui::test]
    async fn restoring_workspace_session_installs_order_pin_active_and_root(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test_app(cx);
        let root = std::env::temp_dir().join(format!("gmark-tab-restore-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let canonical_root = dunce::canonicalize(&root).unwrap();
        let first_path = root.join("first.md");
        let second_path = root.join("second.md");
        let legacy_path = root.join("legacy.md");
        let editor_path = first_path.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "first".to_owned(), Some(editor_path))
        });
        editor.update(visual, |editor, cx| {
            editor.restore_tab_session(
                uuid::Uuid::new_v4(),
                vec![
                    super::RestoredTab {
                        opened: crate::document_io::OpenedDocument::Resident(
                            crate::document_io::OpenedMarkdown {
                                text: "first".to_owned(),
                                encoding: crate::document_io::DocumentEncoding::Utf8,
                                text_encoding: gmark_document_core::TextEncoding::Utf8 { bom: false },
                                file_identity: None,
                                loading_limits: gmark_document_core::LoadingPolicy::default().effective_limits(),
                            },
                        ),
                        path: first_path.clone(),
                        pinned: true,
                        view_mode: Some("source".to_owned()),
                        selection: Some(
                            crate::config::workspace_session::WorkspaceSessionSelection {
                                start: 1,
                                end: 1,
                                reversed: false,
                                anchor_affinity: None,
                                head_affinity: None,
                            },
                        ),
                        scroll_x: Some(0.0),
                        scroll_y: Some(-10.0),
                    },
                    super::RestoredTab {
                        opened: crate::document_io::OpenedDocument::Resident(
                            crate::document_io::OpenedMarkdown {
                                text: "second".to_owned(),
                                encoding: crate::document_io::DocumentEncoding::Utf8,
                                text_encoding: gmark_document_core::TextEncoding::Utf8 { bom: false },
                                file_identity: None,
                                loading_limits: gmark_document_core::LoadingPolicy::default().effective_limits(),
                            },
                        ),
                        path: second_path.clone(),
                        pinned: false,
                        view_mode: Some("split".to_owned()),
                        selection: Some(
                            crate::config::workspace_session::WorkspaceSessionSelection {
                                start: 3,
                                end: 3,
                                reversed: true,
                                anchor_affinity: None,
                                head_affinity: None,
                            },
                        ),
                        scroll_x: Some(0.0),
                        scroll_y: Some(-42.0),
                    },
                    super::RestoredTab {
                        opened: crate::document_io::OpenedDocument::Resident(
                            crate::document_io::OpenedMarkdown {
                                text: "legacy".to_owned(),
                                encoding: crate::document_io::DocumentEncoding::Legacy(
                                    "windows-1252".to_owned(),
                                ),
                                text_encoding: gmark_document_core::TextEncoding::Legacy(
                                    "windows-1252".to_owned(),
                                ),
                                file_identity: None,
                                loading_limits: gmark_document_core::LoadingPolicy::default().effective_limits(),
                            },
                        ),
                        path: legacy_path.clone(),
                        pinned: false,
                        view_mode: Some("source".to_owned()),
                        selection: None,
                        scroll_x: None,
                        scroll_y: None,
                    },
                ],
                1,
                Some(root.clone()),
                Some(318.0),
                Some(false),
                Some(0.62),
                cx,
            );
            assert_eq!(editor.tabs.records.len(), 3);
            assert!(editor.tabs.records[0].pinned);
            assert_eq!(editor.tabs.active, 1);
            assert_eq!(editor.workspace_panel_width(), Some(318.0));
            assert!(!editor.workspace_docked_open_preference());
            assert_eq!(editor.split_pane_ratio, 0.62);
            assert_eq!(editor.source_document.text(), "second");
            assert_eq!(editor.view_mode, ViewMode::Split);
            assert_eq!(editor.last_selection_snapshot.range(), 3..3);
            assert!(
                !editor.last_selection_snapshot.reversed(),
                "collapsed SourceSelection has no directional ordering"
            );
            assert_eq!(f32::from(editor.scroll_handle.offset().y), -42.0);
            let legacy = editor.tabs.records[2].snapshot.as_ref().unwrap();
            assert_eq!(legacy.view_mode, ViewMode::Preview);
            assert!(legacy.show_encoding_conversion_dialog);
            assert_eq!(
                editor.explicit_workspace_root().as_deref(),
                Some(canonical_root.as_path())
            );

            let persisted = editor.workspace_session_snapshot(cx);
            assert_eq!(persisted.tabs.len(), 3);
            assert_eq!(persisted.active_index, 1);
            assert_eq!(
                persisted.workspace_root.as_deref(),
                Some(canonical_root.as_path())
            );
            assert_eq!(persisted.workspace_docked_open, Some(false));
            assert_eq!(persisted.split_pane_ratio, Some(0.62));
            assert_eq!(persisted.tabs[0].view_mode.as_deref(), Some("source"));
            assert_eq!(persisted.tabs[1].view_mode.as_deref(), Some("split"));
            assert_eq!(persisted.tabs[1].scroll_y, Some(-42.0));
            assert_eq!(persisted.tabs[2].view_mode.as_deref(), Some("preview"));
        });
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui::test]
    async fn detaching_active_tab_transfers_full_state_to_new_editor(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test_app(cx);
        let source_path = PathBuf::from("detached.md");
        let editor_path = source_path.clone();
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "detached dirty".to_owned(), Some(editor_path))
        });
        let detached = editor.update(visual, |editor, cx| {
            editor.set_document_dirty_for_test(true);
            editor.view_mode = ViewMode::Source;
            add_inactive_tab(editor, "survivor", "survivor.md");
            let active_id = editor.tabs.records[0].id;
            let detached = editor.detach_tab_by_id(active_id, cx).unwrap();
            assert_eq!(editor.tabs.records.len(), 1);
            assert_eq!(editor.source_document.text(), "survivor");
            detached
        });

        let (detached_editor, detached_visual) = cx.add_window_view(move |_window, cx| {
            let mut editor = super::Editor::from_markdown(cx, String::new(), None);
            editor.install_detached_tab(detached, cx);
            editor
        });
        detached_editor.update(detached_visual, |editor, _cx| {
            assert_eq!(editor.source_document.text(), "detached dirty");
            assert_eq!(editor.file_path.as_ref(), Some(&source_path));
            assert!(editor.document_dirty);
            assert_eq!(editor.view_mode, ViewMode::Source);
        });
    }

    #[gpui::test]
    async fn keyboard_tab_navigation_cycles_in_visual_order(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "first".to_owned(), None)
        });
        editor.update(visual, |editor, cx| {
            add_inactive_tab(editor, "second", "second.md");
            add_inactive_tab(editor, "third", "third.md");
            assert!(editor.toggle_pin_tab(2, cx));
            assert_eq!(editor.tabs.active, 1);
        });
        visual.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.on_next_tab_action(&crate::components::NextTab, window, cx);
                assert_eq!(editor.source_document.text(), "second");
                editor.on_next_tab_action(&crate::components::NextTab, window, cx);
                assert_eq!(editor.source_document.text(), "third");
                editor.on_previous_tab_action(&crate::components::PreviousTab, window, cx);
                assert_eq!(editor.source_document.text(), "second");
            });
        });
    }

    #[gpui::test]
    async fn tab_strip_keyboard_navigation_closes_clean_tabs_and_protects_dirty_tabs(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "first".to_owned(), None)
        });
        visual.simulate_resize(size(px(720.0), px(520.0)));
        editor.update(visual, |editor, cx| {
            add_inactive_tab(editor, "second", "second.md");
            add_inactive_tab(editor, "third", "third.md");
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());

        editor.update_in(visual, |editor, window, _cx| {
            let id = editor.tabs.records[0].id;
            let handle = editor.tabs.focus_handles.get(&id).expect("first tab focus");
            handle.focus(window);
            assert!(handle.is_focused(window));
        });
        visual.simulate_keystrokes("right");
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.tabs.active, 1);
            assert_eq!(editor.source_document.text(), "second");
        });
        visual.update(|window, cx| window.draw(cx).clear());
        visual.simulate_keystrokes("end");
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.tabs.active, 2);
            assert_eq!(editor.source_document.text(), "third");
        });
        visual.update(|window, cx| window.draw(cx).clear());
        visual.simulate_keystrokes("home");
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.tabs.active, 0);
            assert_eq!(editor.source_document.text(), "first");
        });

        let removed_id = editor.read_with(visual, |editor, _cx| editor.tabs.records[1].id);
        editor.update_in(visual, |editor, window, _cx| {
            editor
                .tabs
                .focus_handles
                .get(&removed_id)
                .expect("second tab focus")
                .focus(window);
        });
        visual.simulate_keystrokes("delete");
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.tabs.records.len(), 2);
            assert!(!editor.tabs.focus_handles.contains_key(&removed_id));
            assert_eq!(editor.source_document.text(), "first");
        });

        editor.update_in(visual, |editor, window, _cx| {
            editor
                .tabs
                .new_tab_focus_handle
                .as_ref()
                .expect("new tab focus")
                .focus(window);
        });
        visual.simulate_keystrokes("space");
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear());
        editor.update(visual, |editor, _cx| {
            assert_eq!(editor.tabs.records.len(), 3);
            assert_eq!(editor.tabs.active, 2);
            assert_eq!(editor.source_document.text(), "");
        });

        editor.update(visual, |editor, _cx| editor.document_dirty = true);
        editor.update_in(visual, |editor, window, _cx| {
            let active_id = editor.tabs.records[editor.tabs.active].id;
            editor
                .tabs
                .focus_handles
                .get(&active_id)
                .expect("active tab focus")
                .focus(window);
        });
        visual.simulate_keystrokes("delete");
        visual.run_until_parked();
        editor.update(visual, |editor, _cx| {
            assert!(editor.tabs.show_close_dialog);
            assert_eq!(editor.tabs.records.len(), 3);
            assert!(editor.document_dirty);
        });

        for viewport in [size(px(720.0), px(520.0)), size(px(1180.0), px(780.0))] {
            visual.simulate_resize(viewport);
            visual.update(|window, cx| window.draw(cx).clear());
            let strip = visual.debug_bounds("document-tab-strip").unwrap();
            let new_tab = visual.debug_bounds("document-new-tab").unwrap();
            assert_eq!(new_tab.size, size(px(28.0), px(28.0)));
            assert!(new_tab.left() >= strip.left());
            assert!(new_tab.right() <= strip.right());
        }
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
    }

    #[gpui::test]
    async fn explicit_close_and_quit_keep_distinct_session_intent(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "dirty".to_owned(), None)
        });
        visual.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_document_dirty_for_test(true);
                assert!(!editor.on_window_should_close(window, cx));
                assert!(editor.tabs.remove_session_after_window_close);
                editor.on_cancel_close_dialog(&gpui::ClickEvent::default(), window, cx);
                assert!(!editor.tabs.remove_session_after_window_close);

                assert!(!editor.on_window_should_close_for_quit(window, cx));
                assert!(!editor.tabs.remove_session_after_window_close);
            });
        });
    }

    #[gpui::test]
    async fn window_bounds_observer_populates_workspace_session_snapshot(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test_app(cx);
        let path = PathBuf::from("window-state.md");
        let (editor, visual) = cx.add_window_view(move |_window, cx| {
            super::Editor::from_markdown(cx, "window state".to_owned(), Some(path))
        });
        visual.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.install_workspace_session_window_observer(window, cx);
                let snapshot = editor.workspace_session_snapshot(cx);
                let restored = snapshot
                    .window
                    .expect("window placement should be captured");
                assert!(restored.width > 0.0);
                assert!(restored.height > 0.0);
                assert_eq!(
                    restored.state,
                    crate::config::workspace_session::WorkspaceSessionWindowState::Windowed
                );
            });
        });
    }

    #[gpui::test]
    async fn registry_sessions_open_as_independent_editor_windows(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let root =
            std::env::temp_dir().join(format!("gmark-window-registry-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let first = root.join("first.md");
        let second = root.join("second.md");
        std::fs::write(&first, "first window").unwrap();
        std::fs::write(&second, "second window").unwrap();
        let sessions = [first.clone(), second.clone()].map(|path| {
            crate::config::workspace_session::WorkspaceSession::new(
                uuid::Uuid::new_v4(),
                vec![crate::config::workspace_session::WorkspaceSessionTab::new(
                    path, false,
                )],
                0,
                Some(root.clone()),
            )
        });
        cx.update(|cx| {
            for session in sessions {
                assert!(crate::app_menu::open_workspace_session_window(cx, session));
            }
            assert_eq!(cx.windows().len(), 2);
        });
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui::test]
    async fn multiple_recovery_journals_open_as_dirty_tabs(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let recovery_dir = tempfile::tempdir().unwrap();
        let mut first =
            crate::recovery::RecoveryJournal::create(recovery_dir.path(), None, "alpha".to_owned())
                .unwrap();
        first
            .record(
                "alpha recovered",
                crate::recovery::RecoverySelection {
                    start: 2,
                    end: 2,
                    reversed: false,
                    anchor_affinity: None,
                    head_affinity: None,
                },
                "source",
            )
            .unwrap();
        let mut second =
            crate::recovery::RecoveryJournal::create(recovery_dir.path(), None, "beta".to_owned())
                .unwrap();
        second
            .record(
                "beta recovered",
                crate::recovery::RecoverySelection {
                    start: 3,
                    end: 3,
                    reversed: false,
                    anchor_affinity: None,
                    head_affinity: None,
                },
                "split",
            )
            .unwrap();
        let recovered = crate::recovery::load_recovery_documents(recovery_dir.path()).unwrap();
        let handle = cx
            .update(|cx| crate::app_menu::open_recovered_editor_tabs_window(cx, recovered))
            .expect("recovery window");
        handle
            .update(cx, |editor, _window, cx| {
                assert_eq!(editor.tabs.records.len(), 2);
                assert!(editor.document_dirty);
                assert!(editor.recovered_session);
                assert!(
                    editor.tabs.records[1]
                        .snapshot
                        .as_ref()
                        .is_some_and(
                            |snapshot| snapshot.document_dirty && snapshot.recovered_session
                        )
                );
                let first_source = editor.source_document.text();
                assert!(editor.switch_to_tab_index(1, cx));
                let second_source = editor.source_document.text();
                let mut sources = vec![first_source, second_source];
                sources.sort();
                assert_eq!(sources, vec!["alpha recovered", "beta recovered"]);
                assert!(matches!(
                    editor.view_mode,
                    ViewMode::Source | ViewMode::Split
                ));
            })
            .unwrap();
    }

    #[test]
    fn restored_selection_clamps_to_utf8_boundaries_and_document_end() {
        let selection = crate::config::workspace_session::WorkspaceSessionSelection {
            start: 2,
            end: usize::MAX,
            reversed: true,
            anchor_affinity: None,
            head_affinity: None,
        };
        let restored = super::Editor::restored_selection("你a", Some(&selection));
        assert_eq!(restored.range(), 0..4);
        assert!(restored.reversed());
    }

    #[test]
    fn legacy_view_ids_are_case_insensitive_and_structure_maps_to_preview() {
        for value in ["Source", "source"] {
            assert_eq!(super::Editor::restored_view_mode(Some(value)), ViewMode::Source);
        }
        for value in ["Preview", "Structure", "structure"] {
            assert_eq!(
                super::Editor::restored_view_mode(Some(value)),
                ViewMode::Preview
            );
        }
        assert_eq!(
            super::Editor::restored_view_mode(Some("Split")),
            ViewMode::Split
        );
        assert_eq!(
            super::Editor::restored_view_mode(Some("Live")),
            ViewMode::Rendered
        );
    }
