// @author kongweiguang

#[gpui::test]
async fn pane_divider_drag_updates_the_durable_ratio(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "# divider\n\nBody\n".to_owned(), None)
    });
    visual.simulate_resize(size(px(1080.0), px(720.0)));
    redraw(visual);
    let split = visual.debug_bounds("document-toolbar-action-0").unwrap();
    visual.simulate_click(split.center(), Modifiers::default());
    redraw(visual);
    let right = visual.debug_bounds("pane-split-right").unwrap();
    visual.simulate_click(right.center(), Modifiers::default());
    visual.run_until_parked();
    for _ in 0..2 {
        redraw(visual);
    }

    let before = editor.read_with(visual, |editor, cx| {
        editor
            .pane_workspace
            .as_ref()
            .unwrap()
            .read(cx)
            .workspace()
            .root()
            .ratio()
            .unwrap()
    });
    let divider = visual
        .debug_bounds("pane-divider")
        .expect("split divider hitbox");
    let drag_target = point(divider.center().x + px(96.0), divider.center().y);
    visual.simulate_mouse_down(divider.center(), MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(drag_target, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(drag_target, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    redraw(visual);
    let after = editor.read_with(visual, |editor, cx| {
        editor
            .pane_workspace
            .as_ref()
            .unwrap()
            .read(cx)
            .workspace()
            .root()
            .ratio()
            .unwrap()
    });
    assert!(after > before + 0.05, "divider ratio stayed at {after}");
}

#[gpui::test]
async fn split_panes_keep_content_flush_and_accept_text_input(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "# gmark\n\nBody\n".to_owned(), None)
    });
    visual.simulate_resize(size(px(1080.0), px(720.0)));
    redraw(visual);

    let split = visual
        .debug_bounds("document-toolbar-action-0")
        .expect("split toolbar action");
    visual.simulate_click(split.center(), Modifiers::default());
    visual.run_until_parked();
    redraw(visual);
    let split_right = visual
        .debug_bounds("pane-split-right")
        .expect("split menu exposes right direction");
    visual.simulate_click(split_right.center(), Modifiers::default());
    visual.run_until_parked();
    for _ in 0..4 {
        redraw(visual);
    }

    let (pane_count, pane_editors, mounted_view_count, initially_focused_pane) =
        editor.read_with(visual, |editor, cx| {
            let workspace = editor
                .pane_workspace
                .as_ref()
                .expect("split action creates pane workspace")
                .read(cx);
            let pane_editors = editor
                .pane_canvas_entities
                .borrow()
                .iter()
                .filter_map(|(pane, (_, _, canvas))| match canvas {
                    crate::editor::panes::PaneCanvasEntity::Markdown(canvas) => {
                        Some((*pane, canvas.read(cx).editor()))
                    }
                    crate::editor::panes::PaneCanvasEntity::DocumentHost(_)
                    | crate::editor::panes::PaneCanvasEntity::ReadOnly(_) => None,
                })
                .collect::<Vec<_>>();
            (
                workspace.workspace().pane_count(),
                pane_editors,
                workspace.mounted_view_count(),
                workspace.workspace().focused_pane(),
            )
        });
    assert_eq!(pane_count, 2);
    assert_eq!(pane_editors.len(), 2);
    assert_eq!(mounted_view_count, 2);
    let pane_shell = visual.debug_bounds("pane-shell").expect("pane shell");
    let pane_tab_bar = visual
        .debug_bounds("pane-tab-bar")
        .expect("split pane exposes a local tab bar");
    assert!(pane_tab_bar.top() >= pane_shell.top());
    assert!(pane_tab_bar.bottom() <= pane_shell.bottom());
    assert!(f32::from(pane_tab_bar.size.height) > 0.0);
    for selector in ["pane-tab-active", "pane-tab-new", "pane-tab-split"] {
        let control = visual
            .debug_bounds(selector)
            .expect("pane-local header control");
        assert!(control.top() >= pane_tab_bar.top());
        assert!(control.bottom() <= pane_tab_bar.bottom());
    }

    let (clicked_pane, clicked_pane_editor) = pane_editors
        .iter()
        .find(|(pane, _)| *pane != initially_focused_pane)
        .cloned()
        .expect("split must expose a non-focused pane");
    let clicked_block = clicked_pane_editor.read_with(visual, |pane_editor, _cx| {
        pane_editor.document.visible_blocks()[0].entity.clone()
    });
    let (clicked_block_bounds, expected_caret) = clicked_block.read_with(visual, |block, _cx| {
        let bounds = block
            .last_bounds
            .expect("non-focused pane block must be painted");
        let click = point(bounds.left() + px(1.0), bounds.center().y);
        (bounds, block.index_for_mouse_position(click))
    });
    let click = point(
        clicked_block_bounds.left() + px(1.0),
        clicked_block_bounds.center().y,
    );
    visual.simulate_mouse_down(click, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(click, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    for _ in 0..2 {
        redraw(visual);
    }
    visual.update(|window, cx| {
        assert!(
            clicked_block.read(cx).focus_handle.is_focused(window),
            "clicking a non-focused pane must transfer keyboard focus to its block"
        );
        assert_eq!(
            clicked_block.read(cx).selected_range,
            expected_caret..expected_caret,
            "the nested block must preserve the pointer caret while the pane shell updates focus"
        );
    });
    let stale_sibling_editor = pane_editors
        .iter()
        .find(|(pane, _)| *pane != clicked_pane)
        .map(|(_, editor)| editor.clone())
        .expect("split must retain the previously focused sibling");
    let stale_sibling_block = stale_sibling_editor.read_with(visual, |pane_editor, _cx| {
        pane_editor.document.visible_blocks()[0].entity.clone()
    });
    stale_sibling_editor.update(visual, |pane_editor, cx| {
        // Reproduce a late projection publication in the inactive sibling.
        // It may request its previous block again, but must not steal the
        // platform input handler from the pane the user just clicked.
        pane_editor.focus_block(stale_sibling_block.entity_id());
        cx.notify();
    });
    redraw(visual);
    visual.update(|window, cx| {
        assert!(
            clicked_block.read(cx).focus_handle.is_focused(window),
            "an inactive pane's stale pending focus must not steal keyboard input"
        );
        assert!(!stale_sibling_block.read(cx).focus_handle.is_focused(window));
    });
    let focused_pane_snapshot = clicked_pane_editor.read_with(visual, |pane_editor, cx| {
        pane_editor.accessibility_snapshot(cx)
    });
    let window_snapshot = editor.read_with(visual, |editor, cx| editor.accessibility_snapshot(cx));
    assert_eq!(
        window_snapshot, focused_pane_snapshot,
        "the window accessibility bridge must follow the focused pane canvas"
    );
    visual.update(|window, cx| {
        clicked_block.update(cx, |block, cx| {
            <crate::components::Block as EntityInputHandler>::replace_text_in_range(
                block, None, "x", window, cx,
            );
        });
    });
    visual.run_until_parked();
    for _ in 0..2 {
        redraw(visual);
    }
    let pane_texts = pane_editors
        .iter()
        .map(|(_, pane_editor)| {
            pane_editor.read_with(visual, |pane_editor, _cx| {
                pane_editor.source_document.text()
            })
        })
        .collect::<Vec<_>>();
    let clicked_block_text =
        clicked_block.read_with(visual, |block, _cx| block.display_text().to_owned());
    assert!(
        pane_texts.iter().all(|text| text.contains('x')),
        "clicked block={clicked_block_text:?}, pane sources={pane_texts:?}"
    );
}
