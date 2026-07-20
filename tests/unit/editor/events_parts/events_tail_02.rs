// @author kongweiguang

    #[gpui::test]
    async fn dollar_dollar_prefix_then_enter_wraps_existing_line(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "E = mc^2".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let block = editor.document.visible_blocks()[0].entity.clone();
                block.update(cx, |block, block_cx| {
                    // Home, type the fence in front of the formula, then Enter.
                    block.move_to(0, block_cx);
                    block.replace_text_in_visible_range(0..0, "$$", None, false, block_cx);
                    block.move_to("$$".len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 1);
            let block = visible[0].entity.read(cx);
            assert_eq!(block.kind(), BlockKind::MathBlock);
            // The pre-existing text is kept as the formula body.
            assert_eq!(block.display_text(), "$$\nE = mc^2\n$$");
            assert_eq!(block.selected_range, "$$\n".len().."$$\n".len());
            assert_eq!(editor.document.markdown_text(cx), "$$\nE = mc^2\n$$");
        });
    }

    #[gpui::test]
    async fn enter_inside_math_block_keeps_local_formula_editing(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "$$n^2$$".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let block = editor.document.visible_blocks()[0].entity.clone();
                block.update(cx, |block, block_cx| {
                    block.move_to(3, block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 1);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::MathBlock);
            assert_eq!(visible[0].entity.read(cx).display_text(), "$$n\n^2$$");
            assert_eq!(editor.document.markdown_text(cx), "$$n\n^2$$");
        });
    }

    #[gpui::test]
    async fn auto_created_math_block_exit_shortcut_creates_plain_text_block(
        cx: &mut TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let block = editor.document.visible_blocks()[0].entity.clone();
                block.update(cx, |block, block_cx| {
                    block.replace_text_in_visible_range(
                        0..block.visible_len(),
                        "$$",
                        None,
                        false,
                        block_cx,
                    );
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                    block.on_exit_code_block(&ExitCodeBlock, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::MathBlock);
            assert_eq!(visible[0].entity.read(cx).display_text(), "$$\n\n$$");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            assert_eq!(editor.document.markdown_text(cx), "$$\n\n$$\n\n");
        });
    }

    #[gpui::test]
    async fn raw_like_block_exit_shortcut_creates_plain_text_block(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let cases = [
            (
                BlockRecord::html("<div>\ncontent\n</div>"),
                BlockKind::HtmlBlock,
                "<div>\ncontent\n</div>",
            ),
            (
                BlockRecord::mermaid("```mermaid\nflowchart LR\nA-->B\n```"),
                BlockKind::MermaidBlock,
                "```mermaid\nflowchart LR\nA-->B\n```",
            ),
            (
                BlockRecord::raw_markdown("::: custom\ncontent\n:::"),
                BlockKind::RawMarkdown,
                "::: custom\ncontent\n:::",
            ),
            (
                BlockRecord::comment("<!--\ncomment\n-->"),
                BlockKind::Comment,
                "<!--\ncomment\n-->",
            ),
        ];

        for (record, kind, text) in cases {
            let editor = cx.new(|cx| {
                let mut editor = Editor::from_markdown(cx, String::new(), None);
                let block = Editor::new_block(cx, record.clone());
                editor.document.replace_roots(vec![block], cx);
                editor
            });

            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    let block = editor.document.visible_blocks()[0].entity.clone();
                    block.update(cx, |block, block_cx| {
                        block.on_exit_code_block(&ExitCodeBlock, window, block_cx);
                    });
                });
            });

            editor.update(cx, |editor, cx| {
                let visible = editor.document.visible_blocks();
                assert_eq!(visible.len(), 2);
                assert_eq!(visible[0].entity.read(cx).kind(), kind);
                assert_eq!(visible[0].entity.read(cx).display_text(), text);
                assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
                assert_eq!(visible[1].entity.read(cx).display_text(), "");
            });
        }
    }

    #[gpui::test]
    async fn table_cell_enter_still_moves_to_next_row(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |", "| 3 | 4 |"].join("\n");
        let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

        let mut next_cell_id = None;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let table = editor.document.first_root().expect("table root").clone();
                let (cell, expected_next_cell_id) = {
                    let table = table.read(cx);
                    let runtime = table.table_runtime.as_ref().expect("table runtime");
                    (runtime.rows[0][0].clone(), runtime.rows[1][0].entity_id())
                };
                next_cell_id = Some(expected_next_cell_id);
                cell.update(cx, |block, block_cx| {
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, _cx| {
            assert_eq!(editor.document.visible_blocks().len(), 1);
            assert_eq!(editor.pending_focus, next_cell_id);
        });
    }

    #[gpui::test]
    async fn table_cell_exit_shortcut_inserts_sibling_after_table(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let markdown = ["> [!NOTE]", "> | A | B |", "> | --- | --- |", "> | 1 | 2 |"].join("\n");
        let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let callout = editor.document.first_root().expect("callout root").clone();
                let table = callout
                    .read(cx)
                    .children
                    .iter()
                    .find(|child| child.read(cx).kind() == BlockKind::Table)
                    .expect("nested table")
                    .clone();
                let cell = table
                    .read(cx)
                    .table_runtime
                    .as_ref()
                    .expect("table runtime")
                    .rows[0][0]
                    .clone();
                cell.update(cx, |block, block_cx| {
                    block.on_exit_code_block(&ExitCodeBlock, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let callout = editor.document.first_root().expect("callout root").clone();
            let children = callout.read(cx).children.clone();
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].read(cx).kind(), BlockKind::Table);
            assert_eq!(children[1].read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(children[1].read(cx).display_text(), "");
            assert_eq!(editor.pending_focus, Some(children[1].entity_id()));
        });
    }

    fn table_root(editor: &Editor, cx: &App) -> Entity<Block> {
        editor
            .document
            .visible_blocks()
            .iter()
            .map(|visible| visible.entity.clone())
            .find(|block| block.read(cx).kind() == BlockKind::Table)
            .expect("table root")
    }

    #[gpui::test]
    async fn arrow_down_from_last_row_exits_table_to_following_block(cx: &mut TestAppContext) {
        let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |", "", "after"].join("\n");
        let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

        editor.update(cx, |editor, cx| {
            let table = table_root(editor, cx);
            let cell = table
                .read(cx)
                .table_runtime
                .as_ref()
                .expect("table runtime")
                .rows
                .last()
                .and_then(|row| row.first())
                .cloned()
                .expect("last row cell");
            editor.on_block_event(
                cell,
                &BlockEvent::RequestTableCellMoveVertical { delta: 1 },
                cx,
            );

            let following = editor.document.visible_blocks()[1].entity.clone();
            assert_eq!(following.read(cx).display_text(), "after");
            assert_eq!(editor.pending_focus, Some(following.entity_id()));
        });
    }

    #[gpui::test]
    async fn arrow_up_from_header_exits_table_to_preceding_block(cx: &mut TestAppContext) {
        let markdown = ["before", "", "| A | B |", "| --- | --- |", "| 1 | 2 |"].join("\n");
        let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

        editor.update(cx, |editor, cx| {
            let table = table_root(editor, cx);
            let cell = table
                .read(cx)
                .table_runtime
                .as_ref()
                .expect("table runtime")
                .header
                .first()
                .cloned()
                .expect("header cell");
            editor.on_block_event(
                cell,
                &BlockEvent::RequestTableCellMoveVertical { delta: -1 },
                cx,
            );

            let preceding = editor.document.visible_blocks()[0].entity.clone();
            assert_eq!(preceding.read(cx).display_text(), "before");
            assert_eq!(editor.pending_focus, Some(preceding.entity_id()));
        });
    }

    #[gpui::test]
    async fn arrow_down_into_table_focuses_header_cell(cx: &mut TestAppContext) {
        let markdown = ["before", "", "| A | B |", "| --- | --- |", "| 1 | 2 |"].join("\n");
        let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

        editor.update(cx, |editor, cx| {
            let paragraph = editor
                .document
                .first_root()
                .expect("paragraph root")
                .clone();
            editor.on_block_event(
                paragraph,
                &BlockEvent::RequestFocusNext { preferred_x: None },
                cx,
            );

            let header_cell = table_root(editor, cx)
                .read(cx)
                .table_runtime
                .as_ref()
                .expect("table runtime")
                .header
                .first()
                .map(|cell| cell.entity_id());
            assert_eq!(editor.pending_focus, header_cell);
        });
    }

    #[gpui::test]
    async fn arrow_up_into_table_focuses_last_row_cell(cx: &mut TestAppContext) {
        let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |", "", "after"].join("\n");
        let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

        editor.update(cx, |editor, cx| {
            let paragraph = editor.document.visible_blocks()[1].entity.clone();
            assert_eq!(paragraph.read(cx).display_text(), "after");
            editor.on_block_event(
                paragraph,
                &BlockEvent::RequestFocusPrev { preferred_x: None },
                cx,
            );

            let last_row_cell = table_root(editor, cx)
                .read(cx)
                .table_runtime
                .as_ref()
                .expect("table runtime")
                .rows
                .last()
                .and_then(|row| row.first())
                .map(|cell| cell.entity_id());
            assert_eq!(editor.pending_focus, last_row_cell);
        });
    }

    #[gpui::test]
    async fn block_up_from_table_cell_exits_to_preceding_block(cx: &mut TestAppContext) {
        let markdown = ["before", "", "| A | B |", "| --- | --- |", "| 1 | 2 |"].join("\n");
        let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

        editor.update(cx, |editor, cx| {
            // Start from a body cell, not the header, to confirm Block Up leaves
            // the whole table instead of stepping to the cell above.
            let cell = table_root(editor, cx)
                .read(cx)
                .table_runtime
                .as_ref()
                .expect("table runtime")
                .rows
                .last()
                .and_then(|row| row.first())
                .cloned()
                .expect("body cell");
            editor.on_block_event(cell, &BlockEvent::RequestBlockUp, cx);

            let preceding = editor.document.visible_blocks()[0].entity.clone();
            assert_eq!(preceding.read(cx).display_text(), "before");
            assert_eq!(editor.pending_focus, Some(preceding.entity_id()));
        });
    }

    #[gpui::test]
    async fn block_down_into_table_focuses_header_cell(cx: &mut TestAppContext) {
        let markdown = ["before", "", "| A | B |", "| --- | --- |", "| 1 | 2 |"].join("\n");
        let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

        editor.update(cx, |editor, cx| {
            let paragraph = editor
                .document
                .first_root()
                .expect("paragraph root")
                .clone();
            editor.on_block_event(paragraph, &BlockEvent::RequestBlockDown, cx);

            let header_cell = table_root(editor, cx)
                .read(cx)
                .table_runtime
                .as_ref()
                .expect("table runtime")
                .header
                .first()
                .map(|cell| cell.entity_id());
            assert_eq!(editor.pending_focus, header_cell);
        });
    }

    #[gpui::test]
    async fn down_out_of_code_block_focuses_following_block(cx: &mut TestAppContext) {
        let editor =
            cx.new(|cx| Editor::from_markdown(cx, "```rust\nab\n```\n\nafter".to_string(), None));

        editor.update(cx, |editor, cx| {
            let code = editor.document.first_root().expect("code root").clone();
            assert!(code.read(cx).kind().is_code_block());
            // Down from the language field emits RequestFocusNext; with a block
            // below, focus lands there rather than creating anything.
            editor.on_block_event(
                code,
                &BlockEvent::RequestFocusNext { preferred_x: None },
                cx,
            );

            let following = editor.document.visible_blocks()[1].entity.clone();
            assert_eq!(following.read(cx).display_text(), "after");
            assert_eq!(editor.document.root_count(), 2);
            assert_eq!(editor.pending_focus, Some(following.entity_id()));
        });
    }

    #[gpui::test]
    async fn down_out_of_trailing_code_block_creates_and_focuses_paragraph(
        cx: &mut TestAppContext,
    ) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, "```rust\nab\n```".to_string(), None));

        editor.update(cx, |editor, cx| {
            let code = editor.document.first_root().expect("code root").clone();
            assert_eq!(editor.document.root_count(), 1);
            editor.on_block_event(
                code,
                &BlockEvent::RequestFocusNext { preferred_x: None },
                cx,
            );

            let roots = editor.document.root_blocks();
            assert_eq!(roots.len(), 2);
            assert_eq!(roots[1].read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(roots[1].read(cx).display_text(), "");
            assert_eq!(editor.pending_focus, Some(roots[1].entity_id()));
        });
    }

    #[gpui::test]
    async fn down_out_of_trailing_math_block_creates_and_focuses_paragraph(
        cx: &mut TestAppContext,
    ) {
        // Same miss as code blocks, one of the other multi-line widget blocks.
        let editor = cx.new(|cx| Editor::from_markdown(cx, "$$\nx^2\n$$".to_string(), None));

        editor.update(cx, |editor, cx| {
            let math = editor.document.first_root().expect("math root").clone();
            assert_eq!(math.read(cx).kind(), BlockKind::MathBlock);
            editor.on_block_event(
                math,
                &BlockEvent::RequestFocusNext { preferred_x: None },
                cx,
            );

            let roots = editor.document.root_blocks();
            assert_eq!(roots.len(), 2);
            assert_eq!(roots[1].read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(editor.pending_focus, Some(roots[1].entity_id()));
        });
    }

    #[gpui::test]
    async fn down_at_end_of_trailing_paragraph_creates_nothing(cx: &mut TestAppContext) {
        // Regression guard: ordinary text blocks must not sprout a paragraph.
        let editor = cx.new(|cx| Editor::from_markdown(cx, "hello".to_string(), None));

        editor.update(cx, |editor, cx| {
            let paragraph = editor.document.first_root().expect("paragraph").clone();
            editor.on_block_event(
                paragraph,
                &BlockEvent::RequestFocusNext { preferred_x: None },
                cx,
            );

            // No trailing paragraph is invented for an ordinary text block.
            assert_eq!(editor.document.root_count(), 1);
        });
    }

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

