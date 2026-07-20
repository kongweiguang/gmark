// @author kongweiguang

    #[gpui::test]
    async fn image_paste_text_in_code_block_stays_inside_block(cx: &mut TestAppContext) {
        let editor =
            cx.new(|cx| Editor::from_markdown(cx, "```\nbeforeafter\n```".to_string(), None));

        editor.update(cx, |editor, cx| {
            let block = editor.document.first_root().expect("code block").clone();
            editor.replace_current_block_selection_with_image_text(
                &block,
                &InlineTextTree::plain("before"),
                "![image](./assets/image.png)",
                &InlineTextTree::plain("after"),
                cx,
            );

            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 1);
            assert_eq!(
                visible[0].entity.read(cx).kind(),
                BlockKind::CodeBlock { language: None }
            );
            assert_eq!(
                visible[0].entity.read(cx).display_text(),
                "before![image](./assets/image.png)after"
            );
        });
    }

    #[gpui::test]
    async fn typing_callout_shortcut_materializes_body_and_focuses_it(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

        editor.update(cx, |editor, cx| {
            let paragraph = editor
                .document
                .first_root()
                .expect("root paragraph")
                .clone();
            paragraph.update(cx, |block, cx| {
                block.prepare_undo_capture(crate::components::UndoCaptureKind::CoalescibleText, cx);
                block.replace_text_in_visible_range(0..0, "> [!NOTE]", None, false, cx);
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(
                visible[0].entity.read(cx).kind(),
                BlockKind::Callout(CalloutVariant::Note)
            );
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            assert_eq!(visible[1].entity.read(cx).quote_depth, 1);
            assert_eq!(editor.document.markdown_text(cx), "> [!NOTE]\n> ");
            assert_eq!(editor.pending_focus, Some(visible[1].entity.entity_id()));
        });
    }

    #[gpui::test]
    async fn typing_numbered_list_shortcut_after_separator_preserves_group_boundary(
        cx: &mut TestAppContext,
    ) {
        let editor =
            cx.new(|cx| Editor::from_markdown(cx, "1. aa\n2. bb\n3. cc".to_string(), None));

        let separator_id = editor.update(cx, |editor, cx| {
            let separator = Editor::new_block(cx, BlockRecord::paragraph(String::new()));
            editor.document.insert_blocks_at(
                None,
                editor.document.root_count(),
                vec![separator.clone()],
                cx,
            );
            separator.entity_id()
        });

        editor.update(cx, |editor, cx| {
            let separator = editor
                .document
                .block_entity_by_id(separator_id)
                .expect("separator paragraph");
            assert!(separator.read(cx).list_group_separator_candidate);
            separator.update(cx, |block, cx| {
                block.prepare_undo_capture(crate::components::UndoCaptureKind::CoalescibleText, cx);
                block.replace_text_in_visible_range(0..0, "1. ", None, false, cx);
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 5);
            assert_eq!(visible[0].entity.read(cx).list_ordinal, Some(1));
            assert_eq!(visible[1].entity.read(cx).list_ordinal, Some(2));
            assert_eq!(visible[2].entity.read(cx).list_ordinal, Some(3));
            assert_eq!(visible[3].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[3].entity.read(cx).display_text(), "");
            assert_eq!(visible[4].entity.entity_id(), separator_id);
            assert_eq!(
                visible[4].entity.read(cx).kind(),
                BlockKind::NumberedListItem
            );
            assert_eq!(visible[4].entity.read(cx).display_text(), "");
            assert_eq!(visible[4].entity.read(cx).list_ordinal, Some(1));
            assert_eq!(
                editor.document.markdown_text(cx),
                "1. aa\n2. bb\n3. cc\n\n1. "
            );
        });
    }

    #[gpui::test]
    async fn request_indent_nests_non_empty_list_item(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, "- a\n- b".to_string(), None));

        editor.update(cx, |editor, cx| {
            let second = editor.document.visible_blocks()[1].entity.clone();
            editor.on_block_event(second, &BlockEvent::RequestIndent, cx);

            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(
                visible[0].entity.read(cx).kind(),
                BlockKind::BulletedListItem
            );
            assert_eq!(
                visible[1].entity.read(cx).kind(),
                BlockKind::BulletedListItem
            );
            assert_eq!(visible[1].entity.read(cx).render_depth, 1);
            assert_eq!(editor.document.markdown_text(cx), "- a\n  - b");
        });
    }

    #[gpui::test]
    async fn request_outdent_lifts_list_child_paragraph_after_parent(cx: &mut TestAppContext) {
        let editor =
            cx.new(|cx| Editor::from_markdown(cx, "- item\n\n  child text".to_string(), None));

        let child_id = editor.update(cx, |editor, cx| {
            let child = editor.document.visible_blocks()[1].entity.clone();
            editor.on_block_event(child.clone(), &BlockEvent::RequestOutdent, cx);
            child.entity_id()
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(
                visible[0].entity.read(cx).kind(),
                BlockKind::BulletedListItem
            );
            assert_eq!(visible[0].entity.read(cx).display_text(), "item");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "child text");
            assert_eq!(visible[1].entity.read(cx).render_depth, 0);
            assert_eq!(visible[1].entity.entity_id(), child_id);
            assert_eq!(editor.document.markdown_text(cx), "- item\n\nchild text");
        });
    }

    #[gpui::test]
    async fn empty_list_child_paragraph_backspace_outdents_to_root(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "- item\n\n  child".to_string(), None));

        let child_id = editor.update(cx, |editor, _cx| {
            editor.document.visible_blocks()[1].entity.entity_id()
        });

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let child = editor.document.visible_blocks()[1].entity.clone();
                child.update(cx, |block, block_cx| {
                    block.prepare_undo_capture(
                        crate::components::UndoCaptureKind::NonCoalescible,
                        block_cx,
                    );
                    block.replace_text_in_visible_range(
                        0..block.visible_len(),
                        "",
                        None,
                        false,
                        block_cx,
                    );
                    block.move_to(0, block_cx);
                    block.on_delete_back(&DeleteBack, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(
                visible[0].entity.read(cx).kind(),
                BlockKind::BulletedListItem
            );
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            assert_eq!(visible[1].entity.entity_id(), child_id);
            assert_eq!(visible[1].entity.read(cx).render_depth, 0);
            assert_eq!(editor.document.markdown_text(cx), "- item\n\n");
        });
    }

    #[gpui::test]
    async fn empty_list_child_paragraph_enter_continues_same_level(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "- item\n\n  child".to_string(), None));

        let child_id = editor.update(cx, |editor, _cx| {
            editor.document.visible_blocks()[1].entity.entity_id()
        });

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let child = editor.document.visible_blocks()[1].entity.clone();
                child.update(cx, |block, block_cx| {
                    block.prepare_undo_capture(
                        crate::components::UndoCaptureKind::NonCoalescible,
                        block_cx,
                    );
                    block.replace_text_in_visible_range(
                        0..block.visible_len(),
                        "",
                        None,
                        false,
                        block_cx,
                    );
                    block.move_to(0, block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 3);
            assert_eq!(
                visible[0].entity.read(cx).kind(),
                BlockKind::BulletedListItem
            );
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            assert_eq!(visible[1].entity.entity_id(), child_id);
            assert_eq!(visible[1].entity.read(cx).render_depth, 1);
            assert_eq!(visible[2].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[2].entity.read(cx).display_text(), "");
            assert_eq!(visible[2].entity.read(cx).render_depth, 1);
            assert_eq!(editor.document.markdown_text(cx), "- item\n  \n  ");
        });
    }

    #[gpui::test]
    async fn enter_inside_script_paragraph_creates_new_block(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "H~2~O".to_string(), None));

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
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(visible[0].entity.read(cx).display_text(), "H2O");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            assert_eq!(editor.document.markdown_text(cx), "H~2~O\n\n");
        });
    }

    #[gpui::test]
    async fn enter_inside_inline_math_paragraph_creates_new_block(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "$n^2$".to_string(), None));

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
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[0].entity.read(cx).display_text(), "$n^2$");
            assert!(!visible[0].entity.read(cx).uses_raw_text_editing());
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            assert_eq!(editor.document.markdown_text(cx), "$n^2$\n\n");
        });
    }

    #[gpui::test]
    async fn trailing_fence_line_enter_closes_code_block(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor =
            cx.new(|cx| Editor::from_markdown(cx, "```rust\nlet x = 1;\n```".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let block = editor.document.visible_blocks()[0].entity.clone();
                block.update(cx, |block, block_cx| {
                    // Type a closing fence on a fresh last line, then Enter.
                    let end = block.visible_len();
                    block.replace_text_in_visible_range(end..end, "\n```", None, false, block_cx);
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(
                visible[0].entity.read(cx).kind(),
                BlockKind::CodeBlock {
                    language: Some("rust".into())
                }
            );
            assert_eq!(visible[0].entity.read(cx).display_text(), "let x = 1;");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            assert_eq!(
                editor.document.markdown_text(cx),
                "```rust\nlet x = 1;\n```\n\n"
            );
        });
    }

    #[gpui::test]
    async fn setext_equals_underline_enter_promotes_previous_paragraph_to_h1(
        cx: &mut TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "Title\n\n=====".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let underline = editor.document.visible_blocks()[1].entity.clone();
                underline.update(cx, |block, block_cx| {
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(
                visible[0].entity.read(cx).kind(),
                BlockKind::Heading { level: 1 }
            );
            assert_eq!(visible[0].entity.read(cx).display_text(), "Title");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            assert_eq!(editor.document.markdown_text(cx), "# Title\n\n");
        });

        // Reversible: undo restores the two original paragraphs.
        editor.update(cx, |editor, cx| {
            editor.undo_document(cx);
            assert_eq!(editor.document.markdown_text(cx), "Title\n\n=====");
        });
    }

    #[gpui::test]
    async fn setext_dash_underline_enter_promotes_previous_paragraph_to_h2(
        cx: &mut TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        // A bare "-----" in source parses as a thematic break, so simulate the
        // user typing the underline into the paragraph below the title instead.
        let editor = cx.new(|cx| Editor::from_markdown(cx, "Title\n\nx".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let underline = editor.document.visible_blocks()[1].entity.clone();
                underline.update(cx, |block, block_cx| {
                    let end = block.visible_len();
                    block.replace_text_in_visible_range(0..end, "-----", None, false, block_cx);
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(
                visible[0].entity.read(cx).kind(),
                BlockKind::Heading { level: 2 }
            );
            assert_eq!(visible[0].entity.read(cx).display_text(), "Title");
            assert_eq!(editor.document.markdown_text(cx), "## Title\n\n");
        });
    }

    #[gpui::test]
    async fn dash_underline_without_heading_target_stays_a_separator(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let block = editor.document.visible_blocks()[0].entity.clone();
                block.update(cx, |block, block_cx| {
                    block.replace_text_in_visible_range(0..0, "-----", None, false, block_cx);
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Separator);
        });
    }

    #[gpui::test]
    async fn equals_underline_without_heading_target_stays_a_paragraph(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let block = editor.document.visible_blocks()[0].entity.clone();
                block.update(cx, |block, block_cx| {
                    block.replace_text_in_visible_range(0..0, "=====", None, false, block_cx);
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[0].entity.read(cx).display_text(), "=====");
        });
    }

    #[gpui::test]
    async fn delimiter_row_enter_forms_native_table(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| {
            Editor::from_markdown(cx, "| Name | Score |\n\n| --- | --- |".to_string(), None)
        });

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let delimiter = editor.document.root_blocks()[1].clone();
                delimiter.update(cx, |block, block_cx| {
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let roots = editor.document.root_blocks();
            assert_eq!(roots.len(), 2);
            assert_eq!(roots[0].read(cx).kind(), BlockKind::Table);
            let table = roots[0].read(cx).record.table.clone().expect("table");
            assert_eq!(table.header.len(), 2);
            assert_eq!(table.header[0].serialize_markdown(), "Name");
            assert!(table.rows.is_empty());
            assert_eq!(roots[1].read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(
                editor.document.markdown_text(cx),
                "| Name | Score |\n| --- | --- |\n\n"
            );
        });

        // Reversible in one step back to the two source paragraphs.
        editor.update(cx, |editor, cx| {
            editor.undo_document(cx);
            assert_eq!(
                editor.document.markdown_text(cx),
                "| Name | Score |\n\n| --- | --- |"
            );
        });
    }

    #[gpui::test]
    async fn pipe_row_below_table_is_absorbed_as_a_row(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| {
            Editor::from_markdown(cx, "| Name | Score |\n\n| --- | --- |".to_string(), None)
        });

        // Form the table.
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let delimiter = editor.document.root_blocks()[1].clone();
                delimiter.update(cx, |block, block_cx| {
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        // Type a body row into the paragraph below the table and press Enter.
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let row = editor.document.root_blocks()[1].clone();
                row.update(cx, |block, block_cx| {
                    block.replace_text_in_visible_range(
                        0..0,
                        "| Alice | 10 |",
                        None,
                        false,
                        block_cx,
                    );
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let roots = editor.document.root_blocks();
            assert_eq!(roots[0].read(cx).kind(), BlockKind::Table);
            let table = roots[0].read(cx).record.table.clone().expect("table");
            assert_eq!(table.rows.len(), 1);
            assert_eq!(table.rows[0][0].serialize_markdown(), "Alice");
            assert_eq!(table.rows[0][1].serialize_markdown(), "10");
            assert_eq!(roots[1].read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(roots[1].read(cx).display_text(), "");
        });
    }

    #[gpui::test]
    async fn pipeless_delimiter_row_enter_forms_native_table(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor =
            cx.new(|cx| Editor::from_markdown(cx, "Name | Score\n\n---- | ----".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let delimiter = editor.document.root_blocks()[1].clone();
                delimiter.update(cx, |block, block_cx| {
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let roots = editor.document.root_blocks();
            assert_eq!(roots.len(), 2);
            assert_eq!(roots[0].read(cx).kind(), BlockKind::Table);
            let table = roots[0].read(cx).record.table.clone().expect("table");
            assert_eq!(table.header.len(), 2);
            assert_eq!(table.header[0].serialize_markdown(), "Name");
            assert_eq!(table.header[1].serialize_markdown(), "Score");
            assert!(table.rows.is_empty());
            assert_eq!(roots[1].read(cx).kind(), BlockKind::Paragraph);
        });
    }

    #[gpui::test]
    async fn pipeless_row_below_table_is_absorbed_as_a_row(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor =
            cx.new(|cx| Editor::from_markdown(cx, "Name | Score\n\n---- | ----".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let delimiter = editor.document.root_blocks()[1].clone();
                delimiter.update(cx, |block, block_cx| {
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        // A pipeless body row with the table's column count is absorbed.
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let row = editor.document.root_blocks()[1].clone();
                row.update(cx, |block, block_cx| {
                    block.replace_text_in_visible_range(0..0, "Alice | 10", None, false, block_cx);
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let roots = editor.document.root_blocks();
            assert_eq!(roots[0].read(cx).kind(), BlockKind::Table);
            let table = roots[0].read(cx).record.table.clone().expect("table");
            assert_eq!(table.rows.len(), 1);
            assert_eq!(table.rows[0][0].serialize_markdown(), "Alice");
            assert_eq!(table.rows[0][1].serialize_markdown(), "10");
            assert_eq!(roots[1].read(cx).kind(), BlockKind::Paragraph);
        });
    }

    #[gpui::test]
    async fn ragged_pipeless_row_below_table_is_padded_to_width(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx
            .new(|cx| Editor::from_markdown(cx, "A | B | C\n\n--- | --- | ---".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let delimiter = editor.document.root_blocks()[1].clone();
                delimiter.update(cx, |block, block_cx| {
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        // Two cells typed under a three-column table: absorbed as a row and
        // padded to the header width, matching how pasted ragged rows behave.
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let row = editor.document.root_blocks()[1].clone();
                row.update(cx, |block, block_cx| {
                    block.replace_text_in_visible_range(0..0, "one | two", None, false, block_cx);
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let table = editor.document.root_blocks()[0]
                .read(cx)
                .record
                .table
                .clone()
                .expect("table");
            assert_eq!(table.rows.len(), 1);
            assert_eq!(table.rows[0].len(), 3);
            assert_eq!(table.rows[0][0].serialize_markdown(), "one");
            assert_eq!(table.rows[0][1].serialize_markdown(), "two");
            assert_eq!(table.rows[0][2].serialize_markdown(), "");
        });
    }

    #[gpui::test]
    async fn lone_pipe_row_without_table_context_stays_a_paragraph(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let block = editor.document.root_blocks()[0].clone();
                block.update(cx, |block, block_cx| {
                    block.replace_text_in_visible_range(0..0, "| a | b |", None, false, block_cx);
                    block.move_to(block.visible_len(), block_cx);
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let roots = editor.document.root_blocks();
            assert_eq!(roots[0].read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(roots[0].read(cx).display_text(), "| a | b |");
        });
    }

    #[gpui::test]
    async fn math_block_exit_shortcut_creates_plain_text_block(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "$$n^2$$".to_string(), None));

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
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::MathBlock);
            assert_eq!(visible[0].entity.read(cx).display_text(), "$$n^2$$");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            assert_eq!(editor.document.markdown_text(cx), "$$n^2$$\n\n");
        });
    }

    #[gpui::test]
    async fn dollar_dollar_enter_creates_editable_math_block(cx: &mut TestAppContext) {
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
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 1);
            let block = visible[0].entity.read(cx);
            assert_eq!(block.kind(), BlockKind::MathBlock);
            assert_eq!(block.display_text(), "$$\n\n$$");
            assert_eq!(block.selected_range, 3..3);
            assert!(block.uses_raw_text_editing());
            assert_eq!(editor.document.markdown_text(cx), "$$\n\n$$");
        });
    }

