// @author kongweiguang

    #[gpui::test]
    async fn plain_multiline_paste_with_scripts_splits_physical_lines(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

        editor.update(cx, |editor, cx| {
            let block = editor.document.visible_blocks()[0].entity.clone();
            editor.on_block_event(
                block,
                &BlockEvent::RequestPasteMultiline {
                    leading: InlineTextTree::plain(String::new()),
                    lines: vec![
                        "H~2~O".to_string(),
                        "CO<sub>2</sub>".to_string(),
                        "x<sup>n</sup>".to_string(),
                    ],
                    trailing: InlineTextTree::plain(String::new()),
                    split_physical_lines: true,
                },
                cx,
            );

            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 3);
            assert_eq!(visible[0].entity.read(cx).display_text(), "H2O");
            assert_eq!(visible[1].entity.read(cx).display_text(), "CO2");
            assert_eq!(visible[2].entity.read(cx).display_text(), "xn");
            assert_eq!(editor.document.markdown_text(cx), "H~2~O\n\nCO~2~\n\nx^n^");
        });
    }

    #[gpui::test]
    async fn structural_paste_of_table_renders_native_table(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

        editor.update(cx, |editor, cx| {
            let block = editor.document.visible_blocks()[0].entity.clone();
            editor.on_block_event(
                block,
                &BlockEvent::RequestPasteMultiline {
                    leading: InlineTextTree::plain(String::new()),
                    lines: vec![
                        "| A | B |".to_string(),
                        "| --- | --- |".to_string(),
                        "| 1 | 2 |".to_string(),
                    ],
                    trailing: InlineTextTree::plain(String::new()),
                    split_physical_lines: false,
                },
                cx,
            );

            // The header row must survive: previously the first pasted line was
            // folded into the paragraph, leaving the alignment row to masquerade
            // as the header. The empty paste target is also dropped, and a
            // trailing paragraph is added so the document does not end on the
            // table with no line below it.
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            let table = visible[0].entity.read(cx);
            assert_eq!(table.kind(), BlockKind::Table);
            let data = table.record.table.as_ref().expect("table data");
            assert_eq!(data.header[0].serialize_markdown(), "A");
            assert_eq!(data.header[1].serialize_markdown(), "B");
            assert_eq!(data.rows.len(), 1);
            assert_eq!(data.rows[0][0].serialize_markdown(), "1");
            assert_eq!(data.rows[0][1].serialize_markdown(), "2");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
        });
    }

    #[gpui::test]
    async fn structural_paste_of_code_block_renders_native_code_block(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

        editor.update(cx, |editor, cx| {
            let block = editor.document.visible_blocks()[0].entity.clone();
            editor.on_block_event(
                block,
                &BlockEvent::RequestPasteMultiline {
                    leading: InlineTextTree::plain(String::new()),
                    lines: vec![
                        "```rust".to_string(),
                        "fn main() {}".to_string(),
                        "```".to_string(),
                    ],
                    trailing: InlineTextTree::plain(String::new()),
                    split_physical_lines: false,
                },
                cx,
            );

            // The fence is structural, so the whole paste goes through the block
            // importer rather than the plain-text path: the opening ```rust line is
            // no longer folded into a paragraph, and the empty paste target is
            // dropped. A trailing paragraph is added so the document does not end
            // on the code block with no line below it.
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            let code = visible[0].entity.read(cx);
            assert_eq!(
                code.kind(),
                BlockKind::CodeBlock {
                    language: Some("rust".into())
                }
            );
            assert_eq!(code.display_text(), "fn main() {}");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(
                editor.document.markdown_text(cx),
                "```rust\nfn main() {}\n```\n\n"
            );
        });
    }

    #[gpui::test]
    async fn structural_paste_of_table_preserves_surrounding_text(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, "beforeafter".into(), None));

        editor.update(cx, |editor, cx| {
            let block = editor.document.visible_blocks()[0].entity.clone();
            editor.on_block_event(
                block,
                &BlockEvent::RequestPasteMultiline {
                    leading: InlineTextTree::plain("before"),
                    lines: vec![
                        "| A | B |".to_string(),
                        "| --- | --- |".to_string(),
                        "| 1 | 2 |".to_string(),
                    ],
                    trailing: InlineTextTree::plain("after"),
                    split_physical_lines: false,
                },
                cx,
            );

            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 3);
            assert_eq!(visible[0].entity.read(cx).display_text(), "before");

            let table = visible[1].entity.read(cx);
            assert_eq!(table.kind(), BlockKind::Table);
            let data = table.record.table.as_ref().expect("table data");
            assert_eq!(data.header[0].serialize_markdown(), "A");
            assert_eq!(data.rows[0][0].serialize_markdown(), "1");

            assert_eq!(visible[2].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[2].entity.read(cx).display_text(), "after");
        });
    }

    #[gpui::test]
    async fn structural_paste_of_code_block_preserves_surrounding_text(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, "beforeafter".into(), None));

        editor.update(cx, |editor, cx| {
            let block = editor.document.visible_blocks()[0].entity.clone();
            editor.on_block_event(
                block,
                &BlockEvent::RequestPasteMultiline {
                    leading: InlineTextTree::plain("before"),
                    lines: vec![
                        "```rust".to_string(),
                        "fn main() {}".to_string(),
                        "```".to_string(),
                    ],
                    trailing: InlineTextTree::plain("after"),
                    split_physical_lines: false,
                },
                cx,
            );

            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 3);
            assert_eq!(visible[0].entity.read(cx).display_text(), "before");
            assert_eq!(
                visible[1].entity.read(cx).kind(),
                BlockKind::CodeBlock {
                    language: Some("rust".into())
                }
            );
            assert_eq!(visible[1].entity.read(cx).display_text(), "fn main() {}");
            assert_eq!(visible[2].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[2].entity.read(cx).display_text(), "after");
            // Text already follows the code block, so no extra trailing
            // paragraph is added mid-document.
        });
    }

    #[gpui::test]
    async fn structural_paste_at_document_end_adds_one_trailing_paragraph(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, "intro".into(), None));

        editor.update(cx, |editor, cx| {
            let block = editor.document.visible_blocks()[0].entity.clone();
            block.update(cx, |block, _cx| {
                block.selected_range = block.visible_len()..block.visible_len();
            });
            editor.on_block_event(
                block,
                &BlockEvent::RequestPasteMultiline {
                    leading: InlineTextTree::plain("intro"),
                    lines: vec!["***".to_string()],
                    trailing: InlineTextTree::plain(String::new()),
                    split_physical_lines: false,
                },
                cx,
            );

            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 3);
            assert_eq!(visible[0].entity.read(cx).display_text(), "intro");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Separator);
            assert_eq!(visible[2].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[2].entity.read(cx).display_text(), "");
        });
    }

    #[gpui::test]
    async fn structural_paste_of_quote_at_document_end_adds_trailing_paragraph(
        cx: &mut TestAppContext,
    ) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, "intro".into(), None));

        editor.update(cx, |editor, cx| {
            let block = editor.document.visible_blocks()[0].entity.clone();
            block.update(cx, |block, _cx| {
                block.selected_range = block.visible_len()..block.visible_len();
            });
            editor.on_block_event(
                block,
                &BlockEvent::RequestPasteMultiline {
                    leading: InlineTextTree::plain("intro"),
                    lines: vec!["> quoted".to_string()],
                    trailing: InlineTextTree::plain(String::new()),
                    split_physical_lines: false,
                },
                cx,
            );

            // The quote container cannot hold the caret below it, so a trailing
            // paragraph is added even though quote normalization re-parses the
            // whole document on the way.
            let roots = editor.document.root_blocks();
            assert_eq!(roots.len(), 3);
            assert_eq!(roots[0].read(cx).display_text(), "intro");
            assert_eq!(roots[1].read(cx).kind(), BlockKind::Quote);
            assert_eq!(roots[2].read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(roots[2].read(cx).display_text(), "");
        });
    }
