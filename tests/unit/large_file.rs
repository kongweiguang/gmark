// @author kongweiguang

use std::fs;

use super::*;

fn open_document(text: &str) -> (tempfile::TempDir, LargeDocumentAdapter) {
    let temp = tempfile::tempdir().expect("bounded line tempdir");
    let path = temp.path().join("long-line.txt");
    fs::write(&path, text).expect("bounded line fixture");
    let source = FileSource::open(&path).expect("bounded line source");
    let index = LineIndex::build(&source).expect("bounded line index");
    let document = PieceDocument::open(source, index)
        .expect("bounded line document")
        .into();
    (temp, document)
}

#[test]
fn far_single_line_window_is_bounded_and_keeps_search_anchor_visible() {
    let target = MAX_RENDERED_LINE_BYTES + 12_345;
    let mut text = "a".repeat(target as usize);
    text.push_str("NEEDLE");
    text.push_str(&"z".repeat(MAX_RENDERED_LINE_BYTES as usize));
    text.push('\n');
    let (_temp, document) = open_document(&text);
    let start = source_window_start_for_anchor(text.len() as u64, target);
    let window = read_bounded_line_window(&document, 0, start)
        .expect("bounded read")
        .expect("first line");

    assert!(window.content_range.start > 0);
    assert!(window.content_range.end - window.content_range.start <= MAX_RENDERED_LINE_BYTES);
    assert!(window.content_range.contains(&target));
    assert!(window.text.contains("NEEDLE"));
    assert!(window.leading_truncated && window.trailing_truncated);
    let rendered = rendered_line_window_text(&window, false);
    assert!(rendered.starts_with("… ") && rendered.ends_with(" …"));
}

#[test]
fn source_list_window_keeps_f32_scroll_height_bounded_at_tens_of_millions_of_lines() {
    let total = 24_412_160;
    let last = total - 1;
    let origin = source_list_origin_for_target(total, last);
    assert_eq!(origin, total - SOURCE_LIST_WINDOW_ROWS);
    assert_eq!(last - origin, SOURCE_LIST_WINDOW_ROWS - 1);
    assert_eq!(source_list_origin_for_target(total, 0), 0);
    assert!(SOURCE_LIST_WINDOW_ROWS as f32 * FALLBACK_SOURCE_ROW_HEIGHT < 2_f32.powi(22));
}

#[test]
fn global_scrollbar_pointer_uses_line_space_instead_of_giant_pixel_offsets() {
    let top = px(100.0);
    assert_eq!(
        source_line_from_scrollbar_pointer(px(100.0), top, 800.0, 28.0, 24_000_000),
        0
    );
    assert_eq!(
        source_line_from_scrollbar_pointer(px(900.0), top, 800.0, 28.0, 24_000_000),
        24_000_000
    );
}

#[test]
fn bounded_edit_replaces_only_the_visible_window_and_preserves_suffixes() {
    let target = MAX_RENDERED_LINE_BYTES + 4_096;
    let mut original = "p".repeat(target as usize);
    original.push_str("NEEDLE");
    original.push_str(&"s".repeat(MAX_RENDERED_LINE_BYTES as usize));
    let (_temp, mut document) = open_document(&original);
    let start = source_window_start_for_anchor(original.len() as u64, target);
    let window = read_bounded_line_window(&document, 0, start)
        .expect("bounded read")
        .expect("first line");
    let local = window.text.find("NEEDLE").expect("target in window");
    let mut replacement = window.text.to_string();
    replacement.replace_range(local..local + "NEEDLE".len(), "EDITED");
    replacement.push_str(&window.ending);

    document
        .replace_text(window.replace_range, &replacement)
        .expect("bounded edit");
    let mut expected = original;
    expected.replace_range(target as usize..target as usize + 6, "EDITED");
    assert_eq!(
        document.read_range(0..document.len()).expect("edited text"),
        expected.as_bytes()
    );
}

#[test]
fn bounded_window_never_starts_inside_utf8_codepoint() {
    let prefix = "a".repeat(MAX_RENDERED_LINE_BYTES as usize + 7);
    let unicode_start = prefix.len() as u64;
    let text = format!(
        "{prefix}中文{}",
        "b".repeat(MAX_RENDERED_LINE_BYTES as usize)
    );
    let (_temp, document) = open_document(&text);
    let window = read_bounded_line_window(&document, 0, unicode_start + 1)
        .expect("bounded read")
        .expect("first line");

    assert!(window.content_range.start >= unicode_start + '中'.len_utf8() as u64);
    assert!(std::str::from_utf8(window.text.as_bytes()).is_ok());
    assert!(window.text.len() <= MAX_RENDERED_LINE_BYTES as usize);
}

#[test]
fn unicode_crlf_windows_are_always_valid_edit_transactions() {
    let unit = "中😀e\u{301}かな";
    let first_line = unit.repeat(12_000);
    let text = format!("{first_line}\r\n尾行\r\n");
    let (_temp, document) = open_document(&text);
    let requested_starts = [
        0,
        1,
        2,
        3,
        MAX_RENDERED_LINE_BYTES.saturating_sub(3),
        MAX_RENDERED_LINE_BYTES,
        first_line.len().saturating_sub(1) as u64,
    ];

    for requested_start in requested_starts {
        let window = read_bounded_line_window(&document, 0, requested_start)
            .expect("Unicode bounded read")
            .expect("first Unicode line");
        assert!(std::str::from_utf8(window.text.as_bytes()).is_ok());
        let mut replacement = window.text.to_string();
        replacement.push('✓');
        replacement.push_str(&window.ending);
        let mut candidate = document.clone();
        candidate
            .replace_text(window.replace_range.clone(), &replacement)
            .unwrap_or_else(|error| {
                panic!(
                    "window {:?} from requested byte {requested_start} must be editable: {error}",
                    window.replace_range
                )
            });
    }
}

#[test]
fn provisional_utf16_viewport_decodes_before_shadow_index_is_ready() {
    let temp = tempfile::tempdir().expect("UTF-16 viewport tempdir");
    let path = temp.path().join("large-utf16.txt");
    let mut bytes = vec![0xff, 0xfe];
    for unit in "alpha\n世界\nomega\n".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&path, bytes).expect("UTF-16 fixture");
    let source = FileSource::open(&path).expect("UTF-16 source");
    let rows = read_provisional_source_rows(
        &source,
        3,
        0..3,
        0,
        &TextEncoding::Utf16Le,
        &SearchCancellation::default(),
    )
    .expect("provisional viewport");

    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].1.text.as_ref(), "alpha");
    assert!(rows.iter().all(|(_, row)| !row.text.contains('\0')));
    assert!(rows.iter().all(|(_, row)| !row.text.is_empty()));
}

#[test]
fn screen_lines_top_anchor_tracks_the_visible_source_row() {
    let mut rows = BTreeMap::new();
    rows.insert(
        42,
        Arc::new(BoundedLineWindow::new(
            12_345..12_350,
            12_345..12_351,
            "alpha".to_owned(),
            "\n".to_owned(),
            false,
            false,
        )),
    );
    let screen = ScreenLines {
        visible: 42..43,
        rows: Arc::new(rows),
        ..ScreenLines::default()
    };

    assert_eq!(
        screen.top_source_anchor(),
        Some(SourceAnchor::new(12_345, SourceAffinity::Before))
    );
}

#[gpui::test]
async fn provisional_source_row_is_read_only_until_piece_document_is_installed(
    cx: &mut gpui::TestAppContext,
) {
    let temp = tempfile::tempdir().expect("provisional Source tempdir");
    let path = temp.path().join("provisional.txt");
    fs::write(&path, "alpha\nbeta\n").expect("provisional Source fixture");
    let source = FileSource::open(&path).expect("provisional Source handle");
    let probe =
        gmark_large_document::probe_file(&path, gmark_large_document::ProbeOptions::default())
            .expect("provisional Source probe");
    let view = cx.new(|cx| DiskSourceAdapter::new(path.clone(), probe, source, cx));

    let provisional_block = view.update(cx, |view, cx| {
        view.suspend_for_closed_tab();
        view.displayed_screen_lines = Arc::new(ScreenLines {
            visible: 0..1,
            rows: Arc::new(BTreeMap::from([(
                0,
                Arc::new(BoundedLineWindow::new(
                    0..5,
                    0..6,
                    "alpha".to_owned(),
                    "\n".to_owned(),
                    false,
                    false,
                )),
            )])),
            ..ScreenLines::default()
        });
        view.ensure_source_row_block(0, cx)
            .expect("provisional Source block")
    });
    assert!(provisional_block.read_with(cx, |block, _cx| block.is_read_only()));

    let editable_block = view.update(cx, |view, cx| {
        let source = FileSource::open(&path).expect("exact Source handle");
        let index = LineIndex::build(&source).expect("exact Source index");
        view.document = Some(
            PieceDocument::open(source, index)
                .expect("exact PieceTree document")
                .into(),
        );
        view.ensure_source_row_block(0, cx)
            .expect("editable Source block")
    });

    assert_eq!(
        provisional_block.entity_id(),
        editable_block.entity_id(),
        "the visible row entity must survive provisional-to-exact installation"
    );
    assert!(!editable_block.read_with(cx, |block, _cx| block.is_read_only()));
}

#[test]
fn logical_horizontal_offsets_clamp_at_both_edges() {
    assert_eq!(shift_source_window_start(10, -50, 1_000), 0);
    assert_eq!(shift_source_window_start(900, 500, 1_000), 1_000);
    assert_eq!(source_window_start_for_anchor(10_000, 9_000), 0);
    let long = MAX_RENDERED_LINE_BYTES * 4;
    let anchored = source_window_start_for_anchor(long, long - 10);
    assert!(anchored <= long - MAX_RENDERED_LINE_BYTES);
    assert!(long - 10 >= anchored && long - 10 <= anchored + MAX_RENDERED_LINE_BYTES);
}

#[test]
fn bounded_row_reuses_plain_text_and_builds_line_endings_lazily() {
    let row = BoundedLineWindow::new(
        0..5,
        0..6,
        "alpha".to_owned(),
        "\n".to_owned(),
        false,
        false,
    );

    assert_eq!(row.text.as_ptr(), row.display.as_ptr());
    assert!(row.display_with_endings.get().is_none());
    assert_eq!(row.rendered(false).as_ref(), "alpha");
    assert!(row.display_with_endings.get().is_none());
    assert_eq!(row.rendered(true).as_ref(), "alpha␊");
    assert!(row.display_with_endings.get().is_some());
}

#[test]
fn screen_lines_retain_the_previous_frame_only_for_a_fully_disjoint_jump() {
    let rows = BTreeMap::from([
        (
            10,
            Arc::new(BoundedLineWindow::new(
                100..105,
                100..106,
                "alpha".to_owned(),
                "\n".to_owned(),
                false,
                false,
            )),
        ),
        (
            11,
            Arc::new(BoundedLineWindow::new(
                106..110,
                106..111,
                "beta".to_owned(),
                "\n".to_owned(),
                false,
                false,
            )),
        ),
    ]);
    let screen = ScreenLines {
        visible: 10..12,
        rows: Arc::new(rows),
        ..ScreenLines::default()
    };

    let requested = 100..102;
    let retain = screen.should_retain_previous_frame(&requested);
    assert!(retain);
    let retained = screen.retained_rows(false);
    let (display_line, row) = retained.first().expect("retained first old row");
    assert_eq!(*display_line, 10);
    assert_eq!(row.as_ref(), "alpha");

    let mut mixed = (*screen.rows).clone();
    mixed.insert(
        100,
        Arc::new(BoundedLineWindow::new(
            1_000..1_003,
            1_000..1_004,
            "new".to_owned(),
            "\n".to_owned(),
            false,
            false,
        )),
    );
    let mixed = ScreenLines {
        rows: Arc::new(mixed),
        ..screen
    };
    let retain = mixed.should_retain_previous_frame(&requested);
    assert!(!retain);
}

#[test]
fn built_in_derived_descriptors_publish_their_resource_limits() {
    let json = RegisteredStructuredProvider::for_format(&DocumentFormat::Json)
        .expect("JSON derived provider");
    assert_eq!(
        json.descriptor.max_items,
        Some(DEFAULT_JSON_GRAPH_NODE_LIMIT)
    );

    let delimited =
        RegisteredStructuredProvider::for_format(&DocumentFormat::Delimited { delimiter: b',' })
            .expect("delimited derived provider");
    assert_eq!(
        delimited.descriptor.max_items,
        Some(DEFAULT_DELIMITED_ROW_WINDOW * DEFAULT_DELIMITED_COLUMN_WINDOW)
    );
}
