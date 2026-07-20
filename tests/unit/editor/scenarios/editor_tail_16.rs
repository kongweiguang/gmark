// @author kongweiguang

#[gpui::test]
async fn million_line_source_jump_keeps_local_scroll_geometry_exact(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("million-line Source tempdir");
    let path = temp.path().join("million-lines.txt");
    let mut text = "x\n".repeat(999_999);
    text.push('x');
    fs::write(&path, text).expect("million-line Source fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("million-line Source probe");
    let source = gmark_large_document::FileSource::open(&path).expect("million-line Source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.simulate_resize(size(px(960.0), px(640.0)));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("million-line large view");

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.jump_bottom_for_test(window, cx)
        });
    });
    visual.run_until_parked();
    redraw(visual);

    let (origin, window_len, total_lines) =
        large_view.read_with(visual, |view, _cx| view.source_list_window_for_test());
    assert!(
        origin > 0,
        "a million lines must use a non-zero local origin: total={total_lines}, window={window_len}"
    );
    assert_eq!(window_len, crate::large_file::SOURCE_LIST_WINDOW_ROWS);
    let last = visual
        .debug_bounds("large-file-line-body-999999")
        .expect("last global Source line");
    let previous = visual
        .debug_bounds("large-file-line-body-999998")
        .expect("previous global Source line");
    let row_height =
        large_view.read_with(visual, |view, _cx| view.source_row_height_for_test());
    assert!(
        (f32::from(last.top() - previous.top()) - row_height).abs() < 0.5,
        "local scroll window must not quantize or overlap rows at the global file tail"
    );
}
