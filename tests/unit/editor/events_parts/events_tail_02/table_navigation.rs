// @author kongweiguang

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
