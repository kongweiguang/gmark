// @author kongweiguang

use super::Editor;
use crate::components::{
    Block, BlockDropPlacement, BlockEvent, BlockKind, BlockRecord, CalloutVariant, Delete,
    DeleteBack, ExitCodeBlock, InlineTextTree, Newline, SlashCommand,
};
use gpui::{App, AppContext, Bounds, Entity, TestAppContext, point, px, size};

#[gpui::test]
async fn clicking_canvas_below_last_block_focuses_document_end(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "first\n\nlast".to_string(), None));

    editor.update(cx, |editor, cx| {
        let visible = editor.document.visible_blocks();
        let first = visible.first().expect("first block").entity.clone();
        let last = visible.last().expect("last block").entity.clone();
        last.update(cx, |block, _cx| {
            block.last_bounds = Some(Bounds::new(
                point(px(100.0), px(120.0)),
                size(px(500.0), px(24.0)),
            ));
        });
        editor.active_entity_id = Some(first.entity_id());

        assert!(editor.focus_document_end_from_blank_area(point(px(300.0), px(300.0)), cx));
        assert_eq!(editor.pending_focus, Some(last.entity_id()));
        assert_eq!(editor.active_entity_id, Some(last.entity_id()));
        assert_eq!(last.read(cx).selected_range, 4..4);
    });
}

#[gpui::test]
async fn clicking_inside_last_block_keeps_block_hit_testing_in_charge(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "first\n\nlast".to_string(), None));

    editor.update(cx, |editor, cx| {
        let visible = editor.document.visible_blocks();
        let first = visible.first().expect("first block").entity.clone();
        let last = visible.last().expect("last block").entity.clone();
        last.update(cx, |block, _cx| {
            block.last_bounds = Some(Bounds::new(
                point(px(100.0), px(120.0)),
                size(px(500.0), px(24.0)),
            ));
        });
        editor.active_entity_id = Some(first.entity_id());
        editor.pending_focus = None;

        assert!(!editor.focus_document_end_from_blank_area(point(px(300.0), px(132.0)), cx));
        assert_eq!(editor.active_entity_id, Some(first.entity_id()));
        assert_eq!(editor.pending_focus, None);
    });
}

#[gpui::test]
async fn clicking_below_trailing_code_block_creates_usable_paragraph(cx: &mut TestAppContext) {
    let source = "```rust\nab\n```";
    let editor = cx.new(|cx| Editor::from_markdown(cx, source.to_string(), None));

    editor.update(cx, |editor, cx| {
        let code = editor.document.first_root().expect("code block").clone();
        code.update(cx, |block, _cx| {
            block.last_bounds = Some(Bounds::new(
                point(px(100.0), px(120.0)),
                size(px(500.0), px(72.0)),
            ));
        });

        assert!(editor.focus_document_end_from_blank_area(point(px(300.0), px(300.0)), cx));
        let roots = editor.document.root_blocks();
        assert_eq!(roots.len(), 2);
        let paragraph = roots[1].clone();
        assert_eq!(paragraph.read(cx).kind(), BlockKind::Paragraph);
        assert_eq!(paragraph.read(cx).display_text(), "");
        assert_eq!(editor.pending_focus, Some(paragraph.entity_id()));
        assert_eq!(paragraph.read(cx).selected_range, 0..0);
        assert!(editor.document_dirty);
        assert_eq!(editor.undo_history.len(), 1);

        editor.undo_document(cx);
        assert_eq!(editor.document.root_count(), 1);
        assert_eq!(editor.source_document.text(), source);
    });
}

#[gpui::test]
async fn slash_heading_is_one_undoable_editor_transaction(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "/h1".to_string(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        block.update(cx, |block, cx| block.move_to(block.visible_len(), cx));
        editor.on_block_event(
            block.clone(),
            &BlockEvent::RequestSlashCommand {
                command: SlashCommand::Heading1,
                trigger_range: 0..3,
            },
            cx,
        );
        assert_eq!(block.read(cx).kind(), BlockKind::Heading { level: 1 });
        assert_eq!(editor.source_document.text(), "# ");
        assert_eq!(editor.undo_history.len(), 1);

        editor.undo_document(cx);
        assert_eq!(editor.source_document.text(), "/h1");
        editor.redo_document(cx);
        assert_eq!(editor.source_document.text(), "# ");
    });
}

#[gpui::test]
async fn slash_table_replaces_query_and_installs_native_runtime(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "/bg".to_string(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        block.update(cx, |block, cx| block.move_to(block.visible_len(), cx));
        editor.on_block_event(
            block,
            &BlockEvent::RequestSlashCommand {
                command: SlashCommand::Table,
                trigger_range: 0..3,
            },
            cx,
        );

        let table = editor.document.first_root().expect("table").clone();
        let table_ref = table.read(cx);
        assert_eq!(table_ref.kind(), BlockKind::Table);
        let runtime = table_ref.table_runtime.as_ref().expect("table runtime");
        assert_eq!(runtime.header.len(), 2);
        assert_eq!(runtime.rows.len(), 2);
        assert!(!editor.source_document.text().contains("/bg"));
        assert_eq!(editor.undo_history.len(), 1);
    });
}

#[gpui::test]
async fn slash_commands_emit_expected_source_and_caret(cx: &mut TestAppContext) {
    let cases = [
        (
            "/h3",
            SlashCommand::Heading3,
            BlockKind::Heading { level: 3 },
            "### ",
            0,
        ),
        (
            "/lb",
            SlashCommand::BulletedList,
            BlockKind::BulletedListItem,
            "- ",
            0,
        ),
        (
            "/number",
            SlashCommand::NumberedList,
            BlockKind::NumberedListItem,
            "1. ",
            0,
        ),
        (
            "/task",
            SlashCommand::TaskList,
            BlockKind::TaskListItem { checked: false },
            "- [ ] ",
            0,
        ),
        ("/quote", SlashCommand::Quote, BlockKind::Quote, "> ", 0),
        (
            "/code",
            SlashCommand::CodeBlock,
            BlockKind::CodeBlock { language: None },
            "```\n\n```",
            0,
        ),
        (
            "/image",
            SlashCommand::Image,
            BlockKind::Paragraph,
            "![]()",
            2,
        ),
        (
            "/math",
            SlashCommand::Math,
            BlockKind::MathBlock,
            "$$\n\n$$\n\n",
            3,
        ),
        (
            "/divider",
            SlashCommand::HorizontalRule,
            BlockKind::Separator,
            "---\n\n",
            0,
        ),
    ];

    for (query, command, expected_kind, expected_source, expected_cursor) in cases {
        let editor = cx.new(|cx| Editor::from_markdown(cx, query.to_owned(), None));
        editor.update(cx, |editor, cx| {
            let block = editor.document.first_root().expect("root").clone();
            block.update(cx, |block, cx| block.move_to(block.visible_len(), cx));
            editor.on_block_event(
                block.clone(),
                &BlockEvent::RequestSlashCommand {
                    command,
                    trigger_range: 0..query.len(),
                },
                cx,
            );
            let result_block = if matches!(
                command,
                SlashCommand::Image | SlashCommand::Math | SlashCommand::HorizontalRule
            ) {
                editor.document.first_root().expect("inserted root").clone()
            } else {
                block
            };
            assert_eq!(result_block.read(cx).kind(), expected_kind, "query {query}");
            assert_eq!(
                editor.source_document.text(),
                expected_source,
                "query {query}"
            );
            assert_eq!(
                result_block.read(cx).selected_range,
                expected_cursor..expected_cursor,
                "query {query}"
            );
        });
    }
}

#[gpui::test]
async fn slash_command_inside_text_preserves_content_and_uses_one_undo_step(
    cx: &mut TestAppContext,
) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha /h2 omega".to_owned(), None));
    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("paragraph").clone();
        block.update(cx, |block, cx| block.move_to(9, cx));
        editor.on_block_event(
            block.clone(),
            &BlockEvent::RequestSlashCommand {
                command: SlashCommand::Heading2,
                trigger_range: 6..9,
            },
            cx,
        );

        assert_eq!(block.read(cx).kind(), BlockKind::Heading { level: 2 });
        assert_eq!(editor.source_document.text(), "## alpha  omega");
        assert_eq!(editor.undo_history.len(), 1);
        editor.undo_document(cx);
        assert_eq!(editor.source_document.text(), "alpha /h2 omega");
    });
}

#[gpui::test]
async fn programmatic_block_commands_duplicate_move_and_delete_atomically(cx: &mut TestAppContext) {
    let duplicate_editor = cx.new(|cx| Editor::from_markdown(cx, "one\n\ntwo".to_owned(), None));
    duplicate_editor.update(cx, |editor, cx| {
        let first = editor.document.visible_blocks()[0].entity.clone();
        editor.on_block_event(
            first,
            &BlockEvent::RequestSlashCommand {
                command: SlashCommand::DuplicateBlock,
                trigger_range: 0..0,
            },
            cx,
        );
        assert_eq!(editor.source_document.text(), "one\n\none\n\ntwo");
        assert_eq!(editor.undo_history.len(), 1);
        editor.undo_document(cx);
        assert_eq!(editor.source_document.text(), "one\n\ntwo");
    });

    let move_editor =
        cx.new(|cx| Editor::from_markdown(cx, "one\n\ntwo\n\nthree".to_owned(), None));
    move_editor.update(cx, |editor, cx| {
        let visible = editor.document.visible_blocks();
        let first = visible[0].entity.clone();
        let third = visible[2].entity.entity_id();
        editor.on_block_event(
            first,
            &BlockEvent::RequestMoveBlock {
                source: third,
                placement: BlockDropPlacement::Before,
            },
            cx,
        );
        assert_eq!(editor.source_document.text(), "three\n\none\n\ntwo");
        assert_eq!(editor.undo_history.len(), 1);
        editor.undo_document(cx);
        assert_eq!(editor.source_document.text(), "one\n\ntwo\n\nthree");
    });

    let move_to_tail_editor =
        cx.new(|cx| Editor::from_markdown(cx, "one\n\ntwo\n\nthree".to_owned(), None));
    move_to_tail_editor.update(cx, |editor, cx| {
        let visible = editor.document.visible_blocks();
        let first = visible[0].entity.entity_id();
        let third = visible[2].entity.clone();
        editor.on_block_event(
            third,
            &BlockEvent::RequestMoveBlock {
                source: first,
                placement: BlockDropPlacement::After,
            },
            cx,
        );
        assert_eq!(editor.source_document.text(), "two\n\nthree\n\none");
        assert_eq!(editor.undo_history.len(), 1);
        editor.undo_document(cx);
        assert_eq!(editor.source_document.text(), "one\n\ntwo\n\nthree");
    });

    let delete_editor = cx.new(|cx| Editor::from_markdown(cx, "only".to_owned(), None));
    delete_editor.update(cx, |editor, cx| {
        let only = editor.document.first_root().expect("only block").clone();
        editor.on_block_event(
            only,
            &BlockEvent::RequestSlashCommand {
                command: SlashCommand::DeleteBlock,
                trigger_range: 0..0,
            },
            cx,
        );
        assert_eq!(editor.document.root_count(), 1);
        assert_eq!(editor.source_document.text(), "");
        assert_eq!(editor.undo_history.len(), 1);
        editor.undo_document(cx);
        assert_eq!(editor.source_document.text(), "only");
    });
}

#[gpui::test]
async fn rich_block_slash_transform_and_insert_preserve_existing_content_kind(
    cx: &mut TestAppContext,
) {
    let transform = cx.new(|cx| Editor::from_markdown(cx, "# alpha /h2 omega".to_owned(), None));
    transform.update(cx, |editor, cx| {
        let heading = editor.document.first_root().expect("heading").clone();
        heading.update(cx, |block, _cx| block.selected_range = 9..9);
        editor.on_block_event(
            heading.clone(),
            &BlockEvent::RequestSlashCommand {
                command: SlashCommand::Heading2,
                trigger_range: 6..9,
            },
            cx,
        );
        assert_eq!(heading.read(cx).kind(), BlockKind::Heading { level: 2 });
        assert_eq!(editor.source_document.text(), "## alpha  omega");
        assert_eq!(editor.undo_history.len(), 1);
    });

    let insert = cx.new(|cx| Editor::from_markdown(cx, "# alpha /image omega".to_owned(), None));
    insert.update(cx, |editor, cx| {
        let heading = editor.document.first_root().expect("heading").clone();
        heading.update(cx, |block, _cx| block.selected_range = 12..12);
        editor.on_block_event(
            heading.clone(),
            &BlockEvent::RequestSlashCommand {
                command: SlashCommand::Image,
                trigger_range: 6..12,
            },
            cx,
        );
        assert_eq!(heading.read(cx).kind(), BlockKind::Heading { level: 1 });
        assert_eq!(editor.source_document.text(), "# alpha  omega\n\n![]()");
        assert_eq!(editor.undo_history.len(), 1);
    });
}

#[gpui::test]
async fn structural_revision_deterministically_closes_programmatic_slash_session(
    cx: &mut TestAppContext,
) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "one\n\ntwo".to_owned(), None));
    editor.update(cx, |editor, cx| {
        let first = editor.document.first_root().expect("first").clone();
        first.update(cx, |block, cx| block.open_block_action_menu(cx));
        assert!(first.read(cx).slash_menu.is_some());

        let appended = Editor::new_block(cx, BlockRecord::paragraph("three"));
        editor
            .document
            .insert_blocks_at(None, 2, vec![appended], cx);
        first.update(cx, |block, cx| block.refresh_slash_menu(cx));
        assert!(first.read(cx).slash_menu.is_none());
    });
}

#[gpui::test]
async fn invalid_programmatic_block_moves_are_no_op_without_undo(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "only".to_owned(), None));
    editor.update(cx, |editor, cx| {
        let only = editor.document.first_root().expect("only block").clone();
        assert_eq!(only.read(cx).structural_sibling_index, 0);
        assert_eq!(only.read(cx).structural_sibling_count, 1);

        for command in [SlashCommand::MoveBlockUp, SlashCommand::MoveBlockDown] {
            editor.on_block_event(
                only.clone(),
                &BlockEvent::RequestSlashCommand {
                    command,
                    trigger_range: 0..0,
                },
                cx,
            );
        }

        assert_eq!(editor.source_document.text(), "only");
        assert!(editor.undo_history.is_empty());
        assert!(!editor.document_dirty);
    });
}

#[test]
fn block_drop_index_covers_before_after_and_document_tail() {
    assert_eq!(
        Editor::sibling_drop_insert_index(0, 2, BlockDropPlacement::Before),
        1
    );
    assert_eq!(
        Editor::sibling_drop_insert_index(2, 0, BlockDropPlacement::Before),
        0
    );
    assert_eq!(
        Editor::sibling_drop_insert_index(0, 2, BlockDropPlacement::After),
        2
    );
    assert_eq!(
        Editor::sibling_drop_insert_index(2, 0, BlockDropPlacement::After),
        1
    );
}

#[gpui::test]
async fn enter_commits_visible_slash_command_before_inserting_newline(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let editor = cx.new(|cx| Editor::from_markdown(cx, "/divider".to_owned(), None));

    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            let block = editor
                .document
                .first_root()
                .expect("slash paragraph")
                .clone();
            block.update(cx, |block, block_cx| {
                block.move_to(block.visible_len(), block_cx);
                block.refresh_slash_menu(block_cx);
                block.on_newline(&Newline, window, block_cx);
            });
        });
    });

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("separator").clone();
        assert_eq!(block.read(cx).kind(), BlockKind::Separator);
        assert_eq!(editor.source_document.text(), "---\n\n");
        assert_eq!(editor.undo_history.len(), 1);
    });
}

#[gpui::test]
async fn request_quote_break_creates_new_root_leaf_quote_group(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "> first".to_string(), None));

    editor.update(cx, |editor, cx| {
        let quote = editor.document.first_root().expect("root quote").clone();
        editor.on_block_event(quote, &BlockEvent::RequestQuoteBreak, cx);

        let visible = editor.document.visible_blocks();
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].entity.read(cx).kind(), BlockKind::Quote);
        assert_eq!(visible[0].entity.read(cx).display_text(), "first");
        assert_eq!(visible[1].entity.read(cx).kind(), BlockKind::Quote);
        assert_eq!(visible[1].entity.read(cx).display_text(), "");
        assert_eq!(visible[1].entity.read(cx).quote_depth, 1);
        assert_eq!(editor.document.markdown_text(cx), "> first\n\n> ");
        assert_eq!(editor.pending_focus, Some(visible[1].entity.entity_id()));
    });
}

#[gpui::test]
async fn typing_quote_shortcut_immediately_refreshes_rendered_quote_metadata(
    cx: &mut TestAppContext,
) {
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
}

#[gpui::test]
async fn footnote_reference_jump_and_backref_follow_in_place_definition(cx: &mut TestAppContext) {
    let markdown = "alpha[^note]\n\n[^note]: Footnote body".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let paragraph = editor
            .document
            .first_root()
            .expect("reference paragraph")
            .clone();
        let definition = editor
            .document
            .visible_blocks()
            .iter()
            .find(|visible| visible.entity.read(cx).kind() == BlockKind::FootnoteDefinition)
            .expect("footnote definition block")
            .entity
            .clone();

        editor.on_block_event(
            paragraph.clone(),
            &BlockEvent::RequestJumpToFootnoteDefinition {
                id: "note".to_string(),
            },
            cx,
        );
        assert_eq!(editor.pending_focus, Some(definition.entity_id()));
        assert_eq!(definition.read(cx).selected_range, 0..0);

        let expected_backref_range = paragraph
            .read(cx)
            .current_range_for_footnote_occurrence(0)
            .expect("resolved footnote occurrence");
        editor.on_block_event(
            definition.clone(),
            &BlockEvent::RequestJumpToFootnoteBackref {
                id: "note".to_string(),
            },
            cx,
        );
        assert_eq!(editor.pending_focus, Some(paragraph.entity_id()));
        assert_eq!(paragraph.read(cx).selected_range, expected_backref_range);
    });
}

#[gpui::test]
async fn image_block_insert_preserves_surrounding_paragraph_text(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "beforeafter".to_string(), None));

    editor.update(cx, |editor, cx| {
        let paragraph = editor.document.first_root().expect("paragraph").clone();
        editor.insert_image_block_after_paragraph(
            &paragraph,
            &InlineTextTree::plain("before"),
            "![image](./assets/image.png)",
            &InlineTextTree::plain("after"),
            cx,
        );

        let visible = editor.document.visible_blocks();
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[0].entity.read(cx).display_text(), "before");
        assert_eq!(
            visible[1].entity.read(cx).display_text(),
            "![image](./assets/image.png)"
        );
        assert!(visible[1].entity.read(cx).image_runtime().is_some());
        assert_eq!(visible[2].entity.read(cx).display_text(), "after");
    });
}
