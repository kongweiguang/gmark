// @author kongweiguang

#[gpui::test]
async fn captured_slash_menu_keys_filter_navigate_and_execute(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "/heading".to_string(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(block.visible_len(), block_cx);
            block.refresh_slash_menu(block_cx);
        });
    });
    redraw(cx);

    let down = KeyDownEvent {
        keystroke: Keystroke::parse("down").expect("valid down keystroke"),
        is_held: false,
    };
    let enter = KeyDownEvent {
        keystroke: Keystroke::parse("enter").expect("valid enter keystroke"),
        is_held: false,
    };
    editor.update_in(cx, |editor, window, cx| {
        editor.on_editor_key_down_capture(&down, window, cx);
        editor.on_editor_key_down_capture(&enter, window, cx);
    });
    redraw(cx);

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        assert_eq!(block.read(cx).kind(), BlockKind::Heading { level: 2 });
        assert_eq!(editor.source_document.text(), "## ");
    });
}

#[gpui::test]
async fn slash_menu_navigation_intercepts_real_bound_actions(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual_cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "/heading".to_string(), None));
    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(block.visible_len(), block_cx);
            block.refresh_slash_menu(block_cx);
        });
    });
    redraw(visual_cx);

    visual_cx.simulate_keystrokes("down tab");
    redraw(visual_cx);

    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").read(cx);
        assert_eq!(block.kind(), BlockKind::Heading { level: 2 });
        assert_eq!(editor.source_document.text(), "## ");
    });
}

#[gpui::test]
async fn slash_escape_stays_dismissed_until_the_query_changes(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "/".to_string(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(block.visible_len(), block_cx);
            block.refresh_slash_menu(block_cx);
            assert!(block.slash_menu.is_some());
        });
    });
    redraw(cx);

    let escape = KeyDownEvent {
        keystroke: Keystroke::parse("escape").expect("valid escape keystroke"),
        is_held: false,
    };
    editor.update_in(cx, |editor, window, cx| {
        editor.on_editor_key_down_capture(&escape, window, cx);
    });
    redraw(cx);

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        block.update(cx, |block, block_cx| {
            block.refresh_slash_menu(block_cx);
            assert!(block.slash_menu.is_none());
            let end = block.visible_len();
            block.replace_text_in_visible_range(end..end, "h", None, false, block_cx);
            block.refresh_slash_menu(block_cx);
            assert!(block.slash_menu.is_some());
        });
    });
}

#[gpui::test]
async fn slash_enter_with_no_results_does_not_mutate_the_document(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "/missing".to_string(), None));
    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(block.visible_len(), block_cx);
            block.refresh_slash_menu(block_cx);
        });
    });
    redraw(cx);

    let enter = KeyDownEvent {
        keystroke: Keystroke::parse("enter").expect("valid enter keystroke"),
        is_held: false,
    };
    editor.update_in(cx, |editor, window, cx| {
        editor.on_editor_key_down_capture(&enter, window, cx);
    });
    redraw(cx);

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        assert_eq!(editor.source_document.text(), "/missing");
        assert_eq!(block.read(cx).kind(), BlockKind::Paragraph);
        assert!(block.read(cx).slash_menu.is_some());
    });
}
