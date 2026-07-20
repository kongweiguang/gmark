// @author kongweiguang

    #[gpui::test]
    async fn structural_paste_of_callout_at_document_end_adds_trailing_paragraph(
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
                    lines: vec!["> [!NOTE]".to_string(), "> body".to_string()],
                    trailing: InlineTextTree::plain(String::new()),
                    split_physical_lines: false,
                },
                cx,
            );

            let roots = editor.document.root_blocks();
            assert_eq!(roots.len(), 3);
            assert_eq!(
                roots[1].read(cx).kind(),
                BlockKind::Callout(CalloutVariant::Note)
            );
            assert_eq!(roots[2].read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(roots[2].read(cx).display_text(), "");
        });
    }

    #[gpui::test]
    async fn structural_paste_of_footnote_definition_at_document_end_adds_trailing_paragraph(
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
                    lines: vec!["[^note]: definition body".to_string()],
                    trailing: InlineTextTree::plain(String::new()),
                    split_physical_lines: false,
                },
                cx,
            );

            let roots = editor.document.root_blocks();
            assert_eq!(roots.len(), 3);
            assert_eq!(roots[1].read(cx).kind(), BlockKind::FootnoteDefinition);
            assert_eq!(roots[2].read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(roots[2].read(cx).display_text(), "");
        });
    }

    #[gpui::test]
    async fn structural_paste_of_standalone_image_at_document_end_adds_trailing_paragraph(
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
                    lines: vec!["![alt](pic.png)".to_string()],
                    trailing: InlineTextTree::plain(String::new()),
                    split_physical_lines: false,
                },
                cx,
            );

            // A lone image renders as a self-contained widget, so it gets the
            // same trailing paragraph even though it is a paragraph block.
            let roots = editor.document.root_blocks();
            assert_eq!(roots.len(), 3);
            assert!(roots[1].read(cx).renders_as_standalone_image());
            assert_eq!(roots[2].read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(roots[2].read(cx).display_text(), "");
        });
    }

    #[gpui::test]
    async fn plain_multiline_paste_with_blank_script_lines_skips_separator_blanks(
        cx: &mut TestAppContext,
    ) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

        editor.update(cx, |editor, cx| {
            let block = editor.document.visible_blocks()[0].entity.clone();
            editor.on_block_event(
                block,
                &BlockEvent::RequestPasteMultiline {
                    leading: InlineTextTree::plain(String::new()),
                    lines: vec![
                        "H~2~O".to_string(),
                        String::new(),
                        "CO<sub>2</sub>".to_string(),
                        String::new(),
                        "x<sup>n</sup>".to_string(),
                        String::new(),
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
        });
    }

    #[gpui::test]
    async fn plain_multiline_paste_with_leading_inline_html_splits_physical_lines(
        cx: &mut TestAppContext,
    ) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

        editor.update(cx, |editor, cx| {
            let block = editor.document.visible_blocks()[0].entity.clone();
            editor.on_block_event(
                block,
                &BlockEvent::RequestPasteMultiline {
                    leading: InlineTextTree::plain(String::new()),
                    lines: vec![
                        "<sub>2</sub>".to_string(),
                        "<sup>n</sup>".to_string(),
                        "<span style=\"color:red\">x</span>".to_string(),
                    ],
                    trailing: InlineTextTree::plain(String::new()),
                    split_physical_lines: true,
                },
                cx,
            );

            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 3);
            assert_eq!(visible[0].entity.read(cx).display_text(), "2");
            assert_eq!(visible[1].entity.read(cx).display_text(), "n");
            assert_eq!(visible[2].entity.read(cx).display_text(), "x");
            assert_eq!(
                editor.document.markdown_text(cx),
                "<sub>2</sub>\n\n<sup>n</sup>\n\n<span style=\"color: rgba(255,0,0,1.000);\">x</span>"
            );
        });
    }

    #[gpui::test]
    async fn plain_paste_preserves_tibetan_spaces(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));
        let tibetan = "༄༅།།དཔལ་ལྡན་རྩ་བའི་བླ་མ་རིན་པོ་ཆེ།། བདག་གི་སྤྱི་བོར་པདྨའི་གདན་བཞུགས་ནས།། ";

        editor.update(cx, |editor, cx| {
            let block = editor.document.visible_blocks()[0].entity.clone();
            editor.on_block_event(
                block,
                &BlockEvent::RequestPasteMultiline {
                    leading: InlineTextTree::plain(String::new()),
                    lines: vec![tibetan.to_string()],
                    trailing: InlineTextTree::plain(String::new()),
                    split_physical_lines: true,
                },
                cx,
            );

            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 1);
            assert_eq!(visible[0].entity.read(cx).display_text(), tibetan);
            assert!(visible[0].entity.read(cx).display_text().contains("།། བདག"));
            assert!(visible[0].entity.read(cx).display_text().ends_with(' '));
            assert_eq!(editor.document.markdown_text(cx), tibetan);
        });
    }

    #[gpui::test]
    async fn nested_list_item_backspace_downgrades_to_direct_list_child(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "- a\n  - b".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let nested = editor.document.visible_blocks()[1].entity.clone();
                nested.update(cx, |block, block_cx| {
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
            assert_eq!(visible[1].entity.read(cx).display_text(), "b");
            assert_eq!(visible[1].entity.read(cx).render_depth, 1);
            assert_eq!(editor.document.markdown_text(cx), "- a\n\n  b");
        });
    }

    #[gpui::test]
    async fn empty_nested_list_item_backspace_twice_exits_to_outer_paragraph(
        cx: &mut TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "- a\n  - ".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let nested = editor.document.visible_blocks()[1].entity.clone();
                nested.update(cx, |block, block_cx| {
                    block.move_to(0, block_cx);
                    block.on_delete_back(&DeleteBack, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            assert_eq!(visible[1].entity.read(cx).render_depth, 1);
            assert_eq!(editor.document.markdown_text(cx), "- a\n  ");
        });

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let child = editor.document.visible_blocks()[1].entity.clone();
                child.update(cx, |block, block_cx| {
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
            assert_eq!(visible[1].entity.read(cx).render_depth, 0);
            assert_eq!(editor.document.markdown_text(cx), "- a\n\n");
        });
    }

    #[gpui::test]
    async fn nested_list_item_downgrade_hoists_children_after_paragraph(cx: &mut TestAppContext) {
        let editor =
            cx.new(|cx| Editor::from_markdown(cx, "- a\n  - b\n    - c\n  - d".to_string(), None));

        editor.update(cx, |editor, cx| {
            let nested = editor.document.visible_blocks()[1].entity.clone();
            editor.on_block_event(
                nested,
                &BlockEvent::RequestDowngradeNestedListItemToChildParagraph,
                cx,
            );

            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 4);
            assert_eq!(
                visible[0].entity.read(cx).kind(),
                BlockKind::BulletedListItem
            );
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "b");
            assert_eq!(visible[1].entity.read(cx).render_depth, 1);
            assert_eq!(
                visible[2].entity.read(cx).kind(),
                BlockKind::BulletedListItem
            );
            assert_eq!(visible[2].entity.read(cx).display_text(), "c");
            assert_eq!(visible[2].entity.read(cx).render_depth, 1);
            assert_eq!(
                visible[3].entity.read(cx).kind(),
                BlockKind::BulletedListItem
            );
            assert_eq!(visible[3].entity.read(cx).display_text(), "d");
            assert_eq!(visible[3].entity.read(cx).render_depth, 1);
            assert_eq!(
                editor.document.markdown_text(cx),
                "- a\n\n  b\n  - c\n  - d"
            );
        });
    }

    #[gpui::test]
    async fn nested_numbered_and_task_items_backspace_downgrade_to_list_child(
        cx: &mut TestAppContext,
    ) {
        let cx = cx.add_empty_window();

        let numbered = cx.new(|cx| Editor::from_markdown(cx, "1. a\n  1. b".to_string(), None));
        cx.update(|window, cx| {
            numbered.update(cx, |editor, cx| {
                let nested = editor.document.visible_blocks()[1].entity.clone();
                nested.update(cx, |block, block_cx| {
                    block.move_to(0, block_cx);
                    block.on_delete_back(&DeleteBack, window, block_cx);
                });
            });
        });
        numbered.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "b");
            assert_eq!(visible[1].entity.read(cx).render_depth, 1);
            assert_eq!(editor.document.markdown_text(cx), "1. a\n\n  b");
        });

        let task = cx.new(|cx| Editor::from_markdown(cx, "- [ ] a\n  - [ ] b".to_string(), None));
        cx.update(|window, cx| {
            task.update(cx, |editor, cx| {
                let nested = editor.document.visible_blocks()[1].entity.clone();
                nested.update(cx, |block, block_cx| {
                    block.move_to(0, block_cx);
                    block.on_delete_back(&DeleteBack, window, block_cx);
                });
            });
        });
        task.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "b");
            assert_eq!(visible[1].entity.read(cx).render_depth, 1);
            assert_eq!(editor.document.markdown_text(cx), "- [ ] a\n\n  b");
        });
    }

    #[gpui::test]
    async fn request_quote_break_creates_nested_leaf_quote_group(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, "> outer\n>> inner".to_string(), None));

        editor.update(cx, |editor, cx| {
            let nested_quote = editor.document.visible_blocks()[1].entity.clone();
            editor.on_block_event(nested_quote, &BlockEvent::RequestQuoteBreak, cx);

            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 4);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Quote);
            assert_eq!(visible[0].entity.read(cx).display_text(), "outer");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Quote);
            assert_eq!(visible[1].entity.read(cx).display_text(), "inner");
            assert_eq!(visible[1].entity.read(cx).quote_depth, 2);
            assert_eq!(visible[2].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[2].entity.read(cx).display_text(), "");
            assert_eq!(visible[2].entity.read(cx).quote_depth, 1);
            assert_eq!(visible[3].entity.read(cx).kind(), BlockKind::Quote);
            assert_eq!(visible[3].entity.read(cx).display_text(), "");
            assert_eq!(visible[3].entity.read(cx).quote_depth, 2);
            assert_eq!(
                editor.document.markdown_text(cx),
                "> outer\n> > inner\n> \n> > "
            );
            assert_eq!(editor.pending_focus, Some(visible[3].entity.entity_id()));
        });
    }

    #[gpui::test]
    async fn imported_leaf_quote_backspace_twice_downgrades_to_text_block(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "> a".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let quote = editor.document.first_root().expect("root quote").clone();
                quote.update(cx, |block, block_cx| {
                    block.move_to(block.visible_len(), block_cx);
                    block.on_delete_back(&DeleteBack, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 1);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Quote);
            assert_eq!(visible[0].entity.read(cx).display_text(), "");
            assert_eq!(visible[0].entity.read(cx).quote_depth, 1);
            assert_eq!(editor.document.markdown_text(cx), "> ");
        });

        let empty_quote_id = editor.update(cx, |editor, _cx| {
            editor
                .document
                .first_root()
                .expect("empty quote")
                .entity_id()
        });

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let quote = editor.document.first_root().expect("empty quote").clone();
                quote.update(cx, |block, block_cx| {
                    block.move_to(0, block_cx);
                    block.on_delete_back(&DeleteBack, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 1);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[0].entity.read(cx).display_text(), "");
            assert_eq!(visible[0].entity.read(cx).quote_depth, 0);
            assert_eq!(visible[0].entity.entity_id(), empty_quote_id);
            assert_eq!(editor.document.markdown_text(cx), "");
        });
    }

    #[gpui::test]
    async fn shortcut_created_leaf_quote_backspace_twice_downgrades_to_text_block(
        cx: &mut TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

        editor.update(cx, |editor, cx| {
            let paragraph = editor
                .document
                .first_root()
                .expect("root paragraph")
                .clone();
            paragraph.update(cx, |block, cx| {
                block.prepare_undo_capture(crate::components::UndoCaptureKind::CoalescibleText, cx);
                block.replace_text_in_visible_range(0..0, "> ", None, false, cx);
                block.replace_text_in_visible_range(0..0, "a", None, false, cx);
            });
        });

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let quote = editor
                    .document
                    .first_root()
                    .expect("shortcut quote")
                    .clone();
                quote.update(cx, |block, block_cx| {
                    block.move_to(block.visible_len(), block_cx);
                    block.on_delete_back(&DeleteBack, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let quote = editor.document.first_root().expect("empty shortcut quote");
            assert_eq!(quote.read(cx).kind(), BlockKind::Quote);
            assert_eq!(quote.read(cx).display_text(), "");
            assert_eq!(quote.read(cx).quote_depth, 1);
            assert_eq!(editor.document.markdown_text(cx), "> ");
        });

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let quote = editor
                    .document
                    .first_root()
                    .expect("empty shortcut quote")
                    .clone();
                quote.update(cx, |block, block_cx| {
                    block.move_to(0, block_cx);
                    block.on_delete_back(&DeleteBack, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let paragraph = editor
                .document
                .first_root()
                .expect("text block after downgrade");
            assert_eq!(paragraph.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(paragraph.read(cx).display_text(), "");
            assert_eq!(editor.document.markdown_text(cx), "");
        });
    }

    #[gpui::test]
    async fn root_quote_break_then_backspace_keeps_text_block_slot_after_group(
        cx: &mut TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "> side\n>\n> 1234".to_string(), None));

        let new_leaf_id = editor.update(cx, |editor, cx| {
            let quote = editor.document.first_root().expect("group quote").clone();
            editor.on_block_event(quote, &BlockEvent::RequestQuoteBreak, cx);
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Quote);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            visible[1].entity.entity_id()
        });

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let new_leaf = editor.document.visible_blocks()[1].entity.clone();
                new_leaf.update(cx, |block, block_cx| {
                    block.move_to(0, block_cx);
                    block.on_delete_back(&DeleteBack, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Quote);
            assert_eq!(visible[0].entity.read(cx).display_text(), "side\n\n1234");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            assert_eq!(visible[1].entity.entity_id(), new_leaf_id);
            assert_eq!(visible[1].entity.read(cx).quote_depth, 0);
            assert_eq!(editor.document.markdown_text(cx), "> side\n> \n> 1234\n\n");
        });
    }

    #[gpui::test]
    async fn empty_callout_body_backspace_downgrades_parent_to_quote(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "> [!NOTE]\n> ".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let body = editor.document.visible_blocks()[1].entity.clone();
                body.update(cx, |block, block_cx| {
                    block.move_to(0, block_cx);
                    block.on_delete_back(&DeleteBack, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 1);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Quote);
            assert_eq!(visible[0].entity.read(cx).display_text(), "[!NOTE]");
            assert_eq!(editor.document.markdown_text(cx), "> \\[!NOTE]");
        });
    }

    #[gpui::test]
    async fn callout_exit_break_creates_plain_text_block(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "> [!TIP]\n> body".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let body = editor.document.visible_blocks()[1].entity.clone();
                body.update(cx, |block, block_cx| {
                    block.on_exit_code_block(&ExitCodeBlock, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 3);
            assert_eq!(
                visible[0].entity.read(cx).kind(),
                BlockKind::Callout(CalloutVariant::Tip)
            );
            assert_eq!(visible[2].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[2].entity.read(cx).display_text(), "");
            assert_eq!(visible[2].entity.read(cx).quote_depth, 0);
            assert_eq!(editor.document.markdown_text(cx), "> [!TIP]\n> body\n\n");
            assert_eq!(editor.pending_focus, Some(visible[2].entity.entity_id()));
        });
    }

    #[gpui::test]
    async fn delete_on_empty_leaf_quote_downgrades_to_text_block(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "> ".to_string(), None));

        let empty_quote_id = editor.update(cx, |editor, _cx| {
            editor
                .document
                .first_root()
                .expect("empty quote")
                .entity_id()
        });

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let quote = editor.document.first_root().expect("empty quote").clone();
                quote.update(cx, |block, block_cx| {
                    block.move_to(0, block_cx);
                    block.on_delete(&Delete, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 1);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[0].entity.read(cx).display_text(), "");
            assert_eq!(visible[0].entity.entity_id(), empty_quote_id);
            assert_eq!(editor.document.markdown_text(cx), "");
        });
    }

    #[gpui::test]
    async fn quote_container_with_children_does_not_collapse_from_leaf_exit_path(
        cx: &mut TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, ">\n> - item".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let quote = editor
                    .document
                    .first_root()
                    .expect("container quote")
                    .clone();
                quote.update(cx, |block, block_cx| {
                    block.move_to(0, block_cx);
                    block.on_delete_back(&DeleteBack, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Quote);
            assert_eq!(visible[0].entity.read(cx).display_text(), "");
            assert_eq!(visible[0].entity.read(cx).quote_depth, 1);
            assert!(!visible[0].entity.read(cx).children.is_empty());
            assert_eq!(
                visible[1].entity.read(cx).kind(),
                BlockKind::BulletedListItem
            );
            assert_eq!(editor.document.markdown_text(cx), "> - item");
        });
    }

    #[gpui::test]
    async fn quote_newline_inside_title_stays_in_one_source_authoritative_group(
        cx: &mut TestAppContext,
    ) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, "> firstsecond".to_string(), None));

        editor.update(cx, |editor, cx| {
            let quote = editor.document.first_root().expect("root quote").clone();
            quote.update(cx, |block, cx| {
                block.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
                block.replace_text_in_visible_range(5..5, "\n", None, false, cx);
            });

            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 1);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Quote);
            assert_eq!(visible[0].entity.read(cx).display_text(), "first\nsecond");
            assert_eq!(editor.document.markdown_text(cx), "> first\n> second");
        });
    }

    #[gpui::test]
    async fn root_quote_enter_stays_in_same_group(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let editor = cx.new(|cx| Editor::from_markdown(cx, "> first".to_string(), None));

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let quote = editor.document.first_root().expect("root quote").clone();
                quote.update(cx, |block, block_cx| {
                    block.move_to(block.visible_len(), block_cx);
                });
                quote.update(cx, |block, block_cx| {
                    block.on_newline(&Newline, window, block_cx);
                });
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Quote);
            assert_eq!(visible[0].entity.read(cx).display_text(), "first");
            assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Paragraph);
            assert_eq!(visible[1].entity.read(cx).display_text(), "");
            assert_eq!(visible[1].entity.read(cx).quote_depth, 1);
            assert_eq!(editor.document.markdown_text(cx), "> first\n> ");
        });
    }

    #[gpui::test]
    async fn multiline_edit_inside_quote_reparses_into_child_blocks(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| Editor::from_markdown(cx, "> first".to_string(), None));

        editor.update(cx, |editor, cx| {
            let quote = editor.document.first_root().expect("root quote").clone();
            quote.update(cx, |block, cx| {
                block.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
                block.replace_text_in_visible_range(5..5, "\n- item", None, false, cx);
            });
        });

        editor.update(cx, |editor, cx| {
            let visible = editor.document.visible_blocks();
            assert_eq!(visible.len(), 2);
            assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Quote);
            assert_eq!(visible[0].entity.read(cx).display_text(), "first");
            assert_eq!(
                visible[1].entity.read(cx).kind(),
                BlockKind::BulletedListItem
            );
            assert_eq!(visible[1].entity.read(cx).display_text(), "item");
            assert_eq!(editor.document.markdown_text(cx), "> first\n> - item");
        });
    }

