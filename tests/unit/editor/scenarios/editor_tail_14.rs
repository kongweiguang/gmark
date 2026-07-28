// @author kongweiguang

#[gpui::test]
async fn external_conflict_reload_replaces_local_document_with_disk_version(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let path = temp_markdown_path("external-reload");
    fs::write(&path, "base").unwrap();
    let editor_path = path.clone();
    let (editor, visual_cx) = cx.add_window_view(move |_window, cx| {
        Editor::from_markdown(cx, "base".to_owned(), Some(editor_path))
    });
    redraw(visual_cx);
    fs::write(&path, "disk version").unwrap();
    visual_cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.sync_source_document_from_projection("local version");
            editor.set_document_dirty_for_test(true);
            assert!(!editor.save_to_existing_path(&path, window, cx));
            editor.on_reload_external_conflict(&ClickEvent::default(), window, cx);
        });
    });
    editor.read_with(visual_cx, |editor, _cx| {
        assert_eq!(editor.source_document.text(), "disk version");
        assert!(!editor.document_dirty);
        assert!(!editor.external_file_conflict);
        assert!(!editor.show_external_conflict_dialog);
    });
    let _ = fs::remove_file(path);
}

#[gpui::test]
async fn plain_text_keeps_all_status_modes_visible_and_opens_in_source(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    cx.update(|cx| crate::updater::UpdateCoordinator::init(false, cx));
    let temp = tempfile::tempdir().expect("plain-text mode switch tempdir");
    let path = temp.path().join("sample.txt");
    fs::write(&path, "alpha\nbeta\n").expect("plain-text fixture");
    let probe =
        gmark_paged_document::probe_file(&path, gmark_paged_document::ProbeOptions::default())
            .expect("plain-text probe");
    let source = gmark_paged_document::FileSource::open(&path).expect("plain-text source");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        Editor::from_source_backed_file(cx, path, probe, source)
    });

    visual.run_until_parked();
    redraw(visual);

    assert_eq!(
        editor.read_with(visual, |editor, _cx| editor.view_mode),
        ViewMode::Source
    );
    assert!(visual.debug_bounds("status-bar-mode-menu").is_none());
    let mode_button = visual.debug_bounds("status-bar-mode-switch").unwrap();
    visual.simulate_click(mode_button.center(), Modifiers::default());
    redraw(visual);
    assert!(visual.debug_bounds("status-bar-mode-menu").is_some());
    for selector in [
        "status-bar-mode-Rendered",
        "status-bar-mode-Source",
        "status-bar-mode-Split",
        "status-bar-mode-Preview",
    ] {
        assert!(
            visual.debug_bounds(selector).is_some(),
            "{selector} must remain visible for plain-text files"
        );
    }
}

#[gpui::test]
async fn external_conflict_save_as_and_cancel_preserve_disk_and_close_intent(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let path = temp_markdown_path("external-save-as-cancel");
    fs::write(&path, "base").unwrap();
    let editor_path = path.clone();
    let (editor, visual_cx) = cx.add_window_view(move |_window, cx| {
        Editor::from_markdown(cx, "base".to_owned(), Some(editor_path))
    });
    redraw(visual_cx);
    fs::write(&path, "disk version").unwrap();
    visual_cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.sync_source_document_from_projection("local version");
            editor.set_document_dirty_for_test(true);
            editor.pending_close_after_save = true;
            editor.save_document(window, cx);
        });
    });
    visual_cx.run_until_parked();
    visual_cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            assert!(editor.show_external_conflict_dialog);
            assert!(editor.pending_close_after_save);
            editor.on_cancel_external_conflict(&ClickEvent::default(), window, cx);
            assert!(!editor.pending_close_after_save);

            editor.save_document(window, cx);
        });
    });
    visual_cx.run_until_parked();
    visual_cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_save_as_external_conflict(&ClickEvent::default(), window, cx);
            assert!(editor.pending_save_as);
            assert!(!editor.show_external_conflict_dialog);
        });
    });
    assert_eq!(fs::read_to_string(&path).unwrap(), "disk version");
    let _ = fs::remove_file(path);
}

#[gpui::test]
async fn external_conflict_handles_deleted_and_invalid_utf8_disk_files(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let path = temp_markdown_path("external-missing-invalid-utf8");
    fs::write(&path, "base").unwrap();
    let editor_path = path.clone();
    let (editor, visual_cx) = cx.add_window_view(move |_window, cx| {
        Editor::from_markdown(cx, "base".to_owned(), Some(editor_path))
    });
    redraw(visual_cx);

    fs::remove_file(&path).unwrap();
    visual_cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.sync_source_document_from_projection("local version");
            editor.set_document_dirty_for_test(true);
            assert!(!editor.save_to_existing_path(&path, window, cx));
            let preview = editor.external_conflict_preview.as_ref().unwrap();
            assert!(preview.disk_error.is_some());
            assert_eq!(preview.disk_line_count, 0);
            editor.cancel_external_conflict(cx);
        });
    });

    fs::write(&path, [0xff, b'a', b'\n', 0xfe]).unwrap();
    visual_cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            assert!(!editor.save_to_existing_path(&path, window, cx));
            let preview = editor.external_conflict_preview.as_ref().unwrap();
            assert_eq!(preview.first_difference_line, Some(1));
            assert!(preview.disk_line.contains('\u{fffd}'));
            assert_eq!(preview.disk_bytes, 4);
        });
    });
    assert_eq!(fs::read(&path).unwrap(), [0xff, b'a', b'\n', 0xfe]);
    let _ = fs::remove_file(path);
}

#[gpui::test]
async fn external_conflict_overwrite_completes_pending_close_save(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let path = temp_markdown_path("external-overwrite-close");
    fs::write(&path, "base").unwrap();
    let editor_path = path.clone();
    let (editor, visual_cx) = cx.add_window_view(move |_window, cx| {
        Editor::from_markdown(cx, "base".to_owned(), Some(editor_path))
    });
    redraw(visual_cx);
    fs::write(&path, "disk version").unwrap();

    visual_cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.sync_source_document_from_projection("local version");
            editor.set_document_dirty_for_test(true);
            editor.pending_close_after_save = true;
            assert!(!editor.save_to_existing_path(&path, window, cx));
            assert!(editor.pending_close_after_save);
            editor.on_overwrite_external_conflict(&ClickEvent::default(), window, cx);
            assert!(editor.allow_external_overwrite_once);
            assert!(editor.save_to_existing_path(&path, window, cx));
            assert!(!editor.pending_close_after_save);
            assert!(!editor.allow_external_overwrite_once);
        });
    });

    assert_eq!(fs::read_to_string(&path).unwrap(), "local version");
    let _ = fs::remove_file(path);
}

#[gpui::test]
async fn external_conflict_dialog_stays_within_small_and_large_window_bounds(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let path = std::env::temp_dir()
        .join("gmark-external-conflict-layout")
        .join("a-very-long-directory-name-without-spaces".repeat(4))
        .join("document-with-a-very-long-name.md");
    let editor_path = path.clone();
    let (editor, visual_cx) = cx.add_window_view(move |_window, cx| {
        Editor::from_markdown(cx, "base".to_owned(), Some(editor_path))
    });
    editor.update(visual_cx, |editor, cx| {
        editor.show_external_conflict_dialog = true;
        editor.external_conflict_preview = Some(super::ExternalConflictPreview {
            path: path.display().to_string(),
            first_difference_line: Some(1),
            local_line: "local ".repeat(80),
            disk_line: "disk ".repeat(80),
            local_line_count: 20,
            disk_line_count: 22,
            local_bytes: 1_024,
            disk_bytes: 1_120,
            disk_error: None,
        });
        cx.notify();
    });

    for viewport in [size(px(720.0), px(520.0)), size(px(1180.0), px(780.0))] {
        visual_cx.simulate_resize(viewport);
        redraw(visual_cx);

        let overlay = visual_cx.debug_bounds("external-conflict-overlay").unwrap();
        let dialog = visual_cx.debug_bounds("external-conflict-dialog").unwrap();
        assert_dialog_title_icon(
            visual_cx,
            "external-conflict-dialog",
            "external-conflict-title-icon",
            "external-conflict-title-label",
        );
        assert!(
            dialog.left() >= overlay.left(),
            "dialog={dialog:?} overlay={overlay:?}"
        );
        assert!(
            dialog.right() <= overlay.right(),
            "dialog={dialog:?} overlay={overlay:?}"
        );
        assert!(
            dialog.top() >= overlay.top(),
            "dialog={dialog:?} overlay={overlay:?}"
        );
        assert!(
            dialog.bottom() <= overlay.bottom(),
            "dialog={dialog:?} overlay={overlay:?}"
        );

        for selector in [
            "external-conflict-path",
            "external-conflict-summary",
            "external-conflict-local",
            "external-conflict-disk",
            "cancel-external-conflict",
            "reload-external-conflict",
            "overwrite-external-conflict",
            "save-as-external-conflict",
        ] {
            let bounds = visual_cx.debug_bounds(selector).unwrap();
            assert!(bounds.left() >= dialog.left(), "{selector} escaped left");
            assert!(bounds.right() <= dialog.right(), "{selector} escaped right");
            assert!(bounds.top() >= dialog.top(), "{selector} escaped top");
        }
        for selector in [
            "cancel-external-conflict",
            "reload-external-conflict",
            "overwrite-external-conflict",
            "save-as-external-conflict",
        ] {
            let action = visual_cx.debug_bounds(selector).unwrap();
            assert!(f32::from(action.size.width) >= 72.0, "{selector}");
            assert_eq!(f32::from(action.size.height), 36.0, "{selector}");
            assert!(action.bottom() <= dialog.bottom(), "{selector}");
        }
    }
}

#[gpui::test]
async fn tab_strip_keeps_only_optional_document_actions(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    cx.update(|cx| {
        crate::config::EditorSettings::init(cx, true, crate::config::AutoSavePreference::Off, true);
        crate::config::EditorSettings::set_show_tab_bar_actions_for_test(cx, true);
    });
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "# gmark\n\nBody\n".to_owned(), None)
    });
    let source = editor.read_with(visual, |editor, _cx| editor.source_document.text());
    let revision = editor.read_with(visual, |editor, _cx| editor.source_document.revision());

    for viewport in [size(px(720.0), px(520.0)), size(px(1180.0), px(780.0))] {
        visual.simulate_resize(viewport);
        redraw(visual);
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
        let toolbar = visual.debug_bounds("document-tab-strip").unwrap();
        let content = visual.debug_bounds("editor-content").unwrap();
        let tab_scroll = visual.debug_bounds("document-tab-scroll").unwrap();
        let active_tab = visual.debug_bounds("document-tab-0").unwrap();
        let new_tab = visual.debug_bounds("document-new-tab").unwrap();
        let trailing_tools = visual.debug_bounds("document-tab-trailing-tools").unwrap();

        assert_eq!(f32::from(toolbar.size.height), 36.0);
        assert_eq!(toolbar.bottom(), content.top());
        assert_eq!(trailing_tools.right(), toolbar.right());
        assert_eq!(tab_scroll.left(), toolbar.left());
        assert!(active_tab.left() >= tab_scroll.left());
        assert!(active_tab.right() <= new_tab.left());
        assert!(new_tab.right() <= tab_scroll.right());
        assert!(new_tab.right() <= trailing_tools.left());
        for selector in [
            "document-toolbar-action-0",
            "document-toolbar-action-1",
            "document-toolbar-action-2",
        ] {
            let action = visual.debug_bounds(selector).unwrap();
            assert_eq!(action.size, size(px(28.0), px(28.0)));
            assert!(action.left() >= toolbar.left());
            assert!(action.right() <= toolbar.right());
            assert!(action.top() >= toolbar.top());
            assert!(action.bottom() <= toolbar.bottom());
        }
        for selector in [
            "document-toolbar-action-0",
            "document-toolbar-action-1",
            "document-toolbar-action-2",
        ] {
            let action = visual.debug_bounds(selector).unwrap();
            assert!(action.left() >= trailing_tools.left());
            assert!(action.right() <= trailing_tools.right());
        }
    }

    let find = visual.debug_bounds("document-toolbar-action-1").unwrap();
    visual.simulate_click(find.center(), Modifiers::default());
    visual.run_until_parked();
    assert!(editor.read_with(visual, |editor, _cx| editor.find_panel.is_some()));
    assert_eq!(
        editor.read_with(visual, |editor, _cx| editor.source_document.text()),
        source
    );
    assert_eq!(
        editor.read_with(visual, |editor, _cx| editor.source_document.revision()),
        revision
    );
    assert!(!editor.read_with(visual, |editor, _cx| editor.document_dirty));
}

#[gpui::test]
async fn tab_strip_defaults_to_clean_document_chrome(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (_editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "# gmark\n".to_owned(), None));
    visual.simulate_resize(size(px(720.0), px(520.0)));
    redraw(visual);

    assert!(visual.debug_bounds("document-tab-0").is_some());
    assert!(visual.debug_bounds("document-new-tab").is_some());
    assert!(visual.debug_bounds("document-toolbar-action-0").is_none());
    assert!(visual.debug_bounds("document-toolbar-action-1").is_none());
    assert!(visual.debug_bounds("document-toolbar-action-2").is_none());
    assert!(visual.debug_bounds("document-tab-leading-tools").is_none());
    assert!(visual.debug_bounds("document-tab-trailing-tools").is_none());
}

#[gpui::test]
async fn split_workspace_uses_compact_overlay_at_two_x_scale(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    cx.update(|cx| {
        crate::config::EditorSettings::init(cx, true, crate::config::AutoSavePreference::Off, true);
        crate::config::EditorSettings::set_status_bar_preferences_for_test(
            cx,
            crate::preferences::StatusBarPreferences {
                custom_buttons: vec![crate::preferences::StatusBarButton {
                    id: "mode".into(),
                    label: "Mode".into(),
                    action_id: "toggle_view_mode".into(),
                }],
                ..crate::preferences::StatusBarPreferences::default()
            },
        );
    });
    let (editor, visual_cx) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "# Heading\n\nBody\n".to_owned(), None)
    });
    editor.update(visual_cx, |editor, cx| {
        editor.workspace.is_open = true;
        editor.set_view_mode(ViewMode::Split, cx);
    });

    for (viewport, compact) in [
        (size(px(1180.0), px(780.0)), false),
        (size(px(820.0), px(620.0)), true),
        (size(px(720.0), px(520.0)), true),
    ] {
        visual_cx.simulate_resize(viewport);
        redraw(visual_cx);
        if compact && !editor.read_with(visual_cx, |editor, _cx| editor.workspace.is_open) {
            editor.update(visual_cx, |editor, cx| {
                editor.workspace.is_open = true;
                cx.notify();
            });
            redraw(visual_cx);
        }
        visual_cx.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));

        let main = visual_cx.debug_bounds("editor-main-content").unwrap();
        let titlebar = visual_cx.debug_bounds("editor-titlebar");
        let status_bar = visual_cx.debug_bounds("status-bar").unwrap();
        let content = visual_cx.debug_bounds("editor-content").unwrap();
        let workspace = visual_cx.debug_bounds("workspace-panel").unwrap();
        let source = visual_cx.debug_bounds("editor-source-pane").unwrap();
        let preview = visual_cx.debug_bounds("split-preview-pane").unwrap();
        let mode_switch = visual_cx.debug_bounds("status-bar-mode-switch").unwrap();
        let line_ending_picker = visual_cx
            .debug_bounds("status-bar-line-ending-button")
            .unwrap();
        let document_sidebar_toggle = visual_cx
            .debug_bounds("status-bar-document-sidebar-toggle")
            .unwrap();
        let sidebar_toggle = visual_cx.debug_bounds("status-bar-sidebar-toggle").unwrap();
        let sidebar_open = editor.read_with(visual_cx, |editor, _cx| editor.workspace.is_open);
        assert!(visual_cx.debug_bounds("workspace-collapse").is_none());
        assert!(
            visual_cx
                .debug_bounds("document-tab-leading-tools")
                .is_none()
        );
        if let Some(titlebar) = titlebar {
            assert_eq!(f32::from(titlebar.size.height), 38.0);
            // main shell 从窗口原点开始并用 padding 为 chrome 留位，真实内容不得进入标题栏。
            assert!(titlebar.bottom() <= content.top());
            assert!(titlebar.bottom() <= workspace.top());
            assert!(
                visual_cx
                    .debug_bounds("editor-titlebar-title-label")
                    .is_none()
            );
            // 客户端标题栏已收敛为空白 drag area；窗口级 gmark 标识由原生
            // 标题栏提供，不再在编辑器树内重复渲染。
            assert!(
                visual_cx
                    .debug_bounds("editor-titlebar-leading-icon")
                    .is_none()
            );
        }
        assert_eq!(f32::from(status_bar.size.height), 24.0);
        assert_eq!(sidebar_toggle.left(), status_bar.left());
        assert_eq!(document_sidebar_toggle.right(), status_bar.right());
        assert!(mode_switch.right() <= document_sidebar_toggle.left());
        assert_eq!(mode_switch.size, size(px(24.0), px(24.0)));
        assert!(line_ending_picker.right() <= mode_switch.left());
        assert!(f32::from(mode_switch.left() - line_ending_picker.right()) <= 2.0);
        assert!(visual_cx.debug_bounds("status-bar-mode-menu").is_none());
        assert!(visual_cx.debug_bounds("status-bar-mode-Split").is_none());
        if sidebar_open {
            let sidebar_indicator = visual_cx
                .debug_bounds("status-bar-sidebar-indicator")
                .unwrap();
            assert!(sidebar_indicator.left() >= sidebar_toggle.left());
            assert!(sidebar_indicator.right() <= sidebar_toggle.right());
            assert!(sidebar_indicator.bottom() <= sidebar_toggle.bottom());
            assert!(
                f32::from(sidebar_toggle.bottom() - sidebar_indicator.bottom()) <= 1.0,
                "sidebar indicator must stay visible inside the status-bar border"
            );
        }
        if f32::from(viewport.width) >= 760.0 {
            for selector in [
                "status-bar-word-count",
                "status-bar-cursor",
                "status-bar-custom-button-mode",
            ] {
                let metadata = visual_cx.debug_bounds(selector).unwrap();
                assert!(
                    metadata.right() <= mode_switch.left(),
                    "{selector} must precede the mode switch"
                );
                assert!(
                    metadata.left() >= status_bar.center().x,
                    "{selector} must stay in the right status group"
                );
            }
        }
        assert!(sidebar_toggle.right() <= mode_switch.left());
        if f32::from(viewport.width) >= 900.0 {
            assert!(
                visual_cx
                    .debug_bounds("status-bar-format-overflow-button")
                    .is_none()
            );
        } else {
            assert!(
                visual_cx
                    .debug_bounds("status-bar-format-overflow-button")
                    .is_some()
            );
        }
        assert!(content.left() >= main.left());
        assert!(content.right() <= main.right());
        assert!(source.left() >= content.left());
        assert!(preview.right() <= content.right());
        assert!(source.right() <= preview.left());

        // 图标按钮使用固定逻辑尺寸，避免长标签、缩放和 hover 状态推动相邻内容。
        for selector in [
            "workspace-tab-files",
            "workspace-tab-search",
        ] {
            let control = visual_cx.debug_bounds(selector).unwrap();
            assert_eq!(f32::from(control.size.width), 32.0, "{selector}");
            assert_eq!(f32::from(control.size.height), 32.0, "{selector}");
            assert!(control.left() >= workspace.left(), "{selector}");
            assert!(control.right() <= workspace.right(), "{selector}");
        }
        for selector in [
            "status-bar-sidebar-toggle",
            "status-bar-mode-switch",
            "status-bar-document-sidebar-toggle",
        ] {
            let control = visual_cx.debug_bounds(selector).unwrap();
            assert_eq!(f32::from(control.size.width), 24.0, "{selector}");
            assert_eq!(f32::from(control.size.height), 24.0, "{selector}");
            assert!(f32::from(control.left()) >= 0.0, "{selector}");
            assert!(
                f32::from(control.right()) <= f32::from(viewport.width),
                "{selector}"
            );
        }
        let overlay = visual_cx.debug_bounds("compact-workspace-overlay");
        if compact {
            let overlay = overlay.expect("compact workspace should render as overlay");
            assert_eq!(f32::from(overlay.size.width), 280.0);
            assert_eq!(content.left(), main.left());
            assert_eq!(content.right(), main.right());
            assert!(workspace.left() >= overlay.left());
            assert!(workspace.right() <= overlay.right());
            assert!(overlay.right() <= main.right());
        } else {
            assert!(
                overlay.is_none(),
                "stale compact overlay at viewport={viewport:?}, main={main:?}, overlay={overlay:?}"
            );
            assert!(workspace.right() <= content.left());
            assert_eq!(f32::from(workspace.size.width), 248.0);
        }
    }

    editor.update(visual_cx, |editor, cx| {
        editor.set_status_sidebar_tooltip_hover(true, cx);
    });
    visual_cx
        .executor()
        .advance_clock(Duration::from_millis(499));
    visual_cx.run_until_parked();
    redraw(visual_cx);
    assert!(
        visual_cx
            .debug_bounds("status-bar-sidebar-tooltip")
            .is_none()
    );
    visual_cx.executor().advance_clock(Duration::from_millis(1));
    visual_cx.run_until_parked();
    redraw(visual_cx);
    let status_tooltip = visual_cx
        .debug_bounds("status-bar-sidebar-tooltip")
        .unwrap();
    let main = visual_cx.debug_bounds("editor-main-content").unwrap();
    assert!(status_tooltip.left() >= main.left());
    assert!(status_tooltip.right() <= main.right());
    editor.update(visual_cx, |editor, cx| {
        editor.set_status_sidebar_tooltip_hover(false, cx);
    });

    editor.update(visual_cx, |editor, cx| {
        editor.set_status_mode_tooltip_hover(ViewMode::Split, true, cx);
    });
    visual_cx
        .executor()
        .advance_clock(Duration::from_millis(500));
    visual_cx.run_until_parked();
    redraw(visual_cx);
    let mode_switch = visual_cx.debug_bounds("status-bar-mode-switch").unwrap();
    let mode_tooltip = visual_cx.debug_bounds("status-bar-mode-tooltip").unwrap();
    let status_bar = visual_cx.debug_bounds("status-bar").unwrap();
    assert!(mode_tooltip.left() <= mode_switch.center().x);
    assert!(mode_tooltip.right() >= mode_switch.center().x);
    assert!(mode_tooltip.right() <= status_bar.right());
    assert!(mode_tooltip.bottom() <= mode_switch.top());
    editor.update(visual_cx, |editor, cx| {
        editor.set_status_mode_tooltip_hover(ViewMode::Split, false, cx);
    });

    let content = visual_cx.debug_bounds("editor-content").unwrap();
    visual_cx.simulate_click(
        point(content.right() - px(12.0), content.center().y),
        Modifiers::default(),
    );
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, _cx| {
        assert!(!editor.workspace.is_open);
    });
    redraw(visual_cx);
    let overflow_button = visual_cx
        .debug_bounds("status-bar-format-overflow-button")
        .unwrap();
    visual_cx.simulate_click(overflow_button.center(), Modifiers::default());
    redraw(visual_cx);
    let popup = visual_cx
        .debug_bounds("status-bar-format-overflow")
        .unwrap();
    let overflow_indicator = visual_cx
        .debug_bounds("status-bar-format-overflow-indicator")
        .unwrap();
    assert!(overflow_indicator.left() >= overflow_button.left());
    assert!(overflow_indicator.right() <= overflow_button.right());
    assert_eq!(overflow_indicator.bottom(), overflow_button.bottom());
    let main = visual_cx.debug_bounds("editor-main-content").unwrap();
    assert!(popup.left() >= main.left());
    assert!(popup.right() <= main.right());
    assert!(popup.top() >= main.top());
    assert!(popup.bottom() <= main.bottom());
    assert!(
        visual_cx
            .debug_bounds("status-bar-overflow-encoding")
            .is_some()
    );
    assert!(
        visual_cx
            .debug_bounds("status-bar-overflow-line-ending")
            .is_none(),
        "line-ending picker stays beside the mode button instead of moving into overflow"
    );
    for selector in [
        "status-bar-word-count",
        "status-bar-cursor",
        "status-bar-custom-button-mode",
    ] {
        let item = visual_cx.debug_bounds(selector).unwrap();
        assert!(item.left() >= popup.left(), "{selector}");
        assert!(item.right() <= popup.right(), "{selector}");
        assert!(item.top() >= popup.top(), "{selector}");
        assert!(item.bottom() <= popup.bottom(), "{selector}");
    }
    let custom = visual_cx
        .debug_bounds("status-bar-custom-button-mode")
        .unwrap();
    assert!(custom.left() >= popup.left());
    assert!(custom.right() <= popup.right());
    editor.update(visual_cx, |editor, cx| {
        editor.status_bar.format_overflow_open = false;
        cx.notify();
    });
    redraw(visual_cx);

    let revision = editor.read_with(visual_cx, |editor, _cx| editor.source_document.revision());
    let mode_button = visual_cx.debug_bounds("status-bar-mode-switch").unwrap();
    assert_eq!(mode_button.size, size(px(24.0), px(24.0)));
    visual_cx.simulate_click(mode_button.center(), Modifiers::default());
    redraw(visual_cx);
    let mode_menu = visual_cx.debug_bounds("status-bar-mode-menu").unwrap();
    assert_eq!(mode_menu.size.width, px(120.0));
    let source_mode = visual_cx.debug_bounds("status-bar-mode-Source").unwrap();
    assert!(source_mode.left() >= mode_menu.left());
    assert!(source_mode.right() <= mode_menu.right());
    visual_cx.simulate_click(source_mode.center(), Modifiers::default());
    visual_cx.run_until_parked();
    redraw(visual_cx);
    editor.update(visual_cx, |editor, _cx| {
        assert_eq!(editor.view_mode, ViewMode::Source);
        assert_eq!(editor.source_document.revision(), revision);
        assert!(!editor.status_bar.mode_menu_open);
    });
    visual_cx.simulate_click(mode_button.center(), Modifiers::default());
    redraw(visual_cx);
    let source_indicator = visual_cx
        .debug_bounds("status-bar-mode-Source-indicator")
        .unwrap();
    assert_eq!(source_indicator.size, size(px(14.0), px(14.0)));

    let source = editor.read_with(visual_cx, |editor, _cx| editor.source_document.text());
    let dirty = editor.read_with(visual_cx, |editor, _cx| editor.document_dirty);
    editor.update_in(visual_cx, |editor, window, _cx| {
        let handle = &editor
            .status_bar
            .mode_focus_handles
            .as_ref()
            .expect("status mode focus handles")[3];
        handle.focus(window);
        assert!(handle.is_focused(window));
    });
    redraw(visual_cx);
    let focused_preview = visual_cx.debug_bounds("status-bar-mode-Preview").unwrap();
    assert_eq!(f32::from(focused_preview.size.height), 30.0);
    visual_cx.simulate_keystrokes("space");
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, _cx| {
        assert_eq!(editor.view_mode, ViewMode::Preview);
        assert_eq!(editor.source_document.text(), source);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });

    visual_cx.simulate_click(mode_button.center(), Modifiers::default());
    redraw(visual_cx);
    editor.update_in(visual_cx, |editor, window, _cx| {
        editor
            .status_bar
            .mode_focus_handles
            .as_ref()
            .expect("status mode focus handles")[0]
            .focus(window);
    });
    visual_cx.simulate_keystrokes("enter");
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, _cx| {
        assert_eq!(editor.view_mode, ViewMode::Rendered);
        assert_eq!(editor.source_document.text(), source);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });

    editor.update_in(visual_cx, |editor, window, _cx| {
        let handle = editor
            .status_bar
            .sidebar_focus_handle
            .as_ref()
            .expect("status sidebar focus");
        handle.focus(window);
        assert!(handle.is_focused(window));
    });
    visual_cx.simulate_keystrokes("space");
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, _cx| {
        assert!(editor.workspace.is_open);
        assert_eq!(editor.source_document.text(), source);
        assert_eq!(editor.source_document.revision(), revision);
    });
    redraw(visual_cx);
    editor.update_in(visual_cx, |editor, window, _cx| {
        editor
            .status_bar
            .sidebar_focus_handle
            .as_ref()
            .expect("status sidebar focus")
            .focus(window);
    });
    visual_cx.simulate_keystrokes("enter");
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, _cx| {
        assert!(!editor.workspace.is_open);
        assert_eq!(editor.document_dirty, dirty);
    });

    redraw(visual_cx);
    editor.update_in(visual_cx, |editor, window, _cx| {
        let handle = editor
            .status_bar
            .overflow_focus_handle
            .as_ref()
            .expect("status overflow focus");
        handle.focus(window);
        assert!(handle.is_focused(window));
    });
    visual_cx.simulate_keystrokes("enter");
    visual_cx.run_until_parked();
    redraw(visual_cx);
    assert!(
        visual_cx
            .debug_bounds("status-bar-format-overflow")
            .is_some()
    );
    let overflow_button = visual_cx
        .debug_bounds("status-bar-format-overflow-button")
        .unwrap();
    assert_eq!(f32::from(overflow_button.size.height), 24.0);
    assert!(f32::from(overflow_button.size.width) >= 28.0);
    visual_cx.simulate_keystrokes("escape");
    visual_cx.run_until_parked();
    editor.update(visual_cx, |editor, _cx| {
        assert!(!editor.status_bar.format_overflow_open);
        assert_eq!(editor.source_document.text(), source);
        assert_eq!(editor.source_document.revision(), revision);
        assert_eq!(editor.document_dirty, dirty);
    });
}
#[gpui::test]
async fn status_bar_line_ending_picker_normalizes_the_document(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        let mut editor = Editor::from_markdown(cx, "alpha\nbeta\n".to_owned(), None);
        editor.set_view_mode(ViewMode::Source, cx);
        editor
    });
    visual.simulate_resize(size(px(960.0), px(640.0)));
    redraw(visual);

    let picker = visual
        .debug_bounds("status-bar-line-ending-button")
        .expect("line-ending picker");
    visual.simulate_click(picker.center(), Modifiers::default());
    redraw(visual);
    let menu = visual
        .debug_bounds("status-bar-line-ending-menu")
        .expect("line-ending menu");
    assert_eq!(f32::from(menu.size.width), 92.0);
    assert_eq!(menu.right(), picker.right());

    let crlf = visual
        .debug_bounds("status-bar-line-ending-crlf")
        .expect("CRLF menu item");
    visual.simulate_click(crlf.center(), Modifiers::default());
    visual.run_until_parked();
    redraw(visual);

    assert_eq!(
        editor.read_with(visual, |editor, _cx| editor
            .source_document
            .serialized_bytes()),
        b"alpha\r\nbeta\r\n"
    );
    assert!(editor.read_with(visual, |editor, _cx| !editor
        .status_bar
        .line_ending_menu_open));
}
