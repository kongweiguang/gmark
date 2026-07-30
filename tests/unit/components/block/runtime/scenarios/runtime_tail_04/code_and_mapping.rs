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
fn unicode_footnote_projection_maps_only_utf8_boundaries() {
    let superscript = "¹⁰".to_owned();
    let clean_middle = "¹".len();
    let fragments = vec![InlineFragment {
        text: superscript.clone(),
        style: InlineStyle::default(),
        html_style: None,
        link: None,
        footnote: Some(InlineFootnoteReference {
            id: "éa".to_owned(),
            ordinal: Some(10),
            occurrence_index: 0,
        }),
        math: None,
    }];
    let projection = ExpandedInlineProjection::build(&fragments, clean_middle..clean_middle, None)
        .expect("touched footnote should expand");
    let display = projection.cache.visible_text();

    for (offset, mapped) in projection
        .clean_to_display_cursor
        .iter()
        .copied()
        .enumerate()
    {
        if superscript.is_char_boundary(offset) {
            assert!(
                display.is_char_boundary(mapped),
                "clean {offset} mapped to {mapped}"
            );
        }
    }
    for (offset, mapped) in projection.display_to_clean.iter().copied().enumerate() {
        if display.is_char_boundary(offset) {
            assert!(
                superscript.is_char_boundary(mapped),
                "display {offset} mapped to {mapped}"
            );
        }
    }
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
