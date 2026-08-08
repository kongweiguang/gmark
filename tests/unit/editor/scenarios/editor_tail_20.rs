// @author kongweiguang

#[gpui::test]
async fn multiline_formula_source_renders_without_panicking(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let markdown = "$$\nx + y\n+ z\n+ w\n$$";
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, markdown.into(), None));

    editor.update(visual, |editor, _cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(block.entity_id());
    });
    redraw(visual);

    let block = editor.read_with(visual, |editor, _cx| {
        editor.document.visible_blocks()[0].entity.clone()
    });
    block.read_with(visual, |block, _cx| {
        assert_eq!(block.math_source_text(), "x + y\n+ z\n+ w");
        assert!(block.math_source_last_layout.is_some());
    });
    assert_eq!(
        editor.read_with(visual, |editor, cx| editor.document.markdown_text(cx)),
        markdown
    );
}

#[gpui::test]
async fn formula_source_and_visual_surfaces_receive_real_delete_shortcuts(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "$$\nxy\n$$".into(), None));

    editor.update(visual, |editor, _cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(block.entity_id());
    });
    redraw(visual);
    let source_bounds = visual
        .debug_bounds("math-source-editor")
        .expect("source input should render inside the palette");
    let palette_bounds = visual
        .debug_bounds("math-structure-toolbar")
        .expect("formula palette should render");
    let handle_bounds = visual
        .debug_bounds("math-palette-drag-handle")
        .expect("formula palette handle should render");
    assert!(source_bounds.left() >= palette_bounds.left());
    assert!(source_bounds.right() <= palette_bounds.right());
    assert!(source_bounds.top() >= handle_bounds.bottom());
    assert!(source_bounds.bottom() <= palette_bounds.bottom());
    assert!(visual.debug_bounds("math-visual-editor-surface").is_some());

    let block = editor.read_with(visual, |editor, _cx| {
        editor.document.visible_blocks()[0].entity.clone()
    });
    visual.update(|window, cx| {
        block.update(cx, |block, _cx| {
            let source = block.math_source_text();
            let after_y = source.find('y').expect("y") + 1;
            block.set_math_source_selection(after_y..after_y, false);
            block.math_source_focus_handle.focus(window);
        });
    });
    redraw(visual);
    visual.simulate_keystrokes("backspace");
    redraw(visual);
    assert_eq!(
        editor.read_with(visual, |editor, cx| editor.document.markdown_text(cx)),
        "$$\nx\n$$"
    );

    visual.update(|window, cx| {
        block.update(cx, |block, _cx| {
            let session = block
                .math_edit_session
                .as_mut()
                .expect("structured session");
            let cursor = gmark_math_edit::MathCursor2D::at(
                session.document(),
                gmark_math_edit::MathSlot::root(),
                1,
            )
            .expect("root cursor");
            session.editor_mut().set_cursor(cursor).expect("set cursor");
            block.math_structure_focus_handle.focus(window);
        });
    });
    redraw(visual);
    visual.simulate_keystrokes("backspace");
    redraw(visual);
    assert_eq!(
        editor.read_with(visual, |editor, cx| editor.document.markdown_text(cx)),
        "$$\n\n$$"
    );
}

#[gpui::test]
async fn unsupported_formula_keeps_a_source_only_palette(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "$$\n\\unknown{x}\n$$".into(), None)
    });

    editor.update(visual, |editor, _cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(block.entity_id());
    });
    redraw(visual);

    assert!(visual.debug_bounds("math-structure-toolbar").is_some());
    assert!(visual.debug_bounds("math-source-editor").is_some());
    assert!(visual.debug_bounds("math-palette-symbols").is_none());
    assert!(visual.debug_bounds("math-palette-item-fraction").is_none());

    let block = editor.read_with(visual, |editor, _cx| {
        editor.document.visible_blocks()[0].entity.clone()
    });
    visual.update(|window, cx| {
        block.update(cx, |block, _cx| {
            assert!(block.math_edit_session.is_none());
            let source = block.math_source_text();
            let after_x = source.find('x').expect("x") + 1;
            block.set_math_source_selection(after_x..after_x, false);
            block.math_source_focus_handle.focus(window);
        });
    });
    redraw(visual);
    visual.simulate_keystrokes("backspace");
    redraw(visual);
    assert_eq!(
        editor.read_with(visual, |editor, cx| editor.document.markdown_text(cx)),
        "$$\n\\unknown{}\n$$"
    );
}

#[gpui::test]
async fn inline_formula_source_input_also_lives_in_the_palette(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "a $xy$ b".into(), None));
    let block = editor.read_with(visual, |editor, _cx| {
        editor.document.visible_blocks()[0].entity.clone()
    });

    visual.update(|window, cx| {
        block.update(cx, |block, _cx| {
            block.focus_handle.focus(window);
            block.begin_inline_math_edit("$xy$", 2..6, window);
        });
    });
    redraw(visual);

    let source_bounds = visual
        .debug_bounds("math-source-editor")
        .expect("inline formula source input should render");
    let palette_bounds = visual
        .debug_bounds("math-structure-toolbar")
        .expect("inline formula palette should render");
    assert!(source_bounds.left() >= palette_bounds.left());
    assert!(source_bounds.right() <= palette_bounds.right());
    assert!(visual.debug_bounds("inline-math-visual-editor").is_some());
}

#[gpui::test]
async fn formula_source_pointer_drag_selects_and_replaces_only_latex(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "$$\nxy\n$$".into(), None));
    let block = editor.read_with(visual, |editor, _cx| {
        editor.document.visible_blocks()[0].entity.clone()
    });

    editor.update(visual, |editor, _cx| {
        editor.focus_block(block.entity_id());
    });
    redraw(visual);

    let source_bounds = visual
        .debug_bounds("math-source-editor")
        .expect("source input should render inside the palette");
    let (drag_start, drag_end) = block.read_with(visual, |block, _cx| {
        assert_eq!(block.math_source_text(), "xy");
        let layout_bounds = block
            .math_source_last_bounds
            .expect("source layout bounds should be recorded");
        let line = block
            .math_source_last_layout
            .as_ref()
            .expect("source line should be shaped");
        let y = source_bounds.center().y;
        (
            gpui::point(layout_bounds.left() + gpui::px(1.0), y),
            gpui::point(
                layout_bounds.left()
                    + line.x_for_index(block.math_source_text().len())
                    + gpui::px(1.0),
                y,
            ),
        )
    });

    visual.simulate_mouse_down(
        drag_start,
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    visual.simulate_mouse_move(
        drag_end,
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    visual.simulate_mouse_up(
        drag_end,
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    redraw(visual);

    block.read_with(visual, |block, _cx| {
        assert_eq!(block.math_source_selection(), (0..2, false));
    });
    visual.simulate_keystrokes("z");
    redraw(visual);
    assert_eq!(
        editor.read_with(visual, |editor, cx| editor.document.markdown_text(cx)),
        "$$\nz\n$$"
    );
}

#[gpui::test]
async fn long_formula_source_scrolls_to_keep_the_caret_visible(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let body = "x".repeat(80);
    let markdown = format!("$$\n{body}\n$$");
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, markdown, None));
    let block = editor.read_with(visual, |editor, _cx| {
        editor.document.visible_blocks()[0].entity.clone()
    });

    editor.update(visual, |editor, _cx| {
        editor.focus_block(block.entity_id());
    });
    visual.update(|window, cx| {
        block.update(cx, |block, _cx| {
            let end = block.math_source_text().len();
            block.set_math_source_selection(end..end, false);
            block.math_source_focus_handle.focus(window);
        });
    });
    redraw(visual);

    let source_bounds = visual
        .debug_bounds("math-source-editor")
        .expect("source input should render");
    let (layout_left, caret_x) = block.read_with(visual, |block, _cx| {
        let layout_bounds = block
            .math_source_last_bounds
            .expect("source layout bounds should be recorded");
        let line = block
            .math_source_last_layout
            .as_ref()
            .expect("source line should be shaped");
        (
            layout_bounds.left(),
            layout_bounds.left() + line.x_for_index(block.math_source_text().len()),
        )
    });
    assert!(layout_left < source_bounds.left());
    assert!(caret_x <= source_bounds.right());
    assert!(caret_x >= source_bounds.left());
}

#[gpui::test]
async fn structured_reparse_preserves_the_formula_palette_anchor(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "$$\nx+1\n$$".into(), None));
    let block = editor.read_with(visual, |editor, _cx| {
        editor.document.visible_blocks()[0].entity.clone()
    });

    editor.update(visual, |editor, _cx| {
        editor.focus_block(block.entity_id());
    });
    redraw(visual);
    visual.update(|window, cx| {
        block.update(cx, |block, cx| {
            let formula_anchor = px(420.0);
            block.math_palette_anchor_y = Some(formula_anchor);
            block.math_edit_session = None;
            block.sync_math_edit_focus(true, window, cx);
            assert_eq!(block.math_palette_anchor_y, Some(formula_anchor));
            assert!(block.math_edit_session.is_some());
        });
    });
}
