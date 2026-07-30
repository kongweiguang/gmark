// @author kongweiguang

#[gpui::test]
async fn json_live_edit_status_action_updates_field_and_participates_in_undo_redo(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("JSON live edit tempdir");
    let path = temp.path().join("live-edit.json");
    fs::write(&path, r#"{"value":1}"#).expect("JSON live edit fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("JSON probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("JSON source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.simulate_resize(size(px(900.0), px(620.0)));
    visual.run_until_parked();
    redraw(visual);

    let field_hit = visual
        .debug_bounds("json-graph-field-hit-field:$/value#0")
        .expect("projected scalar field");
    let field = visual
        .debug_bounds("json-graph-field-field:$/value#0")
        .expect("projected scalar row");
    let canvas = visual
        .debug_bounds("json-graph-canvas")
        .expect("JSON graph canvas");
    let root = visual
        .debug_bounds("json-graph-node-node:$")
        .expect("JSON graph root");
    assert!(
        field.left() >= canvas.left()
            && field.right() <= canvas.right()
            && field.top() >= canvas.top()
            && field.bottom() <= canvas.bottom(),
        "root {root:?}, field {field:?}, hit {field_hit:?} must be visible inside canvas {canvas:?}"
    );
    visual.simulate_event(MouseDownEvent {
        position: field.center(),
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 1,
        first_mouse: false,
    });
    visual.simulate_event(MouseUpEvent {
        position: field.center(),
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 1,
    });
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("JSON SourceBacked view");
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.graph_selected_item_for_test()),
        Some("field:$/value#0".to_owned())
    );
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.copy_for_test(window, cx));
    });
    visual.run_until_parked();
    assert_eq!(
        visual.read_from_clipboard().and_then(|item| item.text()),
        Some("1".to_owned()),
        "JSON Preview must copy the selected graph item content"
    );

    visual.simulate_event(MouseDownEvent {
        position: field_hit.center(),
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    visual.simulate_event(MouseUpEvent {
        position: field_hit.center(),
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 2,
    });
    redraw(visual);
    assert!(visual.debug_bounds("json-graph-edit-panel").is_some());
    let cancel = visual
        .debug_bounds("json-graph-edit-cancel")
        .expect("graph edit cancel");
    visual.simulate_event(MouseDownEvent {
        position: cancel.center(),
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 1,
        first_mouse: false,
    });
    visual.simulate_event(MouseUpEvent {
        position: cancel.center(),
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 1,
    });
    visual.run_until_parked();
    redraw(visual);
    assert!(!large_view.read_with(visual, |view, _cx| view.json_graph_edit_open_for_test()));

    assert!(visual.debug_bounds("status-bar-json-graph-edit").is_none());
    editor.update(visual, |editor, cx| {
        editor.set_view_mode(ViewMode::Source, cx)
    });
    redraw(visual);
    let mode_button = visual
        .debug_bounds("status-bar-mode-switch")
        .expect("JSON mode picker");
    visual.simulate_click(mode_button.center(), Modifiers::default());
    redraw(visual);
    let live_edit = visual
        .debug_bounds("status-bar-mode-Rendered")
        .expect("JSON reuses the Markdown live-edit mode button");
    visual.simulate_click(live_edit.center(), Modifiers::default());
    redraw(visual);
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Preview));
    assert!(visual.debug_bounds("json-graph-canvas").is_some());
    assert!(visual.debug_bounds("json-graph-edit-panel").is_some());

    assert!(visual.debug_bounds("status-bar-json-graph-edit").is_none());
    assert!(
        visual
            .debug_bounds("status-bar-json-graph-workspace")
            .is_none()
    );
    let input = large_view.read_with(visual, |view, _cx| view.json_graph_edit_input_for_test());
    input.update(visual, |block, cx| {
        let len = block.display_text().len();
        block.replace_text_in_visible_range(0..len, "2", None, false, cx);
    });
    redraw(visual);
    let save = visual
        .debug_bounds("json-graph-edit-save")
        .expect("graph edit save");
    visual.simulate_click(save.center(), Modifiers::default());
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.source_text_for_test()),
        r#"{"value":2}"#
    );

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.undo_for_test(window, cx));
    });
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.source_text_for_test()),
        r#"{"value":1}"#
    );
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.redo_for_test(window, cx));
    });
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.source_text_for_test()),
        r#"{"value":2}"#
    );
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Preview));
}
