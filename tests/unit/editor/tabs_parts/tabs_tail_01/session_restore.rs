// @author kongweiguang

#[gpui::test]
async fn restoring_workspace_session_installs_order_pin_active_and_root(
    cx: &mut gpui::TestAppContext,
) {
    init_test_app(cx);
    let root = std::env::temp_dir().join(format!("gmark-tab-restore-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let canonical_root = dunce::canonicalize(&root).unwrap();
    let first_path = root.join("first.md");
    let second_path = root.join("second.md");
    let legacy_path = root.join("legacy.md");
    let editor_path = first_path.clone();
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        super::Editor::from_markdown(cx, "first".to_owned(), Some(editor_path))
    });
    editor.update(visual, |editor, cx| {
        editor.restore_tab_session(
            uuid::Uuid::new_v4(),
            vec![
                super::RestoredTab {
                    opened: crate::document_io::OpenedDocument::Resident(
                        crate::document_io::OpenedMarkdown {
                            text: "first".to_owned(),
                            encoding: crate::document_io::DocumentEncoding::Utf8,
                            text_encoding: gmark_document_core::TextEncoding::Utf8 { bom: false },
                            file_identity: None,
                            loading_limits: gmark_document_core::LoadingPolicy::default()
                                .effective_limits(),
                        },
                    ),
                    path: first_path.clone(),
                    pinned: true,
                    view_mode: Some("source".to_owned()),
                    selection: Some(
                        crate::config::workspace_session::WorkspaceSessionSelection {
                            start: 1,
                            end: 1,
                            reversed: false,
                            anchor_affinity: None,
                            head_affinity: None,
                        },
                    ),
                    scroll_x: Some(0.0),
                    scroll_y: Some(-10.0),
                },
                super::RestoredTab {
                    opened: crate::document_io::OpenedDocument::Resident(
                        crate::document_io::OpenedMarkdown {
                            text: "second".to_owned(),
                            encoding: crate::document_io::DocumentEncoding::Utf8,
                            text_encoding: gmark_document_core::TextEncoding::Utf8 { bom: false },
                            file_identity: None,
                            loading_limits: gmark_document_core::LoadingPolicy::default()
                                .effective_limits(),
                        },
                    ),
                    path: second_path.clone(),
                    pinned: false,
                    view_mode: Some("split".to_owned()),
                    selection: Some(
                        crate::config::workspace_session::WorkspaceSessionSelection {
                            start: 3,
                            end: 3,
                            reversed: true,
                            anchor_affinity: None,
                            head_affinity: None,
                        },
                    ),
                    scroll_x: Some(0.0),
                    scroll_y: Some(-42.0),
                },
                super::RestoredTab {
                    opened: crate::document_io::OpenedDocument::Resident(
                        crate::document_io::OpenedMarkdown {
                            text: "legacy".to_owned(),
                            encoding: crate::document_io::DocumentEncoding::Legacy(
                                "windows-1252".to_owned(),
                            ),
                            text_encoding: gmark_document_core::TextEncoding::Legacy(
                                "windows-1252".to_owned(),
                            ),
                            file_identity: None,
                            loading_limits: gmark_document_core::LoadingPolicy::default()
                                .effective_limits(),
                        },
                    ),
                    path: legacy_path.clone(),
                    pinned: false,
                    view_mode: Some("source".to_owned()),
                    selection: None,
                    scroll_x: None,
                    scroll_y: None,
                },
            ],
            1,
            Some(root.clone()),
            Some(318.0),
            Some(false),
            Some(0.62),
            cx,
        );
        assert_eq!(editor.tabs.records.len(), 3);
        assert!(editor.tabs.records[0].pinned);
        assert_eq!(editor.tabs.active, 1);
        assert_eq!(editor.workspace_panel_width(), Some(318.0));
        assert!(!editor.workspace_docked_open_preference());
        assert_eq!(editor.split_pane_ratio, 0.62);
        assert_eq!(editor.source_document.text(), "second");
        assert_eq!(editor.view_mode, ViewMode::Split);
        assert_eq!(editor.last_selection_snapshot.range(), 3..3);
        assert!(
            !editor.last_selection_snapshot.reversed(),
            "collapsed SourceSelection has no directional ordering"
        );
        assert_eq!(f32::from(editor.scroll_handle.offset().y), -42.0);
        let legacy = editor.tabs.records[2].snapshot.as_ref().unwrap();
        assert_eq!(legacy.view_mode, ViewMode::Preview);
        assert!(legacy.show_encoding_conversion_dialog);
        assert_eq!(
            editor.explicit_workspace_root().as_deref(),
            Some(canonical_root.as_path())
        );

        let persisted = editor
            .workspace_session_snapshot_result(cx)
            .expect("canonical workspace session snapshot");
        let persisted_pane = persisted.focused().expect("focused pane");
        assert_eq!(persisted_pane.tabs.len(), 3);
        assert_eq!(persisted_pane.active_tab, Some(persisted_pane.tabs[1].id));
        assert_eq!(
            persisted.workspace_root.as_deref(),
            Some(canonical_root.as_path())
        );
        assert_eq!(persisted.workspace_docked_open, Some(false));
        assert_eq!(persisted.split_pane_ratio, Some(0.62));
        assert_eq!(persisted_pane.tabs[0].state.view_mode.as_deref(), Some("source"));
        assert_eq!(persisted_pane.tabs[1].state.view_mode.as_deref(), Some("split"));
        assert_eq!(persisted_pane.tabs[1].state.scroll_y, Some(-42.0));
        assert_eq!(persisted_pane.tabs[2].state.view_mode.as_deref(), Some("preview"));
    });
    std::fs::remove_dir_all(root).unwrap();
}
