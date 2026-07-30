// @author kongweiguang

    #[gpui::test]
    async fn enter_at_heading_start_inserts_paragraph_before_intact_heading(
        cx: &mut TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "# Heading\n\nAfter".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let heading = editor.document.visible_blocks()[0].entity.clone();
                heading.update(cx, |block, block_cx| {
                    block.move_to(0, block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 3);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[0].entity.read(cx).display_text(), "");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Heading { level: 1 });
            assert_eq!(visible[1].entity.read(cx).display_text(), "Heading");
            assert_eq!(visible[2].entity.read(cx).display_text(), "After");
            assert_eq!(editor.pending_focus, Some(visible[0].entity.entity_id()));
        });
    }

    #[gpui::test]
    async fn deleting_focused_mermaid_preview_removes_the_whole_block(
        cx: &mut TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| {
            Editor::from_markdown(
                cx,
                "before\n\n```mermaid\nflowchart LR\nA --> B\n```\n\nafter".to_string(),
                None,
            )
        });

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let mermaid = editor.document.visible_blocks()[1].entity.clone();
                editor.focus_block(mermaid.entity_id());
                mermaid.update(cx, |block, block_cx| {
                    assert_eq!(
                        block.mermaid_view_mode(),
                        crate::components::MermaidViewMode::Preview
                    );
                    block.on_delete(&Delete, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(editor.document.markdown_text(cx), "before\n\nafter");
            assert_eq!(editor.pending_focus, Some(visible[0].entity.entity_id()));
        });
    }

    #[gpui::test]
    async fn mermaid_fence_then_enter_creates_native_mermaid_block(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "```mermaid".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let block = editor.document.visible_blocks()[0].entity.clone();
                block.update(cx, |block, block_cx| {
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let block = editor.document.visible_blocks()[0].entity.read(cx);
            assert_eq!(block.kind(), BlockKind::MermaidBlock);
            assert_eq!(
                block.mermaid_view_mode(),
                crate::components::MermaidViewMode::Source
            );
            assert_eq!(block.display_text(), "```mermaid\n\n```");
            assert_eq!(block.selected_range, 11..11);
            assert_eq!(editor.document.markdown_text(cx), "```mermaid\n\n```");
        });
    }

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
