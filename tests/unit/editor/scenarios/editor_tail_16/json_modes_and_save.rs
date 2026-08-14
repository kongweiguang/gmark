// @author kongweiguang

#[gpui::test]
async fn json_opens_in_graph_preview_and_reuses_live_source_split_preview_modes(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("JSON graph tempdir");
    let path = temp.path().join("graph.json");
    fs::write(
        &path,
        r#"{"name":"Ada","items":[{"ok":true},{"ok":false}]}"#,
    )
    .expect("JSON fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("JSON probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("JSON source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.simulate_resize(size(px(1100.0), px(720.0)));
    visual.run_until_parked();
    redraw(visual);

    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Preview));
    assert!(visual.debug_bounds("json-graph-canvas").is_some());
    assert!(
        visual
            .debug_bounds("json-graph-port-port:node:$/items#1")
            .is_some(),
        "the child edge must originate from its named parent field row"
    );
    let mode_button = visual
        .debug_bounds("status-bar-mode-switch")
        .expect("JSON mode picker");
    visual.simulate_click(mode_button.center(), Modifiers::default());
    redraw(visual);
    assert!(visual.debug_bounds("status-bar-mode-menu").is_some());
    assert!(visual.debug_bounds("status-bar-mode-Source").is_some());
    assert!(visual.debug_bounds("status-bar-mode-Split").is_some());
    assert!(visual.debug_bounds("status-bar-mode-Preview").is_some());
    assert!(visual.debug_bounds("status-bar-mode-Rendered").is_some());
    assert!(visual.debug_bounds("status-bar-json-graph-edit").is_none());
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("JSON disk view");
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.json_graph_state_for_test())
            .is_some_and(|(nodes, edges, truncated, stale, error)| {
                nodes == 4 && edges == 3 && !truncated && !stale && error.is_none()
            })
    );

    editor.update(visual, |editor, cx| {
        editor.set_view_mode(ViewMode::Rendered, cx);
        assert_eq!(editor.view_mode, ViewMode::Preview);
        editor.toggle_view_mode(cx);
        assert_eq!(editor.view_mode, ViewMode::Source);
        editor.toggle_view_mode(cx);
        assert_eq!(editor.view_mode, ViewMode::Preview);
    });
}

#[gpui::test]
async fn json_graph_edit_writes_back_to_source_without_leaving_preview(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("JSON graph edit tempdir");
    let path = temp.path().join("graph-edit.json");
    fs::write(&path, r#"{"nested":{"value":1}}"#).expect("JSON graph edit fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("JSON probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("JSON source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.simulate_resize(size(px(960.0), px(640.0)));
    visual.run_until_parked();
    redraw(visual);

    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("JSON SourceBacked view");
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.begin_json_graph_node_edit_for_test("node:$/nested#0", window, cx)
        });
    });
    visual.run_until_parked();
    redraw(visual);
    assert!(
        large_view.read_with(visual, |view, _cx| view.json_graph_edit_open_for_test()),
        "the node details action must open graph editing"
    );
    assert!(visual.debug_bounds("json-graph-edit-panel").is_some());
    let panel = visual
        .debug_bounds("json-graph-edit-panel")
        .expect("JSON graph edit panel");
    let input = large_view.read_with(visual, |view, _cx| view.json_graph_edit_input_for_test());
    input.update(visual, |block, cx| {
        let len = block.display_text().len();
        block.replace_text_in_visible_range(0..len, r#"{"value":2}"#, None, false, cx);
    });
    redraw(visual);
    let save = visual
        .debug_bounds("json-graph-edit-save")
        .expect("graph edit save button");
    let button_height = crate::theme::Theme::default_theme()
        .dimensions
        .dialog_button_height;
    assert_eq!(f32::from(save.size.height), button_height);
    assert!(save.left() >= panel.left());
    assert!(save.right() <= panel.right());
    assert!(save.bottom() <= panel.bottom());
    visual.simulate_click(save.center(), Modifiers::default());
    visual.executor().advance_clock(Duration::from_millis(300));
    visual.run_until_parked();

    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Preview));
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.source_text_for_test()),
        r#"{"nested":{"value":2}}"#
    );
    assert!(large_view.read_with(visual, |view, _cx| view.is_dirty()));
}
