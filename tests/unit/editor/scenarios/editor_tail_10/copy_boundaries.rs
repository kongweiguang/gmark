// @author kongweiguang

#[gpui::test]
async fn large_source_character_range_copy_and_cut_use_utf8_source_anchors(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large character selection tempdir");
    let path = temp.path().join("character-selection.txt");
    fs::write(&path, "alpha\n世界🙂\n").expect("large character selection fixture");
    let probe = gmark_paged_document::probe_file(
        &path,
        gmark_paged_document::ProbeOptions {
            max_resident_bytes: 1,
            ..gmark_paged_document::ProbeOptions::default()
        },
    )
    .expect("large character selection probe");
    let source =
        gmark_paged_document::FileSource::open(&path).expect("large character selection source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("large character selection view");

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            // `世界` occupies bytes 6..12; direction is independent from normalized range.
            view.select_source_range_for_test(6..12, true);
            view.copy_for_test(window, cx);
        });
    });
    visual.run_until_parked();
    assert_eq!(
        visual.read_from_clipboard().and_then(|item| item.text()),
        Some("世界".to_owned())
    );

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.select_source_range_for_test(6..12, false);
            view.cut_for_test(window, cx);
        });
    });
    visual.run_until_parked();
    assert_eq!(
        visual.read_from_clipboard().and_then(|item| item.text()),
        Some("世界".to_owned())
    );
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some("alpha\n🙂\n".as_bytes().to_vec())
    );
}

#[gpui::test]
async fn large_source_copy_preserves_crlf_combining_and_zwj_boundaries(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large Unicode boundary tempdir");
    let path = temp.path().join("unicode-boundaries.txt");
    let text = "alpha\r\ne\u{301} 👩‍👩‍👧‍👦\r\n";
    fs::write(&path, text).expect("large Unicode boundary fixture");
    let probe = gmark_paged_document::probe_file(
        &path,
        gmark_paged_document::ProbeOptions {
            max_resident_bytes: 1,
            ..gmark_paged_document::ProbeOptions::default()
        },
    )
    .expect("large Unicode boundary probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("Unicode boundary source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("large Unicode boundary view");

    let samples = ["\r\n", "e\u{301}", "👩‍👩‍👧‍👦"];
    for sample in samples {
        let start = text.find(sample).expect("sample offset") as u64;
        let range = start..start + sample.len() as u64;
        visual.update(|window, cx| {
            large_view.update(cx, |view, cx| {
                view.select_source_range_for_test(range, false);
                view.copy_for_test(window, cx);
            });
        });
        visual.run_until_parked();
        assert_eq!(
            visual.read_from_clipboard().and_then(|item| item.text()),
            Some(sample.to_owned())
        );
    }
}

#[gpui::test]
async fn large_source_selection_export_preserves_original_encoding_or_explicit_utf8(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large encoded export tempdir");
    let path = temp.path().join("encoded-selection.txt");
    let mut encoded = vec![0xff, 0xfe];
    for unit in "alpha\n世界\nomega\n".encode_utf16() {
        encoded.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&path, encoded).expect("UTF-16LE source fixture");
    let probe = gmark_paged_document::probe_file(
        &path,
        gmark_paged_document::ProbeOptions {
            max_resident_bytes: 1,
            ..gmark_paged_document::ProbeOptions::default()
        },
    )
    .expect("UTF-16LE source probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("UTF-16LE source");
    let editor_path = path.clone();
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, editor_path, probe, source)
    });
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("large encoded export view");
    large_view.update(visual, |view, _cx| {
        // Source anchors always address the normalized UTF-8 shadow.
        view.select_source_range_for_test(6..12, false);
    });
    let selection_before =
        large_view.read_with(visual, |view, _cx| view.source_selection_for_test());

    let original_path = temp.path().join("selection-original.txt");
    let original_encoding = large_view
        .read_with(visual, |view, _cx| {
            view.export_selection_to_path_for_test(&original_path, false)
        })
        .expect("original-encoding export");
    assert_eq!(original_encoding, "UTF-16LE");
    let original = fs::read(&original_path).expect("read original-encoding export");
    assert_eq!(&original[..2], &[0xff, 0xfe]);
    let units = original[2..]
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    assert_eq!(
        String::from_utf16(&units).expect("decode UTF-16LE export"),
        "世界"
    );

    let utf8_path = temp.path().join("selection-utf8.txt");
    let utf8_encoding = large_view
        .read_with(visual, |view, _cx| {
            view.export_selection_to_path_for_test(&utf8_path, true)
        })
        .expect("explicit UTF-8 export");
    assert_eq!(utf8_encoding, "UTF-8");
    assert_eq!(
        fs::read(&utf8_path).expect("read UTF-8 export"),
        "世界".as_bytes()
    );
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.source_selection_for_test()),
        selection_before
    );
    assert!(!editor.read_with(visual, |editor, _cx| editor.document_dirty));
}

#[gpui::test]
async fn large_source_clipboard_enforces_the_exact_64_mib_boundary(cx: &mut TestAppContext) {
    use std::io::Write as _;

    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("clipboard boundary tempdir");
    let path = temp.path().join("clipboard-boundary.txt");
    let clipboard_limit = gmark_paged_document::MAX_SYSTEM_CLIPBOARD_BYTES;
    let mut file = fs::File::create(&path).expect("clipboard boundary fixture");
    let chunk = vec![b'a'; 1024 * 1024];
    for _ in 0..64 {
        file.write_all(&chunk).expect("write clipboard fixture");
    }
    file.write_all(b"b").expect("write over-limit byte");
    file.sync_all().expect("sync clipboard fixture");
    drop(file);

    let probe = gmark_paged_document::probe_file(
        &path,
        gmark_paged_document::ProbeOptions {
            max_resident_bytes: 1,
            ..gmark_paged_document::ProbeOptions::default()
        },
    )
    .expect("clipboard boundary probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("clipboard boundary source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("clipboard boundary view");

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.select_source_range_for_test(0..clipboard_limit, false);
            view.copy_for_test(window, cx);
        });
    });
    visual.run_until_parked();
    let copied = visual
        .read_from_clipboard()
        .and_then(|item| item.text())
        .expect("64 MiB clipboard text");
    assert_eq!(copied.len() as u64, clipboard_limit);
    assert!(copied.bytes().all(|byte| byte == b'a'));

    visual.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".to_owned()));
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.select_source_range_for_test(0..clipboard_limit + 1, false);
            view.copy_for_test(window, cx);
        });
    });
    visual.run_until_parked();
    assert_eq!(
        visual.read_from_clipboard().and_then(|item| item.text()),
        Some("sentinel".to_owned())
    );
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.error_for_test())
            .is_some_and(|error| error.contains("64 MiB"))
    );
    assert!(!editor.read_with(visual, |editor, _cx| editor.document_dirty));
}
