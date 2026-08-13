// @author kongweiguang

use std::path::PathBuf;
use std::time::Duration;

use gmark_document_core::SourceAffinity;
use gpui::{Modifiers, point, px, size};
use image::{ImageBuffer, Rgba};

use super::{ClosedTabSnapshot, DocumentKind, SourceDocument, UndoSelectionSnapshot, ViewMode};

fn init_test_app(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::i18n::I18nManager::init(cx);
        crate::theme::ThemeManager::init(cx);
        crate::components::init(cx);
        if cx
            .try_global::<crate::app::document_service::DocumentService>()
            .is_none()
        {
            cx.set_global(crate::app::document_service::DocumentService::new());
        }
    });
}

fn add_inactive_tab(editor: &mut super::Editor, text: &str, path: &str) {
    let snapshot = super::Editor::snapshot_for_opened_document(
        crate::document_io::OpenedMarkdown {
            text: text.to_owned(),
            encoding: crate::document_io::DocumentEncoding::Utf8,
            text_encoding: gmark_document_core::TextEncoding::Utf8 { bom: false },
            file_identity: None,
            loading_limits: gmark_document_core::LoadingPolicy::default().effective_limits(),
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
        crate::config::EditorSettings::init(cx, true, crate::config::AutoSavePreference::Off, true);
        crate::config::EditorSettings::set_show_tab_bar_actions_for_test(cx, true);
    });
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        super::Editor::from_markdown(cx, "draft body".to_owned(), None)
    });
    visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
    editor.update(visual, |editor, _cx| {
        editor.set_document_dirty_for_test(true);
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
    assert!(visual.debug_bounds("document-tab-open-bottom-0").is_none());
    let trailing_tools = visual.debug_bounds("document-tab-trailing-tools").unwrap();
    assert!(button.left() >= first_tab.right());
    assert!(button.left() >= trailing_tools.left());
    assert!(button.right() <= trailing_tools.right());

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
    visual.update(|window, cx| window.draw(cx).clear());
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
    let untyped = visual
        .debug_bounds("new-tab-untyped")
        .expect("untyped new-tab choice");
    visual.simulate_click(untyped.center(), gpui::Modifiers::default());
    visual.run_until_parked();
    editor.update(visual, |editor, cx| {
        assert_eq!(editor.tabs.records.len(), 2);
        assert_eq!(editor.tabs.active, 1);
        assert_eq!(editor.source_document.text(), "");
        assert!(editor.file_path.is_none());
        assert!(!editor.document_dirty);
        assert_eq!(editor.view_mode, ViewMode::Source);
        assert_eq!(editor.document_kind, DocumentKind::Unspecified);
        assert_eq!(editor.document_kind.icon(), "icon/ui/file.svg");
        assert_eq!(editor.save_dialog_defaults().1.as_deref(), Some("Untitled"));
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
async fn new_json_and_csv_tabs_use_format_document_hosts(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| super::Editor::from_markdown(cx, String::new(), None));

    editor.update(visual, |editor, cx| {
        assert!(editor.new_document_tab(DocumentKind::Json, cx));
        assert_eq!(editor.view_mode, ViewMode::Source);
        let json_host = editor
            .document_host
            .clone()
            .expect("new JSON tab must use the format document host");
        assert!(json_host.read(cx).is_json_document());
        assert!(json_host.read(cx).has_registered_structure_view());
        assert!(json_host.read(cx).source_view_for_test());
        assert_eq!(json_host.read(cx).source_text_for_test(), "{\n}\n");

        assert!(editor.new_document_tab(DocumentKind::Csv, cx));
        assert_eq!(editor.view_mode, ViewMode::Source);
        let csv_host = editor
            .document_host
            .clone()
            .expect("new CSV tab must use the format document host");
        assert!(csv_host.read(cx).is_delimited_document());
        assert!(csv_host.read(cx).has_registered_structure_view());
        assert!(csv_host.read(cx).source_view_for_test());
        assert_eq!(
            csv_host.read(cx).source_text_for_test(),
            "Column 1,Column 2\n"
        );

        assert!(editor.switch_to_tab_index(1, cx));
        assert_eq!(editor.document_host.as_ref(), Some(&json_host));
        assert!(json_host.read(cx).is_json_document());
        assert!(json_host.read(cx).has_registered_structure_view());
    });
}

#[gpui::test]
async fn switching_tabs_restores_document_history_view_and_selection(
    cx: &mut gpui::TestAppContext,
) {
    init_test_app(cx);
    let (editor, visual) = cx
        .add_window_view(|_window, cx| super::Editor::from_markdown(cx, "first".to_owned(), None));
    visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
    editor.update(visual, |editor, cx| {
        let first = editor.capture_active_tab(cx);
        let mut second = first;
        second.source_document = SourceDocument::new("second").into();
        second.view_mode = ViewMode::Source;
        second.selection = UndoSelectionSnapshot::collapsed(3, SourceAffinity::Before);
        editor.tabs.records.push(super::TabRecord {
            id: uuid::Uuid::new_v4(),
            pinned: false,
            snapshot: Some(second),
        });
        // Restore the first snapshot as active after the fixture used ownership transfer.
        let first_snapshot = editor.tabs.records[1].snapshot.as_ref().unwrap();
        editor.source_document = SourceDocument::new("first").into();
        editor.last_stable_source = crate::editor::HistorySource::capture(
            editor.source_document.snapshot(),
            "first".to_owned(),
        );
        let _ = first_snapshot;
        assert!(editor.switch_to_tab_index(1, cx));
        assert_eq!(editor.source_document.text(), "second");
        assert!(editor.last_stable_source.matches_text("second"));
        assert_eq!(editor.view_mode, ViewMode::Source);
        assert_eq!(editor.last_selection_snapshot.range(), 3..3);
        assert_eq!(editor.inactive_tab_count(), 1);
        assert!(editor.switch_to_tab_index(0, cx));
        assert_eq!(editor.source_document.text(), "first");
    });
}

#[test]
fn closed_tab_budget_evicts_oldest_complete_snapshots() {
    let snapshot = |text: &str, path: &str| {
        let document = super::Editor::snapshot_for_opened_document(
            crate::document_io::OpenedMarkdown {
                text: text.to_owned(),
                encoding: crate::document_io::DocumentEncoding::Utf8,
                text_encoding: gmark_document_core::TextEncoding::Utf8 { bom: false },
                file_identity: None,
                loading_limits: gmark_document_core::LoadingPolicy::default().effective_limits(),
            },
            PathBuf::from(path),
        );
        ClosedTabSnapshot::from_document(document).expect("closed snapshot conversion")
    };
    let mut closed = vec![
        snapshot("aaaa", "oldest.md"),
        snapshot("bbbb", "middle.md"),
        snapshot("cccc", "latest.md"),
    ];

    // Closed-tab history retains only reopen metadata (path/identity and view
    // state), never the resident source body.  Budget exactly the two newest
    // metadata records so the oldest is evicted without reintroducing inline
    // text storage into the history contract.
    let newest_metadata_budget =
        closed[1].retained_source_bytes() + closed[2].retained_source_bytes();
    super::enforce_closed_tab_budget(&mut closed, 20, newest_metadata_budget);
    assert_eq!(closed.len(), 2);
    assert_eq!(
        closed[0].file_path.as_deref(),
        Some(std::path::Path::new("middle.md"))
    );
    assert_eq!(
        closed[1].file_path.as_deref(),
        Some(std::path::Path::new("latest.md"))
    );

    super::enforce_closed_tab_budget(&mut closed, 1, usize::MAX);
    assert_eq!(closed.len(), 1);
    assert_eq!(
        closed[0].file_path.as_deref(),
        Some(std::path::Path::new("latest.md"))
    );
}

#[gpui::test]
async fn closing_inactive_clean_tab_preserves_active_document(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let (editor, visual) = cx
        .add_window_view(|_window, cx| super::Editor::from_markdown(cx, "active".to_owned(), None));
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
    let (editor, visual) = cx
        .add_window_view(|_window, cx| super::Editor::from_markdown(cx, "dirty".to_owned(), None));
    editor.update(visual, |editor, cx| {
        add_inactive_tab(editor, "survivor", "survivor.md");
        editor.set_document_dirty_for_test(true);
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
    let tempdir = tempfile::tempdir().expect("closed tab fixture");
    let path = tempdir.path().join("closed.md");
    std::fs::write(&path, "first").expect("write closed tab fixture");
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        super::Editor::from_markdown(cx, "first".to_owned(), Some(path.clone()))
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
    std::fs::write(&path, "updated").expect("update closed tab fixture");
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_reopen_closed_tab_action(&crate::components::ReopenClosedTab, window, cx);
            assert_eq!(editor.tabs.records.len(), 2);
            assert_eq!(editor.tabs.active, 1);
            assert_eq!(editor.source_document.text(), "updated");
            assert!(editor.tabs.closed.is_empty());
        });
    });
}

#[gpui::test]
async fn recovery_closed_tab_reopens_from_journal_without_body_cache(
    cx: &mut gpui::TestAppContext,
) {
    init_test_app(cx);
    let recovery_dir = tempfile::tempdir().expect("recovery fixture");
    let mut journal =
        crate::recovery::RecoveryJournal::create(recovery_dir.path(), None, "base".to_owned())
            .expect("create recovery journal");
    let format = SourceDocument::new("journal body").source_format();
    let _ = journal
        .record_formatted(
            "journal body",
            format,
            crate::recovery::RecoverySelection {
                start: 0,
                end: 0,
                reversed: false,
                anchor_affinity: None,
                head_affinity: None,
            },
            "rendered",
        )
        .expect("write recovery journal");
    drop(journal);
    let recovered = crate::recovery::load_recovery_documents(recovery_dir.path())
        .expect("load recovery journal")
        .into_iter()
        .next()
        .expect("recovered document");
    let document_id = gmark_document_runtime::DocumentId::from_uuid(
        uuid::Uuid::parse_str(&recovered.document_id).expect("recovery document id"),
    );
    let service = cx.update(|cx| {
        cx.global::<crate::app::document_service::DocumentService>()
            .clone()
    });
    let source = crate::app::document_service::ResidentMarkdownSource::from_recovered(
        recovered.source.clone(),
        recovered.file_path.clone(),
        recovered.source_format.clone(),
    )
    .expect("build recovery source");
    let shared = service
        .open_recovery(document_id, source)
        .expect("open shared recovery");
    let recovered_for_editor = recovered.clone();
    let (editor, visual) = cx.add_window_view(move |_window, cx| {
        super::Editor::from_shared_recovery(cx, shared, recovered_for_editor)
            .expect("construct recovery editor")
    });
    editor.update(visual, |editor, cx| {
        let closed = editor.capture_active_tab(cx);
        editor.push_closed_tab(closed, cx);
        assert_eq!(editor.tabs.closed.len(), 1);
        editor.tabs.closed[0].source = super::ClosedDocumentSource::Recovery {
            document_id,
            journal_path: Some(recovered.journal_path.clone()),
        };
    });
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_reopen_closed_tab_action(&crate::components::ReopenClosedTab, window, cx);
            assert_eq!(editor.source_document.text(), "journal body");
            assert!(editor.tabs.closed.is_empty());
        });
    });
}

#[gpui::test]
async fn untitled_closed_tab_without_recovery_fails_closed(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| super::Editor::from_markdown(cx, "only".to_owned(), None));
    editor.update(visual, |editor, cx| {
        let closed = editor.capture_active_tab(cx);
        editor.push_closed_tab(closed, cx);
        assert_eq!(editor.tabs.closed.len(), 1);
    });
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_reopen_closed_tab_action(&crate::components::ReopenClosedTab, window, cx);
            assert_eq!(editor.source_document.text(), "");
            assert!(editor.tabs.closed.is_empty());
        });
    });
}

#[gpui::test]
async fn image_tab_uses_preview_mode_and_survives_tab_switches(cx: &mut gpui::TestAppContext) {
    init_test_app(cx);
    let image_dir = tempfile::tempdir().expect("image preview tempdir");
    let image_path = image_dir.path().join("workspace-preview.png");
    ImageBuffer::from_pixel(1_600, 640, Rgba([24u8, 96, 180, 255]))
        .save(&image_path)
        .expect("write image preview fixture");
    let (editor, visual) = cx
        .add_window_view(|_window, cx| super::Editor::from_markdown(cx, "draft".to_owned(), None));

    editor.update(visual, |editor, cx| {
        editor.install_image_preview_tab(image_path.clone(), cx);
        assert_eq!(editor.image_preview_path.as_ref(), Some(&image_path));
        assert_eq!(editor.file_path.as_ref(), Some(&image_path));
        assert_eq!(editor.view_mode, ViewMode::Preview);
        assert!(!editor.is_document_dirty());
        editor.image_preview_zoom = 1.5;

        assert!(editor.switch_to_tab_index(0, cx));
        assert!(editor.image_preview_path.is_none());
        assert!(editor.switch_to_tab_index(1, cx));
        assert_eq!(editor.image_preview_path.as_ref(), Some(&image_path));
        assert_eq!(editor.view_mode, ViewMode::Preview);
        assert_eq!(editor.image_preview_zoom, 1.5);
    });
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("image-preview").is_some());
    assert!(visual.debug_bounds("image-preview-content").is_some());
    assert!(visual.debug_bounds("image-preview-zoom-toolbar").is_some());
    assert!(visual.debug_bounds("image-preview-zoom-out").is_some());
    assert!(visual.debug_bounds("image-preview-actual-size").is_some());
    assert!(visual.debug_bounds("image-preview-zoom-in").is_some());
    assert!(visual.debug_bounds("image-preview-fit-width").is_some());
    let canvas_before = visual
        .debug_bounds("image-preview-canvas")
        .expect("image preview canvas");
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: canvas_before.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-120.0))),
        modifiers: Modifiers {
            control: true,
            ..Modifiers::default()
        },
        ..Default::default()
    });
    visual.update(|window, cx| window.draw(cx).clear());
    let canvas_after = visual
        .debug_bounds("image-preview-canvas")
        .expect("zoomed image preview canvas");
    assert!(canvas_after.size.width > canvas_before.size.width);
}
