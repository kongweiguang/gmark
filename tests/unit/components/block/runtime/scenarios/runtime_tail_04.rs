// @author kongweiguang

#[gpui::test]
async fn tab_inserts_character_in_code_block(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::with_plain_text(BlockKind::CodeBlock { language: None }, "ab"),
        )
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.move_to(1, block_cx);
            block.on_indent_block(&IndentBlock, window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "a    b");
        assert_eq!(block.selected_range, 5..5);
    });
}

#[gpui::test]
async fn enter_after_typed_fence_uses_preserved_markdown_and_opens_code_block(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph(String::new())));

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.focus_handle.focus(window);
            block.sync_inline_projection_for_focus(true);
            for ch in "```java".chars() {
                <Block as EntityInputHandler>::replace_text_in_range(
                    block,
                    None,
                    &ch.to_string(),
                    window,
                    block_cx,
                );
            }
            assert_eq!(block.display_text(), "```java");
            assert_eq!(block.record.title.serialize_markdown(), "\\`\\`\\`java");
            assert_eq!(block.cursor_offset(), block.visible_len());
            block.on_newline(&Newline, window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(
            block.kind(),
            BlockKind::CodeBlock {
                language: Some("java".into())
            }
        );
        assert_eq!(block.display_text(), "");
        assert_eq!(block.selected_range, 0..0);
    });
}

#[test]
fn expanded_code_cursor_offset_stays_before_closing_backtick() {
    let fragments = vec![InlineFragment {
        text: "123".to_string(),
        style: InlineStyle {
            code: true,
            ..InlineStyle::default()
        },
        html_style: None,
        link: None,
        footnote: None,
        math: None,
    }];

    assert_eq!(expanded_display_offset_for_clean(&fragments, 0), 1);
    assert_eq!(expanded_display_offset_for_clean(&fragments, 3), 5);
    assert_eq!(expanded_display_cursor_offset_for_clean(&fragments, 0), 1);
    assert_eq!(expanded_display_cursor_offset_for_clean(&fragments, 3), 4);
}

#[test]
fn expanded_code_cursor_offset_keeps_plain_text_boundaries() {
    let fragments = vec![
        InlineFragment {
            text: "a".to_string(),
            style: InlineStyle::default(),
            html_style: None,
            link: None,
            footnote: None,
            math: None,
        },
        InlineFragment {
            text: "bc".to_string(),
            style: InlineStyle {
                code: true,
                ..InlineStyle::default()
            },
            html_style: None,
            link: None,
            footnote: None,
            math: None,
        },
    ];

    assert_eq!(expanded_display_cursor_offset_for_clean(&fragments, 1), 1);
    assert_eq!(expanded_display_cursor_offset_for_clean(&fragments, 3), 4);
}

#[test]
fn typing_inside_manual_backticks_keeps_cursor_inside_code_span() {
    let tree = InlineTextTree::plain("``");
    let result = tree.replace_visible_range(1..1, "1", InlineInsertionAttributes::default());

    assert_eq!(result.tree.visible_text(), "1");
    assert_eq!(
        result.tree.fragments,
        vec![InlineFragment {
            text: "1".to_string(),
            style: InlineStyle {
                code: true,
                ..InlineStyle::default()
            },
            html_style: None,
            link: None,
            footnote: None,
            math: None,
        }]
    );

    let clean_cursor = result.map_offset(2);
    assert_eq!(clean_cursor, 1);
    assert_eq!(
        expanded_display_cursor_offset_for_clean(&result.tree.fragments, clean_cursor),
        2
    );
}

#[gpui::test]
async fn enter_inside_multiline_inline_code_inserts_hard_line_without_splitting(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("`line 1\nline 2`"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        let offset = "line 1\n".len();
        block.selected_range = offset..offset;
        cx.notify();
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.on_newline(&Newline, window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        let text = "line 1\n\nline 2";
        assert_eq!(block.kind(), BlockKind::Paragraph);
        assert_eq!(block.display_text(), text);
        assert_eq!(block.selected_range, "line 1\n\n".len().."line 1\n\n".len());
        assert!(
            block
                .inline_spans()
                .iter()
                .any(|span| { span.style.code && span.range == (0..text.len()) })
        );
    });
}

#[gpui::test]
async fn inline_math_focus_stays_rendered_rich_and_keeps_links(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("**bold** $x^2$ [repo](https://example.com)"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        // The math source is shown inline (`$x^2$`) while bold and the link stay
        // collapsed; the block never falls back to raw Markdown editing.
        assert_eq!(block.display_text(), "bold $x^2$ repo");

        // Focusing with the caret inside the math keeps the rendered-rich
        // projection rather than dumping the whole block to raw source, so the
        // link in the same block keeps its link attribute.
        let caret = "bold $".len();
        block.move_to(caret, cx);
        block.sync_inline_projection_for_focus(true);
        assert!(!block.uses_raw_text_editing());
        assert!(block.record.title.has_mixed_inline_visuals());
        assert!(block.record.title.has_inline_links());
        assert!(
            block.inline_spans().iter().any(|span| span.link.is_some()),
            "link must stay styled while editing the math in the same block"
        );
        assert_eq!(
            block.record.title.serialize_markdown(),
            "**bold** $x^2$ [repo](https://example.com)"
        );
    });
}

#[gpui::test]
async fn script_spans_focus_stay_rendered_rich(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("x^2^ and H~2~O"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        assert_eq!(block.display_text(), "x2 and H2O");
        assert_eq!(block.inline_spans()[0].style.script, InlineScript::Normal);
        assert_eq!(
            block.inline_spans()[1].style.script,
            InlineScript::Superscript
        );
        assert!(!block.uses_raw_text_editing());
        assert_eq!(block.display_text(), "x2 and H2O");
        assert_eq!(block.record.title.serialize_markdown(), "x^2^ and H~2~O");
    });
}

#[gpui::test]
async fn link_anchor_emphasis_delimiters_are_revealed_when_caret_inside(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("[**bold**](https://example.com)"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        // Collapsed, only the styled anchor text is shown.
        assert_eq!(block.display_text(), "bold");

        // With the caret inside the bold anchor text, the projection reveals both
        // the link syntax and the anchor's own `**` emphasis markers, so they can
        // be edited instead of staying invisible.
        block.move_to(2, cx);
        block.sync_inline_projection_for_focus(true);
        assert_eq!(block.display_text(), "[**bold**](https://example.com)");
    });
}

#[gpui::test]
async fn mermaid_block_uses_raw_text_editing(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let markdown = "```mermaid\nflowchart LR\nA --> B\n```";
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::mermaid(markdown)));

    block.update(cx, |block, _cx| {
        assert_eq!(block.kind(), BlockKind::MermaidBlock);
        assert!(block.uses_raw_text_editing());
        assert_eq!(block.display_text(), markdown);
        assert_eq!(block.record.markdown_line(0, None), markdown);
    });
}

#[gpui::test]
async fn enter_inside_projected_inline_code_inserts_hard_line_without_splitting(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("`line 1\nline 2`"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        let offset = "line 1\n".len();
        block.selected_range = offset..offset;
        block.sync_inline_projection_for_focus(true);
        cx.notify();
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.on_newline(&Newline, window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        let text = "line 1\n\nline 2";
        assert_eq!(block.kind(), BlockKind::Paragraph);
        assert_eq!(block.record.title.visible_text(), text);
        assert!(
            block
                .record
                .title
                .render_cache()
                .spans()
                .iter()
                .any(|span| span.style.code && span.range == (0..text.len()))
        );
    });
}

#[gpui::test]
async fn enter_outside_inline_code_still_splits_paragraph(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("alpha beta"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = "alpha".len().."alpha".len();
        cx.notify();
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.on_newline(&Newline, window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.kind(), BlockKind::Paragraph);
        assert_eq!(block.display_text(), "alpha");
        assert_eq!(block.selected_range, "alpha".len().."alpha".len());
    });
}

#[gpui::test]
async fn enter_inside_comment_block_inserts_hard_line_without_splitting(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::comment("<!--\n**not bold** [not link](https://example.com)\n-->"),
        )
    });

    block.update(cx, |block, cx| {
        let offset = "<!--\n".len();
        block.selected_range = offset..offset;
        cx.notify();
    });

    cx.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            block.on_newline(&Newline, window, block_cx);
        });
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.kind(), BlockKind::Comment);
        assert_eq!(
            block.display_text(),
            "<!--\n\n**not bold** [not link](https://example.com)\n-->"
        );
        assert_eq!(block.inline_spans().len(), 1);
        assert_eq!(block.inline_spans()[0].range, 0..block.display_text().len());
        assert_eq!(block.inline_spans()[0].style, InlineStyle::default());
    });
}

#[gpui::test]
async fn paragraph_shortcut_creates_task_item_directly(cx: &mut TestAppContext) {
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph(String::new())));

    block.update(cx, |block, cx| {
        block.apply_title_edit(
            InlineTextTree::plain("- [x] task"),
            10,
            None,
            None,
            None,
            false,
            cx,
        );
    });

    let kind = block.read_with(cx, |block, _cx| block.kind());
    let text = block.read_with(cx, |block, _cx| block.display_text().to_string());
    assert_eq!(kind, BlockKind::TaskListItem { checked: true });
    assert_eq!(text, "task");
}

#[gpui::test]
async fn paragraph_shortcut_creates_parenthesized_numbered_list_directly(cx: &mut TestAppContext) {
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph(String::new())));

    block.update(cx, |block, cx| {
        block.apply_title_edit(
            InlineTextTree::plain("1) item"),
            7,
            None,
            None,
            None,
            false,
            cx,
        );
    });

    let kind = block.read_with(cx, |block, _cx| block.kind());
    let text = block.read_with(cx, |block, _cx| block.display_text().to_string());
    assert_eq!(kind, BlockKind::NumberedListItem);
    assert_eq!(text, "item");
}

#[gpui::test]
async fn bullet_shortcut_upgrades_to_task_item_after_box_prefix(cx: &mut TestAppContext) {
    let block = cx.new(|cx| Block::with_record(cx, BlockRecord::paragraph(String::new())));

    block.update(cx, |block, cx| {
        block.apply_title_edit(InlineTextTree::plain("- "), 2, None, None, None, false, cx);
    });
    let kind = block.read_with(cx, |block, _cx| block.kind());
    assert_eq!(kind, BlockKind::BulletedListItem);

    block.update(cx, |block, cx| {
        block.apply_title_edit(
            InlineTextTree::plain("[ ] "),
            4,
            None,
            None,
            None,
            false,
            cx,
        );
    });

    let kind = block.read_with(cx, |block, _cx| block.kind());
    let text = block.read_with(cx, |block, _cx| block.display_text().to_string());
    assert_eq!(kind, BlockKind::TaskListItem { checked: false });
    assert_eq!(text, "");
}

#[gpui::test]
async fn inline_code_projection_only_expands_touched_span(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a `code` b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a code b"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a `code` b"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 9..9;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a code b"
    );
}

#[gpui::test]
async fn inline_code_projection_expands_only_the_selected_code_span(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("`one` and `two`"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 1..1;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "`one` and two"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 10..10;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "one and `two`"
    );
}

#[gpui::test]
async fn bold_projection_only_expands_touched_span(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a **bold** b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a bold b"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a **bold** b"
    );
}

#[gpui::test]
async fn bold_projection_expands_only_the_selected_bold_span(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("**one** and **two**"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 1..1;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "**one** and two"
    );

    block.update(cx, |block, _cx| {
        block.clear_inline_projection();
        block.selected_range = "one and ".len().."one and ".len();
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "one and **two**"
    );
}

#[gpui::test]
async fn bold_projection_expands_selected_range_and_html_strong(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a **bold** b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 2..6;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a **bold** b"
    );

    let html_block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("<strong>bold</strong>"),
            ),
        )
    });

    html_block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        html_block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "**bold**"
    );
}

#[gpui::test]
async fn bold_projection_marker_edit_unwraps_bold_style(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("**bold**"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
        assert_eq!(block.display_text(), "**bold**");
        block.replace_text_in_visible_range(0..2, "", None, false, cx);
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "bold");
        assert_eq!(block.record.title.serialize_markdown(), "bold");
        assert!(
            block
                .record
                .title
                .render_cache()
                .spans()
                .iter()
                .all(|span| !span.style.bold)
        );
    });
}

#[gpui::test]
async fn bold_projection_insertion_inside_span_preserves_bold_style(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("**bold**"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
        assert_eq!(block.display_text(), "**bold**");
        block.replace_text_in_visible_range(3..3, "X", None, false, cx);
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "**bXold**");
        assert_eq!(block.record.title.serialize_markdown(), "**bXold**");
        assert!(block.record.title.render_cache().spans()[0].style.bold);
    });
}

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
