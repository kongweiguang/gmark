// @author kongweiguang

#[test]
fn shared_document_window_close_state_only_prompts_on_last_window_lease() {
    let document_id = gmark_document_runtime::DocumentId::new();
    let state = crate::editor::close::EditorDocumentCloseState {
        document_id,
        dirty: true,
        global_lease_count: 2,
        window_lease_count: 1,
    };
    assert!(!state.closes_last_lease());

    let last_window_state = crate::editor::close::EditorDocumentCloseState {
        window_lease_count: 2,
        ..state
    };
    assert!(last_window_state.closes_last_lease());
}

#[gpui::test]
async fn explicit_close_and_quit_keep_distinct_session_intent(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let (editor, visual) = cx
        .add_window_view(|_window, cx| super::Editor::from_markdown(cx, "dirty".to_owned(), None));
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.set_document_dirty_for_test(true);
            assert!(!editor.on_window_should_close(window, cx));
            assert!(editor.tabs.remove_session_after_window_close);
            editor.on_cancel_close_dialog(&gpui::ClickEvent::default(), window, cx);
            assert!(!editor.tabs.remove_session_after_window_close);

            assert!(!editor.on_window_should_close_for_quit(window, cx));
            assert!(!editor.tabs.remove_session_after_window_close);
        });
    });
}

#[gpui::test]
async fn window_bounds_observer_populates_workspace_session_snapshot(
    cx: &mut gpui::TestAppContext,
) {
    init_test_app(cx);
    let path = PathBuf::from("window-state.md");
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        super::Editor::from_markdown(cx, "window state".to_owned(), Some(path))
    });
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.install_workspace_session_window_observer(window, cx);
            let snapshot = editor
                .workspace_session_snapshot_result(cx)
                .expect("canonical workspace session snapshot");
            let restored = snapshot
                .window
                .expect("window placement should be captured");
            assert!(restored.width > 0.0);
            assert!(restored.height > 0.0);
            assert_eq!(
                restored.state,
                crate::config::workspace_session::WorkspaceSessionWindowState::Windowed
            );
        });
    });
}

#[gpui::test]
async fn registry_sessions_open_as_independent_editor_windows(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let root = std::env::temp_dir().join(format!("gmark-window-registry-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let first = root.join("first.md");
    let second = root.join("second.md");
    std::fs::write(&first, "first window").unwrap();
    std::fs::write(&second, "second window").unwrap();
    let sessions = [first.clone(), second.clone()].map(|path| {
        let mut session = crate::config::workspace_session::WorkspaceSession::single_pane(
            uuid::Uuid::new_v4(),
            Some(root.clone()),
        );
        let pane_id = session.focused_pane;
        let tab = crate::config::workspace_session::WorkspaceSessionTab::new(path, false);
        session.panes.insert(
            pane_id,
            crate::config::workspace_session::WorkspaceSessionPane::new(
                vec![tab.clone()],
                Some(tab.id),
            ),
        );
        session
    });
    cx.update(|cx| {
        for session in sessions {
            assert!(crate::app_menu::open_workspace_session_window(cx, session));
        }
        assert_eq!(cx.windows().len(), 2);
    });
    std::fs::remove_dir_all(root).unwrap();
}

#[gpui::test]
async fn multiple_recovery_journals_open_as_dirty_tabs(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let recovery_dir = tempfile::tempdir().unwrap();
    let mut first =
        crate::recovery::RecoveryJournal::create(recovery_dir.path(), None, "alpha".to_owned())
            .unwrap();
    first
        .record(
            "alpha recovered",
            crate::recovery::RecoverySelection {
                start: 2,
                end: 2,
                reversed: false,
                anchor_affinity: None,
                head_affinity: None,
            },
            "source",
        )
        .unwrap();
    let mut second =
        crate::recovery::RecoveryJournal::create(recovery_dir.path(), None, "beta".to_owned())
            .unwrap();
    second
        .record(
            "beta recovered",
            crate::recovery::RecoverySelection {
                start: 3,
                end: 3,
                reversed: false,
                anchor_affinity: None,
                head_affinity: None,
            },
            "split",
        )
        .unwrap();
    let recovered = crate::recovery::load_recovery_documents(recovery_dir.path()).unwrap();
    let handle = cx
        .update(|cx| crate::app_menu::open_recovered_editor_tabs_window(cx, recovered))
        .expect("recovery window");
    handle
        .update(cx, |editor, _window, cx| {
            assert_eq!(editor.tabs.records.len(), 2);
            assert!(editor.document_dirty);
            assert!(editor.recovered_session);
            assert!(
                editor.tabs.records[1]
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.document_dirty && snapshot.recovered_session)
            );
            let first_source = editor.source_document.text();
            assert!(editor.switch_to_tab_index(1, cx));
            let second_source = editor.source_document.text();
            let mut sources = vec![first_source, second_source];
            sources.sort();
            assert_eq!(sources, vec!["alpha recovered", "beta recovered"]);
            assert!(matches!(
                editor.view_mode,
                ViewMode::Source | ViewMode::Split
            ));
        })
        .unwrap();
}

#[test]
fn restored_selection_clamps_to_utf8_boundaries_and_document_end() {
    let selection = crate::config::workspace_session::WorkspaceSessionSelection {
        start: 2,
        end: usize::MAX,
        reversed: true,
        anchor_affinity: None,
        head_affinity: None,
    };
    let restored = super::Editor::restored_selection("你a", Some(&selection));
    assert_eq!(restored.range(), 0..4);
    assert!(restored.reversed());
}

#[test]
fn legacy_view_ids_are_case_insensitive_and_structure_maps_to_preview() {
    for value in ["Source", "source"] {
        assert_eq!(
            super::Editor::restored_view_mode(Some(value)),
            ViewMode::Source
        );
    }
    for value in ["Preview", "Structure", "structure"] {
        assert_eq!(
            super::Editor::restored_view_mode(Some(value)),
            ViewMode::Preview
        );
    }
    assert_eq!(
        super::Editor::restored_view_mode(Some("Split")),
        ViewMode::Split
    );
    assert_eq!(
        super::Editor::restored_view_mode(Some("Live")),
        ViewMode::Rendered
    );
}
