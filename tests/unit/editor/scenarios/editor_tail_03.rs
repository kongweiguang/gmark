// @author kongweiguang

#[gpui::test]
async fn callout_footnotes_number_and_render_in_place(cx: &mut TestAppContext) {
    let markdown = [
        "> [!WARNING]",
        "> Callout footnote reference.[^final]",
        "> ",
        "> [^final]: Nested footnote text.",
        "> Tail paragraph.",
    ]
    .join("\n");
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown.clone(), None));

    editor.read_with(cx, |editor, cx| {
        let visible = editor.document.visible_blocks();

        let reference_block = visible
            .iter()
            .find(|visible| {
                visible
                    .entity
                    .read(cx)
                    .display_text()
                    .contains("Callout footnote reference.")
            })
            .expect("callout footnote reference")
            .entity
            .clone();
        assert_eq!(
            reference_block.read(cx).display_text(),
            format!("Callout footnote reference.{}", superscript_ordinal(1))
        );

        let definition = visible
            .iter()
            .find(|visible| visible.entity.read(cx).kind() == BlockKind::FootnoteDefinition)
            .expect("callout footnote definition")
            .entity
            .clone();
        assert_eq!(definition.read(cx).display_text(), "final");
        assert_eq!(definition.read(cx).quote_depth, 1);
        assert_eq!(definition.read(cx).footnote_definition_ordinal(), Some(1));
        assert_eq!(editor.document.markdown_text(cx), markdown);
    });
}

#[gpui::test]
async fn callout_variants_use_stable_semantic_svg_icon_slots(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let source = [
        "> [!NOTE] Note title",
        "> body",
        "",
        "> [!TIP] Tip title",
        "> body",
        "",
        "> [!IMPORTANT] Important title",
        "> body",
        "",
        "> [!WARNING] Warning title",
        "> body",
        "",
        "> [!CAUTION] Caution title",
        "> body",
    ]
    .join("\n");
    let expected_source = source.clone();
    let (editor, visual_cx) =
        cx.add_window_view(move |_window, cx| Editor::from_markdown(cx, source, None));
    let (revision, dirty) = editor.read_with(visual_cx, |editor, _cx| {
        (editor.source_document.revision(), editor.document_dirty)
    });

    for viewport in [size(px(720.0), px(520.0)), size(px(1180.0), px(780.0))] {
        visual_cx.simulate_resize(viewport);
        redraw(visual_cx);
        visual_cx.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
        let content = visual_cx.debug_bounds("editor-content").unwrap();
        let mut previous_top = None;
        for selector in [
            "callout-icon-note",
            "callout-icon-tip",
            "callout-icon-important",
            "callout-icon-warning",
            "callout-icon-caution",
        ] {
            let icon = visual_cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("missing {selector}"));
            assert_eq!(icon.size, size(px(18.0), px(18.0)));
            assert!(icon.left() >= content.left());
            assert!(icon.right() <= content.right());
            if let Some(previous_top) = previous_top {
                assert!(icon.top() > previous_top);
            } else {
                assert!(icon.top() >= content.top());
                assert!(icon.bottom() <= content.bottom());
            }
            previous_top = Some(icon.top());
        }
    }

    editor.read_with(visual_cx, |editor, _cx| {
        assert_eq!(editor.source_document.text(), expected_source);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });
}

#[gpui::test]
async fn root_reference_binds_to_nested_quote_footnote_definition(cx: &mut TestAppContext) {
    let markdown = "Root reference.[^note]\n\n> [^note]: Nested quote footnote".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown.clone(), None));

    editor.read_with(cx, |editor, cx| {
        let visible = editor.document.visible_blocks();

        let root_reference = visible
            .iter()
            .find(|visible| visible.entity.read(cx).quote_depth == 0)
            .expect("root reference block")
            .entity
            .clone();
        assert_eq!(
            root_reference.read(cx).display_text(),
            format!("Root reference.{}", superscript_ordinal(1))
        );

        let definition = visible
            .iter()
            .find(|visible| visible.entity.read(cx).kind() == BlockKind::FootnoteDefinition)
            .expect("nested quote footnote definition")
            .entity
            .clone();
        assert_eq!(definition.read(cx).display_text(), "note");
        assert_eq!(definition.read(cx).quote_depth, 1);
        assert_eq!(definition.read(cx).footnote_definition_ordinal(), Some(1));
        assert_eq!(editor.document.markdown_text(cx), markdown);
    });
}

#[gpui::test]
async fn unresolved_footnote_reference_stays_literal_and_unlinked(cx: &mut TestAppContext) {
    let markdown = "Missing footnote[^missing].".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown.clone(), None));

    editor.read_with(cx, |editor, cx| {
        let block = editor
            .document
            .first_root()
            .expect("root paragraph")
            .clone();
        assert_eq!(block.read(cx).display_text(), markdown);
        assert!(
            block
                .read(cx)
                .inline_footnote_hit_at("Missing footnote".len())
                .is_none()
        );
        assert!(editor.footnote_registry.binding("missing").is_none());
        assert_eq!(editor.document.markdown_text(cx), markdown);
    });
}

#[gpui::test]
async fn toggling_source_mode_preserves_root_image_runtime(cx: &mut TestAppContext) {
    let markdown = "![diagram](./assets/diagram.png)".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
    });

    editor.read_with(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root block").clone();
        assert!(block.read(cx).image_runtime().is_some());
    });
}

#[gpui::test]
async fn standalone_image_resize_is_transient_until_one_undoable_commit(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let source = "![diagram](https://example.com/diagram.png \"Caption\")";
    let (editor, visual_cx) =
        cx.add_window_view(move |_window, cx| Editor::from_markdown(cx, source.to_owned(), None));

    editor.update_in(visual_cx, |editor, window, cx| {
        let block = editor.document.first_root().expect("image block").clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.focus_handle.focus(window);
            block.select_rendered_image(block_cx);
        });
    });
    redraw(visual_cx);
    let content = visual_cx.debug_bounds("editor-content").unwrap();
    let frame = visual_cx.debug_bounds("rendered-image-frame").unwrap();
    let handle = visual_cx.debug_bounds("image-resize-handle").unwrap();
    assert!(frame.left() >= content.left());
    assert!(frame.right() <= content.right());
    assert!(handle.left() >= frame.left());
    assert!(handle.right() <= frame.right());
    assert!(handle.top() >= frame.top());
    assert!(handle.bottom() <= frame.bottom());

    visual_cx.simulate_mouse_move(handle.center(), None, Modifiers::default());
    visual_cx
        .executor()
        .advance_clock(Duration::from_millis(520));
    visual_cx.run_until_parked();
    redraw(visual_cx);
    let tooltip = visual_cx.debug_bounds("ui-tooltip").unwrap();
    let content = visual_cx.debug_bounds("editor-content").unwrap();
    assert!(tooltip.left() >= content.left());
    assert!(tooltip.right() <= content.right());
    assert!(tooltip.top() >= content.top());
    assert!(tooltip.bottom() <= content.bottom());

    for viewport in [size(px(720.0), px(520.0)), size(px(1180.0), px(780.0))] {
        visual_cx.simulate_resize(viewport);
        redraw(visual_cx);
        visual_cx.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
        let content = visual_cx.debug_bounds("editor-content").unwrap();
        let frame = visual_cx.debug_bounds("rendered-image-frame").unwrap();
        let handle = visual_cx.debug_bounds("image-resize-handle").unwrap();
        assert!(
            frame.left() >= content.left(),
            "viewport={viewport:?} frame={frame:?} content={content:?}"
        );
        assert!(
            frame.right() <= content.right(),
            "viewport={viewport:?} frame={frame:?} content={content:?}"
        );
        assert!(handle.left() >= frame.left());
        assert!(handle.right() <= frame.right());
    }

    let escape = KeyDownEvent {
        keystroke: Keystroke::parse("escape").expect("valid escape keystroke"),
        is_held: false,
    };
    editor.update_in(visual_cx, |editor, window, cx| {
        let block = editor.document.first_root().expect("image block").clone();
        block.update(cx, |block, block_cx| {
            block.on_block_key_down(&escape, window, block_cx);
            assert!(!block.image_selected);
            block.select_rendered_image(block_cx);
        });
    });
    redraw(visual_cx);
    let handle = visual_cx.debug_bounds("image-resize-handle").unwrap();
    let full_width = f32::from(
        visual_cx
            .debug_bounds("rendered-image-frame")
            .unwrap()
            .size
            .width,
    );
    visual_cx.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::default());
    let (start_x, available_width) = editor.read_with(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("image block").read(cx);
        let session = block.image_resize_session.expect("resize session");
        (session.start_x, session.available_width)
    });
    let drag_position = point(start_x - px(available_width * 0.2), handle.center().y);
    visual_cx.simulate_mouse_move(drag_position, MouseButton::Left, Modifiers::default());
    redraw(visual_cx);
    let preview_width = f32::from(
        visual_cx
            .debug_bounds("rendered-image-frame")
            .unwrap()
            .size
            .width,
    );
    assert!((preview_width - full_width * 0.8).abs() <= 1.0);
    editor.read_with(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("image block").read(cx);
        assert_eq!(block.current_image_width_percent(), 80);
        assert_eq!(editor.source_document.text(), source);
        assert!(!editor.document_dirty);
    });
    visual_cx.simulate_mouse_up(drag_position, MouseButton::Left, Modifiers::default());
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, cx| {
        assert_eq!(
            editor.source_document.text(),
            "![diagram](https://example.com/diagram.png \"Caption\"){width=80%}"
        );
        assert!(editor.document_dirty);
        assert_eq!(
            editor
                .document
                .first_root()
                .unwrap()
                .read(cx)
                .image_runtime()
                .unwrap()
                .width_percent,
            80
        );
        editor.undo_document(cx);
        assert_eq!(editor.source_document.text(), source);
        editor.redo_document(cx);
        assert_eq!(
            editor.source_document.text(),
            "![diagram](https://example.com/diagram.png \"Caption\"){width=80%}"
        );
    });
}

#[gpui::test]
async fn resizing_image_to_full_width_removes_attribute_and_is_undoable(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let scaled = "![diagram](https://example.com/diagram.png){width=80%}";
    let editor = cx.new(|cx| Editor::from_markdown(cx, scaled.to_owned(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("image block").clone();
        block.update(cx, |block, block_cx| {
            block.select_rendered_image(block_cx);
            block.start_image_resize(px(400.0), 500.0, block_cx);
            block.update_image_resize(px(500.0), block_cx);
            assert_eq!(block.current_image_width_percent(), 100);
            assert!(block.finish_image_resize(block_cx));
        });
    });
    cx.run_until_parked();
    editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.source_document.text(),
            "![diagram](https://example.com/diagram.png)"
        );
        editor.undo_document(cx);
        assert_eq!(editor.source_document.text(), scaled);
    });
}

#[gpui::test]
async fn preview_image_cannot_enter_resize_selection(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual_cx) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(
            cx,
            "![diagram](https://example.com/diagram.png){width=80%}".to_owned(),
            None,
        )
    });
    editor.update(visual_cx, |editor, cx| {
        editor.set_view_mode(ViewMode::Preview, cx);
        let block = editor.document.first_root().expect("image block").clone();
        block.update(cx, |block, block_cx| {
            assert!(block.is_read_only());
            block.select_rendered_image(block_cx);
            assert!(!block.image_selected);
        });
    });
    redraw(visual_cx);
    assert!(visual_cx.debug_bounds("rendered-image-frame").is_some());
    assert!(visual_cx.debug_bounds("image-resize-handle").is_none());
}

#[gpui::test]
async fn table_append_controls_have_bounded_tooltips_without_mutating_source(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let source = ["| A | B |", "| --- | ---: |", "| 1 | 2 |"].join("\n");
    let expected_source = source.clone();
    let (editor, visual_cx) =
        cx.add_window_view(move |_window, cx| Editor::from_markdown(cx, source, None));
    let (revision, dirty) = editor.read_with(visual_cx, |editor, _cx| {
        (editor.source_document.revision(), editor.document_dirty)
    });
    editor.update(visual_cx, |editor, cx| {
        let table = editor.document.first_root().expect("table root").clone();
        table.update(cx, |table, table_cx| {
            table.table_append_column_hovered = true;
            table.table_append_row_hovered = true;
            table_cx.notify();
        });
    });

    for viewport in [size(px(720.0), px(520.0)), size(px(1180.0), px(780.0))] {
        visual_cx.simulate_resize(viewport);
        redraw(visual_cx);
        visual_cx.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
        let content = visual_cx.debug_bounds("editor-content").unwrap();
        let column = visual_cx
            .debug_bounds("table-append-column-button")
            .unwrap();
        let row = visual_cx.debug_bounds("table-append-row-button").unwrap();
        for control in [column, row] {
            assert!(control.left() >= content.left());
            assert!(control.right() <= content.right());
            assert!(control.top() >= content.top());
            assert!(control.bottom() <= content.bottom());
        }
    }

    for selector in ["table-append-column-button", "table-append-row-button"] {
        let control = visual_cx.debug_bounds(selector).unwrap();
        visual_cx.simulate_mouse_move(control.center(), None, Modifiers::default());
        visual_cx
            .executor()
            .advance_clock(Duration::from_millis(520));
        visual_cx.run_until_parked();
        redraw(visual_cx);
        let tooltip = visual_cx.debug_bounds("ui-tooltip").unwrap();
        let content = visual_cx.debug_bounds("editor-content").unwrap();
        assert!(tooltip.left() >= content.left());
        assert!(tooltip.right() <= content.right());
        assert!(tooltip.top() >= content.top());
        assert!(tooltip.bottom() <= content.bottom());
    }

    editor.read_with(visual_cx, |editor, _cx| {
        assert_eq!(editor.source_document.text(), expected_source);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });
}

#[gpui::test]
async fn footnote_backref_tooltip_is_bounded_and_hover_is_non_mutating(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let source = "Reference[^note]\n\n[^note]: definition".to_owned();
    let expected_source = source.clone();
    let (editor, visual_cx) =
        cx.add_window_view(move |_window, cx| Editor::from_markdown(cx, source, None));
    let (revision, dirty) = editor.read_with(visual_cx, |editor, _cx| {
        (editor.source_document.revision(), editor.document_dirty)
    });

    for viewport in [size(px(720.0), px(520.0)), size(px(1180.0), px(780.0))] {
        visual_cx.simulate_resize(viewport);
        redraw(visual_cx);
        visual_cx.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
        let backref = visual_cx.debug_bounds("footnote-backref").unwrap();
        let icon = visual_cx.debug_bounds("footnote-backref-icon").unwrap();
        let content = visual_cx.debug_bounds("editor-content").unwrap();
        assert_eq!(backref.size, size(px(20.0), px(20.0)));
        assert_eq!(icon.size, size(px(14.0), px(14.0)));
        assert!(backref.left() >= content.left());
        assert!(backref.right() <= content.right());
        assert!(backref.top() >= content.top());
        assert!(backref.bottom() <= content.bottom());
        assert!(icon.left() >= backref.left());
        assert!(icon.right() <= backref.right());
        assert!(icon.top() >= backref.top());
        assert!(icon.bottom() <= backref.bottom());
    }

    let backref = visual_cx.debug_bounds("footnote-backref").unwrap();
    visual_cx.simulate_mouse_move(backref.center(), None, Modifiers::default());
    visual_cx
        .executor()
        .advance_clock(Duration::from_millis(520));
    visual_cx.run_until_parked();
    redraw(visual_cx);
    let tooltip = visual_cx.debug_bounds("ui-tooltip").unwrap();
    let content = visual_cx.debug_bounds("editor-content").unwrap();
    assert!(tooltip.left() >= content.left());
    assert!(tooltip.right() <= content.right());
    assert!(tooltip.top() >= content.top());
    assert!(tooltip.bottom() <= content.bottom());

    editor.read_with(visual_cx, |editor, _cx| {
        assert_eq!(editor.source_document.text(), expected_source);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });
}

#[gpui::test]
async fn toggling_source_mode_preserves_reference_style_root_image_runtime(
    cx: &mut TestAppContext,
) {
    let markdown = "![diagram][ref]\n\n[ref]: ./assets/diagram.png".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
    });

    editor.read_with(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root block").clone();
        let runtime = block.read(cx).image_runtime().expect("image runtime");
        assert_eq!(runtime.src, "./assets/diagram.png");
    });
}

#[gpui::test]
async fn toggling_source_mode_preserves_quote_child_image_runtime(cx: &mut TestAppContext) {
    let markdown = "> ![diagram](./assets/diagram.png)".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
    });

    editor.read_with(cx, |editor, cx| {
        let quote = editor.document.first_root().expect("quote root").clone();
        let image_block = quote
            .read(cx)
            .children
            .first()
            .expect("quote image child")
            .clone();
        assert!(image_block.read(cx).image_runtime().is_some());
    });
}

#[gpui::test]
async fn toggling_source_mode_preserves_list_item_image_runtime(cx: &mut TestAppContext) {
    let markdown = "- ![diagram](./assets/diagram.png)".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
    });

    editor.read_with(cx, |editor, cx| {
        let block = editor
            .document
            .first_root()
            .expect("list item root")
            .clone();
        assert!(block.read(cx).image_runtime().is_some());
    });
}

#[gpui::test]
async fn toggling_source_mode_preserves_list_child_image_runtime(cx: &mut TestAppContext) {
    let markdown = "- item\n  ![diagram](./assets/diagram.png)".to_string();
    let editor = cx.new(|cx| Editor::from_markdown(cx, markdown, None));

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
    });

    editor.read_with(cx, |editor, cx| {
        let list_item = editor
            .document
            .first_root()
            .expect("list item root")
            .clone();
        let image_block = list_item
            .read(cx)
            .children
            .first()
            .expect("list child image")
            .clone();
        assert!(image_block.read(cx).image_runtime().is_some());
    });
}

#[gpui::test]
async fn undo_reverts_recent_rendered_typing(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.active_entity_id = Some(block.entity_id());
        block.update(cx, |block, cx| {
            block.prepare_undo_capture(crate::components::UndoCaptureKind::CoalescibleText, cx);
            block.replace_text_in_visible_range(5..5, " beta", None, false, cx);
        });
    });

    editor.update(cx, |editor, cx| {
        assert_eq!(editor.document.markdown_text(cx), "alpha beta");
        assert_eq!(editor.undo_history.len(), 1);
        editor.undo_document(cx);
        assert_eq!(editor.document.markdown_text(cx), "alpha");
    });
}

#[gpui::test]
async fn auto_pair_is_one_undoable_transaction_in_live_and_source_modes(cx: &mut TestAppContext) {
    for source_mode in [false, true] {
        let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

        editor.update(cx, |editor, cx| {
            let block = editor.document.first_root().expect("root").clone();
            editor.active_entity_id = Some(block.entity_id());
            block.update(cx, |block, cx| {
                if source_mode {
                    block.set_source_document_mode();
                }
                assert!(block.try_apply_auto_pair_input(0..0, "(", cx));
                assert_eq!(block.display_text(), "()");
                assert_eq!(block.selected_range, 1..1);
            });
        });

        editor.update(cx, |editor, cx| {
            assert_eq!(editor.document.markdown_text(cx), "()");
            assert_eq!(editor.undo_history.len(), 1);
            editor.undo_document(cx);
            assert_eq!(editor.document.markdown_text(cx), "");
            editor.redo_document(cx);
            assert_eq!(editor.document.markdown_text(cx), "()");
        });
    }
}

#[gpui::test]
async fn auto_pair_skip_and_empty_pair_backspace_preserve_revisioned_history(
    cx: &mut TestAppContext,
) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, String::new(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.active_entity_id = Some(block.entity_id());
        block.update(cx, |block, cx| {
            assert!(block.try_apply_auto_pair_input(0..0, "[", cx));
        });
    });

    let paired_revision = editor.read_with(cx, |editor, _cx| editor.source_document.revision());
    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        block.update(cx, |block, cx| {
            assert!(block.try_apply_auto_pair_input(1..1, "]", cx));
            assert_eq!(block.selected_range, 2..2);
            assert_eq!(block.display_text(), "[]");
        });
    });
    editor.read_with(cx, |editor, _cx| {
        assert_eq!(editor.source_document.revision(), paired_revision);
        assert_eq!(editor.undo_history.len(), 1);
    });

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        block.update(cx, |block, cx| {
            block.selected_range = 1..1;
            assert!(block.try_delete_empty_auto_pair(cx));
        });
    });

    editor.update(cx, |editor, cx| {
        assert_eq!(editor.document.markdown_text(cx), "");
        assert_eq!(editor.undo_history.len(), 2);
        assert!(editor.source_document.revision() > paired_revision);
        editor.undo_document(cx);
        assert_eq!(editor.document.markdown_text(cx), "[]");
        editor.undo_document(cx);
        assert_eq!(editor.document.markdown_text(cx), "");
    });
}

#[gpui::test]
async fn consecutive_text_edits_within_window_coalesce_into_one_undo(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "a".to_string(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.active_entity_id = Some(block.entity_id());

        block.update(cx, |block, cx| {
            block.prepare_undo_capture(crate::components::UndoCaptureKind::CoalescibleText, cx);
            block.replace_text_in_visible_range(1..1, "b", None, false, cx);
        });
        block.update(cx, |block, cx| {
            block.prepare_undo_capture(crate::components::UndoCaptureKind::CoalescibleText, cx);
            block.replace_text_in_visible_range(2..2, "c", None, false, cx);
        });
    });

    editor.update(cx, |editor, cx| {
        assert_eq!(editor.document.markdown_text(cx), "abc");
        assert_eq!(editor.undo_history.len(), 1);

        editor.undo_document(cx);
        assert_eq!(editor.document.markdown_text(cx), "a");
    });
}
