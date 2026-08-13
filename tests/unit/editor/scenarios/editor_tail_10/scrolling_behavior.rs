// @author kongweiguang

#[gpui::test]
async fn clicking_bottom_padding_of_short_document_focuses_document_end(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    // Keep enough blocks to expose the rendered tail padding after the shared
    // Source top inset is applied; the assertion is about tail hit testing,
    // not about a document that accidentally fits inside the viewport.
    let markdown = (0..10)
        .map(|index| format!("# section {index}\n\nparagraph {index}"))
        .collect::<Vec<_>>()
        .join("\n\n")
        + "\n\nlast";
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_markdown(cx, markdown, None)
    });
    visual.simulate_resize(size(px(900.0), px(700.0)));
    redraw(visual);

    let (first, last, last_bounds, max_scroll_y, current_scroll_y) =
        editor.read_with(visual, |editor, cx| {
            let visible = editor.document.visible_blocks();
            let first = visible.first().expect("first block").entity.clone();
            let last = visible.last().expect("last block").entity.clone();
            (
                first,
                last.clone(),
                last.read(cx).last_bounds.expect("last block bounds"),
                f32::from(editor.scroll_handle.max_offset().height.max(px(0.0))),
                -f32::from(editor.scroll_handle.offset().y),
            )
        });
    assert!(max_scroll_y > 0.5, "bottom padding should be scrollable");
    assert!(current_scroll_y <= 0.5, "test starts at the top");

    editor.update(visual, |editor, cx| {
        editor.active_entity_id = Some(first.entity_id());
        editor.pending_focus = None;
        editor
            .scroll_handle
            .set_offset(point(px(0.0), px(-max_scroll_y)));
        cx.notify();
    });
    redraw(visual);
    let tail = visual
        .debug_bounds("editor-document-tail-blank")
        .expect("rendered tail padding");
    let click = point(
        tail.left() + px(8.0),
        tail.top() + px(8.0),
    );
    assert!(click.y > last_bounds.bottom());
    editor.update(visual, |editor, cx| {
        // Directly exercise the same public tail-insertion contract that the
        // blank-area handler delegates to when no trailing text block exists.
        assert!(editor.ensure_editable_document_tail(cx));
    });
    redraw(visual);

    editor.read_with(visual, |editor, cx| {
        let trailing = editor
            .document
            .visible_blocks()
            .last()
            .expect("tail paragraph")
            .entity
            .clone();
        assert_eq!(editor.active_entity_id, Some(trailing.entity_id()));
        assert_ne!(trailing.entity_id(), last.entity_id());
        assert_eq!(trailing.read(cx).selected_range, 0..0);
    });
}

#[gpui::test]
async fn editor_scrollbar_separates_stable_hitbox_from_hover_thumb(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let markdown = (0..120)
        .map(|index| format!("# Heading {index}\n\nParagraph {index} with enough text to scroll."))
        .collect::<Vec<_>>()
        .join("\n\n");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_markdown(cx, markdown, None));
    visual.simulate_resize(size(px(720.0), px(520.0)));
    editor.update(visual, |editor, cx| {
        editor.scrollbar_hovered = true;
        editor.scrollbar_visible_until = Instant::now() + Duration::from_secs(1);
        cx.notify();
    });
    redraw(visual);

    let source = editor.read_with(visual, |editor, _cx| editor.source_document.text());
    let revision = editor.read_with(visual, |editor, _cx| editor.source_document.revision());
    let dirty = editor.read_with(visual, |editor, _cx| editor.document_dirty);
    let content = visual.debug_bounds("editor-content").unwrap();
    let hitbox = visual.debug_bounds("editor-scrollbar-hitbox").unwrap();
    let idle_thumb = visual.debug_bounds("editor-scrollbar-thumb").unwrap();
    assert_eq!(f32::from(hitbox.size.width), 14.0);
    assert_eq!(f32::from(idle_thumb.size.width), 6.0);
    assert_eq!(idle_thumb.right(), hitbox.right());
    assert!(
        hitbox.left() >= content.left(),
        "hitbox={hitbox:?} content={content:?}"
    );
    assert!(
        hitbox.right() <= content.right(),
        "hitbox={hitbox:?} content={content:?}"
    );

    visual.simulate_mouse_move(hitbox.center(), None, Modifiers::default());
    redraw(visual);
    let hovered_hitbox = visual.debug_bounds("editor-scrollbar-hitbox").unwrap();
    let hovered_thumb = visual.debug_bounds("editor-scrollbar-thumb").unwrap();
    assert_eq!(f32::from(hovered_hitbox.size.width), 14.0);
    assert_eq!(f32::from(hovered_thumb.size.width), 10.0);
    assert_eq!(hovered_thumb.right(), hovered_hitbox.right());

    visual.simulate_mouse_down(
        hovered_hitbox.center(),
        MouseButton::Left,
        Modifiers::default(),
    );
    redraw(visual);
    editor.update(visual, |editor, _cx| {
        assert!(editor.scrollbar_drag.is_some());
    });
    visual.simulate_mouse_up(
        hovered_hitbox.center(),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.run_until_parked();
    editor.update(visual, |editor, _cx| {
        assert!(editor.scrollbar_drag.is_none());
        assert_eq!(editor.source_document.text(), source);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });

    visual.simulate_resize(size(px(1180.0), px(780.0)));
    redraw(visual);
    let content = visual.debug_bounds("editor-source-pane").unwrap();
    let hitbox = visual.debug_bounds("editor-scrollbar-hitbox").unwrap();
    assert!(hitbox.left() >= content.left());
    assert!(hitbox.right() <= content.right());
    visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
}
