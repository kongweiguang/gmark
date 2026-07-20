// @author kongweiguang

#[gpui::test]
async fn typing_closing_bold_marker_places_caret_after_marker(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("**bold*"),
            ),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 7..7;
        block.sync_inline_projection_for_focus(true);
        assert_eq!(block.display_text(), "**bold*");
        block.replace_text_in_visible_range(7..7, "*", None, false, cx);
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "**bold**");
        assert_eq!(block.cursor_offset(), "**bold**".len());
        assert_eq!(
            block.collapsed_caret_affinity,
            super::CollapsedCaretAffinity::OuterEnd
        );
    });
}

#[gpui::test]
async fn typing_inside_span_keeps_default_affinity(cx: &mut TestAppContext) {
    // Inserting an ordinary character inside a bold span must not jump the
    // caret outside the span.
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
        // Insert "X" inside the bold word (display offset 3 = after "**b").
        block.replace_text_in_visible_range(3..3, "X", None, false, cx);
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "**bXold**");
        assert_eq!(
            block.collapsed_caret_affinity,
            super::CollapsedCaretAffinity::Default
        );
    });
}

#[gpui::test]
async fn typing_bold_markers_char_by_char_produces_bold_not_italic(cx: &mut TestAppContext) {
    // Typing `**bold**` one character at a time must yield bold, not italic.
    // The clean parse is committed on each keystroke, so the intermediate
    // `**bold*` must not collapse to a literal `*` plus an italic `bold`.
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(BlockKind::Paragraph, InlineTextTree::from_markdown("")),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
        for ch in "**bold**".chars() {
            let caret = block.cursor_offset();
            block.replace_text_in_visible_range(caret..caret, &ch.to_string(), None, false, cx);
        }
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.record.title.visible_text(), "bold");
        assert_eq!(block.record.title.serialize_markdown(), "**bold**");
        assert!(
            block
                .record
                .title
                .render_cache()
                .spans()
                .iter()
                .all(|span| span.style.bold && !span.style.italic),
            "typed `**bold**` must be bold, not italic"
        );
    });
}

#[gpui::test]
async fn typing_after_closing_italic_marker_inserts_plain_text(cx: &mut TestAppContext) {
    // After typing `*italic*` the caret sits after the closing `*`, so further
    // typing must be plain text rather than being absorbed back into the span.
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(BlockKind::Paragraph, InlineTextTree::from_markdown("")),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
        for ch in "*italic* x".chars() {
            let caret = block.cursor_offset();
            block.replace_text_in_visible_range(caret..caret, &ch.to_string(), None, false, cx);
        }
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.record.title.visible_text(), "italic x");
        assert_eq!(block.record.title.serialize_markdown(), "*italic* x");
        // The trailing " x" must be a plain (non-italic) fragment.
        let trailing_is_italic = block
            .record
            .title
            .fragments
            .iter()
            .any(|fragment| fragment.text.contains('x') && fragment.style.italic);
        assert!(
            !trailing_is_italic,
            "text after closing `*` must not be italic"
        );
    });
}

#[gpui::test]
async fn typing_after_closing_bold_marker_inserts_plain_text(cx: &mut TestAppContext) {
    // Same as above for bold: typing past the closing `**` must be plain.
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(BlockKind::Paragraph, InlineTextTree::from_markdown("")),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
        for ch in "**bold** more".chars() {
            let caret = block.cursor_offset();
            block.replace_text_in_visible_range(caret..caret, &ch.to_string(), None, false, cx);
        }
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.record.title.visible_text(), "bold more");
        assert_eq!(block.record.title.serialize_markdown(), "**bold** more");
        let trailing_is_bold = block
            .record
            .title
            .fragments
            .iter()
            .any(|fragment| fragment.text.contains("more") && fragment.style.bold);
        assert!(
            !trailing_is_bold,
            "text after closing `**` must not be bold"
        );
    });
}

#[gpui::test]
async fn strikethrough_projection_only_expands_touched_span(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a ~~gone~~ b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a gone b"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a ~~gone~~ b"
    );
}

#[gpui::test]
async fn script_projection_expands_only_touched_span(cx: &mut TestAppContext) {
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
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "x2 and H2O"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 1..1;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "x^2^ and H2O"
    );

    block.update(cx, |block, _cx| {
        block.clear_inline_projection();
        block.selected_range = "x2 and H".len().."x2 and H".len();
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "x2 and H~2~O"
    );
}

#[gpui::test]
async fn standalone_script_projection_uses_html_marker_fallback(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("<sup>2</sup> and <sub>n</sub>"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "<sup>2</sup> and n"
    );

    block.update(cx, |block, _cx| {
        block.clear_inline_projection();
        block.selected_range = "2 and ".len().."2 and ".len();
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "2 and <sub>n</sub>"
    );
}

#[gpui::test]
async fn script_projection_marker_edit_unwraps_script_style(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(BlockKind::Paragraph, InlineTextTree::from_markdown("x^2^")),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 1..1;
        block.sync_inline_projection_for_focus(true);
        assert_eq!(block.display_text(), "x^2^");
        block.replace_text_in_visible_range(1..2, "", None, false, cx);
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "x2");
        assert_eq!(block.record.title.serialize_markdown(), "x2");
        assert!(
            block
                .inline_spans()
                .iter()
                .all(|span| span.style.script == InlineScript::Normal)
        );
    });
}

#[gpui::test]
async fn subscript_projection_marker_edit_unwraps_script_style(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(BlockKind::Paragraph, InlineTextTree::from_markdown("H~2~O")),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 1..1;
        block.sync_inline_projection_for_focus(true);
        assert_eq!(block.display_text(), "H~2~O");
        block.replace_text_in_visible_range(1..2, "", None, false, cx);
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "H2O");
        assert_eq!(block.record.title.serialize_markdown(), "H2O");
        assert!(
            block
                .record
                .title
                .render_cache()
                .spans()
                .iter()
                .all(|span| span.style.script == InlineScript::Normal)
        );
    });
}

#[gpui::test]
async fn script_projection_insertion_inside_span_preserves_script_style(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(BlockKind::Paragraph, InlineTextTree::from_markdown("x^2^")),
        )
    });

    block.update(cx, |block, cx| {
        block.selected_range = 1..1;
        block.sync_inline_projection_for_focus(true);
        assert_eq!(block.display_text(), "x^2^");
        block.replace_text_in_visible_range(3..3, "3", None, false, cx);
    });

    block.read_with(cx, |block, _cx| {
        assert_eq!(block.display_text(), "x^23^");
        assert_eq!(block.record.title.serialize_markdown(), "x^23^");
        assert_eq!(
            block.record.title.render_cache().spans()[1].style.script,
            InlineScript::Superscript
        );
    });
}

#[gpui::test]
async fn inline_code_projection_right_escape_stays_outside_after_rebuild(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a `123` b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 5..5;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 6);

    block.update(cx, |block, _cx| {
        let (target, affinity) = block
            .projected_move_right_target(block.cursor_offset())
            .expect("inner end should jump to outer end");
        block.assign_collapsed_selection_offset(target, affinity, None);
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a `123` b"
    );
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 7);

    block.update(cx, |block, _cx| {
        let target = block.next_boundary(block.cursor_offset());
        block.move_to_with_preferred_x(target, None, _cx);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 8);
}

#[gpui::test]
async fn inline_code_projection_left_escape_stays_outside_after_rebuild(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a `123` b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 3);

    block.update(cx, |block, _cx| {
        let (target, affinity) = block
            .projected_move_left_target(block.cursor_offset())
            .expect("inner start should jump to outer start");
        block.assign_collapsed_selection_offset(target, affinity, None);
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 2);

    block.update(cx, |block, _cx| {
        let target = block.previous_boundary(block.cursor_offset());
        block.move_to_with_preferred_x(target, None, _cx);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 1);
}

#[gpui::test]
async fn strikethrough_projection_right_escape_stays_outside_after_rebuild(
    cx: &mut TestAppContext,
) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a ~~123~~ b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 5..5;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 7);

    block.update(cx, |block, _cx| {
        let (target, affinity) = block
            .projected_move_right_target(block.cursor_offset())
            .expect("inner end should jump to outer end");
        block.assign_collapsed_selection_offset(target, affinity, None);
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 9);

    block.update(cx, |block, _cx| {
        let target = block.next_boundary(block.cursor_offset());
        block.move_to_with_preferred_x(target, None, _cx);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 10);
}

#[gpui::test]
async fn strikethrough_projection_left_escape_stays_outside_after_rebuild(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a ~~bc~~ d"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 4);

    block.update(cx, |block, _cx| {
        let (target, affinity) = block
            .projected_move_left_target(block.cursor_offset())
            .expect("expected projected move left target");
        block.assign_collapsed_selection_offset(target, affinity, None);
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 2);

    block.update(cx, |block, _cx| {
        let target = block.previous_boundary(block.cursor_offset());
        block.move_to_with_preferred_x(target, None, _cx);
    });
    assert_eq!(block.read_with(cx, |block, _cx| block.cursor_offset()), 1);
}

#[gpui::test]
async fn word_start_boundaries_step_over_whole_words(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("hello world foo"),
            ),
        )
    });

    block.read_with(cx, |block, _cx| {
        // Word starts are at offsets 0 ("hello"), 6 ("world"), 12 ("foo").
        assert_eq!(block.next_word_start(0), 6);
        assert_eq!(block.next_word_start(3), 6);
        assert_eq!(block.next_word_start(6), 12);
        assert_eq!(block.next_word_start(12), 15);

        assert_eq!(block.previous_word_start(15), 12);
        assert_eq!(block.previous_word_start(12), 6);
        assert_eq!(block.previous_word_start(7), 6);
        assert_eq!(block.previous_word_start(6), 0);
        assert_eq!(block.previous_word_start(0), 0);
    });
}

#[gpui::test]
async fn inline_link_projection_only_expands_touched_span(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("a [link](https://example.com) b"),
            ),
        )
    });

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a link b"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 2..2;
        block.sync_inline_projection_for_focus(true);
    });
    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "a [link](https://example.com) b"
    );
}

#[gpui::test]
async fn reference_style_link_resolves_and_expands_preserving_raw_syntax(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        let mut block = Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("[reference link][ref-link]"),
            ),
        );
        block.set_runtime_context(
            None,
            Arc::default(),
            Arc::new(parse_link_reference_definitions(
                "[ref-link]: https://example.com",
            )),
            Arc::default(),
        );
        block
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "reference link"
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.inline_link_at(0).map(str::to_string)),
        Some("https://example.com".to_string())
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "[reference link][ref-link]"
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.record.title.serialize_markdown()),
        "[reference link][ref-link]"
    );
}

#[gpui::test]
async fn reference_style_link_hit_exposes_raw_prompt_and_resolved_open_target(
    cx: &mut TestAppContext,
) {
    let block = cx.new(|cx| {
        let mut block = Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("[reference link][ref-links]"),
            ),
        );
        block.set_runtime_context(
            None,
            Arc::default(),
            Arc::new(parse_link_reference_definitions(
                "[ref-links]: https://example.com",
            )),
            Arc::default(),
        );
        block
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.inline_link_hit_at(0).cloned()),
        Some(InlineLinkHit {
            prompt_target: "ref-links".to_string(),
            open_target: "https://example.com".to_string(),
        })
    );
}

#[gpui::test]
async fn inline_link_with_title_expands_title_but_opens_destination(cx: &mut TestAppContext) {
    let markdown = "[ABC](https://abc.com \"https://abc.com\")";
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown(markdown),
            ),
        )
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.inline_link_hit_at(0).cloned()),
        Some(InlineLinkHit {
            prompt_target: "https://abc.com".to_string(),
            open_target: "https://abc.com".to_string(),
        })
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        markdown
    );
    assert_eq!(
        block.read_with(cx, |block, _cx| block.record.title.serialize_markdown()),
        markdown
    );
}

#[gpui::test]
async fn autolink_expands_with_angle_brackets_when_touched(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("<https://example.com>"),
            ),
        )
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "https://example.com"
    );

    block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block.display_text().to_string()),
        "<https://example.com>"
    );
}

#[gpui::test]
async fn projected_reference_target_stays_link_hit_testable(cx: &mut TestAppContext) {
    let block = cx.new(|cx| {
        let mut block = Block::with_record(
            cx,
            BlockRecord::new(
                BlockKind::Paragraph,
                InlineTextTree::from_markdown("[reference link][ref-link]"),
            ),
        );
        block.set_runtime_context(
            None,
            Arc::default(),
            Arc::new(parse_link_reference_definitions(
                "[ref-link]: https://example.com",
            )),
            Arc::default(),
        );
        block
    });

    let target_offset = block.update(cx, |block, _cx| {
        block.selected_range = 0..0;
        block.sync_inline_projection_for_focus(true);
        block
            .display_text()
            .find("ref-link")
            .expect("projection should expose reference target")
    });

    assert_eq!(
        block.read_with(cx, |block, _cx| block
            .inline_link_hit_at(target_offset)
            .cloned()),
        Some(InlineLinkHit {
            prompt_target: "ref-link".to_string(),
            open_target: "https://example.com".to_string(),
        })
    );
}

