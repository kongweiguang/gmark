// @author kongweiguang

#[gpui::test]
async fn detaching_active_tab_transfers_full_state_to_new_editor(cx: &mut gpui::TestAppContext) {
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
async fn failed_detached_window_can_reattach_the_original_tab(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        super::Editor::from_markdown(cx, "detached".to_owned(), None)
    });

    editor.update(visual, |editor, cx| {
        add_inactive_tab(editor, "survivor", "survivor.md");
        let active_id = editor.tabs.records[0].id;
        let detached = editor.detach_tab_by_id(active_id, cx).unwrap();
        assert_eq!(editor.tabs.records.len(), 1);

        assert!(editor.reattach_detached_tab(detached, cx));
        assert_eq!(editor.tabs.records.len(), 2);
        assert_eq!(editor.source_document.text(), "detached");
    });
}

#[gpui::test]
async fn keyboard_tab_navigation_cycles_in_visual_order(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let (editor, visual) = cx
        .add_window_view(|_window, cx| super::Editor::from_markdown(cx, "first".to_owned(), None));
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
    let (editor, visual) = cx
        .add_window_view(|_window, cx| super::Editor::from_markdown(cx, "first".to_owned(), None));
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
