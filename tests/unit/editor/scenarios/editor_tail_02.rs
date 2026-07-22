// @author kongweiguang

#[gpui::test]
async fn redo_restores_text_reverted_by_undo(cx: &mut TestAppContext) {
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
        editor.undo_document(cx);
        assert_eq!(editor.document.markdown_text(cx), "alpha");
        assert_eq!(editor.redo_history.len(), 1);

        editor.redo_document(cx);
        assert_eq!(editor.document.markdown_text(cx), "alpha beta");
        assert!(editor.redo_history.is_empty());
    });
}

#[gpui::test]
async fn fresh_edit_clears_pending_redo_history(cx: &mut TestAppContext) {
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
        editor.undo_document(cx);
        assert_eq!(editor.redo_history.len(), 1);

        // A new edit invalidates the redo stack so it cannot revive stale text.
        let block = editor.document.first_root().expect("root").clone();
        block.update(cx, |block, cx| {
            block.prepare_undo_capture(crate::components::UndoCaptureKind::CoalescibleText, cx);
            block.replace_text_in_visible_range(5..5, " gamma", None, false, cx);
        });
    });

    editor.update(cx, |editor, cx| {
        editor.finalize_pending_undo_capture(cx);
        assert!(editor.redo_history.is_empty());

        editor.redo_document(cx);
        assert_eq!(editor.document.markdown_text(cx), "alpha gamma");
    });
}

#[gpui::test]
async fn toggle_view_mode_preserves_paragraph_caret_position(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| Editor::from_markdown(cx, "alpha\n\nbeta".to_string(), None));

    editor.update(cx, |editor, cx| {
        let target = editor.document.visible_blocks()[1].entity.clone();
        target.update(cx, |block, _cx| {
            block.selected_range = 2..2;
        });
        editor.active_entity_id = Some(target.entity_id());

        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        let source = editor.document.first_root().expect("source root").clone();
        assert_eq!(source.read(cx).selected_range, 9..9);
        assert!(source.read(cx).show_source_line_numbers());

        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
        let visible = editor.document.visible_blocks();
        assert_eq!(visible.len(), 2);
        assert!(
            visible
                .iter()
                .all(|visible| !visible.entity.read(cx).show_source_line_numbers())
        );
        assert_eq!(visible[1].entity.read(cx).display_text(), "beta");
        assert_eq!(visible[1].entity.read(cx).selected_range, 2..2);
        assert_eq!(editor.pending_focus, Some(visible[1].entity.entity_id()));
    });
}

#[gpui::test]
async fn toggle_view_mode_ends_stale_code_block_pointer_selection(cx: &mut TestAppContext) {
    let editor =
        cx.new(|cx| Editor::from_markdown(cx, "```rust\nfn main() {}\n```".to_string(), None));

    editor.update(cx, |editor, cx| {
        let target = editor.document.visible_blocks()[0].entity.clone();
        target.update(cx, |block, _cx| {
            block.selected_range = 3..7;
            block.is_selecting = true;
            block.code_language_is_selecting = true;
        });
        editor.active_entity_id = Some(target.entity_id());

        editor.toggle_view_mode(cx);

        assert!(matches!(editor.view_mode, ViewMode::Source));
        target.read_with(cx, |block, _cx| {
            assert!(!block.is_selecting);
            assert!(!block.code_language_is_selecting);
            assert_eq!(block.selected_range, 3..7);
        });
    });
}

#[gpui::test]
async fn ctrl_tab_toggles_view_mode(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "alpha".to_string(), None));

    redraw(cx);
    cx.simulate_keystrokes("ctrl-tab");
    redraw(cx);

    editor.update(cx, |editor, _cx| {
        assert!(matches!(editor.view_mode, ViewMode::Source));
    });

    cx.simulate_keystrokes("ctrl-tab");
    redraw(cx);

    editor.update(cx, |editor, _cx| {
        assert!(matches!(editor.view_mode, ViewMode::Rendered));
    });
}

#[gpui::test]
async fn ctrl_a_selects_entire_source_document_in_source_mode(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "alpha\n\nbeta".to_string(), None)
    });

    editor.update(cx, |editor, cx| {
        editor.toggle_view_mode(cx);
        assert!(matches!(editor.view_mode, ViewMode::Source));
        let source = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(source.entity_id());
        source.update(cx, |block, _cx| {
            block.selected_range = 1..3;
        });
    });
    redraw(cx);

    cx.simulate_keystrokes("ctrl-a");
    redraw(cx);

    editor.read_with(cx, |editor, cx| {
        let source = editor.document.visible_blocks()[0].entity.read(cx);
        assert_eq!(source.selected_range, 0..source.visible_len());
        assert!(editor.cross_block_selection.is_none());
    });
}

#[gpui::test]
async fn source_mode_keyboard_copy_cut_paste_and_history_match_text_editor(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "alpha beta".to_string(), None));

    editor.update(visual, |editor, cx| {
        editor.set_view_mode(ViewMode::Source, cx);
        let source = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(source.entity_id());
        source.update(cx, |block, _cx| block.selected_range = 0..5);
    });
    redraw(visual);

    visual.simulate_keystrokes("ctrl-c");
    assert_eq!(
        visual.read_from_clipboard().and_then(|item| item.text()),
        Some("alpha".to_owned())
    );

    visual.simulate_keystrokes("ctrl-x");
    redraw(visual);
    editor.read_with(visual, |editor, cx| {
        assert_eq!(editor.document.raw_source_text(cx), " beta");
        assert_eq!(editor.source_document.text(), " beta");
    });

    visual.write_to_clipboard(gpui::ClipboardItem::new_string("gamma\nline".to_owned()));
    visual.simulate_keystrokes("ctrl-v");
    redraw(visual);
    editor.read_with(visual, |editor, cx| {
        assert_eq!(editor.document.raw_source_text(cx), "gamma\nline beta");
        assert_eq!(editor.source_document.text(), "gamma\nline beta");
    });

    visual.simulate_keystrokes("ctrl-z");
    redraw(visual);
    editor.read_with(visual, |editor, cx| {
        assert_eq!(editor.document.raw_source_text(cx), " beta");
        assert_eq!(editor.source_document.text(), " beta");
    });

    visual.simulate_keystrokes("ctrl-y");
    redraw(visual);
    editor.read_with(visual, |editor, cx| {
        assert_eq!(editor.document.raw_source_text(cx), "gamma\nline beta");
        assert_eq!(editor.source_document.text(), "gamma\nline beta");
    });
}

#[gpui::test]
async fn ctrl_a_selects_only_focused_block_text_in_rendered_mode(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "alpha\n\nbeta".to_string(), None)
    });

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[1].entity.clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, _cx| {
            block.selected_range = 1..1;
        });
    });
    redraw(cx);

    cx.simulate_keystrokes("ctrl-a");
    redraw(cx);

    editor.read_with(cx, |editor, cx| {
        let first = editor.document.visible_blocks()[0].entity.read(cx);
        let second = editor.document.visible_blocks()[1].entity.read(cx);
        assert_eq!(first.selected_range, 0..0);
        assert_eq!(second.selected_range, 0..second.visible_len());
        assert!(editor.cross_block_selection.is_none());
    });
}

#[gpui::test]
async fn repeated_ctrl_a_selects_all_rendered_blocks(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let markdown =
        "alpha\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n\ngamma";
    let (editor, cx) = cx.add_window_view({
        let markdown = markdown.to_string();
        move |_window, cx| Editor::from_markdown(cx, markdown.clone(), None)
    });

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(0, block_cx);
        });
    });
    redraw(cx);

    cx.simulate_keystrokes("ctrl-a");
    redraw(cx);

    editor.read_with(cx, |editor, cx| {
        let first = editor.document.visible_blocks()[0].entity.read(cx);
        assert_eq!(first.selected_range, 0..first.visible_len());
        assert!(editor.cross_block_selection.is_none());
    });

    cx.simulate_keystrokes("ctrl-a");
    redraw(cx);

    editor.read_with(cx, |editor, cx| {
        let visible = editor.document.visible_blocks();
        let first_id = visible[0].entity.entity_id();
        let last = visible.last().expect("visible blocks");
        let last_id = last.entity.entity_id();
        let last_len = last.entity.read(cx).visible_len();
        let selection = editor
            .cross_block_selection
            .expect("second Ctrl+A should select the rendered document");
        assert_eq!(selection.anchor.entity_id, first_id);
        assert_eq!(selection.anchor.offset, 0);
        assert_eq!(selection.focus.entity_id, last_id);
        assert_eq!(selection.focus.offset, last_len);
        for visible in visible {
            let block = visible.entity.read(cx);
            let len = block.visible_len();
            if len > 0 {
                assert_eq!(block.editor_selection_range, Some(0..len));
            }
        }
    });

    let selected_after_second = editor.read_with(cx, |editor, _cx| editor.cross_block_selection);
    cx.simulate_keystrokes("ctrl-a");
    redraw(cx);

    editor.read_with(cx, |editor, cx| {
        assert_eq!(
            editor.cross_block_selection, selected_after_second,
            "third Ctrl+A should keep the full rendered document selected"
        );
        for visible in editor.document.visible_blocks() {
            let block = visible.entity.read(cx);
            let len = block.visible_len();
            if len > 0 {
                assert_eq!(block.editor_selection_range, Some(0..len));
            }
        }
    });
}

#[gpui::test]
async fn rendered_ctrl_a_cycle_expires_before_second_press(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "alpha\n\nbeta".to_string(), None)
    });

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[1].entity.clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(1, block_cx);
        });
    });
    redraw(cx);

    cx.simulate_keystrokes("ctrl-a");
    redraw(cx);

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[1].entity.clone();
        block.update(cx, |block, _cx| {
            block.selected_range = 1..1;
        });
        let cycle = editor
            .rendered_select_all_cycle
            .as_mut()
            .expect("first Ctrl+A should arm the rendered select-all cycle");
        cycle.last_pressed_at =
            Instant::now() - (Editor::RENDERED_SELECT_ALL_CYCLE_WINDOW + Duration::from_millis(1));
    });

    cx.simulate_keystrokes("ctrl-a");
    redraw(cx);

    editor.read_with(cx, |editor, cx| {
        let second = editor.document.visible_blocks()[1].entity.read(cx);
        assert_eq!(second.selected_range, 0..second.visible_len());
        assert!(editor.cross_block_selection.is_none());
        assert_eq!(
            editor
                .rendered_select_all_cycle
                .expect("cycle should be reset by expired second press")
                .count,
            1
        );
    });
}

#[gpui::test]
async fn tab_key_inserts_tab_in_focused_paragraph(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "ab".to_string(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(1, block_cx);
        });
    });
    redraw(cx);

    cx.simulate_keystrokes("tab");
    redraw(cx);

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        assert_eq!(block.read(cx).display_text(), "a    b");
        assert_eq!(editor.document.markdown_text(cx), "a    b");
    });
}

#[gpui::test]
async fn tab_key_inserts_tab_in_focused_code_block(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "```rust\nab\n```".to_string(), None)
    });

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(1, block_cx);
        });
    });
    redraw(cx);

    cx.simulate_keystrokes("tab");
    redraw(cx);

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        assert_eq!(block.read(cx).display_text(), "a    b");
        assert_eq!(editor.document.markdown_text(cx), "```rust\na    b\n```");
    });
}

#[gpui::test]
async fn captured_tab_key_inserts_visible_indent_in_paragraph(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "ab".to_string(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(1, block_cx);
        });
    });
    redraw(cx);

    let event = KeyDownEvent {
        keystroke: Keystroke::parse("tab").expect("valid tab keystroke"),
        is_held: false,
    };
    editor.update_in(cx, |editor, window, cx| {
        editor.on_editor_key_down_capture(&event, window, cx);
    });
    redraw(cx);

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        assert_eq!(block.read(cx).display_text(), "a    b");
    });
}

#[gpui::test]
async fn captured_slash_menu_keys_filter_navigate_and_execute(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "/heading".to_string(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(block.visible_len(), block_cx);
            block.refresh_slash_menu(block_cx);
        });
    });
    redraw(cx);

    let down = KeyDownEvent {
        keystroke: Keystroke::parse("down").expect("valid down keystroke"),
        is_held: false,
    };
    let enter = KeyDownEvent {
        keystroke: Keystroke::parse("enter").expect("valid enter keystroke"),
        is_held: false,
    };
    editor.update_in(cx, |editor, window, cx| {
        editor.on_editor_key_down_capture(&down, window, cx);
        editor.on_editor_key_down_capture(&enter, window, cx);
    });
    redraw(cx);

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        assert_eq!(block.read(cx).kind(), BlockKind::Heading { level: 2 });
        assert_eq!(editor.source_document.text(), "## ");
    });
}

#[gpui::test]
async fn slash_menu_navigation_intercepts_real_bound_actions(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual_cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "/heading".to_string(), None));
    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(block.visible_len(), block_cx);
            block.refresh_slash_menu(block_cx);
        });
    });
    redraw(visual_cx);

    visual_cx.simulate_keystrokes("down tab");
    redraw(visual_cx);

    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").read(cx);
        assert_eq!(block.kind(), BlockKind::Heading { level: 2 });
        assert_eq!(editor.source_document.text(), "## ");
    });
}

#[gpui::test]
async fn slash_escape_stays_dismissed_until_the_query_changes(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "/".to_string(), None));

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(block.visible_len(), block_cx);
            block.refresh_slash_menu(block_cx);
            assert!(block.slash_menu.is_some());
        });
    });
    redraw(cx);

    let escape = KeyDownEvent {
        keystroke: Keystroke::parse("escape").expect("valid escape keystroke"),
        is_held: false,
    };
    editor.update_in(cx, |editor, window, cx| {
        editor.on_editor_key_down_capture(&escape, window, cx);
    });
    redraw(cx);

    editor.update(cx, |editor, cx| {
        let block = editor.document.visible_blocks()[0].entity.clone();
        block.update(cx, |block, block_cx| {
            block.refresh_slash_menu(block_cx);
            assert!(block.slash_menu.is_none());
            let end = block.visible_len();
            block.replace_text_in_visible_range(end..end, "h", None, false, block_cx);
            block.refresh_slash_menu(block_cx);
            assert!(block.slash_menu.is_some());
        });
    });
}

#[gpui::test]
async fn slash_enter_with_no_results_does_not_mutate_the_document(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "/missing".to_string(), None));
    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.move_to(block.visible_len(), block_cx);
            block.refresh_slash_menu(block_cx);
        });
    });
    redraw(cx);

    let enter = KeyDownEvent {
        keystroke: Keystroke::parse("enter").expect("valid enter keystroke"),
        is_held: false,
    };
    editor.update_in(cx, |editor, window, cx| {
        editor.on_editor_key_down_capture(&enter, window, cx);
    });
    redraw(cx);

    editor.update(cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        assert_eq!(editor.source_document.text(), "/missing");
        assert_eq!(block.read(cx).kind(), BlockKind::Paragraph);
        assert!(block.read(cx).slash_menu.is_some());
    });
}

#[gpui::test]
async fn selection_toolbar_formats_without_losing_selection_and_undo_restores_source(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let (editor, visual_cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "alpha beta".to_string(), None));
    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.selected_range = 0..5;
            block.selection_reversed = false;
            block.refresh_selection_toolbar();
            block_cx.notify();
        });
    });
    for viewport in [size(px(720.0), px(520.0)), size(px(1180.0), px(780.0))] {
        visual_cx.simulate_resize(viewport);
        redraw(visual_cx);
        let toolbar = visual_cx.debug_bounds("selection-toolbar").unwrap();
        let content = visual_cx.debug_bounds("editor-content").unwrap();
        let selection = editor.update(visual_cx, |editor, cx| {
            editor
                .document
                .first_root()
                .expect("root")
                .read(cx)
                .active_range_or_cursor_bounds()
                .expect("selection bounds")
        });
        assert_eq!(f32::from(toolbar.size.height), 32.0);
        assert!(toolbar.left() >= content.left());
        assert!(toolbar.right() <= content.right());
        assert!(toolbar.bottom() <= selection.top());
    }

    let bold_tooltip_target = visual_cx
        .debug_bounds("selection-toolbar-bold")
        .expect("bold button");
    visual_cx.simulate_mouse_move(bold_tooltip_target.center(), None, Modifiers::default());
    visual_cx
        .executor()
        .advance_clock(Duration::from_millis(520));
    visual_cx.run_until_parked();
    redraw(visual_cx);
    let tooltip = visual_cx.debug_bounds("ui-tooltip").unwrap();
    let content = visual_cx.debug_bounds("editor-content").unwrap();
    assert!(tooltip.left() >= content.left());
    assert!(tooltip.right() <= content.right());

    let overflow_button = visual_cx
        .debug_bounds("selection-toolbar-overflow")
        .expect("overflow button");
    visual_cx.simulate_click(overflow_button.center(), Modifiers::default());
    redraw(visual_cx);
    let overflow = visual_cx
        .debug_bounds("selection-toolbar-overflow-menu")
        .expect("overflow menu");
    let content = visual_cx.debug_bounds("editor-content").unwrap();
    let selection = editor.update(visual_cx, |editor, cx| {
        editor
            .document
            .first_root()
            .expect("root")
            .read(cx)
            .active_range_or_cursor_bounds()
            .expect("selection bounds")
    });
    assert!(overflow.left() >= content.left());
    assert!(overflow.right() <= content.right());
    assert!(overflow.bottom() <= selection.top());
    let underline = visual_cx
        .debug_bounds("selection-toolbar-underline")
        .expect("underline button");
    visual_cx.simulate_click(underline.center(), Modifiers::default());
    redraw(visual_cx);
    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        assert_eq!(editor.source_document.text(), "<u>alpha</u> beta");
        assert_eq!(block.read(cx).selection_clean_range(), 0..5);
        editor.undo_document(cx);
        assert_eq!(editor.source_document.text(), "alpha beta");
    });
    redraw(visual_cx);

    let bold = visual_cx
        .debug_bounds("selection-toolbar-bold")
        .expect("bold button");
    visual_cx.simulate_click(bold.center(), Modifiers::default());
    redraw(visual_cx);

    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        assert_eq!(editor.source_document.text(), "**alpha** beta");
        assert_eq!(block.read(cx).selection_clean_range(), 0..5);
        assert!(block.read(cx).selection_toolbar_visible());
        editor.undo_document(cx);
        assert_eq!(editor.source_document.text(), "alpha beta");
    });
}

#[gpui::test]
async fn selection_toolbar_roving_focus_intercepts_real_key_dispatch(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let (editor, visual_cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "alpha beta".to_string(), None));
    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.selected_range = 0..5;
            block.refresh_selection_toolbar();
            block_cx.notify();
        });
    });
    redraw(visual_cx);

    visual_cx.simulate_keystrokes("alt-f10");
    redraw(visual_cx);
    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").read(cx);
        assert_eq!(block.selected_range, 0..5);
        assert!(block.selection_toolbar_keyboard_active);
        assert_eq!(block.selection_toolbar_keyboard_index, 0);
    });

    visual_cx.simulate_keystrokes("right");
    redraw(visual_cx);

    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").read(cx);
        assert_eq!(block.selected_range, 0..5);
        assert!(block.selection_toolbar_keyboard_active);
        assert_eq!(block.selection_toolbar_keyboard_index, 1);
        assert!(block.selection_toolbar_visible());
    });

    visual_cx.simulate_keystrokes("enter");
    redraw(visual_cx);
    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").read(cx);
        assert_eq!(editor.source_document.text(), "**alpha** beta");
        assert_eq!(block.selection_clean_range(), 0..5);
        assert!(block.selection_toolbar_visible());
    });
}

#[gpui::test]
async fn selection_toolbar_escape_dismisses_only_until_selection_changes(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual_cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "alpha beta".to_string(), None));
    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        editor.focus_block(block.entity_id());
        block.update(cx, |block, block_cx| {
            block.selected_range = 0..5;
            block.refresh_selection_toolbar();
            block_cx.notify();
        });
    });
    redraw(visual_cx);
    assert!(visual_cx.debug_bounds("selection-toolbar").is_some());

    let escape = KeyDownEvent {
        keystroke: Keystroke::parse("escape").expect("valid escape keystroke"),
        is_held: false,
    };
    editor.update_in(visual_cx, |editor, window, cx| {
        editor.on_editor_key_down_capture(&escape, window, cx);
    });
    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").read(cx);
        assert_eq!(block.selection_toolbar_dismissed_range, Some(0..5));
        assert!(!block.selection_toolbar_visible());
    });
    redraw(visual_cx);
    redraw(visual_cx);
    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("root").clone();
        assert_eq!(block.read(cx).selected_range, 0..5);
        assert!(!block.read(cx).selection_toolbar_visible());
        block.update(cx, |block, block_cx| {
            block.selected_range = 0..4;
            block.refresh_selection_toolbar();
            assert!(block.selection_toolbar_visible());
            block_cx.notify();
        });
    });
    redraw(visual_cx);
    assert!(visual_cx.debug_bounds("selection-toolbar").is_some());
}

#[gpui::test]
async fn code_toolbar_copies_body_without_mutating_document_and_clears_feedback(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let source = "```rust\nfn main() {}\n```";
    let (editor, visual_cx) =
        cx.add_window_view(move |_window, cx| Editor::from_markdown(cx, source.to_owned(), None));
    for viewport in [size(px(720.0), px(520.0)), size(px(1180.0), px(780.0))] {
        visual_cx.simulate_resize(viewport);
        redraw(visual_cx);
        let toolbar = visual_cx.debug_bounds("code-block-toolbar").unwrap();
        let control = visual_cx.debug_bounds("code-language-control").unwrap();
        let copy = visual_cx.debug_bounds("code-block-copy").unwrap();
        let content = visual_cx.debug_bounds("editor-content").unwrap();
        assert_eq!(f32::from(toolbar.size.height), 28.0);
        assert!(toolbar.left() >= content.left());
        assert!(toolbar.right() <= content.right());
        assert!(control.left() >= toolbar.left());
        assert!(
            copy.right() <= toolbar.right(),
            "toolbar={toolbar:?} copy={copy:?} control={control:?} viewport={viewport:?}"
        );
        assert!(control.right() <= copy.left());
    }

    let (revision, dirty) = editor.read_with(visual_cx, |editor, _cx| {
        (editor.source_document.revision(), editor.document_dirty)
    });
    let copy = visual_cx.debug_bounds("code-block-copy").unwrap();
    visual_cx.simulate_mouse_move(copy.center(), None, Modifiers::default());
    visual_cx
        .executor()
        .advance_clock(Duration::from_millis(520));
    visual_cx.run_until_parked();
    redraw(visual_cx);
    let tooltip = visual_cx.debug_bounds("ui-tooltip").unwrap();
    let content = visual_cx.debug_bounds("editor-content").unwrap();
    assert!(tooltip.left() >= content.left());
    assert!(tooltip.right() <= content.right());
    visual_cx.simulate_click(copy.center(), Modifiers::default());
    redraw(visual_cx);
    assert_eq!(
        visual_cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("fn main() {}")
    );
    editor.update(visual_cx, |editor, cx| {
        let block = editor.document.first_root().expect("code block").read(cx);
        assert!(block.code_copy_feedback);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });
    visual_cx
        .executor()
        .advance_clock(Duration::from_millis(1_200));
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, cx| {
        assert!(
            !editor
                .document
                .first_root()
                .expect("code block")
                .read(cx)
                .code_copy_feedback
        );
    });
}
