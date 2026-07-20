// @author kongweiguang

#[gpui::test]
async fn status_bar_file_state_uses_semantic_icons_and_conflict_opens_comparison(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let path = temp_markdown_path("status-bar-conflict-action");
    fs::write(&path, "disk version").unwrap();
    let source = "local version";
    let editor_path = path.clone();
    let (editor, visual_cx) = cx.add_window_view(move |_window, cx| {
        Editor::from_markdown(cx, source.to_owned(), Some(editor_path))
    });
    editor.update(visual_cx, |editor, cx| {
        editor.external_file_conflict = true;
        cx.notify();
    });
    let (revision, dirty) = editor.read_with(visual_cx, |editor, _cx| {
        (editor.source_document.revision(), editor.document_dirty)
    });

    for viewport in [size(px(720.0), px(520.0)), size(px(1180.0), px(780.0))] {
        visual_cx.simulate_resize(viewport);
        redraw(visual_cx);
        visual_cx.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
        let bar = visual_cx.debug_bounds("status-bar").unwrap();
        let status = visual_cx.debug_bounds("status-bar-recovery").unwrap();
        let icon = visual_cx
            .debug_bounds("status-bar-recovery-conflict-icon")
            .unwrap();
        let label = visual_cx.debug_bounds("status-bar-recovery-label").unwrap();
        assert_eq!(status.size.height, px(24.0));
        assert!(status.size.width <= px(160.0));
        assert_eq!(icon.size, size(px(16.0), px(16.0)));
        assert!(status.left() >= bar.left());
        assert!(status.right() <= bar.right());
        assert!(icon.left() >= status.left());
        assert!(icon.right() <= label.left());
        assert!(label.right() <= status.right());
    }

    editor.update_in(visual_cx, |editor, window, _cx| {
        let handle = editor
            .status_bar
            .conflict_focus_handle
            .as_ref()
            .expect("conflict status focus");
        handle.focus(window);
        assert!(handle.is_focused(window));
    });
    visual_cx.simulate_keystrokes("space");
    visual_cx.run_until_parked();
    redraw(visual_cx);
    editor.read_with(visual_cx, |editor, _cx| {
        assert!(editor.show_external_conflict_dialog);
        assert!(editor.external_conflict_preview.is_some());
        assert_eq!(editor.source_document.text(), source);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });
    assert!(visual_cx.debug_bounds("external-conflict-dialog").is_some());

    editor.update(visual_cx, |editor, cx| {
        editor.cancel_external_conflict(cx);
        editor.external_file_conflict = false;
        editor.recovered_session = true;
        cx.notify();
    });
    redraw(visual_cx);
    let status = visual_cx.debug_bounds("status-bar-recovery").unwrap();
    let icon = visual_cx
        .debug_bounds("status-bar-recovery-restored-icon")
        .unwrap();
    assert_eq!(status.size.height, px(24.0));
    assert_eq!(icon.size, size(px(16.0), px(16.0)));
    visual_cx.simulate_click(status.center(), Modifiers::default());
    visual_cx.run_until_parked();
    editor.read_with(visual_cx, |editor, _cx| {
        assert!(!editor.show_external_conflict_dialog);
        assert_eq!(editor.source_document.text(), source);
        assert_eq!(editor.source_document.revision(), revision);
    });

    fs::remove_file(path).unwrap();
}

#[gpui::test]
async fn close_and_encoding_dialog_actions_stay_visible_at_two_x_scale(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual_cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "document".to_owned(), None));
    visual_cx.simulate_resize(size(px(720.0), px(520.0)));
    editor.update(visual_cx, |editor, cx| {
        editor.show_unsaved_changes_dialog = true;
        cx.notify();
    });
    redraw(visual_cx);
    visual_cx.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));

    let overlay = visual_cx.debug_bounds("unsaved-changes-overlay").unwrap();
    let dialog = visual_cx.debug_bounds("unsaved-changes-dialog").unwrap();
    assert_dialog_title_icon(
        visual_cx,
        "unsaved-changes-dialog",
        "unsaved-changes-title-icon",
        "unsaved-changes-title-label",
    );
    assert!(dialog.left() >= overlay.left());
    assert!(dialog.right() <= overlay.right());
    assert!(dialog.top() >= overlay.top());
    assert!(dialog.bottom() <= overlay.bottom());
    let message = visual_cx
        .debug_bounds("unsaved-changes-message")
        .expect("unsaved changes body");
    let first_action = visual_cx.debug_bounds("cancel-close-dialog").unwrap();
    assert!(f32::from(message.size.height) >= 16.0);
    assert!(message.top() >= dialog.top());
    assert!(message.bottom() < first_action.top());
    for selector in [
        "cancel-close-dialog",
        "discard-and-close-dialog",
        "save-and-close-dialog",
    ] {
        let action = visual_cx.debug_bounds(selector).unwrap();
        assert!(action.left() >= dialog.left(), "{selector} escaped left");
        assert!(action.right() <= dialog.right(), "{selector} escaped right");
        assert!(action.top() >= dialog.top(), "{selector} escaped top");
        assert!(f32::from(action.size.width) >= 72.0, "{selector} width");
        assert_eq!(f32::from(action.size.height), 36.0, "{selector} height");
        assert!(
            action.bottom() <= dialog.bottom(),
            "{selector} escaped bottom"
        );
    }

    editor.update(visual_cx, |editor, cx| {
        editor.show_unsaved_changes_dialog = false;
        editor.show_encoding_conversion_dialog = true;
        cx.notify();
    });
    redraw(visual_cx);
    let overlay = visual_cx
        .debug_bounds("encoding-conversion-overlay")
        .unwrap();
    let dialog = visual_cx
        .debug_bounds("encoding-conversion-dialog")
        .unwrap();
    assert_dialog_title_icon(
        visual_cx,
        "encoding-conversion-dialog",
        "encoding-conversion-title-icon",
        "encoding-conversion-title-label",
    );
    assert!(dialog.left() >= overlay.left());
    assert!(dialog.right() <= overlay.right());
    assert!(dialog.top() >= overlay.top());
    assert!(dialog.bottom() <= overlay.bottom());
    for selector in ["keep-legacy-read-only", "convert-encoding-utf8"] {
        let action = visual_cx.debug_bounds(selector).unwrap();
        assert!(action.left() >= dialog.left(), "{selector} escaped left");
        assert!(action.right() <= dialog.right(), "{selector} escaped right");
        assert!(action.top() >= dialog.top(), "{selector} escaped top");
        assert!(f32::from(action.size.width) >= 72.0, "{selector} width");
        assert_eq!(f32::from(action.size.height), 36.0, "{selector} height");
        assert!(
            action.bottom() <= dialog.bottom(),
            "{selector} escaped bottom"
        );
    }

    editor.update(visual_cx, |editor, cx| {
        editor.show_encoding_conversion_dialog = false;
        editor.info_dialog = Some(InfoDialogKind::About);
        cx.notify();
    });
    redraw(visual_cx);
    let overlay = visual_cx.debug_bounds("info-dialog-overlay").unwrap();
    let dialog = visual_cx.debug_bounds("info-dialog").unwrap();
    assert_dialog_title_icon(
        visual_cx,
        "info-dialog",
        "info-dialog-title-icon",
        "info-dialog-title-label",
    );
    let dismiss = visual_cx.debug_bounds("dismiss-info-dialog").unwrap();
    assert!(dialog.left() >= overlay.left());
    assert!(dialog.right() <= overlay.right());
    assert!(dialog.top() >= overlay.top());
    assert!(dialog.bottom() <= overlay.bottom());
    assert!(dismiss.left() >= dialog.left());
    assert!(dismiss.right() <= dialog.right());
    assert_eq!(f32::from(dismiss.size.height), 36.0);

    editor.update(visual_cx, |editor, cx| {
        editor.info_dialog = None;
        editor.export_in_progress = true;
        cx.notify();
    });
    redraw(visual_cx);
    let main = visual_cx.debug_bounds("editor-main-content").unwrap();
    let progress = visual_cx.debug_bounds("export-progress").unwrap();
    let cancel = visual_cx.debug_bounds("cancel-export").unwrap();
    assert!(progress.left() >= main.left());
    assert!(progress.right() <= main.right());
    assert!(progress.top() >= main.top());
    assert!(progress.bottom() <= main.bottom());
    assert!(cancel.left() >= progress.left());
    assert!(cancel.right() <= progress.right());
    assert!(cancel.top() >= progress.top());
    assert!(cancel.bottom() <= progress.bottom());
}

#[gpui::test]
async fn table_and_drop_dialogs_use_standard_compact_layout(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual_cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "document".to_owned(), None));
    visual_cx.simulate_resize(size(px(720.0), px(520.0)));
    editor.update(visual_cx, |editor, cx| {
        editor.table_insert_dialog = Some(super::context_menu::TableInsertDialogState {
            target: super::context_menu::TableInsertTarget::Append,
            body_rows: 3,
            columns: 4,
        });
        cx.notify();
    });
    redraw(visual_cx);

    let overlay = visual_cx
        .debug_bounds("table-insert-dialog-overlay")
        .unwrap();
    let dialog = visual_cx.debug_bounds("table-insert-dialog").unwrap();
    assert_dialog_title_icon(
        visual_cx,
        "table-insert-dialog",
        "table-insert-title-icon",
        "table-insert-title-label",
    );
    assert!(dialog.left() >= overlay.left());
    assert!(dialog.right() <= overlay.right());
    assert!(dialog.top() >= overlay.top());
    assert!(dialog.bottom() <= overlay.bottom());
    for selector in ["cancel-table-insert-dialog", "confirm-table-insert-dialog"] {
        let action = visual_cx.debug_bounds(selector).unwrap();
        assert!(action.left() >= dialog.left(), "{selector}");
        assert!(action.right() <= dialog.right(), "{selector}");
        assert!(f32::from(action.size.width) >= 72.0, "{selector}");
        assert_eq!(f32::from(action.size.height), 36.0, "{selector}");
    }

    editor.update(visual_cx, |editor, cx| {
        editor.table_insert_dialog = None;
        editor.show_drop_replace_dialog = true;
        cx.notify();
    });
    redraw(visual_cx);
    let overlay = visual_cx.debug_bounds("drop-replace-overlay").unwrap();
    let dialog = visual_cx.debug_bounds("drop-replace-dialog").unwrap();
    assert_dialog_title_icon(
        visual_cx,
        "drop-replace-dialog",
        "drop-replace-title-icon",
        "drop-replace-title-label",
    );
    assert!(dialog.left() >= overlay.left());
    assert!(dialog.right() <= overlay.right());
    assert!(dialog.top() >= overlay.top());
    assert!(dialog.bottom() <= overlay.bottom());
    for selector in [
        "cancel-drop-replace-dialog",
        "discard-and-replace-drop-dialog",
        "save-and-replace-drop-dialog",
    ] {
        let action = visual_cx.debug_bounds(selector).unwrap();
        assert!(action.left() >= dialog.left(), "{selector}");
        assert!(action.right() <= dialog.right(), "{selector}");
        assert!(f32::from(action.size.width) >= 72.0, "{selector}");
        assert_eq!(f32::from(action.size.height), 36.0, "{selector}");
    }
    visual_cx.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
}

#[gpui::test]
async fn recovery_debounce_persists_latest_dirty_revision_off_ui_thread(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().unwrap();
    let journal =
        crate::recovery::RecoveryJournal::create(temp.path(), None, "alpha".to_owned()).unwrap();
    let journal_path = journal.path().to_path_buf();
    let (editor, visual_cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "alpha".to_owned(), None));
    redraw(visual_cx);
    editor.update(visual_cx, |editor, cx| {
        editor.recovery_journal = Some(Arc::new(Mutex::new(journal)));
        let revision = editor.source_document.revision();
        let end = editor.source_document.len();
        editor
            .source_document
            .apply_transaction(gmark_document::Transaction::new(
                revision,
                vec![gmark_document::TextEdit::new(end..end, " latest")],
            ))
            .unwrap();
        editor.document_dirty = true;
        editor.schedule_recovery_journal(cx);
    });
    visual_cx.run_until_parked();
    visual_cx
        .executor()
        .advance_clock(Duration::from_millis(1_000));
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, cx| {
        let revision = editor.source_document.revision();
        let end = editor.source_document.len();
        editor
            .source_document
            .apply_transaction(gmark_document::Transaction::new(
                revision,
                vec![gmark_document::TextEdit::new(end..end, " newest")],
            ))
            .unwrap();
        editor.schedule_recovery_journal(cx);
    });
    visual_cx
        .executor()
        .advance_clock(Duration::from_millis(1_100));
    visual_cx.run_until_parked();
    assert!(!journal_path.exists(), "debounce must wait for true idle");
    visual_cx
        .executor()
        .advance_clock(Duration::from_millis(2_100));
    visual_cx.run_until_parked();

    let recovered = crate::recovery::replay_journal(&journal_path).unwrap();
    assert_eq!(recovered.source, "alpha latest newest");
}

#[gpui::test]
async fn app_opens_recovery_session_as_dirty_editor_window(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().unwrap();
    let mut journal =
        crate::recovery::RecoveryJournal::create(temp.path(), None, String::new()).unwrap();
    journal
        .record(
            "recovered text",
            crate::recovery::RecoverySelection {
                start: 14,
                end: 14,
                reversed: false,
                anchor_affinity: None,
                head_affinity: None,
            },
            "rendered",
        )
        .unwrap();
    let recovered = crate::recovery::replay_journal(journal.path()).unwrap();
    let window = cx.update(|cx| crate::app_menu::open_recovered_editor_window(cx, recovered));
    cx.run_until_parked();

    window
        .update(cx, |editor, window, cx| {
            assert_eq!(editor.source_document.text(), "recovered text");
            assert!(editor.document_dirty);
            assert!(!editor.on_window_should_close(window, cx));
        })
        .expect("recovered window");
}

fn redraw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear());
    cx.run_until_parked();
}

#[gpui::test]
async fn large_document_uses_the_standard_editor_shell(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let temp = tempfile::tempdir().expect("large document tempdir");
    let path = temp.path().join("large-shell.md");
    let text = (0..5_000)
        .map(|line| format!("large document line {line}\n"))
        .collect::<String>();
    fs::write(&path, text).expect("large document fixture");
    let probe = gmark_large_document::probe_file(
        &path,
        gmark_large_document::ProbeOptions {
            large_file_threshold: 1,
            ..gmark_large_document::ProbeOptions::default()
        },
    )
    .expect("large document probe");
    assert_eq!(probe.strategy, gmark_large_document::OpenStrategy::Large);
    let source = gmark_large_document::FileSource::open(&path).expect("large document source");
    let (editor, visual) =
        cx.add_window_view(move |_window, cx| Editor::from_large_file(cx, path, probe, source));

    for viewport in [size(px(1180.0), px(780.0)), size(px(720.0), px(520.0))] {
        visual.simulate_resize(viewport);
        redraw(visual);
        visual.update(|window, _cx| {
            assert_eq!(
                window.scale_factor(),
                2.0,
                "large-file visual coverage runs at 200% scale"
            )
        });
        let shell = visual.debug_bounds("editor-main-content").unwrap();
        let content = visual.debug_bounds("editor-content").unwrap();
        let large_content = visual.debug_bounds("large-document-tab-content").unwrap();
        let tab_strip = visual.debug_bounds("document-tab-strip").unwrap();
        let status_bar = visual.debug_bounds("status-bar").unwrap();
        let large_status = visual.debug_bounds("status-bar-large-file-status").unwrap();

        // Windows/macOS 使用应用内标题栏；Linux/FreeBSD 的客户端装饰由平台窗口层提供。
        // 大文件外壳契约由下面的主内容、Tab 和状态栏共同验证，不绑定平台装饰实现。
        if cfg!(any(target_os = "windows", target_os = "macos")) {
            assert!(visual.debug_bounds("editor-titlebar").is_some());
        }
        assert!(visual.debug_bounds("status-bar-mode-switch").is_some());
        assert!(visual.debug_bounds("status-bar-mode-Source").is_some());
        for unavailable in [
            "status-bar-mode-Rendered",
            "status-bar-mode-Split",
            "status-bar-mode-Preview",
        ] {
            assert!(
                visual.debug_bounds(unavailable).is_none(),
                "large documents expose one fixed Source mode, not a misleading switch"
            );
        }
        assert!(
            visual
                .debug_bounds("status-bar-format-overflow-button")
                .is_some()
        );
        assert!(visual.debug_bounds("large-file-source-mode").is_none());
        assert!(large_content.left() >= content.left());
        assert!(large_content.right() <= content.right());
        assert_eq!(tab_strip.bottom(), content.top());
        assert_eq!(status_bar.top(), shell.bottom());
        assert!(large_status.right() <= status_bar.right());
        if let Some(first_body) = visual.debug_bounds("large-file-line-body-0") {
            let source_surface = visual
                .debug_bounds("large-file-source-horizontal-scroll")
                .expect("large Source surface");
            assert!(
                first_body.top() >= source_surface.top() + px(47.0),
                "large Source keeps the same reading top inset as ordinary Source"
            );
        }
    }

    for theme_id in ["gmark-light", "gmark"] {
        visual.update(|_window, cx| {
            assert!(cx.update_global::<ThemeManager, _>(|manager, _cx| {
                manager.set_theme_by_id(theme_id)
            }));
            cx.refresh_windows();
        });
        redraw(visual);
        assert!(visual.debug_bounds("large-document-tab-content").is_some());
        assert!(visual.debug_bounds("status-bar-mode-switch").is_some());
        assert!(visual.debug_bounds("large-file-scrollbar").is_some());
    }

    visual.executor().advance_clock(Duration::from_millis(50));
    redraw(visual);
    let large_view = editor
        .read_with(visual, |editor, _cx| editor.source_surface.clone())
        .expect("large document view");
    let initial_scroll_top =
        large_view.read_with(visual, |view, _cx| view.scroll_top_line_for_test());
    large_view.read_with(visual, |view, _cx| view.scroll_to_line_for_test(4_000));
    // 只提交一帧来启动远端 viewport 读取，不先把 executor 跑到空闲；立即跳回顶部
    // 因而覆盖的是仍在执行且与顶部预取窗不相交的请求，可真实验证取消门禁。
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(
        visual
            .debug_bounds("large-file-retained-frame-progress")
            .is_some(),
        "a disjoint jump must retain the previous ScreenLines instead of painting a blank frame"
    );
    let distant_scroll_top =
        large_view.read_with(visual, |view, _cx| view.scroll_top_line_for_test());
    assert!(distant_scroll_top > initial_scroll_top);
    large_view.read_with(visual, |view, _cx| {
        view.scroll_to_line_for_test(initial_scroll_top)
    });
    redraw(visual);
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.scroll_top_line_for_test()),
        initial_scroll_top
    );
    assert!(
        large_view.read_with(visual, |view, _cx| view.viewport_cancellations_for_test()) > 0,
        "a disjoint jump supersedes the in-flight viewport read"
    );

    let inactive_body = visual
        .debug_bounds("large-file-line-body-0")
        .expect("inactive large source row body");
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
    });
    redraw(visual);
    let active_body = visual
        .debug_bounds("large-file-line-body-0")
        .expect("active large source row body");
    assert_eq!(active_body, inactive_body);
    assert!(
        large_view.read_with(visual, |view, _cx| view.source_row_height_for_test()) > 24.0,
        "large Source must inherit ordinary editor typography instead of the old 22 px row"
    );
    assert!(large_view.read_with(visual, |view, _cx| {
        view.active_edit_for_test()
            .is_some_and(|(_, block)| block.read(_cx).compact_source_host())
    }));
    visual.simulate_keystrokes("ctrl-g");
    redraw(visual);
    assert!(visual.debug_bounds("large-file-navigation-panel").is_some());
    large_view.update(visual, |view, cx| view.close_navigation_for_test(cx));
    redraw(visual);

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
    });
    redraw(visual);
    let focused_scroll_top =
        large_view.read_with(visual, |view, _cx| view.scroll_top_line_for_test());
    visual.simulate_keystrokes("pagedown");
    redraw(visual);
    assert!(
        large_view.read_with(visual, |view, _cx| view.scroll_top_line_for_test())
            > focused_scroll_top
    );
    visual.simulate_keystrokes("pageup");
    redraw(visual);
    assert_eq!(
        large_view.read_with(visual, |view, _cx| view.scroll_top_line_for_test()),
        focused_scroll_top
    );
    assert!(large_view.read_with(visual, |view, _cx| view.source_view_for_test()));
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Source));
    editor.update(visual, |editor, cx| {
        editor.set_view_mode(ViewMode::Preview, cx)
    });
    assert!(editor.read_with(visual, |editor, _cx| editor.view_mode == ViewMode::Source));
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.status_text().to_string())
            .contains("Preview needs a resident Markdown projection")
    );
    editor.update(visual, |editor, cx| {
        editor.set_view_mode(ViewMode::Source, cx)
    });
    assert!(visual.debug_bounds("large-file-find-panel").is_none());
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_find_in_document_action(&crate::components::FindInDocument, window, cx);
        });
    });
    redraw(visual);
    assert!(visual.debug_bounds("large-file-find-panel").is_some());
    assert!(visual.debug_bounds("large-file-search-input").is_some());
    assert!(visual.debug_bounds("large-file-scrollbar").is_some());
    assert!(editor.read_with(visual, |editor, _cx| editor.source_surface.is_some()));
    visual.simulate_input("stale query");
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("line 400");
    visual.run_until_parked();
    assert_eq!(
        large_view.read_with(visual, |view, cx| view.search_text_for_test(cx)),
        "line 400"
    );

    visual.simulate_keystrokes("escape");
    visual.simulate_keystrokes("ctrl-g");
    redraw(visual);
    assert!(visual.debug_bounds("large-file-navigation-panel").is_some());
    assert!(visual.debug_bounds("large-file-navigation-input").is_some());
    visual.simulate_input("400");
    visual.run_until_parked();
    assert_eq!(
        large_view.read_with(visual, |view, cx| view.cursor_position(cx)),
        (400, 1)
    );
    visual.simulate_keystrokes("enter");
    // The key action is delivered while the visual executor parks; draw only after
    // the host callback has removed the transient panel.
    visual.run_until_parked();
    redraw(visual);
    assert!(!large_view.read_with(visual, |view, _cx| view.navigation_visible_for_test()));

    let overflow = visual
        .debug_bounds("status-bar-format-overflow-button")
        .unwrap();
    visual.simulate_click(overflow.center(), Modifiers::default());
    redraw(visual);
    assert!(
        visual
            .debug_bounds("status-bar-large-reopen-utf16-le")
            .is_some(),
        "manual encoding reopen must be reachable from the standard status overflow"
    );
    let line_endings = visual
        .debug_bounds("status-bar-large-line-endings")
        .unwrap();
    visual.simulate_click(line_endings.center(), Modifiers::default());
    redraw(visual);
    assert!(large_view.read_with(visual, |view, _cx| view.line_endings_visible()));

    let overflow = visual
        .debug_bounds("status-bar-format-overflow-button")
        .unwrap();
    visual.simulate_click(overflow.center(), Modifiers::default());
    redraw(visual);
    let follow = visual.debug_bounds("status-bar-large-follow").unwrap();
    visual.simulate_click(follow.center(), Modifiers::default());
    redraw(visual);
    assert!(large_view.read_with(visual, |view, _cx| view.follow_enabled()));

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
    });
    let (_, edit_block) = large_view
        .read_with(visual, |view, _cx| view.active_edit_for_test())
        .expect("active large line edit");
    let cached_before_edit =
        large_view.read_with(visual, |view, _cx| view.source_cache_len_for_test());
    assert!(cached_before_edit > 0);
    let (unchanged_line, unchanged_row_block) = large_view
        .read_with(visual, |view, _cx| {
            view.inactive_source_row_block_for_test()
        })
        .expect("unchanged source row block");
    let line_end = edit_block.read_with(visual, |block, _cx| block.display_text().len());
    edit_block.update(visual, |block, cx| {
        block.replace_text_in_visible_range(line_end..line_end, "x", None, false, cx);
    });
    visual.run_until_parked();
    assert_eq!(
        large_view.read_with(visual, |view, _cx| {
            view.source_row_block_for_test(unchanged_line)
        }),
        Some(unchanged_row_block),
        "byte-range shifts must retain Block entities for rows whose visible text is unchanged"
    );
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.undo_for_test(window, cx));
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
    });
    visual.run_until_parked();
    let (_, edit_block) = large_view
        .read_with(visual, |view, _cx| view.active_edit_for_test())
        .expect("reanchored large line edit after undo");
    edit_block.update(visual, |block, cx| {
        block.replace_text_in_visible_range(5..5, "\n", None, false, cx);
    });
    assert!(
        large_view.read_with(visual, |view, _cx| view.source_cache_len_for_test()) > 0,
        "typing must retain the last painted viewport until replacement rows arrive"
    );
    visual.run_until_parked();
    assert!(large_view.read_with(visual, |view, _cx| view.source_cache_len_for_test()) <= 1_024);
    assert!(large_view.read_with(visual, |view, _cx| view.source_row_is_current_for_test(0)));
    assert!(!large_view.read_with(visual, |view, _cx| view.follow_enabled()));
    assert_eq!(
        large_view
            .read_with(visual, |view, _cx| view.active_edit_for_test())
            .map(|(line, _)| line),
        Some(1)
    );
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.recovered_text_for_test())
            .is_some_and(|text| text.starts_with(b"large\n document line 0\n"))
    );
    assert!(large_view.read_with(visual, |view, _cx| view.error_for_test().is_none()));
    assert!(
        large_view.read_with(visual, |view, _cx| view
            .structure_error_for_test()
            .is_none()),
        "successful Source editing must not show a structured-view warning as an error banner"
    );

    redraw(visual);
    let stable_active_body = visual
        .debug_bounds("large-file-line-body-1")
        .expect("reanchored active row body");
    for _ in 0..3 {
        redraw(visual);
        assert_eq!(
            visual
                .debug_bounds("large-file-line-body-1")
                .expect("stable active row body"),
            stable_active_body,
            "settled viewport rows must not alternate geometry between frames"
        );
    }
    assert!(editor.read_with(visual, |editor, _cx| editor.document_dirty));
    visual.simulate_keystrokes("ctrl-z");
    visual.run_until_parked();
    redraw(visual);
    assert!(!editor.read_with(visual, |editor, _cx| editor.document_dirty));
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.recovered_text_for_test())
            .is_some_and(|text| text.starts_with(b"large document line 0\n"))
    );
    assert!(visual.update(|window, cx| { large_view.read(cx).host_is_focused_for_test(window) }));

    visual.simulate_keystrokes("ctrl-y");
    visual.run_until_parked();
    redraw(visual);
    assert!(editor.read_with(visual, |editor, _cx| editor.document_dirty));
    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| {
            view.begin_line_edit_for_test(400, window, cx)
        });
    });
    large_view.read_with(visual, |view, _cx| view.scroll_to_line_for_test(400));
    redraw(visual);
    let scroll_top_before_save =
        large_view.read_with(visual, |view, _cx| view.scroll_top_line_for_test());
    assert!(scroll_top_before_save > 0);
    visual.simulate_keystrokes("ctrl-s");
    visual.run_until_parked();
    redraw(visual);
    assert!(!editor.read_with(visual, |editor, _cx| editor.document_dirty));
    let scroll_top_after_save =
        large_view.read_with(visual, |view, _cx| view.scroll_top_line_for_test());
    assert!(
        scroll_top_after_save.abs_diff(scroll_top_before_save) <= 2,
        "saving a rebuilt large-file baseline must preserve the visible line anchor: before={scroll_top_before_save}, after={scroll_top_after_save}"
    );
    assert!(
        fs::read(editor.read_with(visual, |editor, _cx| {
            editor.file_path.clone().expect("large source path")
        }))
        .expect("overwritten large document")
        .starts_with(b"large\n document line 0\n")
    );
    visual
        .executor()
        .advance_clock(Duration::from_millis(1_100));
    visual.run_until_parked();
    assert!(
        large_view
            .read_with(visual, |view, _cx| view.pending_external_change_for_test())
            .is_none(),
        "the external monitor must discard a pre-save snapshot"
    );

    visual.update(|window, cx| {
        large_view.update(cx, |view, cx| view.begin_line_edit_for_test(0, window, cx));
    });
    let (_, edit_block) = large_view
        .read_with(visual, |view, _cx| view.active_edit_for_test())
        .expect("active large line edit after save");
    edit_block.update(visual, |block, cx| {
        block.replace_text_in_visible_range(0..0, "saved-as ", None, false, cx);
    });
    visual.run_until_parked();

    let saved_as = temp.path().join("large-shell-saved-as.md");
    let saved_as_for_action = saved_as.clone();
    visual.update(|window, cx| {
        let window_handle = window.window_handle();
        large_view.update(cx, move |view, cx| {
            view.save_as_path(saved_as_for_action, window_handle, cx);
        });
    });
    visual.run_until_parked();
    redraw(visual);
    assert!(!editor.read_with(visual, |editor, _cx| editor.document_dirty));
    assert_eq!(
        editor.read_with(visual, |editor, _cx| editor.file_path.clone()),
        Some(saved_as.clone())
    );
    assert!(
        fs::read(&saved_as)
            .expect("saved large document")
            .starts_with(b"saved-as large\n document line 0\n")
    );
}
