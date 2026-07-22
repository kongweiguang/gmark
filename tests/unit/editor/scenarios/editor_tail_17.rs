// @author kongweiguang

#[gpui::test]
async fn table_fragment_merge_is_explicit_and_one_undo_step(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let source = "| A | B |\n| --- | --- |\n| 1 | 2 |\n\n";
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_markdown(cx, source.to_owned(), None));
    let paragraph = editor.read_with(visual, |editor, _cx| {
        editor
            .document
            .visible_blocks()
            .last()
            .unwrap()
            .entity
            .clone()
    });

    editor.update(visual, |editor, cx| {
        editor.on_block_event(
            paragraph,
            &BlockEvent::RequestPasteMultiline {
                leading: InlineTextTree::plain(String::new()),
                lines: vec!["| 3 | 4 |".to_owned()],
                trailing: InlineTextTree::plain(String::new()),
                split_physical_lines: false,
            },
            cx,
        );
        assert!(editor.table_fragment_merge.is_some());
        assert!(editor.source_document.text().contains("| 3 | 4 |"));
        editor.confirm_table_fragment_merge(0, cx);
        assert!(editor.table_fragment_merge.is_none());
        assert_eq!(
            editor.source_document.text(),
            "| A | B |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |"
        );
        editor.undo_document(cx);
        assert!(editor.source_document.text().contains("| 3 | 4 |"));
        assert_eq!(
            editor.document.visible_blocks()[0].entity.read(cx).kind(),
            BlockKind::Table
        );
    });
}

#[gpui::test]
async fn stale_table_fragment_candidate_preserves_pasted_rows(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let source = "| A | B |\n| --- | --- |\n\n";
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_markdown(cx, source.to_owned(), None));
    let paragraph = editor.read_with(visual, |editor, _cx| {
        editor
            .document
            .visible_blocks()
            .last()
            .unwrap()
            .entity
            .clone()
    });
    editor.update(visual, |editor, cx| {
        editor.on_block_event(
            paragraph,
            &BlockEvent::RequestPasteMultiline {
                leading: InlineTextTree::plain(String::new()),
                lines: vec!["| x | y |".to_owned()],
                trailing: InlineTextTree::plain(String::new()),
                split_physical_lines: false,
            },
            cx,
        );
        let fragment = editor.document.visible_blocks()[1].entity.clone();
        fragment.update(cx, |block, cx| {
            let end = block.display_text().len();
            block.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
            block.replace_text_in_visible_range(end..end, "!", None, false, cx);
        });
    });
    visual.run_until_parked();
    editor.update(visual, |editor, cx| {
        editor.confirm_table_fragment_merge(0, cx);
        assert!(editor.table_fragment_merge.is_none());
        assert!(editor.source_document.text().contains("| x | y |!"));
    });
}

#[gpui::test]
async fn workspace_link_completion_writes_standard_relative_markdown_and_undoes_once(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let root = std::env::temp_dir().join("gmark-link-completion-workspace");
    let current = root.join("notes").join("Current.md");
    let target = root.join("guides").join("Guide.md");
    let current_for_editor = current.clone();
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_markdown(cx, String::new(), Some(current_for_editor))
    });
    editor.update(visual, |editor, cx| {
        editor
            .workspace
            .install_markdown_snapshot_for_test(root, vec![current, target]);
        let block = editor.document.first_root().unwrap().clone();
        block.update(cx, |block, cx| {
            block.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
            block.replace_text_in_visible_range(0..0, "[[gu", None, false, cx);
        });
    });
    visual.run_until_parked();
    editor.update(visual, |editor, cx| {
        assert!(editor.workspace_link_completion.is_some());
        editor.accept_workspace_link_completion(0, cx);
    });
    visual.run_until_parked();
    editor.update(visual, |editor, cx| {
        assert_eq!(editor.source_document.text(), "[Guide](../guides/Guide.md)");
        editor.undo_document(cx);
        assert_eq!(editor.source_document.text(), "[[gu");
    });
}

#[gpui::test]
async fn focused_complex_source_block_shows_read_only_live_preview(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "$$\nx^2\n$$".to_owned(), None));
    editor.update(visual, |editor, _cx| {
        let math = editor.document.first_root().unwrap().clone();
        editor.focus_block(math.entity_id());
    });
    redraw(visual);
    assert!(visual.debug_bounds("complex-source-live-preview").is_some());
    assert!(
        visual
            .debug_bounds("complex-source-live-preview-result")
            .is_some()
    );
    editor.read_with(visual, |editor, _cx| assert!(!editor.document_dirty));
}

#[gpui::test]
async fn mermaid_overlay_is_read_only_and_escape_restores_block_focus(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let source = "```mermaid\nflowchart LR\nA --> B\n```";
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_markdown(cx, source.to_owned(), None));
    redraw(visual);
    visual.executor().advance_clock(Duration::from_millis(300));
    visual.run_until_parked();
    redraw(visual);
    let open = visual
        .debug_bounds("mermaid-open-overlay")
        .expect("Mermaid enlarged-view button");
    let before = editor.read_with(visual, |editor, _cx| {
        (editor.source_document.text(), editor.document_dirty)
    });
    visual.simulate_click(open.center(), Modifiers::default());
    editor.read_with(visual, |editor, _cx| {
        assert!(editor.diagram_overlay.is_some());
    });
    redraw(visual);
    visual.run_until_parked();
    redraw(visual);
    assert!(visual.debug_bounds("diagram-overlay").is_some());
    editor.update_in(visual, |editor, window, _cx| {
        assert!(
            editor
                .diagram_overlay
                .as_ref()
                .expect("overlay state")
                .close_focus_handle
                .is_focused(window),
            "overlay close control must own keyboard focus"
        );
    });
    assert!(visual.debug_bounds("diagram-overlay-close").is_some());
    visual.simulate_keystrokes("escape");
    editor.read_with(visual, |editor, _cx| {
        assert!(editor.diagram_overlay.is_none(), "Escape must clear overlay state");
    });
    redraw(visual);
    editor.read_with(visual, |editor, cx| {
        assert_eq!(
            (editor.source_document.text(), editor.document_dirty),
            before
        );
        let block = editor.document.first_root().unwrap();
        assert_eq!(block.read(cx).kind(), BlockKind::MermaidBlock);
    });
    editor.update_in(visual, |editor, window, cx| {
        assert!(
            editor
                .document
                .first_root()
                .unwrap()
                .read(cx)
                .focus_handle
                .is_focused(window)
        );
    });
}
