// @author kongweiguang

#[gpui::test]
async fn large_document_reopens_with_an_explicit_encoding_without_changing_tabs(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("explicit encoding tempdir");
    let path = temp.path().join("explicit-encoding.md");
    fs::write(&path, "café\n").expect("explicit encoding fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("explicit encoding probe");
    let source = gmark_large_document::FileSource::open(&path).expect("explicit encoding source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("explicit encoding large view");
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.encoding_label()),
        "UTF-8"
    );

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.reopen_with_encoding(
                gmark_large_document::TextEncoding::Legacy("windows-1252".to_owned()),
                window,
                cx,
            )
        });
    });
    visual.run_until_parked();
    redraw(visual);
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.encoding_label()),
        "WINDOWS-1252"
    );
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.recovered_text_for_test())
            .is_some_and(|text| text == "cafÃ©\n".as_bytes())
    );

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
    });
    let (_, edit_block) = large_view
        .read_with(visual, |view, _cx| view.active_edit_for_test())
        .expect("explicit encoding edit block");
    edit_block.update(visual, |block, cx| {
        let end = block.display_text().len();
        block.replace_text_in_visible_range(end..end, "x", None, false, cx);
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.reopen_with_encoding(
                gmark_large_document::TextEncoding::Utf8 { bom: false },
                window,
                cx,
            )
        });
    });
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.encoding_label()),
        "WINDOWS-1252",
        "dirty documents must not be discarded by an encoding reopen"
    );
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.error_for_test())
            .is_some_and(|error| error.contains("Save or undo"))
    );
    visual.simulate_keystrokes("ctrl-z");
    visual.run_until_parked();

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.reopen_with_encoding(
                gmark_large_document::TextEncoding::Utf8 { bom: false },
                window,
                cx,
            )
        });
    });
    visual.run_until_parked();
    redraw(visual);
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.encoding_label()),
        "UTF-8"
    );
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.recovered_text_for_test())
            .is_some_and(|text| text == "café\n".as_bytes())
    );
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Source));
    assert!(large_view.read_with(visual, |view, _cx| view.error_for_test().is_none()));
}

/// Opt-in wall-clock soak for the real GPUI input/render pipeline. The fixture is copied to a
/// temporary file, so edit/save cycles never mutate the checked-in performance corpus.
#[gpui::test]
#[ignore = "eight-hour interactive production soak"]
async fn large_document_interactive_soak(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let fixture = PathBuf::from(
        std::env::var_os("GMARK_INTERACTIVE_SOAK_FIXTURE")
            .expect("GMARK_INTERACTIVE_SOAK_FIXTURE must name an existing large Markdown file"),
    );
    let duration = Duration::from_secs(
        std::env::var("GMARK_INTERACTIVE_SOAK_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(28_800),
    );
    let progress_path = std::env::var_os("GMARK_INTERACTIVE_SOAK_PROGRESS").map(PathBuf::from);
    let temp = tempfile::tempdir().expect("interactive soak tempdir");
    let path = temp.path().join("interactive-soak.md");
    fs::copy(&fixture, &path).expect("copy interactive soak fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("interactive soak probe");
    let source = gmark_large_document::FileSource::open(&path).expect("interactive soak source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.simulate_resize(size(px(1180.0), px(780.0)));
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("interactive soak large view");

    let ready_deadline = Instant::now() + Duration::from_secs(30);
    loop {
        visual.update(|window, cx| {
            large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
        });
        visual.run_until_parked();
        if large_view.read_with(visual, |view, _cx| view.active_edit_for_test().is_some()) {
            break;
        }
        assert!(
            Instant::now() < ready_deadline,
            "large Source never became editable"
        );
        std::thread::sleep(Duration::from_millis(50));
        redraw(visual);
    }

    let started = Instant::now();
    let baseline_rss = current_process_rss_mib().unwrap_or_default();
    let mut maximum_rss = baseline_rss;
    let mut cycles = 0u64;
    let mut edits = 0u64;
    let mut saves = 0u64;
    let mut searches = 0u64;
    let mut structure_transitions = 0u64;
    while started.elapsed() < duration {
        large_view.update(visual, |view, cx| view.scroll_page_for_test(true, cx));
        redraw(visual);
        large_view.update(visual, |view, cx| view.scroll_page_for_test(false, cx));
        redraw(visual);

        let line = large_view.read_with(visual, |view, _cx| view.scroll_top_line_for_test());
        visual.update(|window, cx| {
            large_view.update(cx, |view, cx| {
                view.begin_line_edit_for_test(line, window, cx)
            });
        });
        if let Some((_, block)) =
            large_view.read_with(visual, |view, _cx| view.active_edit_for_test())
        {
            block.update(visual, |block, cx| {
                let end = block.display_text().len();
                block.replace_text_in_visible_range(end..end, "x", None, false, cx);
            });
            edits += 1;
        }
        visual.run_until_parked();
        visual.simulate_keystrokes("ctrl-z");
        visual.run_until_parked();
        visual.simulate_keystrokes("ctrl-y");
        visual.run_until_parked();

        if cycles.is_multiple_of(6_000) {
            visual.simulate_keystrokes("ctrl-s");
            visual.run_until_parked();
            saves += 1;
        } else {
            visual.simulate_keystrokes("ctrl-z");
            visual.run_until_parked();
        }

        if cycles.is_multiple_of(25) {
            visual.simulate_keystrokes("ctrl-f");
            redraw(visual);
            visual.simulate_keystrokes("ctrl-a");
            visual.simulate_input("生产报告");
            visual.run_until_parked();
            visual.simulate_keystrokes("escape");
            searches += 1;
        }
        if cycles.is_multiple_of(50)
            && large_view.read_with(visual, |view, _cx| view.has_structure_view())
        {
            large_view.update(visual, |view, cx| view.show_structure_view(cx));
            redraw(visual);
            large_view.update(visual, |view, cx| view.show_source_view(cx));
            structure_transitions += 1;
        }

        redraw(visual);
        assert!(large_view.read_with(visual, |view, _cx| view.error_for_test().is_none()));
        assert!(
            large_view.read_with(visual, |view, _cx| view.source_cache_len_for_test())
                <= crate::large_file::MAX_SOURCE_CACHED_ROWS
        );
        maximum_rss = maximum_rss.max(current_process_rss_mib().unwrap_or_default());
        cycles += 1;

        if cycles.is_multiple_of(10)
            && let Some(progress_path) = &progress_path
        {
            let progress = serde_json::json!({
                "schema_version": 1,
                "completed": false,
                "elapsed_seconds": started.elapsed().as_secs_f64(),
                "cycles": cycles,
                "edits": edits,
                "saves": saves,
                "searches": searches,
                "structure_transitions": structure_transitions,
                "baseline_rss_mib": baseline_rss,
                "maximum_rss_mib": maximum_rss,
            });
            fs::write(
                progress_path,
                serde_json::to_vec_pretty(&progress).expect("serialize interactive soak progress"),
            )
            .expect("write interactive soak progress");
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    if let Some(progress_path) = progress_path {
        let progress = serde_json::json!({
            "schema_version": 1,
            "completed": true,
            "elapsed_seconds": started.elapsed().as_secs_f64(),
            "cycles": cycles,
            "edits": edits,
            "saves": saves,
            "searches": searches,
            "structure_transitions": structure_transitions,
            "baseline_rss_mib": baseline_rss,
            "maximum_rss_mib": maximum_rss,
            "rss_growth_mib": (maximum_rss - baseline_rss).max(0.0),
        });
        fs::write(
            progress_path,
            serde_json::to_vec_pretty(&progress)
                .expect("serialize completed interactive soak progress"),
        )
        .expect("write completed interactive soak progress");
    }
}

#[gpui::test]
async fn large_markdown_switches_between_all_tables_and_keeps_source_mapping(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large Markdown tempdir");
    let path = temp.path().join("multiple-tables.md");
    fs::write(
        &path,
        "# Report\n\n| name | score |\n| --- | ---: |\n| Ada | 10 |\n\nBetween tables.\n\n| city | country | note |\n| :--- | :--- | ---: |\n| Paris | France | first |\n| Tokyo | Japan | second |\n\nDone.\n",
    )
    .expect("multi-table Markdown fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large Markdown probe");
    let source = gmark_large_document::FileSource::open(&path).expect("large Markdown source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.simulate_resize(size(px(1180.0), px(780.0)));
    visual.run_until_parked();
    redraw(visual);

    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large Markdown view");
    assert!(large_view.read_with(visual, |view, _cx| view.source_view_for_test()));
    let overflow = visual
        .debug_bounds("status-bar-format-overflow-button")
        .expect("large-file overflow button");
    visual.simulate_click(overflow.center(), Modifiers::default());
    redraw(visual);
    let structure = visual
        .debug_bounds("status-bar-large-structure")
        .expect("structured data action");
    visual.simulate_click(structure.center(), Modifiers::default());
    redraw(visual);
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Source));
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.markdown_table_state_for_test()),
        Some((0, 2, vec!["name".to_owned(), "score".to_owned()], 1))
    );
    assert!(
        visual
            .debug_bounds("large-file-markdown-table-switcher")
            .is_some()
    );

    let next = visual
        .debug_bounds("large-file-markdown-table-next")
        .expect("next table button");
    visual.simulate_click(next.center(), Modifiers::default());
    visual.run_until_parked();
    redraw(visual);
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.markdown_table_state_for_test()),
        Some((
            1,
            2,
            vec!["city".to_owned(), "country".to_owned(), "note".to_owned()],
            2,
        ))
    );

    large_view.update(visual, |view, cx| {
        view.jump_structured_row_to_source_for_test(0, cx)
    });
    assert_eq!(
        large_view.read_with(visual, |view, cx| view.cursor_position(cx)),
        (11, 1)
    );
}

#[gpui::test]
async fn wide_large_csv_only_materializes_the_active_column_window(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("wide CSV tempdir");
    let path = temp.path().join("wide.csv");
    let headers = (0..64)
        .map(|column| format!("h{column}"))
        .collect::<Vec<_>>();
    let values = (0..64)
        .map(|column| format!("v{column}"))
        .collect::<Vec<_>>();
    fs::write(
        &path,
        format!("{}\n{}\n", headers.join(","), values.join(",")),
    )
    .expect("wide CSV fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("wide CSV probe");
    let source = gmark_large_document::FileSource::open(&path).expect("wide CSV source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.simulate_resize(size(px(960.0), px(640.0)));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("wide CSV large view");
    large_view.update(visual, |view, cx| view.show_structure_view(cx));
    visual.run_until_parked();
    redraw(visual);
    visual.run_until_parked();
    redraw(visual);

    assert!(
        visual
            .debug_bounds("large-file-structured-column-pager")
            .is_some()
    );
    let (start, materialized) =
        large_view.read_with(visual, |view, _cx| view.structured_column_window_for_test());
    assert_eq!(start, 0);
    assert!(materialized <= 16, "materialized {materialized} columns");

    let next = visual
        .debug_bounds("large-file-structured-columns-next")
        .expect("next column window");
    visual.simulate_click(next.center(), Modifiers::default());
    visual.run_until_parked();
    redraw(visual);
    visual.run_until_parked();
    redraw(visual);
    let (start, materialized) =
        large_view.read_with(visual, |view, _cx| view.structured_column_window_for_test());
    assert_eq!(start, 16);
    assert!(materialized <= 16, "materialized {materialized} columns");
}

#[gpui::test]
async fn large_log_follow_and_external_conflict_preserve_local_edits(cx: &mut TestAppContext) {
    use std::io::Write as _;

    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large log tempdir");
    let path = temp.path().join("follow.log");
    fs::write(&path, "first\n").expect("large log fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large log probe");
    let source = gmark_large_document::FileSource::open(&path).expect("large log source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large log view");
    assert!(large_view.read_with(visual, |view, _cx| view.follow_enabled()));
    editor.update(visual, |editor, cx| {
        editor.external_file_conflict = true;
        editor.restart_file_watcher(cx);
    });
    assert!(!editor.read_with(visual, |editor, _cx| editor.external_file_conflict));

    let mut writer = fs::OpenOptions::new()
        .append(true)
        .open(temp.path().join("follow.log"))
        .expect("append log");
    writer.write_all(b"second\n").expect("append second line");
    writer.sync_all().expect("sync appended log");
    visual
        .executor()
        .advance_clock(Duration::from_millis(1_100));
    visual.run_until_parked();
    redraw(visual);
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some(b"first\nsecond\n".to_vec())
    );
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.pending_external_change_for_test())
            .is_none()
    );

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
    });
    let (_, edit) = large_view
        .read_with(visual, |view, _cx| view.active_edit_for_test())
        .expect("active log edit");
    edit.update(visual, |block, cx| {
        block.replace_text_in_visible_range(0..0, "local ", None, false, cx);
    });
    visual.run_until_parked();
    assert!(editor.read_with(visual, |editor, _cx| editor.document_dirty));
    assert!(!large_view.read_with(visual, |view, _cx| view.follow_enabled()));

    writer.write_all(b"remote\n").expect("append remote line");
    writer.sync_all().expect("sync remote line");
    visual
        .executor()
        .advance_clock(Duration::from_millis(1_100));
    visual.run_until_parked();
    redraw(visual);
    assert!(matches!(
        large_view.read_with(visual, |view, _cx| view.pending_external_change_for_test()),
        Some(gmark_large_document::ExternalChange::Appended { .. })
    ));
    assert!(
        visual
            .debug_bounds("large-file-external-change-banner")
            .is_some()
    );
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.recovered_text_for_test())
            .is_some_and(|text| {
                text.starts_with(b"local first\nsecond\n") && !text.ends_with(b"remote\n")
            })
    );

    large_view.update(visual, |view, cx| view.keep_local_for_test(cx));
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.pending_external_change_for_test())
            .is_none()
    );
    assert!(large_view.read_with(visual, |view, _cx| view.external_monitor_paused_for_test()));
    assert!(editor.read_with(visual, |editor, _cx| editor.document_dirty));
}
