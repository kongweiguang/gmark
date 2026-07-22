// @author kongweiguang

#[gpui::test]
async fn new_tab_button_offers_untyped_markdown_json_and_csv_document_types(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let (_editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, String::new(), None));
    visual.simulate_resize(size(px(900.0), px(620.0)));
    redraw(visual);

    let add = visual
        .debug_bounds("document-new-tab")
        .expect("new tab button");
    visual.simulate_click(add.center(), Modifiers::default());
    redraw(visual);

    assert!(visual.debug_bounds("new-tab-type-menu").is_some());
    assert!(visual.debug_bounds("new-tab-untyped").is_some());
    assert!(visual.debug_bounds("new-tab-markdown").is_some());
    assert!(visual.debug_bounds("new-tab-json").is_some());
    assert!(visual.debug_bounds("new-tab-csv").is_some());
}

#[gpui::test]
async fn json_new_tab_choice_creates_an_unsaved_document_without_a_path(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, String::new(), None));
    visual.simulate_resize(size(px(900.0), px(620.0)));
    redraw(visual);

    let add = visual.debug_bounds("document-new-tab").unwrap();
    visual.simulate_click(add.center(), Modifiers::default());
    redraw(visual);
    let json = visual.debug_bounds("new-tab-json").unwrap();
    visual.simulate_click(json.center(), Modifiers::default());

    editor.update(visual, |editor, cx| {
        assert!(editor.file_path.is_none());
        assert_eq!(editor.source_document.text(), "{\n}\n");
        assert_eq!(editor.document_kind, super::DocumentKind::Json);
        assert_eq!(editor.view_mode, ViewMode::Source);
        assert_eq!(editor.document_kind.icon(), "icon/ui/code.svg");
        assert_eq!(editor.save_dialog_defaults().1.as_deref(), Some("Untitled.json"));
        assert!(editor.switch_to_tab_index(0, cx));
        assert_eq!(editor.document_kind, super::DocumentKind::Markdown);
        assert!(editor.switch_to_tab_index(1, cx));
        assert_eq!(editor.document_kind, super::DocumentKind::Json);
    });
}

#[gpui::test]
async fn csv_new_tab_choice_creates_an_unsaved_document_without_a_path(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, String::new(), None));
    visual.simulate_resize(size(px(900.0), px(620.0)));
    redraw(visual);

    let add = visual.debug_bounds("document-new-tab").unwrap();
    visual.simulate_click(add.center(), Modifiers::default());
    redraw(visual);
    let csv = visual.debug_bounds("new-tab-csv").unwrap();
    visual.simulate_click(csv.center(), Modifiers::default());

    editor.update(visual, |editor, _cx| {
        assert!(editor.file_path.is_none());
        assert_eq!(editor.source_document.text(), "Column 1,Column 2\n");
        assert_eq!(editor.document_kind, super::DocumentKind::Csv);
        assert_eq!(editor.view_mode, ViewMode::Source);
        assert_eq!(editor.document_kind.icon(), "icon/ui/table.svg");
        assert_eq!(editor.save_dialog_defaults().1.as_deref(), Some("Untitled.csv"));
    });
}

#[test]
fn new_document_kind_controls_only_missing_save_extensions() {
    let mut untyped = PathBuf::from("Untitled");
    super::DocumentKind::Unspecified.apply_default_extension(&mut untyped);
    assert_eq!(untyped, PathBuf::from("Untitled"));

    let mut json = PathBuf::from("Untitled");
    super::DocumentKind::Json.apply_default_extension(&mut json);
    assert_eq!(json, PathBuf::from("Untitled.json"));

    let mut csv = PathBuf::from("report");
    super::DocumentKind::Csv.apply_default_extension(&mut csv);
    assert_eq!(csv, PathBuf::from("report.csv"));

    let mut explicit = PathBuf::from("report.txt");
    super::DocumentKind::Csv.apply_default_extension(&mut explicit);
    assert_eq!(explicit, PathBuf::from("report.txt"));
}

#[gpui::test]
async fn million_line_source_jump_keeps_local_scroll_geometry_exact(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("million-line Source tempdir");
    let path = temp.path().join("million-lines.txt");
    let mut text = "x\n".repeat(999_999);
    text.push('x');
    fs::write(&path, text).expect("million-line Source fixture");
    let probe = gmark_paged_document::probe_file(
        &path,
        gmark_paged_document::ProbeOptions {
            max_resident_bytes: 1,
            ..gmark_paged_document::ProbeOptions::default()
        },
    )
    .expect("million-line Source probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("million-line Source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.simulate_resize(size(px(960.0), px(640.0)));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("million-line large view");

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.jump_bottom_for_test(window, cx));
    });
    visual.run_until_parked();
    redraw(visual);

    let (origin, window_len, total_lines) =
        large_view.read_with(visual, |view, _cx| view.source_list_window_for_test());
    assert!(
        origin > 0,
        "a million lines must use a non-zero local origin: total={total_lines}, window={window_len}"
    );
    assert_eq!(window_len, crate::document_host::SOURCE_LIST_WINDOW_ROWS);
    let last = visual
        .debug_bounds("document-host-line-body-999999")
        .expect("last global Source line");
    let previous = visual
        .debug_bounds("document-host-line-body-999998")
        .expect("previous global Source line");
    let row_height = large_view.read_with(visual, |view, _cx| view.source_row_height_for_test());
    assert!(
        (f32::from(last.top() - previous.top()) - row_height).abs() < 0.5,
        "local scroll window must not quantize or overlap rows at the global file tail"
    );
}

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
    let input = large_view.read_with(visual, |view, _cx| view.json_graph_edit_input_for_test());
    input.update(visual, |block, cx| {
        let len = block.display_text().len();
        block.replace_text_in_visible_range(0..len, r#"{"value":2}"#, None, false, cx);
    });
    redraw(visual);
    let save = visual
        .debug_bounds("json-graph-edit-save")
        .expect("graph edit save button");
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

#[gpui::test]
async fn json_graph_invalid_edit_keeps_source_and_draft_open(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("JSON invalid graph edit tempdir");
    let path = temp.path().join("invalid-graph-edit.json");
    fs::write(&path, r#"{"nested":{"value":1}}"#).expect("JSON fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("JSON probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("JSON source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
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
    redraw(visual);
    let input = large_view.read_with(visual, |view, _cx| view.json_graph_edit_input_for_test());
    input.update(visual, |block, cx| {
        let len = block.display_text().len();
        block.replace_text_in_visible_range(0..len, r#"{"value":}"#, None, false, cx);
    });
    redraw(visual);
    let save = visual
        .debug_bounds("json-graph-edit-save")
        .expect("graph edit save");
    visual.simulate_click(save.center(), Modifiers::default());
    redraw(visual);
    assert!(visual.debug_bounds("json-graph-edit-error").is_some());
    assert!(visual.debug_bounds("json-graph-edit-panel").is_some());
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.source_text_for_test()),
        r#"{"nested":{"value":1}}"#
    );
}

#[gpui::test]
async fn stale_json_graph_edit_is_rejected_and_can_reload_current_value(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("stale JSON graph edit tempdir");
    let path = temp.path().join("stale-graph-edit.json");
    fs::write(&path, r#"{"nested":{"value":1}}"#).expect("JSON fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("JSON probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("JSON source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
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
    let graph_input =
        large_view.read_with(visual, |view, _cx| view.json_graph_edit_input_for_test());
    graph_input.update(visual, |block, cx| {
        let len = block.display_text().len();
        block.replace_text_in_visible_range(0..len, r#"{"value":9}"#, None, false, cx);
    });

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
    });
    let (_, source_input) = large_view
        .read_with(visual, |view, _cx| view.active_edit_for_test())
        .expect("source edit");
    source_input.update(visual, |block, cx| {
        let len = block.display_text().len();
        block.replace_text_in_visible_range(0..len, r#"{"nested":{"value":2}}"#, None, false, cx);
    });
    visual.executor().advance_clock(Duration::from_millis(300));
    visual.run_until_parked();
    redraw(visual);

    let save = visual
        .debug_bounds("json-graph-edit-save")
        .expect("stale graph edit save");
    visual.simulate_click(save.center(), Modifiers::default());
    redraw(visual);
    assert!(visual.debug_bounds("json-graph-edit-reload").is_some());
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.source_text_for_test()),
        r#"{"nested":{"value":2}}"#
    );

    let reload = visual
        .debug_bounds("json-graph-edit-reload")
        .expect("reload latest graph value");
    visual.simulate_click(reload.center(), Modifiers::default());
    assert_eq!(
        graph_input.read_with(visual, |block, _cx| block.display_text().to_owned()),
        r#"{"value":2}"#
    );
}

#[gpui::test]
async fn json_graph_edit_in_split_updates_source_without_closing_split(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("split JSON graph edit tempdir");
    let path = temp.path().join("split-graph-edit.json");
    fs::write(&path, r#"{"nested":{"value":1}}"#).expect("JSON fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("JSON probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("JSON source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.run_until_parked();
    editor.update(visual, |editor, cx| {
        editor.set_view_mode(ViewMode::Split, cx)
    });
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
    redraw(visual);
    let input = large_view.read_with(visual, |view, _cx| view.json_graph_edit_input_for_test());
    input.update(visual, |block, cx| {
        let len = block.display_text().len();
        block.replace_text_in_visible_range(0..len, r#"{"value":2}"#, None, false, cx);
    });
    redraw(visual);
    let save = visual
        .debug_bounds("json-graph-edit-save")
        .expect("split graph edit save");
    visual.simulate_click(save.center(), Modifiers::default());
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.source_text_for_test()),
        r#"{"nested":{"value":2}}"#
    );
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Split));
    redraw(visual);
    assert!(visual.debug_bounds("json-graph-split-source").is_some());
    assert!(visual.debug_bounds("json-graph-split-preview").is_some());
}

#[gpui::test]
async fn oversized_json_graph_value_routes_to_source_without_materializing_editor(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("oversized JSON graph edit tempdir");
    let path = temp.path().join("oversized-graph-edit.json");
    let source_text = format!(r#"{{"blob":"{}"}}"#, "x".repeat(300 * 1024));
    fs::write(&path, source_text).expect("large JSON fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("JSON probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("JSON source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("JSON SourceBacked view");
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.begin_json_graph_item_edit_for_test("field:$/blob#0", window, cx)
        });
    });
    redraw(visual);
    assert!(visual.debug_bounds("json-graph-edit-source").is_some());
    assert!(visual.debug_bounds("json-graph-edit-save").is_none());
    assert!(visual.debug_bounds("json-graph-edit-error").is_some());
}

#[gpui::test]
async fn json_split_edit_keeps_last_valid_graph_until_repaired(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("JSON stale graph tempdir");
    let path = temp.path().join("stale.json");
    fs::write(&path, r#"{"value":1}"#).expect("JSON fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("JSON probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("JSON source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.run_until_parked();
    editor.update(visual, |editor, cx| {
        editor.set_view_mode(ViewMode::Split, cx)
    });
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("JSON disk view");

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
    });
    let (_, edit) = large_view
        .read_with(visual, |view, _cx| view.active_edit_for_test())
        .expect("JSON line edit");
    edit.update(visual, |block, cx| {
        let len = block.display_text().len();
        block.replace_text_in_visible_range(0..len, r#"{"value":}"#, None, false, cx);
    });
    visual.executor().advance_clock(Duration::from_millis(300));
    visual.run_until_parked();
    redraw(visual);
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Split));
    assert!(visual.debug_bounds("json-graph-stale-banner").is_some());
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.json_graph_state_for_test())
            .is_some_and(|(_, _, _, stale, error)| stale && error.is_some())
    );

    let (_, edit) = large_view
        .read_with(visual, |view, _cx| view.active_edit_for_test())
        .expect("JSON line edit remains active");
    edit.update(visual, |block, cx| {
        let len = block.display_text().len();
        block.replace_text_in_visible_range(0..len, r#"{"value":2}"#, None, false, cx);
    });
    visual.executor().advance_clock(Duration::from_millis(300));
    visual.run_until_parked();
    redraw(visual);
    let repaired_state = large_view.read_with(visual, |view, _cx| view.json_graph_state_for_test());
    assert!(
        repaired_state.is_some_and(|(_, _, _, stale, error)| !stale && error.is_none()),
        "repaired graph state: {repaired_state:?}"
    );
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.json_graph_state_for_test())
            .is_some_and(|(_, _, _, stale, error)| !stale && error.is_none())
    );
}

#[gpui::test]
async fn json_tabs_keep_independent_modes_and_persist_split_ratio(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("JSON tab tempdir");
    let first_path = temp.path().join("first.json");
    let second_path = temp.path().join("second.json");
    fs::write(&first_path, r#"{"first":1}"#).expect("first JSON");
    fs::write(&second_path, r#"{"second":{"value":2}}"#).expect("second JSON");
    let first_probe = gmark_paged_document::probe_file(
        &first_path,
        gmark_paged_document::ProbeOptions::default(),
    )
    .expect("first probe");
    let first_source = gmark_paged_document::FileSource::open(&first_path).expect("first source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, first_path, first_probe, first_source)
    });
    visual.run_until_parked();

    let second_probe = gmark_paged_document::probe_file(
        &second_path,
        gmark_paged_document::ProbeOptions::default(),
    )
    .expect("second probe");
    let second_source =
        gmark_paged_document::FileSource::open(&second_path).expect("second source");
    editor.update(visual, |editor, cx| {
        editor.install_new_source_backed_tab(second_path, second_probe, second_source, cx);
        editor.split_pane_ratio = 0.61;
        editor.set_view_mode(ViewMode::Split, cx);
    });
    visual.run_until_parked();
    redraw(visual);
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Split));
    assert!(visual.debug_bounds("json-graph-split-divider").is_some());
    let divider_line = visual
        .debug_bounds("json-graph-split-divider-line")
        .expect("JSON split must render exactly one visible divider line");
    assert_eq!(f32::from(divider_line.size.width), 1.0);

    editor.update(visual, |editor, cx| {
        assert!(editor.switch_to_tab_index(0, cx));
        assert_eq!(editor.view_mode, ViewMode::Preview);
        assert!(editor.switch_to_tab_index(1, cx));
        assert_eq!(editor.view_mode, ViewMode::Split);
        let session = editor.workspace_session_snapshot(cx);
        assert_eq!(session.tabs[1].view_mode.as_deref(), Some("split"));
        assert_eq!(session.split_pane_ratio, Some(0.61));
    });
}

#[gpui::test]
async fn json_graph_split_click_and_keyboard_enter_locate_source(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("JSON graph interaction tempdir");
    let path = temp.path().join("interaction.json");
    let source_text = r#"{"nested":{"value":1},"tail":true}"#;
    fs::write(&path, source_text).expect("JSON interaction fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("JSON probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("JSON source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.simulate_resize(size(px(1100.0), px(720.0)));
    visual.run_until_parked();
    editor.update(visual, |editor, cx| {
        editor.set_view_mode(ViewMode::Split, cx)
    });
    visual.run_until_parked();
    redraw(visual);

    let fit = visual
        .debug_bounds("json-graph-fit")
        .expect("JSON graph fit control");
    visual.simulate_click(fit.center(), Modifiers::default());
    visual.run_until_parked();
    redraw(visual);
    let child = visual
        .debug_bounds("json-graph-node-node:$/nested#0")
        .expect("nested graph card");
    let canvas = visual
        .debug_bounds("json-graph-canvas")
        .expect("JSON graph canvas");
    assert!(child.left() >= canvas.left() && child.right() <= canvas.right());
    assert!(child.top() >= canvas.top() && child.bottom() <= canvas.bottom());
    let child_click = point(child.left() + px(12.0), child.top() + px(12.0));
    visual.simulate_event(MouseDownEvent {
        position: child_click,
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 1,
        first_mouse: false,
    });
    visual.simulate_event(MouseUpEvent {
        position: child_click,
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 1,
    });
    visual.run_until_parked();
    redraw(visual);
    assert!(visual.debug_bounds("json-graph-node-details").is_some());
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("JSON SourceBacked view");
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.graph_selected_item_for_test()),
        Some("node:$/nested#0".to_owned()),
        "nested card bounds: {child:?}"
    );
    let selection = large_view
        .read_with(visual, |view, _cx| view.source_selection_for_test())
        .expect("source selection after split click");
    let nested_start = source_text.find(r#"{"value":1}"#).unwrap() as u64;
    assert_eq!(
        selection.range(),
        nested_start..nested_start + r#"{"value":1}"#.len() as u64
    );
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Split));

    redraw(visual);
    let focus = visual
        .debug_bounds("json-graph-focus-subtree")
        .expect("focus selected subtree control");
    visual.simulate_click(focus.center(), Modifiers::default());
    visual.run_until_parked();
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.json_graph_root_identity_for_test()),
        Some(("$/nested#0".to_owned(), "nested".to_owned()))
    );

    editor.update(visual, |editor, cx| {
        editor.set_view_mode(ViewMode::Preview, cx)
    });
    visual.run_until_parked();
    redraw(visual);
    let root = visual
        .debug_bounds("json-graph-node-node:$/nested#0")
        .expect("root graph card");
    let root_click = point(root.left() + px(12.0), root.top() + px(12.0));
    visual.simulate_event(MouseDownEvent {
        position: root_click,
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 1,
        first_mouse: false,
    });
    visual.simulate_event(MouseUpEvent {
        position: root_click,
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 1,
    });
    visual.simulate_keystrokes("enter");
    visual.run_until_parked();
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Source));
}

#[gpui::test]
async fn json_graph_starts_expanded_and_search_selects_a_deep_match(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("JSON search tempdir");
    let path = temp.path().join("search.json");
    fs::write(
        &path,
        r#"{"level1":{"level2":{"level3":{"target":"needle"}}}}"#,
    )
    .expect("JSON search fixture");
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
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("JSON SourceBacked view");
    let target_selector = "json-graph-node-node:$/level1#0/level2#0/level3#0";
    assert!(
        visual.debug_bounds(target_selector).is_some(),
        "projected JSON containers start fully expanded"
    );

    let search = large_view.read_with(visual, |view, _cx| view.json_search_input_for_test());
    search.update(visual, |block, cx| {
        block.replace_text_in_visible_range(
            0..block.display_text().len(),
            "needle",
            None,
            false,
            cx,
        );
    });
    visual.run_until_parked();
    redraw(visual);

    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.graph_selected_item_for_test()),
        Some("node:$/level1#0/level2#0/level3#0".to_owned())
    );
    assert!(visual.debug_bounds(target_selector).is_some());
    assert!(visual.debug_bounds("json-graph-search-count").is_some());
    assert!(visual.debug_bounds("json-graph-search-previous").is_some());
    assert!(visual.debug_bounds("json-graph-search-next").is_some());
}

#[gpui::test]
async fn json_graph_search_next_cycles_all_loaded_matches(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("JSON search navigation tempdir");
    let path = temp.path().join("search-navigation.json");
    fs::write(
        &path,
        r#"{"left":{"value":"needle"},"right":{"value":"needle"}}"#,
    )
    .expect("JSON search fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("JSON probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("JSON source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("JSON SourceBacked view");
    let search = large_view.read_with(visual, |view, _cx| view.json_search_input_for_test());
    search.update(visual, |block, cx| {
        block.replace_text_in_visible_range(
            0..block.display_text().len(),
            "needle",
            None,
            false,
            cx,
        );
    });
    visual.run_until_parked();
    redraw(visual);
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.json_graph_search_state_for_test()),
        (2, 0)
    );
    let next = visual
        .debug_bounds("json-graph-search-next")
        .expect("next JSON graph search match");
    visual.simulate_click(next.center(), Modifiers::default());
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.json_graph_search_state_for_test()),
        (2, 1)
    );
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.graph_selected_item_for_test()),
        Some("node:$/right#1".to_owned())
    );
}

#[gpui::test]
async fn empty_json_stays_in_preview_while_loading_then_installs_one_empty_root(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("empty JSON tempdir");
    let path = temp.path().join("empty.json");
    fs::write(&path, "{}").expect("empty JSON fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("empty JSON probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("empty JSON source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });

    redraw(visual);
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Preview));
    assert!(visual.debug_bounds("json-graph-empty-state").is_some());

    visual.run_until_parked();
    redraw(visual);
    assert!(visual.debug_bounds("json-graph-canvas").is_some());
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("empty JSON SourceBacked view");
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.json_graph_state_for_test())
            .is_some_and(|(nodes, edges, truncated, stale, error)| {
                nodes == 1 && edges == 0 && !truncated && !stale && error.is_none()
            })
    );
}
