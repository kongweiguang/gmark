// @author kongweiguang

//! Window-close behavior for dirty documents mounted in multiple panes.

use super::*;

// Reason: Keep shared-document discard coverage grouped with the pane-close scenarios so the
// main tail file stays readable without changing the behavior exercised by the test.
#[gpui::test]
async fn pane_window_discard_clears_shared_dirty_document(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "draft".to_owned(), None));

    editor.update(visual, |editor, cx| {
        editor.split_pane_toward(crate::editor::panes::PaneSplitDirection::Right, cx)
    });
    visual.run_until_parked();
    redraw(visual);

    let pane_editors = editor.read_with(visual, |editor, cx| {
        editor
            .pane_canvas_entities
            .borrow()
            .values()
            .filter_map(|(_, _, canvas)| match canvas {
                crate::editor::panes::PaneCanvasEntity::Markdown(canvas) => {
                    Some(canvas.read(cx).editor())
                }
                crate::editor::panes::PaneCanvasEntity::DocumentHost(_)
                | crate::editor::panes::PaneCanvasEntity::ReadOnly(_) => None,
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(pane_editors.len(), 2);
    pane_editors[0].update(visual, |pane_editor, _cx| {
        pane_editor.set_document_dirty_for_test(true);
    });
    visual.run_until_parked();

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            assert!(!editor.on_window_should_close(window, cx));
            assert!(editor.show_unsaved_changes_dialog);
            editor.on_discard_and_close(&gpui::ClickEvent::default(), window, cx);
        });
    });

    for pane_editor in pane_editors {
        assert!(!pane_editor.read_with(visual, |pane_editor, _cx| {
            pane_editor.source_document.is_dirty()
        }));
    }
}

// Reason: Preserve the focused-pane distinction while checking that a background dirty tab still
// participates in the window-close confirmation and discard path.
#[gpui::test]
async fn pane_window_close_prompts_for_background_dirty_document(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "focused".to_owned(), None));

    editor.update(visual, |editor, cx| {
        editor.split_pane_toward(crate::editor::panes::PaneSplitDirection::Right, cx)
    });
    visual.run_until_parked();
    redraw(visual);

    let (focused_pane, background_pane) = editor.read_with(visual, |editor, cx| {
        let workspace = editor.pane_workspace.as_ref().unwrap().read(cx);
        let focused = workspace.workspace().focused_pane();
        let other = workspace
            .workspace()
            .pane_ids()
            .into_iter()
            .find(|pane| *pane != focused)
            .expect("background pane");
        (focused, other)
    });
    editor.update(visual, |editor, cx| {
        assert!(editor.new_document_tab_in_pane(background_pane, DocumentKind::Markdown, cx));
        editor
            .pane_workspace
            .as_ref()
            .expect("pane workspace")
            .update(cx, |workspace, _cx| {
                workspace.workspace_mut().focus(focused_pane).unwrap();
            });
    });
    visual.run_until_parked();
    redraw(visual);

    let background_editor = editor.read_with(visual, |editor, cx| {
        let canvases = editor.pane_canvas_entities.borrow();
        let (_, _, canvas) = canvases.get(&background_pane).expect("background canvas");
        match canvas {
            crate::editor::panes::PaneCanvasEntity::Markdown(canvas) => canvas.read(cx).editor(),
            crate::editor::panes::PaneCanvasEntity::DocumentHost(_)
            | crate::editor::panes::PaneCanvasEntity::ReadOnly(_) => {
                panic!("background markdown canvas expected")
            }
        }
    });
    background_editor.update(visual, |pane_editor, _cx| {
        pane_editor.set_document_dirty_for_test(true);
    });
    visual.run_until_parked();

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            assert_eq!(
                editor
                    .pane_workspace
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .workspace()
                    .focused_pane(),
                focused_pane
            );
            assert!(!editor.on_window_should_close(window, cx));
            assert!(editor.show_unsaved_changes_dialog);
            editor.on_discard_and_close(&gpui::ClickEvent::default(), window, cx);
        });
    });
    assert!(!background_editor.read_with(visual, |pane_editor, _cx| {
        pane_editor.source_document.is_dirty()
    }));
}

// Reason: Validate that discarding a window clears every dirty document collected by the close
// coordinator, not only the currently focused pane.
#[gpui::test]
async fn pane_window_discard_clears_multiple_dirty_documents(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "first".to_owned(), None));

    editor.update(visual, |editor, cx| {
        editor.split_pane_toward(crate::editor::panes::PaneSplitDirection::Right, cx)
    });
    visual.run_until_parked();
    redraw(visual);
    let background_pane = editor.read_with(visual, |editor, cx| {
        let workspace = editor.pane_workspace.as_ref().unwrap().read(cx);
        let focused = workspace.workspace().focused_pane();
        workspace
            .workspace()
            .pane_ids()
            .into_iter()
            .find(|pane| *pane != focused)
            .expect("background pane")
    });

    editor.update(visual, |editor, cx| {
        assert!(editor.new_document_tab_in_pane(background_pane, DocumentKind::Markdown, cx));
    });
    visual.run_until_parked();
    redraw(visual);

    let markdown_canvases = editor.read_with(visual, |editor, cx| {
        editor
            .pane_canvas_entities
            .borrow()
            .values()
            .filter_map(|(_, _, canvas)| match canvas {
                crate::editor::panes::PaneCanvasEntity::Markdown(canvas) => {
                    Some(canvas.read(cx).editor())
                }
                crate::editor::panes::PaneCanvasEntity::DocumentHost(_)
                | crate::editor::panes::PaneCanvasEntity::ReadOnly(_) => None,
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(markdown_canvases.len(), 2);
    for canvas in &markdown_canvases {
        canvas.update(visual, |pane_editor, _cx| {
            pane_editor.set_document_dirty_for_test(true);
        });
    }
    visual.run_until_parked();

    editor.update(visual, |editor, cx| {
        let states = editor.document_close_states(cx);
        assert_eq!(states.iter().filter(|state| state.dirty).count(), 2);
        assert!(editor.discard_all_document_changes_for_window_close(cx));
        assert!(
            editor
                .document_close_states(cx)
                .into_iter()
                .all(|state| !state.dirty)
        );
    });
}

// Reason: Keep an external lease alive to prove discard honors ownership boundaries instead of
// silently clearing a document that another view still references.
#[gpui::test]
async fn pane_window_discard_respects_external_document_lease(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "draft".to_owned(), None));

    editor.update(visual, |editor, cx| {
        editor.split_pane_toward(crate::editor::panes::PaneSplitDirection::Right, cx)
    });
    visual.run_until_parked();
    let external_lease = editor.read_with(visual, |editor, cx| {
        let canvases = editor.pane_canvas_entities.borrow();
        let (_, _, canvas) = canvases.values().next().expect("mounted markdown canvas");
        match canvas {
            crate::editor::panes::PaneCanvasEntity::Markdown(canvas) => canvas
                .read(cx)
                .editor()
                .read(cx)
                .source_document
                .handle()
                .expect("document handle")
                .lease(),
            crate::editor::panes::PaneCanvasEntity::DocumentHost(_)
            | crate::editor::panes::PaneCanvasEntity::ReadOnly(_) => {
                panic!("markdown canvas expected")
            }
        }
    });
    let pane_editor = editor.read_with(visual, |editor, cx| {
        let canvases = editor.pane_canvas_entities.borrow();
        let (_, _, canvas) = canvases.values().next().expect("mounted markdown canvas");
        match canvas {
            crate::editor::panes::PaneCanvasEntity::Markdown(canvas) => canvas.read(cx).editor(),
            crate::editor::panes::PaneCanvasEntity::DocumentHost(_)
            | crate::editor::panes::PaneCanvasEntity::ReadOnly(_) => {
                panic!("markdown canvas expected")
            }
        }
    });
    pane_editor.update(visual, |pane_editor, _cx| {
        pane_editor.set_document_dirty_for_test(true);
    });
    visual.run_until_parked();

    editor.update(visual, |editor, cx| {
        assert!(editor.discard_all_document_changes_for_window_close(cx));
        assert!(
            editor
                .document_close_states(cx)
                .into_iter()
                .any(|state| { state.dirty && !state.closes_last_lease() })
        );
    });
    drop(external_lease);
}
