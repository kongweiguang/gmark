// @author kongweiguang

#[gpui::test]
async fn large_external_truncation_reload_replaces_the_clean_baseline(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large reload tempdir");
    let path = temp.path().join("reload.txt");
    fs::write(&path, "original long content\n").expect("large reload fixture");
    let probe = gmark_paged_document::probe_file(
        &path,
        gmark_paged_document::ProbeOptions {
            max_resident_bytes: 1,
            ..gmark_paged_document::ProbeOptions::default()
        },
    )
    .expect("large reload probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("large reload source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("large reload view");

    fs::write(temp.path().join("reload.txt"), "new\n").expect("truncate source");
    visual
        .executor()
        .advance_clock(Duration::from_millis(1_100));
    visual.run_until_parked();
    redraw(visual);
    assert!(matches!(
        large_view.read_with(visual, |view, _cx| view.pending_external_change_for_test()),
        Some(gmark_paged_document::ExternalChange::Truncated { .. })
            | Some(gmark_paged_document::ExternalChange::Modified)
    ));
    assert!(
        visual
            .debug_bounds("document-host-external-change-banner")
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
    let probe = gmark_paged_document::probe_file(
        &path,
        gmark_paged_document::ProbeOptions {
            max_resident_bytes: 1,
            ..gmark_paged_document::ProbeOptions::default()
        },
    )
    .expect("large copy probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("large copy source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.run_until_parked();
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
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
    let probe = gmark_paged_document::probe_file(
        &path,
        gmark_paged_document::ProbeOptions {
            max_resident_bytes: 1,
            ..gmark_paged_document::ProbeOptions::default()
        },
    )
    .expect("large copy snapshot probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("large copy snapshot source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.run_until_parked();
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
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
async fn switching_tabs_keeps_large_source_copy_snapshot_and_source_state(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large copy tab-switch tempdir");
    let path = temp.path().join("copy-tab-switch.txt");
    fs::write(&path, "alpha\nbeta\n").expect("large copy tab-switch fixture");
    let probe = gmark_paged_document::probe_file(
        &path,
        gmark_paged_document::ProbeOptions {
            max_resident_bytes: 1,
            ..gmark_paged_document::ProbeOptions::default()
        },
    )
    .expect("large copy tab-switch probe");
    let source =
        gmark_paged_document::FileSource::open(&path).expect("large copy tab-switch source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.run_until_parked();
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
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
    assert!(editor.read_with(visual, |editor, _cx| editor.document_host.is_none()));
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_previous_tab_action(&crate::components::PreviousTab, window, cx);
        });
    });
    visual.run_until_parked();
    assert!(editor.read_with(visual, |editor, _cx| {
        editor
            .document_host
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
    let probe = gmark_paged_document::probe_file(
        &path,
        gmark_paged_document::ProbeOptions {
            max_resident_bytes: 1,
            ..gmark_paged_document::ProbeOptions::default()
        },
    )
    .expect("large close cancellation probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("large close source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    visual.run_until_parked();
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
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
    assert!(large_view.read_with(visual, |view, _cx| { view.is_closed_suspended_for_test() }));

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_reopen_closed_tab_action(&crate::components::ReopenClosedTab, window, cx);
        });
    });
    visual.run_until_parked();
    let reopened_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("reopened large close view");
    assert!(
        reopened_view != large_view,
        "body-free closed history must rebuild a fresh Host entity"
    );
    assert!(!reopened_view.read_with(visual, |view, _cx| { view.is_closed_suspended_for_test() }));
    assert!(editor.read_with(visual, |editor, _cx| editor.document_host.is_some()));
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
    let probe = gmark_paged_document::probe_file(
        &path,
        gmark_paged_document::ProbeOptions {
            max_resident_bytes: 1,
            ..gmark_paged_document::ProbeOptions::default()
        },
    )
    .expect("large provisional reopen probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("large provisional source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("provisional large view");

    // 不运行 executor，确保初次索引仍在 pending；关闭必须取消旧 worker，重开必须另起代次。
    editor.update(visual, |editor, cx| editor.request_close_tab_index(0, cx));
    assert!(large_view.read_with(visual, |view, _cx| { view.is_closed_suspended_for_test() }));
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_reopen_closed_tab_action(&crate::components::ReopenClosedTab, window, cx);
        });
    });
    visual.run_until_parked();

    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.recovered_text_for_test()),
        Some(text.as_bytes().to_vec())
    );
    let reopened_view = editor
        .read_with(visual, |editor, _cx| editor.document_host.clone())
        .expect("reopened provisional large view");
    assert!(
        reopened_view != large_view,
        "body-free closed history must rebuild a fresh Host entity"
    );
    assert!(!reopened_view.read_with(visual, |view, _cx| { view.is_closed_suspended_for_test() }));
}
