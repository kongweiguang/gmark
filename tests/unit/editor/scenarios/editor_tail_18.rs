// @author kongweiguang

#[test]
fn resident_editor_selection_is_owned_by_the_unified_document_session() {
    let mut document = super::document_session::EditorDocumentSession::new(
        gmark_document::SourceDocument::new("alpha\nbeta"),
    );
    let selection = gmark_paged_document::SourceSelection::from_range(2..8, true);

    document.sync_source_selection(selection);

    assert_eq!(document.source_selection(), selection);
}

#[gpui::test]
async fn csv_preview_and_live_tables_expose_draggable_vertical_scrollbars(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("CSV scrollbar tempdir");
    let path = temp.path().join("scrollbars.csv");
    let rows = (0..80)
        .map(|row| format!("row-{row},{row}"))
        .collect::<Vec<_>>()
        .join("\r\n");
    fs::write(&path, format!("name,value\r\n{rows}\r\n")).expect("CSV scrollbar fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("CSV scrollbar probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("CSV scrollbar source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.simulate_resize(size(px(960.0), px(420.0)));
    visual.run_until_parked();
    redraw(visual);

    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Preview));
    let preview_track = visual
        .debug_bounds("document-host-structured-scrollbar")
        .expect("CSV Preview vertical scrollbar");
    assert!(
        visual
            .debug_bounds("document-host-structured-scrollbar-thumb")
            .is_some()
    );
    visual.simulate_click(
        point(preview_track.center().x, preview_track.bottom() - px(2.0)),
        Modifiers::default(),
    );
    visual.run_until_parked();
    redraw(visual);
    let document_host = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("CSV DocumentHost");
    assert_eq!(
        document_host.read_with(visual, |view, _cx| view.document_view_ids_for_test()),
        Some((
            "delimited-table".to_owned(),
            Some("delimited-table".to_owned())
        ))
    );
    let preview_top_row = document_host.read_with(visual, |view, _cx| {
        view.structured_scroll_top_row_for_test()
    });
    assert!(
        preview_top_row > 0,
        "clicking the Preview scrollbar must move beyond the first rows; top row={preview_top_row}"
    );

    editor.update(visual, |editor, cx| {
        editor.set_view_mode(ViewMode::Rendered, cx)
    });
    visual.run_until_parked();
    redraw(visual);
    assert!(
        visual
            .debug_bounds("document-host-structured-scrollbar")
            .is_some(),
        "CSV Live editing must keep the vertical scrollbar"
    );
    editor.update(visual, |editor, cx| editor.set_view_mode(ViewMode::Source, cx));
    visual.run_until_parked();
    assert_eq!(
        document_host.read_with(visual, |view, _cx| view.document_view_ids_for_test()),
        Some(("source".to_owned(), Some("source".to_owned())))
    );
}
