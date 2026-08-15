// @author kongweiguang

/// 校验共享操作区只保留一次顶部留白，并确保按钮没有越过面板边界。
fn assert_standard_dialog_actions(
    visual: &mut VisualTestContext,
    panel_selector: &'static str,
    actions_selector: &'static str,
    button_selectors: &[&'static str],
) {
    let panel = visual.debug_bounds(panel_selector).unwrap();
    let actions = visual.debug_bounds(actions_selector).unwrap();
    assert!(actions.left() >= panel.left(), "{actions_selector} escaped left");
    assert!(actions.right() <= panel.right(), "{actions_selector} escaped right");
    assert!(actions.bottom() <= panel.bottom(), "{actions_selector} escaped bottom");

    let first = visual.debug_bounds(button_selectors[0]).unwrap();
    let top_gap = f32::from(first.top()) - f32::from(actions.top());
    let bottom_gap = f32::from(panel.bottom()) - f32::from(first.bottom());
    // Windows runner 的字体度量与设备像素舍入会产生数个逻辑像素差异；4 px 仍会
    // 拦截旧实现额外 20 px 的底部空白，避免把平台度量差异误判成布局回归。
    let padding_rounding_tolerance = 4.0;
    assert!(
        (top_gap - bottom_gap).abs() <= padding_rounding_tolerance,
        "{panel_selector} action padding should be symmetric: top={top_gap}, bottom={bottom_gap}"
    );

    for selector in button_selectors {
        let button = visual.debug_bounds(selector).unwrap();
        assert_eq!(f32::from(button.size.height), 36.0, "{selector} height");
        assert!(button.left() >= panel.left(), "{selector} escaped left");
        assert!(button.right() <= panel.right(), "{selector} escaped right");
        assert!(button.top() >= actions.top(), "{selector} escaped action top");
        assert!(button.bottom() <= panel.bottom(), "{selector} escaped bottom");
        assert_eq!(button.size.height, first.size.height, "{selector} height mismatch");
    }
}

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
        editor.recovered_session = true;
        cx.notify();
    });
    redraw(visual_cx);
    assert!(visual_cx.debug_bounds("status-bar-recovery").is_none());
    assert!(
        visual_cx
            .debug_bounds("status-bar-recovery-restored-icon")
            .is_none()
    );
    assert!(
        visual_cx
            .debug_bounds("status-bar-recovery-label")
            .is_none()
    );

    editor.update(visual_cx, |editor, cx| {
        editor.recovered_session = false;
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
    assert!(
        f32::from(dialog.size.width) >= 520.0,
        "standard dialogs should leave enough horizontal room for short content"
    );
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
            "{selector} escaped bottom: action={action:?}, dialog={dialog:?}"
        );
    }
    assert_standard_dialog_actions(
        visual_cx,
        "unsaved-changes-dialog",
        "unsaved-changes-actions",
        &[
            "cancel-close-dialog",
            "discard-and-close-dialog",
            "save-and-close-dialog",
        ],
    );

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
    assert!(visual_cx.debug_bounds("about-star-message").is_none());

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
        assert!(action.top() >= dialog.top(), "{selector} escaped top");
        assert!(f32::from(action.size.width) >= 72.0, "{selector}");
        assert_eq!(f32::from(action.size.height), 36.0, "{selector}");
        assert!(
            action.bottom() <= dialog.bottom(),
            "{selector} escaped bottom: action={action:?}, dialog={dialog:?}"
        );
    }
    assert_standard_dialog_actions(
        visual_cx,
        "table-insert-dialog",
        "table-insert-dialog-actions",
        &["cancel-table-insert-dialog", "confirm-table-insert-dialog"],
    );

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
        editor.set_document_dirty_for_test(true);
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
    let window = cx
        .update(|cx| crate::app_menu::open_recovered_editor_window(cx, recovered))
        .unwrap();
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
