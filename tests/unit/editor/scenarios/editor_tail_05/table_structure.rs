// @author kongweiguang

#[gpui::test]
async fn parsed_table_runtime_installs_column_alignment_on_cells(cx: &mut TestAppContext) {
    let markdown = [
        "| Left | Center | Right |",
        "| :--- | :---: | ---: |",
        "| a | b | c |",
    ]
    .join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.read_with(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        assert_eq!(table.read(cx).kind(), BlockKind::Table);
        let runtime = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("table runtime");
        assert_eq!(
            runtime.header[0].read(cx).table_cell_alignment(),
            Some(TableColumnAlignment::Left)
        );
        assert_eq!(
            runtime.header[1].read(cx).table_cell_alignment(),
            Some(TableColumnAlignment::Center)
        );
        assert_eq!(
            runtime.rows[0][2].read(cx).table_cell_alignment(),
            Some(TableColumnAlignment::Right)
        );
    });
}

#[gpui::test]
async fn append_column_updates_table_and_focuses_new_header_cell(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | ---: |", "| 1 | 2 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        editor.append_table_column(&table, cx);

        let record = table
            .read(cx)
            .record
            .table
            .as_ref()
            .expect("table record after append");
        assert_eq!(record.header.len(), 3);
        assert_eq!(record.rows[0].len(), 3);
        assert_eq!(
            record.alignments,
            vec![
                TableColumnAlignment::Default,
                TableColumnAlignment::Right,
                TableColumnAlignment::Right,
            ]
        );

        let runtime = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("rebuilt runtime");
        let focused = runtime.header[2].entity_id();
        assert_eq!(editor.pending_focus, Some(focused));
    });
}

#[gpui::test]
async fn append_row_updates_table_and_focuses_first_cell_of_new_row(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | :---: |", "| 1 | 2 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        editor.append_table_row(&table, cx);

        let record = table
            .read(cx)
            .record
            .table
            .as_ref()
            .expect("table record after append");
        assert_eq!(record.rows.len(), 2);
        assert_eq!(record.rows[1].len(), 2);
        assert!(
            record.rows[1]
                .iter()
                .all(|cell| cell.serialize_markdown().is_empty())
        );

        let runtime = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("rebuilt runtime");
        let focused = runtime.rows[1][0].entity_id();
        assert_eq!(editor.pending_focus, Some(focused));
    });
}

#[gpui::test]
async fn setting_column_alignment_updates_record_and_selection(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        editor.set_table_column_alignment(&table, 1, TableColumnAlignment::Right, cx);

        let record = table.read(cx).record.table.as_ref().expect("table record");
        assert_eq!(
            record.alignments,
            vec![TableColumnAlignment::Default, TableColumnAlignment::Right]
        );
        assert_eq!(
            editor.table_axis_selection,
            Some(super::TableAxisSelection {
                table_block_id: table.entity_id(),
                kind: crate::components::TableAxisKind::Column,
                index: 1,
            })
        );
    });
}

#[gpui::test]
async fn moving_table_row_updates_focus_and_selection(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |", "| 3 | 4 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        // Visual row 2 is the second body row; move it up above the first.
        editor.move_table_row(&table, 2, -1, cx);

        let record = table.read(cx).record.table.as_ref().expect("table record");
        assert_eq!(record.rows[0][0].serialize_markdown(), "3");
        assert_eq!(
            editor.table_axis_selection,
            Some(super::TableAxisSelection {
                table_block_id: table.entity_id(),
                kind: crate::components::TableAxisKind::Row,
                index: 1,
            })
        );

        let runtime = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("rebuilt runtime");
        assert_eq!(editor.pending_focus, Some(runtime.rows[0][0].entity_id()));
    });
}

#[gpui::test]
async fn moving_first_body_row_up_swaps_with_header(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |", "| 3 | 4 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        // Visual row 1 (first body row) moves up into the header position.
        editor.move_table_row(&table, 1, -1, cx);

        let record = table.read(cx).record.table.as_ref().expect("table record");
        assert_eq!(record.header[0].serialize_markdown(), "1");
        assert_eq!(record.rows[0][0].serialize_markdown(), "A");
        assert_eq!(
            editor.table_axis_selection,
            Some(super::TableAxisSelection {
                table_block_id: table.entity_id(),
                kind: crate::components::TableAxisKind::Row,
                index: 0,
            })
        );
    });
}

#[gpui::test]
async fn moving_header_row_down_swaps_with_first_body(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |", "| 3 | 4 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        // Visual row 0 (header) moves down, swapping with the first body row.
        editor.move_table_row(&table, 0, 1, cx);

        let record = table.read(cx).record.table.as_ref().expect("table record");
        assert_eq!(record.header[0].serialize_markdown(), "1");
        assert_eq!(record.rows[0][0].serialize_markdown(), "A");
        assert_eq!(
            editor.table_axis_selection,
            Some(super::TableAxisSelection {
                table_block_id: table.entity_id(),
                kind: crate::components::TableAxisKind::Row,
                index: 1,
            })
        );
    });
}
