// @author kongweiguang

#[gpui::test]
async fn large_external_truncation_reload_replaces_the_clean_baseline(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large reload tempdir");
    let path = temp.path().join("reload.txt");
    fs::write(&path, "original long content\n").expect("large reload fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large reload probe");
    let source = gmark_large_document::FileSource::open(&path).expect("large reload source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large reload view");

    fs::write(temp.path().join("reload.txt"), "new\n").expect("truncate source");
    visual
        .executor()
        .advance_clock(Duration::from_millis(1_100));
    visual.run_until_parked();
    redraw(visual);
    assert!(matches!(
        large_view.read_with(visual, |view, _cx| view.pending_external_change_for_test()),
        Some(gmark_large_document::ExternalChange::Truncated { .. })
            | Some(gmark_large_document::ExternalChange::Modified)
    ));
    assert!(
        visual
            .debug_bounds("large-file-external-change-banner")
            .is_some()
    );

    visual.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".to_owned()));
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.select_source_range_for_test(0.."original long content\n".len() as u64, false);
            view.copy_for_test(window, cx);
            view.reload_from_disk_for_test(window, cx);
        });
    });
    visual.run_until_parked();
    redraw(visual);
    assert_eq!(
        visual.read_from_clipboard().and_then(|item| item.text()),
        Some("sentinel".to_owned()),
        "external identity reload must cancel an in-flight copy"
    );
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some(b"new\n".to_vec())
    );
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.pending_external_change_for_test())
            .is_none()
    );
    assert!(!editor.read_with(visual, |editor, _cx| editor.document_dirty));
}

#[gpui::test]
async fn large_source_copy_reads_the_selection_snapshot_off_the_ui_thread(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large copy tempdir");
    let path = temp.path().join("copy.txt");
    fs::write(&path, "alpha\n世界\n").expect("large copy fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large copy probe");
    let source = gmark_large_document::FileSource::open(&path).expect("large copy source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large copy view");

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.select_lines_for_test(0..2);
            view.copy_for_test(window, cx);
        });
    });
    visual.run_until_parked();

    assert_eq!(
        visual.read_from_clipboard().and_then(|item| item.text()),
        Some("alpha\n世界\n".to_owned())
    );
}

#[gpui::test]
async fn large_source_copy_keeps_command_snapshot_while_the_document_changes(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large copy snapshot tempdir");
    let path = temp.path().join("copy-snapshot.txt");
    fs::write(&path, "alpha\nbeta\n").expect("large copy snapshot fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large copy snapshot probe");
    let source = gmark_large_document::FileSource::open(&path).expect("large copy snapshot source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large copy snapshot view");

    visual.write_to_clipboard(gpui::ClipboardItem::new_string("updated".to_owned()));
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.select_source_range_for_test(0..5, false);
            view.copy_for_test(window, cx);
            // 后台 worker 尚未获得执行机会；同一 UI transaction 立即生成新的 PieceTree 根。
            view.select_source_range_for_test(0..5, false);
            view.paste_for_test(window, cx);
        });
    });
    visual.run_until_parked();

    assert_eq!(
        visual.read_from_clipboard().and_then(|item| item.text()),
        Some("alpha".to_owned())
    );
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some(b"updated\nbeta\n".to_vec())
    );
}

#[gpui::test]
async fn switching_tabs_keeps_large_source_copy_snapshot_and_source_state(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large copy tab-switch tempdir");
    let path = temp.path().join("copy-tab-switch.txt");
    fs::write(&path, "alpha\nbeta\n").expect("large copy tab-switch fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large copy tab-switch probe");
    let source =
        gmark_large_document::FileSource::open(&path).expect("large copy tab-switch source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large copy tab-switch view");
    visual.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".to_owned()));

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            large_view.update(cx, |view, cx| {
                view.select_source_range_for_test(0..5, true);
                view.copy_for_test(window, cx);
            });
            // 切换标签只转移实体 owner，不等同于关闭；命令触发时的不可变快照必须完成。
            editor.on_new_tab_action(&crate::components::NewTab, window, cx);
        });
    });
    visual.run_until_parked();

    assert_eq!(
        visual.read_from_clipboard().and_then(|item| item.text()),
        Some("alpha".to_owned())
    );
    assert!(editor.read_with(visual, |editor, _cx| editor.source_surface.is_none()));
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_previous_tab_action(&crate::components::PreviousTab, window, cx);
        });
    });
    visual.run_until_parked();
    assert!(editor.read_with(visual, |editor, _cx| {
        editor
            .source_surface
            .as_ref()
            .is_some_and(|restored| *restored == large_view)
    }));
    let selection = large_view
        .read_with(visual, |view, _cx| view.source_selection_for_test())
        .expect("restored large Source selection");
    assert_eq!(selection.range(), 0..5);
    assert!(selection.reversed());
}

#[gpui::test]
async fn closing_large_tab_cancels_copy_and_reopen_resumes_background_lifetime(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large close cancellation tempdir");
    let path = temp.path().join("close-copy.txt");
    fs::write(&path, "alpha\nbeta\n").expect("large close cancellation fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large close cancellation probe");
    let source = gmark_large_document::FileSource::open(&path).expect("large close source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("active large close view");
    visual.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".to_owned()));

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            large_view.update(cx, |view, cx| {
                view.select_source_range_for_test(0..5, false);
                view.copy_for_test(window, cx);
            });
            editor.request_close_tab_index(0, cx);
        });
    });
    visual.run_until_parked();

    assert_eq!(
        visual.read_from_clipboard().and_then(|item| item.text()),
        Some("sentinel".to_owned()),
        "a closed tab must not complete an old clipboard write"
    );
    assert!(large_view.read_with(visual, |view, _cx| {
        view.is_closed_suspended_for_test()
    }));

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_reopen_closed_tab_action(
                &crate::components::ReopenClosedTab,
                window,
                cx,
            );
        });
    });
    visual.run_until_parked();
    assert!(!large_view.read_with(visual, |view, _cx| {
        view.is_closed_suspended_for_test()
    }));
    assert!(editor.read_with(visual, |editor, _cx| editor.source_surface.is_some()));
}

#[gpui::test]
async fn reopening_large_tab_restarts_an_index_cancelled_before_first_snapshot(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large provisional reopen tempdir");
    let path = temp.path().join("provisional-reopen.txt");
    let text = "alpha\n世界🙂\nomega\n";
    fs::write(&path, text).expect("large provisional reopen fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large provisional reopen probe");
    let source = gmark_large_document::FileSource::open(&path).expect("large provisional source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("provisional large view");

    // 不运行 executor，确保初次索引仍在 pending；关闭必须取消旧 worker，重开必须另起代次。
    editor.update(visual, |editor, cx| editor.request_close_tab_index(0, cx));
    assert!(large_view.read_with(visual, |view, _cx| {
        view.is_closed_suspended_for_test()
    }));
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_reopen_closed_tab_action(
                &crate::components::ReopenClosedTab,
                window,
                cx,
            );
        });
    });
    visual.run_until_parked();

    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some(text.as_bytes().to_vec())
    );
    assert!(!large_view.read_with(visual, |view, _cx| {
        view.is_closed_suspended_for_test()
    }));
}

#[gpui::test]
async fn large_source_character_range_copy_and_cut_use_utf8_source_anchors(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large character selection tempdir");
    let path = temp.path().join("character-selection.txt");
    fs::write(&path, "alpha\n世界🙂\n").expect("large character selection fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large character selection probe");
    let source =
        gmark_large_document::FileSource::open(&path).expect("large character selection source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
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
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large Unicode boundary probe");
    let source = gmark_large_document::FileSource::open(&path).expect("Unicode boundary source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
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
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("UTF-16LE source probe");
    let source = gmark_large_document::FileSource::open(&path).expect("UTF-16LE source");
    let editor_path = path.clone();
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_large_file(cx, editor_path, probe, source)
    });
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large encoded export view");
    large_view.update(visual, |view, _cx| {
        // SourceSurface anchors always address the normalized UTF-8 shadow.
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
    let clipboard_limit = gmark_large_document::MAX_SYSTEM_CLIPBOARD_BYTES;
    let mut file = fs::File::create(&path).expect("clipboard boundary fixture");
    let chunk = vec![b'a'; 1024 * 1024];
    for _ in 0..64 {
        file.write_all(&chunk).expect("write clipboard fixture");
    }
    file.write_all(b"b").expect("write over-limit byte");
    file.sync_all().expect("sync clipboard fixture");
    drop(file);

    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("clipboard boundary probe");
    let source = gmark_large_document::FileSource::open(&path).expect("clipboard boundary source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
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

#[gpui::test]
async fn large_source_shaped_layout_cache_reuses_and_invalidates_complete_keys(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large layout cache tempdir");
    let path = temp.path().join("layout-cache.txt");
    fs::write(&path, "alpha\n世界🙂\nomega\n".repeat(128)).expect("layout cache fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("layout cache probe");
    let source = gmark_large_document::FileSource::open(&path).expect("layout cache source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large layout cache view");
    let (initial_hits, initial_misses, initial_entries) = large_view
        .read_with(visual, |view, cx| {
            view.source_layout_cache_metrics_for_test(cx)
        });
    assert!(initial_misses > 0);
    assert!((1..=512).contains(&initial_entries));

    visual.update(|_window, cx| cx.refresh_windows());
    redraw(visual);
    let (reused_hits, reused_misses, reused_entries) = large_view.read_with(visual, |view, cx| {
        view.source_layout_cache_metrics_for_test(cx)
    });
    assert!(reused_hits > initial_hits);
    assert_eq!(reused_misses, initial_misses);
    assert_eq!(reused_entries, initial_entries);

    visual.update(|_window, cx| {
        assert!(cx.update_global::<ThemeManager, _>(|manager, _cx| {
            manager.set_theme_by_id("gmark-light")
        }));
        cx.refresh_windows();
    });
    redraw(visual);
    let (_theme_hits, theme_misses, theme_entries) = large_view.read_with(visual, |view, cx| {
        view.source_layout_cache_metrics_for_test(cx)
    });
    assert!(theme_misses > reused_misses);
    assert_eq!(theme_entries, reused_entries);
}

#[gpui::test]
async fn large_source_pointer_selection_is_character_precise_cross_line_and_reversible(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large pointer selection tempdir");
    let path = temp.path().join("pointer-selection.txt");
    fs::write(&path, "alpha bravo\n世界🙂 tail\nthird line\n")
        .expect("large pointer selection fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large pointer selection probe");
    let source =
        gmark_large_document::FileSource::open(&path).expect("large pointer selection source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large pointer selection view");

    let first = visual
        .debug_bounds("large-file-line-body-0")
        .expect("first source row bounds");
    let third = visual
        .debug_bounds("large-file-line-body-2")
        .expect("third source row bounds");
    let forward_start = point(first.left() + px(28.0), first.center().y);
    let forward_end = point(third.left() + px(42.0), third.center().y);
    visual.simulate_mouse_down(forward_start, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(forward_end, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(forward_end, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();

    let forward = large_view
        .read_with(visual, |view, _cx| view.source_selection_for_test())
        .expect("forward source selection");
    assert!(!forward.reversed());
    assert!(forward.range().start > 0 && forward.range().start < 11);
    assert!(forward.range().end > 28 && forward.range().end < 38);

    visual.simulate_mouse_down(forward_end, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(forward_start, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(forward_start, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    let reversed = large_view
        .read_with(visual, |view, _cx| view.source_selection_for_test())
        .expect("reversed source selection");
    assert!(reversed.reversed());
    assert!(reversed.range().start > 0 && reversed.range().start < 11);
    assert!(reversed.range().end > 28 && reversed.range().end < 38);
    assert!(large_view.read_with(visual, |view, _cx| {
        view.source_row_block_count_for_test() <= 512
    }));
    let (_revision, generation, _epoch, column, visible, rows, epochs_match, revision_matches) =
        large_view.read_with(visual, |view, _cx| view.screen_lines_contract_for_test());
    assert!(generation > 0);
    assert_eq!(column, 0);
    assert!(!visible.is_empty());
    assert!((1..=512).contains(&rows));
    assert!(epochs_match);
    assert!(revision_matches);
    let metrics = large_view.read_with(visual, |view, _cx| view.metrics_for_test());
    assert!(metrics.viewport_requests > 0 && metrics.viewport_installs > 0);
    assert!((1..=512).contains(&metrics.max_cached_rows));
    assert_eq!(metrics.blank_frames_after_content, 0);

    visual.simulate_mouse_down(forward_start, MouseButton::Right, Modifiers::default());
    visual.simulate_mouse_up(forward_start, MouseButton::Right, Modifiers::default());
    assert!(large_view.read_with(visual, |view, _cx| {
        view.source_context_menu_open_for_test()
    }));
    redraw(visual);
    assert!(
        visual
            .debug_bounds("large-file-source-context-menu")
            .is_some()
    );
    assert!(
        visual
            .debug_bounds("large-source-context-export-utf8")
            .is_some()
    );
    visual.update(|window, cx| {
        assert!(
            large_view
                .read(cx)
                .source_context_menu_is_focused_for_test(window)
        );
    });
    visual.simulate_keystrokes("escape");
    redraw(visual);
    assert!(!large_view.read_with(visual, |view, _cx| {
        view.source_context_menu_open_for_test()
    }));
    visual.update(|window, cx| {
        assert!(large_view.read(cx).host_is_focused_for_test(window));
    });
}

#[gpui::test]
async fn large_source_drag_autoscroll_extends_selection_beyond_mounted_viewport(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large drag autoscroll tempdir");
    let path = temp.path().join("drag-autoscroll.txt");
    let text = (0..400)
        .map(|line| format!("source line {line:04} with selectable text\n"))
        .collect::<String>();
    fs::write(&path, text).expect("large drag autoscroll fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large drag autoscroll probe");
    let source = gmark_large_document::FileSource::open(&path).expect("drag autoscroll source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.simulate_resize(size(px(720.0), px(520.0)));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large drag autoscroll view");
    let first = visual
        .debug_bounds("large-file-line-body-0")
        .expect("first source row");
    let viewport = visual
        .debug_bounds("large-file-source-horizontal-scroll")
        .expect("source viewport");
    let start = point(first.left() + px(8.0), first.center().y);
    let edge = point(first.left() + px(48.0), viewport.bottom() - px(2.0));
    visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    large_view.update(visual, |view, cx| {
        view.start_drag_autoscroll_for_test(1, cx);
    });

    for _ in 0..24 {
        large_view.update(visual, |view, cx| {
            assert!(view.drag_autoscroll_tick_for_test(cx));
        });
        visual.run_until_parked();
        redraw(visual);
    }
    visual.simulate_mouse_up(edge, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();

    assert!(large_view.read_with(visual, |view, _cx| view.scroll_top_line_for_test()) > 0);
    let selection = large_view
        .read_with(visual, |view, _cx| view.source_selection_for_test())
        .expect("autoscroll Source selection");
    assert!(selection.range().end > 200, "selection={selection:?}");
}

#[gpui::test]
async fn large_source_ime_composition_commits_one_piece_tree_undo_transaction(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large IME tempdir");
    let path = temp.path().join("ime-source.txt");
    fs::write(&path, "alpha\n").expect("large IME fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large IME probe");
    let source = gmark_large_document::FileSource::open(&path).expect("large IME source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large IME view");
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
    });
    redraw(visual);
    let block = large_view
        .read_with(visual, |view, _cx| view.active_edit_for_test())
        .expect("active large source block")
        .1;

    visual.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            let composing = "拼音🙂";
            let utf16_end = composing.encode_utf16().count();
            <crate::components::Block as EntityInputHandler>::replace_and_mark_text_in_range(
                block,
                None,
                composing,
                Some(utf16_end..utf16_end),
                window,
                block_cx,
            );
        });
    });
    visual.run_until_parked();
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some(b"alpha\n".to_vec()),
        "marked text must stay transient until IME commit"
    );

    visual.update(|window, cx| {
        block.update(cx, |block, block_cx| {
            <crate::components::Block as EntityInputHandler>::replace_text_in_range(
                block,
                None,
                "中文🙂",
                window,
                block_cx,
            );
        });
    });
    visual.run_until_parked();
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some("alpha中文🙂\n".as_bytes().to_vec())
    );

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.undo_for_test(window, cx));
    });
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some(b"alpha\n".to_vec())
    );
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.redo_for_test(window, cx));
    });
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some("alpha中文🙂\n".as_bytes().to_vec())
    );
}

#[gpui::test]
async fn large_source_cross_line_paste_is_one_reversible_source_transaction(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large paste tempdir");
    let path = temp.path().join("paste-source.txt");
    fs::write(&path, "alpha\nbeta\ngamma\n").expect("large paste fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large paste probe");
    let source = gmark_large_document::FileSource::open(&path).expect("large paste source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large paste view");
    visual.write_to_clipboard(gpui::ClipboardItem::new_string("中\n🙂".to_owned()));
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.select_source_range_for_test(3..13, true);
            view.paste_for_test(window, cx);
        });
    });
    visual.run_until_parked();
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some("alp中\n🙂mma\n".as_bytes().to_vec())
    );
    let pasted_selection = large_view
        .read_with(visual, |view, _cx| view.source_selection_for_test())
        .expect("selection after paste");

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.undo_for_test(window, cx));
    });
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some(b"alpha\nbeta\ngamma\n".to_vec())
    );
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.source_selection_for_test()),
        Some(gmark_large_document::SourceSelection::from_range(
            3..13,
            true
        ))
    );
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.redo_for_test(window, cx));
    });
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some("alp中\n🙂mma\n".as_bytes().to_vec())
    );
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.source_selection_for_test()),
        Some(pasted_selection)
    );
}

#[gpui::test]
async fn large_source_select_all_upgrades_from_active_line_to_lazy_document_range(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large select-all tempdir");
    let path = temp.path().join("select-all-source.txt");
    let source_text = "alpha\n世界🙂\nomega\n";
    fs::write(&path, source_text).expect("large select-all fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large select-all probe");
    let source = gmark_large_document::FileSource::open(&path).expect("large select-all source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large select-all view");
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
    });
    redraw(visual);

    visual.simulate_keystrokes("ctrl-a");
    let line_selection = large_view
        .read_with(visual, |view, _cx| view.source_selection_for_test())
        .expect("active-line selection");
    assert_eq!(line_selection.range(), 0..5);

    visual.simulate_keystrokes("ctrl-a");
    let document_selection = large_view
        .read_with(visual, |view, _cx| view.source_selection_for_test())
        .expect("whole-document selection");
    assert_eq!(document_selection.range(), 0..source_text.len() as u64);
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.active_edit_for_test())
            .is_none()
    );
}

#[gpui::test]
async fn large_recovery_replays_inside_the_standard_editor_shell(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large recovery tempdir");
    let path = temp.path().join("recovered-large.md");
    fs::write(&path, "alpha\nbeta\n").expect("large recovery source");
    let source = gmark_large_document::FileSource::open(&path).expect("recovery source");
    let mut journal = gmark_large_document::LargeRecoveryJournal::create(
        temp.path().join("recovery"),
        &source,
        gmark_large_document::TextEncoding::Utf8 { bom: false },
    )
    .expect("large recovery journal");
    journal
        .record_replace(0..5, "ALPHA", None, "source")
        .expect("recovery edit");
    let journal_path = journal.path().to_path_buf();
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large recovery probe");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_large_recovery(cx, path, probe, source, journal_path)
    });
    visual.simulate_resize(size(px(960.0), px(640.0)));
    redraw(visual);

    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large recovery view");
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some(b"ALPHA\nbeta\n".to_vec())
    );
    assert!(large_view.read_with(visual, |view, _cx| view.has_recovery_journal_for_test()));
    assert!(editor.read_with(visual, |editor, _cx| editor.document_dirty));
    assert!(visual.debug_bounds("editor-titlebar").is_some());
    assert!(visual.debug_bounds("document-tab-strip").is_some());
    assert!(visual.debug_bounds("large-document-tab-content").is_some());
    assert!(visual.debug_bounds("status-bar").is_some());
}

#[gpui::test]
async fn large_jsonl_follow_rebuilds_structure_and_reports_invalid_appended_record(
    cx: &mut TestAppContext,
) {
    use std::io::Write as _;

    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("JSONL follow tempdir");
    let path = temp.path().join("follow.jsonl");
    fs::write(&path, "{\"id\":1}\n").expect("JSONL fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("JSONL follow probe");
    let source = gmark_large_document::FileSource::open(&path).expect("JSONL follow source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("JSONL follow view");
    assert!(large_view.read_with(visual, |view, _cx| view.has_structure_view()));
    large_view.update(visual, |view, cx| view.toggle_follow(cx));

    let mut writer = fs::OpenOptions::new()
        .append(true)
        .open(temp.path().join("follow.jsonl"))
        .expect("open JSONL append");
    writer
        .write_all(b"{\"id\":2}\n")
        .expect("append valid JSONL record");
    writer.sync_all().expect("sync valid JSONL append");
    visual
        .executor()
        .advance_clock(Duration::from_millis(1_100));
    visual.run_until_parked();
    redraw(visual);
    assert!(large_view.read_with(visual, |view, _cx| view.has_structure_view()));

    let invalid_offset = fs::metadata(temp.path().join("follow.jsonl"))
        .expect("JSONL metadata")
        .len()
        + b"{\"broken\":".len() as u64;
    writer
        .write_all(b"{\"broken\":]}\n")
        .expect("append invalid JSONL record");
    writer.sync_all().expect("sync invalid JSONL append");
    visual
        .executor()
        .advance_clock(Duration::from_millis(1_100));
    visual.run_until_parked();
    redraw(visual);

    assert_eq!(
        large_view
            .read_with(visual, |view, _cx| view.structure_error_for_test())
            .and_then(|(_, offset)| offset),
        Some(invalid_offset)
    );
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.recovered_text_for_test())
            .is_some_and(|text| text.ends_with(b"{\"broken\":]}\n"))
    );
    assert!(
        visual
            .debug_bounds("large-file-structure-error-jump")
            .is_some()
    );
}

#[gpui::test]
async fn large_json_expands_child_containers_without_building_a_dom(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large json tempdir");
    let path = temp.path().join("large-tree.json");
    fs::write(&path, r#"[{"id":1}, [2, 3, {"nested":true}], "tail"]"#).expect("large json fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large json probe");
    let source = gmark_large_document::FileSource::open(&path).expect("large json source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large json view");
    assert!(large_view.read_with(visual, |view, _cx| view.source_view_for_test()));
    editor.update(visual, |editor, cx| {
        editor.set_view_mode(ViewMode::Source, cx);
    });
    assert!(!large_view.read_with(visual, |view, _cx| view.structure_view_for_test()));
    editor.update(visual, |editor, cx| {
        editor.set_view_mode(ViewMode::Rendered, cx);
    });
    assert!(large_view.read_with(visual, |view, _cx| view.source_view_for_test()));
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Source));
    large_view.update(visual, |view, cx| {
        view.show_structure_view(cx);
    });
    visual.run_until_parked();
    assert!(large_view.read_with(visual, |view, _cx| view.structure_view_for_test()));
    let (epoch, revision, generation) = large_view
        .read_with(visual, |view, _cx| view.installed_projection_for_test())
        .expect("registered projection snapshot");
    assert!(epoch > 0);
    assert_eq!(revision, 0);
    assert!(generation > 0);
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.json_visible_rows_for_test()),
        Some(3)
    );
    large_view.update(visual, |view, cx| view.expand_json_row_for_test(1, cx));
    visual.run_until_parked();
    redraw(visual);
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.json_visible_rows_for_test()),
        Some(6)
    );
    assert!(visual.debug_bounds("large-document-tab-content").is_some());

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
    });
    let (_, edit_block) = large_view
        .read_with(visual, |view, _cx| view.active_edit_for_test())
        .expect("active JSON source edit");
    edit_block.update(visual, |block, cx| {
        let end = block.display_text().len();
        block.replace_text_in_visible_range(end..end, " ", None, false, cx);
    });
    visual.run_until_parked();
    redraw(visual);
    assert!(editor.read_with(visual, |editor, _cx| editor.document_dirty));
    assert!(!large_view.read_with(visual, |view, _cx| view.has_structure_view()));

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.on_undo(&Undo, window, cx));
    });
    visual.run_until_parked();
    redraw(visual);
    assert!(!editor.read_with(visual, |editor, _cx| editor.document_dirty));
    assert!(large_view.read_with(visual, |view, _cx| view.has_structure_view()));
}

#[gpui::test]
async fn invalid_large_json_reports_the_byte_and_jumps_back_to_source(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("invalid JSON tempdir");
    let path = temp.path().join("invalid-large.json");
    let text = "{\n  \"ok\": 1,\n  \"broken\": ]\n}\n";
    fs::write(&path, text).expect("invalid JSON fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("invalid JSON probe");
    let source = gmark_large_document::FileSource::open(&path).expect("invalid JSON source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.simulate_resize(size(px(960.0), px(640.0)));
    visual.run_until_parked();
    redraw(visual);

    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("invalid JSON large view");
    let (message, byte_offset) = large_view
        .read_with(visual, |view, _cx| view.structure_error_for_test())
        .expect("structured JSON error");
    assert!(message.contains("invalid JSON near byte"));
    let byte_offset = byte_offset.expect("JSON error byte offset");
    assert!(visual.debug_bounds("large-file-structure-notice").is_some());
    let jump = visual
        .debug_bounds("large-file-structure-error-jump")
        .expect("JSON error jump action");
    visual.simulate_click(jump.center(), Modifiers::default());
    redraw(visual);

    let expected_line = text.as_bytes()[..byte_offset as usize]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1;
    assert_eq!(
        large_view.read_with(visual, |view, cx| view.cursor_position(cx).0),
        expected_line
    );
    assert!(visual.debug_bounds("editor-titlebar").is_some());
    assert!(visual.debug_bounds("status-bar").is_some());
}

#[gpui::test]
async fn invalid_large_jsonl_record_reports_global_byte_and_jumps_to_source(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("invalid JSONL tempdir");
    let path = temp.path().join("invalid-large.jsonl");
    let text = "{\"ok\":1}\n[1,2,3]\n{\"broken\":]}\n";
    fs::write(&path, text).expect("invalid JSONL fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("invalid JSONL probe");
    let source = gmark_large_document::FileSource::open(&path).expect("invalid JSONL source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.simulate_resize(size(px(960.0), px(640.0)));
    visual.run_until_parked();
    redraw(visual);

    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("invalid JSONL large view");
    let (message, byte_offset) = large_view
        .read_with(visual, |view, _cx| view.structure_error_for_test())
        .expect("structured JSONL error");
    assert!(message.contains("invalid JSON near byte"));
    let byte_offset = byte_offset.expect("JSONL error byte offset");
    assert_eq!(byte_offset, text.rfind(']').expect("invalid token") as u64);

    let jump = visual
        .debug_bounds("large-file-structure-error-jump")
        .expect("JSONL error jump action");
    visual.simulate_click(jump.center(), Modifiers::default());
    redraw(visual);
    assert_eq!(
        large_view.read_with(visual, |view, cx| view.cursor_position(cx).0),
        3
    );
    assert!(large_view.read_with(visual, |view, _cx| view.source_view_for_test()));
}

fn flush_split_projection(cx: &mut gpui::VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(30));
    cx.run_until_parked();
    redraw(cx);
}

fn activate_visual_window(cx: &mut VisualTestContext) -> AnyWindowHandle {
    cx.update(|window, _cx| window.activate_window());
    cx.run_until_parked();
    cx.cx
        .update(|cx| cx.active_window().expect("window should be active"))
}

#[test]
fn centered_column_ratio_stays_full_before_shrink_start() {
    let theme = Theme::default_theme();
    assert_eq!(
        crate::ui::centered_column_ratio(900.0, &theme.dimensions),
        1.0
    );
    assert_eq!(
        crate::ui::centered_column_ratio(theme.dimensions.centered_shrink_start, &theme.dimensions,),
        1.0
    );
}

#[test]
fn centered_column_ratio_reaches_new_minimum() {
    let theme = Theme::default_theme();
    let ratio =
        crate::ui::centered_column_ratio(theme.dimensions.centered_shrink_end, &theme.dimensions);
    assert!((ratio - 0.58).abs() < f32::EPSILON);
}

#[test]
fn centered_column_width_caps_wide_viewports_and_yields_to_compact_space() {
    let mut theme = Theme::default_theme();
    theme.dimensions.centered_max_width = 880.0;
    assert_eq!(
        crate::ui::centered_column_width(1600.0, &theme.dimensions),
        880.0
    );
    assert_eq!(
        crate::ui::centered_column_width(720.0, &theme.dimensions),
        720.0 - theme.dimensions.editor_padding * 2.0
    );
}

#[test]
fn reading_padding_uses_fixed_top_typewriter_target_and_half_viewport_bottom() {
    let theme = Theme::default_theme();
    assert_eq!(super::render::editor_top_padding(false, 700.0), 48.0);
    assert_eq!(super::render::editor_top_padding(true, 700.0), 315.0);
    assert_eq!(super::render::editor_top_padding(true, 80.0), 48.0);
    assert_eq!(
        super::render::editor_bottom_padding(700.0, &theme.dimensions),
        theme.dimensions.editor_padding
            + (theme.dimensions.block_min_height * 0.75).max(16.0)
            + 350.0
    );
}

#[test]
fn scrollbar_geometry_and_inverse_mapping_stay_aligned() {
    let geometry = Editor::scrollbar_geometry(400.0, 600.0, 300.0);
    assert_eq!(geometry.track_height, 400.0);
    assert!(geometry.thumb_height >= 28.0);
    assert!((geometry.thumb_top - (400.0 - geometry.thumb_height) * 0.5).abs() < 0.001);

    let scroll_y = Editor::scroll_offset_for_thumb_top(
        geometry.thumb_top,
        geometry.track_height,
        geometry.thumb_height,
        geometry.max_scroll_y,
    );
    assert!((scroll_y - 300.0).abs() < 0.001);
}

#[test]
fn scrollbar_offset_mapping_clamps_to_track_bounds() {
    let geometry = Editor::scrollbar_geometry(300.0, 450.0, 0.0);
    assert_eq!(
        Editor::scroll_offset_for_thumb_top(
            -25.0,
            geometry.track_height,
            geometry.thumb_height,
            geometry.max_scroll_y,
        ),
        0.0
    );
    assert_eq!(
        Editor::scroll_offset_for_thumb_top(
            999.0,
            geometry.track_height,
            geometry.thumb_height,
            geometry.max_scroll_y,
        ),
        geometry.max_scroll_y
    );
}

#[gpui::test]
async fn clicking_bottom_padding_of_short_document_focuses_document_end(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(
            cx,
            "# first\n\nsecond\n\nthird\n\nfourth\n\nlast".to_owned(),
            None,
        )
    });
    visual.simulate_resize(size(px(900.0), px(700.0)));
    redraw(visual);

    let (first, last, last_bounds, max_scroll_y, current_scroll_y) =
        editor.read_with(visual, |editor, cx| {
            let visible = editor.document.visible_blocks();
            let first = visible.first().expect("first block").entity.clone();
            let last = visible.last().expect("last block").entity.clone();
            (
                first,
                last.clone(),
                last.read(cx).last_bounds.expect("last block bounds"),
                f32::from(editor.scroll_handle.max_offset().height.max(px(0.0))),
                -f32::from(editor.scroll_handle.offset().y),
            )
        });
    assert!(max_scroll_y > 0.5, "bottom padding should be scrollable");
    assert!(current_scroll_y <= 0.5, "test starts at the top");

    editor.update(visual, |editor, cx| {
        editor.active_entity_id = Some(first.entity_id());
        editor.pending_focus = None;
        cx.notify();
    });
    let content = visual.debug_bounds("editor-content").unwrap();
    let click = point(
        last_bounds.left() + px(8.0),
        (last_bounds.bottom() + px(80.0)).min(content.bottom() - px(8.0)),
    );
    assert!(click.y > last_bounds.bottom());
    visual.simulate_click(click, Modifiers::default());
    redraw(visual);

    editor.read_with(visual, |editor, cx| {
        assert_eq!(editor.active_entity_id, Some(last.entity_id()));
        let cursor = last.read(cx).visible_len();
        assert_eq!(last.read(cx).selected_range, cursor..cursor);
    });
}

#[gpui::test]
async fn editor_scrollbar_separates_stable_hitbox_from_hover_thumb(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let markdown = (0..120)
        .map(|index| format!("# Heading {index}\n\nParagraph {index} with enough text to scroll."))
        .collect::<Vec<_>>()
        .join("\n\n");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_markdown(cx, markdown, None));
    visual.simulate_resize(size(px(720.0), px(520.0)));
    editor.update(visual, |editor, cx| {
        editor.scrollbar_hovered = true;
        editor.scrollbar_visible_until = Instant::now() + Duration::from_secs(1);
        cx.notify();
    });
    redraw(visual);

    let source = editor.read_with(visual, |editor, _cx| editor.source_document.text());
    let revision = editor.read_with(visual, |editor, _cx| editor.source_document.revision());
    let dirty = editor.read_with(visual, |editor, _cx| editor.document_dirty);
    let content = visual.debug_bounds("editor-content").unwrap();
    let hitbox = visual.debug_bounds("editor-scrollbar-hitbox").unwrap();
    let idle_thumb = visual.debug_bounds("editor-scrollbar-thumb").unwrap();
    assert_eq!(f32::from(hitbox.size.width), 14.0);
    assert_eq!(f32::from(idle_thumb.size.width), 6.0);
    assert_eq!(idle_thumb.right(), hitbox.right());
    assert!(
        hitbox.left() >= content.left(),
        "hitbox={hitbox:?} content={content:?}"
    );
    assert!(
        hitbox.right() <= content.right(),
        "hitbox={hitbox:?} content={content:?}"
    );

    visual.simulate_mouse_move(hitbox.center(), None, Modifiers::default());
    redraw(visual);
    let hovered_hitbox = visual.debug_bounds("editor-scrollbar-hitbox").unwrap();
    let hovered_thumb = visual.debug_bounds("editor-scrollbar-thumb").unwrap();
    assert_eq!(f32::from(hovered_hitbox.size.width), 14.0);
    assert_eq!(f32::from(hovered_thumb.size.width), 10.0);
    assert_eq!(hovered_thumb.right(), hovered_hitbox.right());

    visual.simulate_mouse_down(
        hovered_hitbox.center(),
        MouseButton::Left,
        Modifiers::default(),
    );
    redraw(visual);
    editor.update(visual, |editor, _cx| {
        assert!(editor.scrollbar_drag.is_some());
    });
    visual.simulate_mouse_up(
        hovered_hitbox.center(),
        MouseButton::Left,
        Modifiers::default(),
    );
    visual.run_until_parked();
    editor.update(visual, |editor, _cx| {
        assert!(editor.scrollbar_drag.is_none());
        assert_eq!(editor.source_document.text(), source);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });

    visual.simulate_resize(size(px(1180.0), px(780.0)));
    redraw(visual);
    let content = visual.debug_bounds("editor-source-pane").unwrap();
    let hitbox = visual.debug_bounds("editor-scrollbar-hitbox").unwrap();
    assert!(hitbox.left() >= content.left());
    assert!(hitbox.right() <= content.right());
    visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
}

#[test]
fn minimal_projection_edit_keeps_utf8_common_prefix_and_suffix() {
    let edit = Editor::minimal_projection_edit("标题 alpha 结尾", "标题 beta 结尾")
        .expect("changed text should produce an edit");
    assert_eq!(edit.range(), &(7..11));
    assert_eq!(edit.replacement(), "bet");

    let insertion = Editor::minimal_projection_edit("前后", "前中后")
        .expect("insertion should produce an edit");
    assert_eq!(insertion.range(), &(3..3));
    assert_eq!(insertion.replacement(), "中");

    let deletion =
        Editor::minimal_projection_edit("前中后", "前后").expect("deletion should produce an edit");
    assert_eq!(deletion.range(), &(3..6));
    assert_eq!(deletion.replacement(), "");
    assert!(Editor::minimal_projection_edit("相同", "相同").is_none());
}

#[test]
fn prepared_projection_uses_stable_snapshot_and_preserves_lines() {
    let mut document = gmark_document::SourceDocument::new("alpha\n中文\n");
    let snapshot = document.snapshot();
    document
        .apply_transaction(gmark_document::Transaction::new(
            document.revision(),
            vec![gmark_document::TextEdit::new(0..5, "changed")],
        ))
        .expect("newer edit should apply");

    let prepared = PreparedSplitProjection::from_snapshot(snapshot);
    assert_eq!(prepared.revision, gmark_document::Revision::INITIAL);
    assert_eq!(prepared.lines, ["alpha", "中文", ""]);
    assert_eq!(prepared.regions.len(), 2);
    assert_eq!(prepared.regions[0].kind, ProjectionRegionKind::Paragraph);
    assert_eq!(prepared.regions[0].lines, 0..2);
    assert_eq!(prepared.regions[0].bytes, 0..12);
    assert_eq!(prepared.regions[1].kind, ProjectionRegionKind::Blank);
    assert_eq!(prepared.regions[1].lines, 2..3);
    assert_eq!(prepared.regions[1].bytes, 13..13);
    assert_eq!(document.text(), "changed\n中文\n");

    let empty =
        PreparedSplitProjection::from_snapshot(gmark_document::SourceDocument::new("").snapshot());
    assert_eq!(empty.lines, [""]);
    assert_eq!(empty.regions[0].kind, ProjectionRegionKind::Blank);
    assert_eq!(empty.regions[0].bytes, 0..0);
}

#[test]
fn prepared_projection_is_safe_to_share_with_background_workers() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PreparedSplitProjection>();
}

#[test]
fn prepared_projection_classifies_top_level_regions_without_losing_source_ranges() {
    let source =
        "# Title\n\n- one\n- two\n\n```rust\nfn main() {}\n```\n\n<!-- note -->\n\n$$\nx + y\n$$";
    let prepared = PreparedSplitProjection::from_snapshot(
        gmark_document::SourceDocument::new(source).snapshot(),
    );
    let kinds = prepared
        .regions
        .iter()
        .map(|region| region.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            ProjectionRegionKind::AtxHeading,
            ProjectionRegionKind::Blank,
            ProjectionRegionKind::List,
            ProjectionRegionKind::Blank,
            ProjectionRegionKind::FencedCode,
            ProjectionRegionKind::Blank,
            ProjectionRegionKind::Comment,
            ProjectionRegionKind::Blank,
            ProjectionRegionKind::DisplayMath,
        ]
    );

    for region in &prepared.regions {
        assert!(source.is_char_boundary(region.bytes.start));
        assert!(source.is_char_boundary(region.bytes.end));
        assert_eq!(
            &source[region.bytes.clone()],
            prepared.lines[region.lines.clone()].join("\n")
        );
    }
}

fn assert_incremental_projection_matches_full(
    previous_source: &str,
    current_source: &str,
) -> PreparedSplitProjection {
    let previous = PreparedSplitProjection::from_snapshot(
        gmark_document::SourceDocument::new(previous_source).snapshot(),
    );
    let incremental = PreparedSplitProjection::from_snapshot_incremental(
        gmark_document::SourceDocument::new(current_source).snapshot(),
        &previous,
    );
    let full = PreparedSplitProjection::from_snapshot(
        gmark_document::SourceDocument::new(current_source).snapshot(),
    );
    assert_eq!(incremental.source, full.source);
    assert_eq!(incremental.lines, full.lines);
    assert_eq!(incremental.regions, full.regions);
    let signatures = |prepared: &PreparedSplitProjection| {
        prepared
            .nodes
            .iter()
            .map(|nodes| {
                nodes.as_ref().map(|nodes| {
                    nodes
                        .iter()
                        .map(|node| (node.record.kind.clone(), node.record.markdown_line(0, None)))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(signatures(&incremental), signatures(&full));
    incremental
}
