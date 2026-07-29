// @author kongweiguang

#[gpui::test]
async fn block_action_table_icon_opens_dialog_and_confirms_table(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "alpha".to_owned(), None));
    editor.update(visual, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, cx| {
            block.move_to(block.visible_len(), cx);
        });
    });
    redraw(visual);
    editor.update(visual, |editor, cx| {
        editor
            .document
            .first_root()
            .expect("root")
            .update(cx, |block, cx| block.open_block_action_menu(cx));
    });
    redraw(visual);
    visual.simulate_keystrokes(
        "down down down down down down down down down down down down",
    );
    redraw(visual);

    let table_action = visual
        .debug_bounds("slash-command-icon-table")
        .expect("table block action");
    visual.simulate_click(table_action.center(), Modifiers::default());
    redraw(visual);
    assert!(editor.read_with(visual, |editor, _cx| editor.table_insert_dialog.is_some()));

    let confirm = visual
        .debug_bounds("confirm-table-insert-dialog")
        .expect("table insert confirm");
    visual.simulate_click(confirm.center(), Modifiers::default());
    visual.run_until_parked();

    editor.update(visual, |editor, cx| {
        let roots = editor.document.root_blocks();
        assert_eq!(roots.len(), 3);
        assert_eq!(roots[0].read(cx).kind(), BlockKind::Paragraph);
        assert_eq!(roots[1].read(cx).kind(), BlockKind::Table);
        assert!(roots[1].read(cx).table_runtime.is_some());
        assert_eq!(roots[2].read(cx).kind(), BlockKind::Paragraph);
        assert!(editor.document_dirty);
    });
}

#[gpui::test]
async fn context_insert_table_icon_opens_dialog_and_confirms_table(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "alpha".to_owned(), None));
    editor.update(visual, |editor, cx| {
        editor.context_menu = Some(super::context_menu::ContextMenuState::Insert {
            position: point(px(24.0), px(24.0)),
            target: super::context_menu::TableInsertTarget::Append,
            insert_hovered: true,
            submenu_hovered: true,
            submenu_open: true,
        });
        cx.notify();
    });
    redraw(visual);

    let table_icon = visual
        .debug_bounds("editor-context-menu-insert-table-icon")
        .expect("table insert icon");
    visual.simulate_click(table_icon.center(), Modifiers::default());
    redraw(visual);
    assert!(editor.read_with(visual, |editor, _cx| editor.table_insert_dialog.is_some()));

    let confirm = visual
        .debug_bounds("confirm-table-insert-dialog")
        .expect("table insert confirm");
    visual.simulate_click(confirm.center(), Modifiers::default());
    visual.run_until_parked();
    editor.update(visual, |editor, cx| {
        let roots = editor.document.root_blocks();
        assert_eq!(roots.len(), 3);
        assert_eq!(roots[1].read(cx).kind(), BlockKind::Table);
        assert!(roots[1].read(cx).table_runtime.is_some());
    });
}
