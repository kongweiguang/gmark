// @author kongweiguang

#[gpui::test]
async fn dismissing_menu_panel_from_body_preserves_navigation(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.menu_bar_expanded = true;
        editor.open_menu_bar(0, cx);
        editor.set_menu_bar_hovered(true, cx);
        editor.set_menu_panel_hovered(true, cx);
        assert_eq!(editor.menu_bar_open, Some(0));

        editor.dismiss_menu_bar_from_body(cx);
        assert!(editor.menu_bar_expanded);
        assert_eq!(editor.menu_bar_open, None);
        assert!(!editor.menu_bar_hovered);
        assert!(!editor.menu_panel_hovered);
        assert!(!editor.menu_submenu_panel_hovered);
        assert!(editor.menu_close_task.is_none());
    });
}

#[gpui::test]
async fn menu_launcher_toggles_its_panel_without_hiding_navigation(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        assert!(editor.menu_bar_expanded);
        assert_eq!(editor.menu_bar_open, None);

        editor.toggle_menu_bar_expanded(cx);
        assert!(editor.menu_bar_expanded);
        assert_eq!(editor.menu_bar_open, Some(0));

        editor.toggle_menu_bar_expanded(cx);
        assert!(editor.menu_bar_expanded);
        assert_eq!(editor.menu_bar_open, None);

        editor.toggle_menu_bar_expanded(cx);
        assert!(editor.menu_bar_expanded);
        assert_eq!(editor.menu_bar_open, Some(0));
    });
}

#[gpui::test]
async fn closing_menu_panels_preserves_expanded_navigation(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.open_menu_bar(0, cx);
        editor.set_menu_bar_hovered(true, cx);
        editor.set_menu_panel_hovered(true, cx);

        editor.close_menu_panels(cx);

        assert!(editor.menu_bar_expanded);
        assert_eq!(editor.menu_bar_open, None);
        assert!(!editor.menu_bar_hovered);
        assert!(!editor.menu_panel_hovered);
        assert!(editor.menu_close_task.is_none());
    });
}

#[gpui::test]
async fn in_window_menu_keyboard_navigation_preserves_editor_focus(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual_cx) = cx.add_window_view(|window, cx| {
        let editor = Editor::from_markdown(cx, "alpha".to_string(), None);
        editor
            .document
            .first_root()
            .expect("paragraph")
            .read(cx)
            .focus_handle
            .focus(window);
        editor
    });
    visual_cx.simulate_resize(size(px(720.0), px(520.0)));

    let key_event = |key: &str| KeyDownEvent {
        keystroke: Keystroke::parse(key).expect("valid menu key"),
        is_held: false,
    };
    let menus = vec![
        OwnedMenu {
            name: "File".into(),
            items: vec![
                OwnedMenuItem::Separator,
                OwnedMenuItem::Action {
                    name: "Unavailable".to_owned(),
                    action: Box::new(NoRecentFiles),
                    os_action: None,
                },
                OwnedMenuItem::Action {
                    name: "Save".to_owned(),
                    action: Box::new(SaveDocument),
                    os_action: None,
                },
                OwnedMenuItem::Submenu(OwnedMenu {
                    name: "Recent".into(),
                    items: vec![
                        OwnedMenuItem::Separator,
                        OwnedMenuItem::Action {
                            name: "Save child".to_owned(),
                            action: Box::new(SaveDocument),
                            os_action: None,
                        },
                    ],
                }),
            ],
        },
        OwnedMenu {
            name: "Edit".into(),
            items: vec![OwnedMenuItem::Action {
                name: "Save again".to_owned(),
                action: Box::new(SaveDocument),
                os_action: None,
            }],
        },
    ];
    editor.update_in(visual_cx, |editor, window, cx| {
        let f10 = key_event("f10");
        assert!(
            editor.handle_in_window_menu_key_with_menus(&f10, &menus, window, cx),
            "F10 event: {f10:?}"
        );
        let first = Editor::edge_menu_item(&menus[0].items, true).expect("first command");
        assert_eq!(first, 2, "separator and disabled placeholder are skipped");
        assert_eq!(editor.menu_bar_open, Some(0));
        assert_eq!(editor.menu_keyboard_item, Some(first));

        assert!(editor.handle_in_window_menu_key_with_menus(
            &key_event("escape"),
            &menus,
            window,
            cx
        ));
        assert_eq!(editor.menu_bar_open, None);

        let alt = key_event("alt");
        assert!(
            editor.handle_in_window_menu_key_with_menus(&alt, &menus, window, cx),
            "Alt event: {alt:?}"
        );
        assert_eq!(editor.menu_bar_open, Some(0));
        assert_eq!(editor.menu_keyboard_item, Some(first));

        let next =
            Editor::adjacent_menu_item(&menus[0].items, Some(first), true).expect("next command");
        assert!(editor.handle_in_window_menu_key_with_menus(
            &key_event("down"),
            &menus,
            window,
            cx
        ));
        assert_eq!(editor.menu_keyboard_item, Some(next));

        let submenu_index = menus[0]
            .items
            .iter()
            .position(|item| matches!(item, gpui::OwnedMenuItem::Submenu(_)))
            .expect("file menu submenu");
        editor.menu_keyboard_item = Some(submenu_index);
        assert!(editor.handle_in_window_menu_key_with_menus(
            &key_event("right"),
            &menus,
            window,
            cx
        ));
        assert_eq!(editor.menu_submenu_open, Some(submenu_index));
        assert!(editor.menu_keyboard_submenu_item.is_some());
        assert!(editor.handle_in_window_menu_key_with_menus(
            &key_event("left"),
            &menus,
            window,
            cx
        ));
        assert_eq!(editor.menu_submenu_open, None);

        assert!(editor.handle_in_window_menu_key_with_menus(
            &key_event("escape"),
            &menus,
            window,
            cx
        ));
        assert_eq!(editor.menu_bar_open, None);
        assert_eq!(editor.menu_keyboard_item, None);
        assert!(
            editor
                .document
                .first_root()
                .expect("paragraph")
                .read(cx)
                .focus_handle
                .is_focused(window)
        );
    });
}

#[gpui::test]
async fn moving_pointer_away_keeps_in_window_menu_open(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.open_menu_bar(0, cx);
        editor.open_menu_submenu(2, cx);
        editor.set_menu_submenu_panel_hovered(true, cx);
        editor.set_menu_panel_hovered(false, cx);
        editor.set_menu_bar_hovered(false, cx);

        assert_eq!(editor.menu_bar_open, Some(0));
        assert_eq!(editor.menu_submenu_open, Some(2));
        assert!(editor.menu_submenu_panel_hovered);
        assert!(editor.menu_close_task.is_none());

        editor.set_menu_submenu_panel_hovered(false, cx);
        assert!(editor.menu_close_task.is_none());
        assert_eq!(editor.menu_bar_open, Some(0));

        editor.close_menu_bar(cx);
    });
}

// The gap bridge and the submenu panel overlap, so moving the cursor from the
// bridge onto the submenu emits `bridge: false` and `panel: true` in the same
// gesture. With both regions sharing one hover flag the stale `bridge: false`
// could win and tear the menu down, which made reaching the recent-files list
// fail intermittently. Track the two regions independently so the handoff
// always keeps the menu open, regardless of event order.
#[gpui::test]
async fn submenu_survives_bridge_to_panel_hover_handoff(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.open_menu_bar(0, cx);
        editor.open_menu_submenu(3, cx);

        // Crossing the gap: only the bridge is hovered.
        editor.set_menu_panel_hovered(false, cx);
        editor.set_menu_bar_hovered(false, cx);
        editor.set_menu_submenu_bridge_hovered(true, cx);
        assert!(editor.menu_close_task.is_none());

        // Handoff into the submenu panel. The bridge reporting `false` after
        // the panel is already hovered must not schedule a close.
        editor.set_menu_submenu_panel_hovered(true, cx);
        editor.set_menu_submenu_bridge_hovered(false, cx);

        assert_eq!(editor.menu_bar_open, Some(0));
        assert_eq!(editor.menu_submenu_open, Some(3));
        assert!(editor.menu_submenu_panel_hovered);
        assert!(
            editor.menu_close_task.is_none(),
            "menu must stay open across the bridge-to-panel handoff"
        );

        editor.close_menu_bar(cx);
    });
}

#[gpui::test]
async fn starting_and_ending_scrollbar_drag_updates_editor_state(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        editor.pending_scroll_active_block_into_view = true;
        editor.pending_scroll_recheck_after_layout = true;

        editor.start_scrollbar_drag(12.0, 320.0, 64.0, 500.0, cx);
        assert_eq!(
            editor.scrollbar_drag,
            Some(super::ScrollbarDragSession {
                pointer_offset_y: 12.0,
                track_height: 320.0,
                thumb_height: 64.0,
                max_scroll_y: 500.0,
            })
        );
        assert!(!editor.pending_scroll_active_block_into_view);
        assert!(!editor.pending_scroll_recheck_after_layout);

        editor.update_scrollbar_drag(172.0, cx);
        let offset_y = -f32::from(editor.scroll_handle.offset().y);
        assert!(offset_y > 0.0);

        editor.end_scrollbar_drag(cx);
        assert!(editor.scrollbar_drag.is_none());
    });
}

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

#[gpui::test]
async fn selecting_first_body_row_does_not_highlight_header(cx: &mut TestAppContext) {
    use crate::components::{TableAxisHighlight, TableAxisKind};
    let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |", "| 3 | 4 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        // Visual row 1 is the first body row; the header (row 0) must stay clear.
        editor.select_table_axis(table.entity_id(), TableAxisKind::Row, 1, cx);

        let runtime = table.read(cx).table_runtime.clone().expect("runtime");
        for cell in &runtime.header {
            assert_eq!(
                cell.read(cx).table_axis_highlight,
                TableAxisHighlight::None,
                "header should not be highlighted"
            );
        }
        for cell in &runtime.rows[0] {
            assert_eq!(
                cell.read(cx).table_axis_highlight,
                TableAxisHighlight::Selected
            );
        }
        for cell in &runtime.rows[1] {
            assert_eq!(cell.read(cx).table_axis_highlight, TableAxisHighlight::None);
        }
    });
}

#[gpui::test]
async fn selecting_header_row_highlights_only_header(cx: &mut TestAppContext) {
    use crate::components::{TableAxisHighlight, TableAxisKind};
    let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        editor.select_table_axis(table.entity_id(), TableAxisKind::Row, 0, cx);

        let runtime = table.read(cx).table_runtime.clone().expect("runtime");
        for cell in &runtime.header {
            assert_eq!(
                cell.read(cx).table_axis_highlight,
                TableAxisHighlight::Selected
            );
        }
        for cell in &runtime.rows[0] {
            assert_eq!(cell.read(cx).table_axis_highlight, TableAxisHighlight::None);
        }
    });
}

#[gpui::test]
async fn body_row_preview_survives_stale_header_leave(cx: &mut TestAppContext) {
    use crate::components::TableAxisKind;
    let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        let id = table.entity_id();

        // Pointer crosses from the header handle down onto the first body row.
        // The body handle's enter arrives first, then the header handle's leave;
        // the stale leave must not clear the preview the pointer moved onto.
        editor.preview_table_axis(id, TableAxisKind::Row, 1, true, cx);
        editor.preview_table_axis(id, TableAxisKind::Row, 0, false, cx);
        assert_eq!(
            editor.table_axis_preview,
            Some(super::TableAxisSelection {
                table_block_id: id,
                kind: TableAxisKind::Row,
                index: 1,
            }),
            "body row preview must survive the header's stale leave"
        );

        // Leaving the body handle that owns the preview still clears it.
        editor.preview_table_axis(id, TableAxisKind::Row, 1, false, cx);
        assert_eq!(editor.table_axis_preview, None);
    });
}

#[gpui::test]
async fn deleting_table_column_moves_selection_to_nearest_survivor(cx: &mut TestAppContext) {
    let markdown = ["| A | B | C |", "| --- | --- | --- |", "| 1 | 2 | 3 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        editor.delete_table_column(&table, 2, cx);

        let record = table.read(cx).record.table.as_ref().expect("table record");
        assert_eq!(record.header.len(), 2);
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
async fn deleting_table_header_promotes_next_row(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        editor.delete_table_header_row(&table, cx);

        let record = table.read(cx).record.table.as_ref().expect("table record");
        assert_eq!(record.header[0].serialize_markdown(), "1");
        assert_eq!(record.header[1].serialize_markdown(), "2");
        assert!(record.rows.is_empty());

        let runtime = table
            .read(cx)
            .table_runtime
            .as_ref()
            .expect("rebuilt runtime");
        assert_eq!(editor.pending_focus, Some(runtime.header[0].entity_id()));
    });
}

#[gpui::test]
async fn deleting_last_body_row_leaves_header_only_table(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        // Deleting the only body row used to be blocked; now it leaves a
        // header-only table behind.
        editor.delete_table_row(&table, 0, cx);

        let record = table.read(cx).record.table.as_ref().expect("table record");
        assert!(record.rows.is_empty());
        assert_eq!(record.header[0].serialize_markdown(), "A");
        assert_eq!(editor.document.root_count(), 1);
        assert_eq!(table.read(cx).kind(), BlockKind::Table);
    });
}

#[gpui::test]
async fn removing_table_block_replaces_it_with_empty_paragraph(cx: &mut TestAppContext) {
    let markdown = [
        "intro",
        "",
        "| A | B |",
        "| --- | --- |",
        "| 1 | 2 |",
        "",
        "outro",
    ]
    .join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.root_blocks()[1].clone();
        assert_eq!(table.read(cx).kind(), BlockKind::Table);
        editor.remove_table_block(&table, cx);

        let roots = editor.document.root_blocks();
        assert_eq!(roots.len(), 3);
        assert_eq!(roots[0].read(cx).display_text(), "intro");
        assert_eq!(roots[1].read(cx).kind(), BlockKind::Paragraph);
        assert_eq!(roots[1].read(cx).display_text(), "");
        assert_eq!(roots[2].read(cx).display_text(), "outro");
        assert_eq!(editor.pending_focus, Some(roots[1].entity_id()));
    });
}

#[gpui::test]
async fn removing_the_only_table_leaves_one_empty_paragraph(cx: &mut TestAppContext) {
    let markdown = ["| A | B |", "| --- | --- |", "| 1 | 2 |"].join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        editor.remove_table_block(&table, cx);

        let roots = editor.document.root_blocks();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].read(cx).kind(), BlockKind::Paragraph);
        assert_eq!(roots[0].read(cx).display_text(), "");
    });
}

#[gpui::test]
async fn standalone_root_image_installs_runtime_and_resolves_relative_path(
    cx: &mut TestAppContext,
) {
    let markdown = "![diagram](./assets/diagram.png \"System diagram\")".to_string();
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root block").clone();
        let runtime = block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "diagram");
        assert_eq!(runtime.title.as_deref(), Some("System diagram"));
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("assets/diagram.png")
            )
        );
    });
}

#[gpui::test]
async fn standalone_root_image_with_underscores_installs_runtime(cx: &mut TestAppContext) {
    let markdown =
        "![1.1_进制转换例子](./NetworkEngineerSummer.assets/1.1_进制转换例子.jpg)".to_string();
    let file_path = PathBuf::from("D:/workspace/docs/note.md");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown.clone(), Some(file_path.clone())));

    editor.read_with(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root block").clone();
        let runtime = block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.alt, "1.1_进制转换例子");
        assert_eq!(
            runtime.resolved_source,
            ImageResolvedSource::Local(
                file_path
                    .parent()
                    .expect("file parent")
                    .join("NetworkEngineerSummer.assets/1.1_进制转换例子.jpg")
            )
        );
        assert_eq!(editor.document.markdown_text(cx), markdown);
    });
}

