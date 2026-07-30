// @author kongweiguang

#[gpui::test]
async fn italic_projection_only_expands_touched_span(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a *italic* b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a italic b"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a *italic* b"
    );
}

#[gpui::test]
async fn italic_projection_marker_edit_unwraps_italic_style(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(BlockKind::Paragraph, InlineTextTree::from_markdown("*it*")),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
        assert_eq!(block.display_text(), "*it*");
        block.replace_text_in_visible_range(0..1, "", None, false, cx);
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "it");
        assert_eq!(block.record.title.serialize_markdown(), "it");
        assert!(
            block
                .record
                .title
                .render_cache()
                .spans()
                .iter()
                .all(|span| !span.style.italic)
        );
    });
}

#[gpui::test]
async fn typing_closing_italic_marker_places_caret_after_marker(cx: &mut TestAppContext) {
    // `*italic` is literal until the closing `*` is typed; afterwards the caret
    // must land *after* the closing marker so further typing stays plain.
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("*italic"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 7..7;
        block.sync_inline_projection_for_focus(true);
        assert_eq!(block.display_text(), "*italic");
        block.replace_text_in_visible_range(7..7, "*", None, false, cx);
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "*italic*");
        assert_eq!(block.cursor_offset(), "*italic*".len());
        assert_eq!(
            block.collapsed_caret_affinity,
            super::CollapsedCaretAffinity::OuterEnd
        );
    });
}

#[gpui::test]
async fn mermaid_workbench_mode_is_runtime_only_and_respects_read_only(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let source = "```mermaid\nflowchart LR\nA --> B\n```";
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::mermaid(source)));
    let markdown_before = block.read_with(cx, |block, _cx| block.record.markdown_line(0, None));

    assert_eq!(
        block.read_with(cx, |block, _cx| block.mermaid_view_mode()),
        MermaidViewMode::Preview
    );
    cx.update(|window, cx| {
        block.update(cx, |block, cx| {
            block.set_mermaid_view_mode(MermaidViewMode::Source, window, cx);
            assert_eq!(block.mermaid_view_mode(), MermaidViewMode::Source);
            assert!(block.focus_handle.is_focused(window));
            block.set_mermaid_view_mode(MermaidViewMode::Split, window, cx);
            assert_eq!(block.mermaid_view_mode(), MermaidViewMode::Split);
        });
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.record.markdown_line(0, None)),
        markdown_before,
        "view mode must not mutate the fenced Markdown source"
    );

    block.update(cx, |block, cx| {
        block.copy_mermaid_source(cx);
        assert!(block.mermaid_copy_feedback);
        block.set_read_only(true);
        assert_eq!(block.mermaid_view_mode(), MermaidViewMode::Preview);
    });
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some(source)
    );
    cx.update(|window, cx| {
        block.update(cx, |block, cx| {
            block.set_mermaid_view_mode(MermaidViewMode::Source, window, cx);
            assert_eq!(block.mermaid_view_mode(), MermaidViewMode::Preview);
        });
    });
}
