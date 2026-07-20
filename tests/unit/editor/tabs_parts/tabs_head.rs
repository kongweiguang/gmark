// @author kongweiguang

    use std::path::PathBuf;
    use std::time::Duration;

    use gpui::{px, size};
    use gmark_large_document::SourceAffinity;

    use super::{SourceDocument, UndoSelectionSnapshot, ViewMode};

    fn init_test_app(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::i18n::I18nManager::init(cx);
            crate::theme::ThemeManager::init(cx);
            crate::components::init(cx);
        });
    }

    fn add_inactive_tab(editor: &mut super::Editor, text: &str, path: &str) {
        let snapshot = super::Editor::snapshot_for_opened_document(
            crate::document_io::OpenedMarkdown {
                text: text.to_owned(),
                encoding: crate::document_io::DocumentEncoding::Utf8,
            },
            PathBuf::from(path),
        );
        editor.tabs.records.push(super::TabRecord {
            id: uuid::Uuid::new_v4(),
            pinned: false,
            snapshot: Some(snapshot),
        });
    }

    #[gpui::test]
    async fn new_tab_button_keeps_layout_stable_and_isolates_document_state(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test_app(cx);
        cx.update(|cx| {
            crate::config::EditorSettings::init(
                cx,
                true,
                crate::config::AutoSavePreference::Off,
                true,
            );
            crate::config::EditorSettings::set_show_tab_bar_actions_for_test(cx, true);
        });
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "draft body".to_owned(), None)
        });
        visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
        editor.update(visual, |editor, _cx| {
            editor.document_dirty = true;
        });
        visual.update(|window, cx| window.draw(cx).clear());

        let strip_before = visual.debug_bounds("document-tab-strip").unwrap();
        let button = visual.debug_bounds("document-new-tab").unwrap();
        let leading_before = visual.debug_bounds("document-tab-leading-0").unwrap();
        let title_before = visual.debug_bounds("document-tab-title-0").unwrap();
        let close_before = visual.debug_bounds("document-tab-close-0").unwrap();
        assert_eq!(f32::from(strip_before.size.height), super::TAB_STRIP_HEIGHT);
        assert_eq!(f32::from(button.size.width), 28.0);
        assert_eq!(f32::from(button.size.height), 28.0);
        assert_eq!(leading_before.size, size(px(16.0), px(16.0)));
        assert!(title_before.left() > leading_before.right());
        assert!(title_before.right() <= close_before.left());
        let first_tab = visual.debug_bounds("document-tab-0").unwrap();
        let trailing_tools = visual.debug_bounds("document-tab-trailing-tools").unwrap();
        assert!(button.left() >= first_tab.right());
        assert!(button.right() <= trailing_tools.left());

        editor.update(visual, |editor, cx| {
            assert!(editor.toggle_pin_tab(0, cx));
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let leading_after_pin = visual.debug_bounds("document-tab-leading-0").unwrap();
        let title_after_pin = visual.debug_bounds("document-tab-title-0").unwrap();
        assert_eq!(leading_after_pin, leading_before);
        assert_eq!(title_after_pin.left(), title_before.left());
        assert_eq!(title_after_pin.right(), title_before.right());

        visual.simulate_click(button.center(), gpui::Modifiers::default());
        visual.run_until_parked();
        editor.update(visual, |editor, cx| {
            assert_eq!(editor.tabs.records.len(), 2);
            assert_eq!(editor.tabs.active, 1);
            assert_eq!(editor.source_document.text(), "");
            assert!(editor.file_path.is_none());
            assert!(!editor.document_dirty);
            let first = editor.tabs.records[0].snapshot.as_ref().unwrap();
            assert_eq!(first.source_document.text(), "draft body");
            assert!(first.document_dirty);

            assert!(editor.switch_to_tab_index(0, cx));
            assert_eq!(editor.source_document.text(), "draft body");
            assert!(editor.document_dirty);
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let strip_after = visual.debug_bounds("document-tab-strip").unwrap();
        assert_eq!(strip_before.top(), strip_after.top());
        assert_eq!(strip_before.bottom(), strip_after.bottom());

        editor.update(visual, |editor, cx| {
            for index in 0..12 {
                add_inactive_tab(editor, "body", &format!("overflow-{index}.md"));
            }
            cx.notify();
        });
        visual.update(|window, cx| window.draw(cx).clear());
        let scroll = visual.debug_bounds("document-tab-scroll").unwrap();
        let scrolling_button = visual.debug_bounds("document-new-tab").unwrap();
        let trailing_tools = visual.debug_bounds("document-tab-trailing-tools").unwrap();
        assert!(scrolling_button.left() >= scroll.left());
        assert!(scrolling_button.right() > scroll.right());
        assert!(scroll.right() <= trailing_tools.left());
        assert_eq!(trailing_tools.right(), strip_after.right());
    }

    #[gpui::test]
    async fn switching_tabs_restores_document_history_view_and_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "first".to_owned(), None)
        });
        visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
        editor.update(visual, |editor, cx| {
            let first = editor.capture_active_tab(cx);
            let mut second = first;
            second.source_document = SourceDocument::new("second");
            second.last_stable_source_text = "second".to_owned();
            second.view_mode = ViewMode::Source;
            second.selection = UndoSelectionSnapshot::collapsed(3, SourceAffinity::Before);
            editor.tabs.records.push(super::TabRecord {
                id: uuid::Uuid::new_v4(),
                pinned: false,
                snapshot: Some(second),
            });
            // Restore the first snapshot as active after the fixture used ownership transfer.
            let first_snapshot = editor.tabs.records[1].snapshot.as_ref().unwrap();
            editor.source_document = SourceDocument::new("first");
            editor.last_stable_source_text = "first".to_owned();
            let _ = first_snapshot;
            assert!(editor.switch_to_tab_index(1, cx));
            assert_eq!(editor.source_document.text(), "second");
            assert_eq!(editor.view_mode, ViewMode::Source);
            assert_eq!(editor.last_selection_snapshot.range(), 3..3);
            assert_eq!(editor.inactive_tab_count(), 1);
            assert!(editor.switch_to_tab_index(0, cx));
            assert_eq!(editor.source_document.text(), "first");
        });
    }

    #[gpui::test]
    async fn closing_inactive_clean_tab_preserves_active_document(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "active".to_owned(), None)
        });
        editor.update(visual, |editor, cx| {
            add_inactive_tab(editor, "inactive", "inactive.md");
            editor.request_close_tab_index(1, cx);
            assert_eq!(editor.tabs.records.len(), 1);
            assert_eq!(editor.tabs.active, 0);
            assert_eq!(editor.source_document.text(), "active");
            assert_eq!(editor.tabs.closed.len(), 1);
        });
    }

    #[gpui::test]
    async fn dirty_close_cancel_then_discard_is_loss_explicit(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "dirty".to_owned(), None)
        });
        editor.update(visual, |editor, cx| {
            add_inactive_tab(editor, "survivor", "survivor.md");
            editor.document_dirty = true;
            editor.request_close_tab_index(0, cx);
            assert!(editor.tabs.show_close_dialog);
        });
        visual.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.on_cancel_tab_close(&gpui::ClickEvent::default(), window, cx);
                assert!(!editor.tabs.show_close_dialog);
                assert!(editor.document_dirty);
                assert_eq!(editor.tabs.records.len(), 2);

                editor.request_close_tab_index(0, cx);
                editor.on_discard_tab_close(&gpui::ClickEvent::default(), window, cx);
                assert_eq!(editor.tabs.records.len(), 1);
                assert_eq!(editor.source_document.text(), "survivor");
                assert!(editor.tabs.closed.is_empty());
            });
        });
    }

    #[gpui::test]
    async fn clean_close_and_reopen_restores_document(cx: &mut gpui::TestAppContext) {
        init_test_app(cx);
        let (editor, visual) = cx.add_window_view(|_window, cx| {
            super::Editor::from_markdown(cx, "first".to_owned(), None)
        });
        editor.update(visual, |editor, cx| {
            add_inactive_tab(
                editor,
                "second",
                "a-very-long-document-name-that-must-truncate-without-moving-actions.md",
            );
            editor.request_close_tab_index(0, cx);
            assert_eq!(editor.source_document.text(), "second");
            assert_eq!(editor.tabs.closed.len(), 1);
        });
        visual.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.on_reopen_closed_tab_action(&crate::components::ReopenClosedTab, window, cx);
                assert_eq!(editor.tabs.records.len(), 2);
                assert_eq!(editor.tabs.active, 1);
                assert_eq!(editor.source_document.text(), "first");
                assert!(editor.tabs.closed.is_empty());
            });
        });
    }
