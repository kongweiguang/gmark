// @author kongweiguang

#[gpui::test]
async fn pane_tab_close_uses_zed_icon_slot_and_keeps_inactive_canvas_mounted(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "# active\n".to_owned(), None));
    editor.update(visual, |editor, cx| {
        assert!(editor.new_untitled_tab(cx));
        editor.split_pane_toward(crate::editor::panes::PaneSplitDirection::Right, cx);
    });
    visual.run_until_parked();
    for _ in 0..3 {
        redraw(visual);
    }

    let close = visual
        .debug_bounds("pane-tab-close")
        .expect("pane-local tab exposes its leading close/icon slot");
    assert_eq!(close.size, size(px(24.0), px(24.0)));

    let (pane, active, inactive, canvas_id) = editor.read_with(visual, |editor, cx| {
        let workspace = editor.pane_workspace.as_ref().unwrap().read(cx);
        let (pane, state) = workspace
            .workspace()
            .pane_states()
            .find(|(_, state)| state.tabs().len() == 2)
            .expect("legacy tabs migrate together into one pane");
        let active = state.active_tab_id().unwrap();
        let inactive = state
            .tabs()
            .iter()
            .map(|tab| tab.id())
            .find(|tab| *tab != active)
            .unwrap();
        let canvases = editor.pane_canvas_entities.borrow();
        let (mounted_tab, _, canvas) = canvases.get(&pane).expect("active pane canvas");
        assert_eq!(*mounted_tab, active);
        let canvas_id = match canvas {
            crate::editor::panes::PaneCanvasEntity::Markdown(entity) => entity.entity_id(),
            crate::editor::panes::PaneCanvasEntity::DocumentHost(entity) => entity.entity_id(),
            crate::editor::panes::PaneCanvasEntity::ReadOnly(entity) => entity.entity_id(),
        };
        (pane, active, inactive, canvas_id)
    });

    editor.update(visual, |editor, cx| {
        let workspace = editor.pane_workspace.clone().unwrap();
        assert!(editor.close_pane_tab_now(&workspace, pane, inactive, cx));
    });
    visual.run_until_parked();
    redraw(visual);

    editor.read_with(visual, |editor, cx| {
        let workspace = editor.pane_workspace.as_ref().unwrap().read(cx);
        let state = workspace.workspace().pane(pane).unwrap();
        assert_eq!(state.tabs().len(), 1);
        assert_eq!(state.active_tab_id(), Some(active));
        let current_canvas_id = match &editor.pane_canvas_entities.borrow().get(&pane).unwrap().2 {
            crate::editor::panes::PaneCanvasEntity::Markdown(entity) => entity.entity_id(),
            crate::editor::panes::PaneCanvasEntity::DocumentHost(entity) => entity.entity_id(),
            crate::editor::panes::PaneCanvasEntity::ReadOnly(entity) => entity.entity_id(),
        };
        assert_eq!(
            current_canvas_id, canvas_id,
            "closing an inactive tab must not rebuild the active editor canvas"
        );
    });
}

#[gpui::test]
async fn dirty_pane_tab_cancel_preserves_content_and_discard_closes_after_success(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let (editor, visual) = cx
        .add_window_view(|_window, cx| Editor::from_markdown(cx, "# original\n".to_owned(), None));
    editor.update(visual, |editor, cx| {
        editor.split_pane_toward(crate::editor::panes::PaneSplitDirection::Right, cx);
    });
    visual.run_until_parked();
    redraw(visual);

    let pane = editor.read_with(visual, |editor, cx| {
        editor
            .pane_workspace
            .as_ref()
            .unwrap()
            .read(cx)
            .workspace()
            .focused_pane()
    });
    editor.update(visual, |editor, cx| {
        assert!(editor.new_document_tab_in_pane(pane, DocumentKind::Markdown, cx));
    });
    visual.run_until_parked();
    redraw(visual);

    let (tab, canvas, canvas_id) = editor.read_with(visual, |editor, cx| {
        let workspace = editor.pane_workspace.as_ref().unwrap().read(cx);
        let tab = workspace
            .workspace()
            .pane(pane)
            .unwrap()
            .active_tab_id()
            .unwrap();
        let canvas = match &editor.pane_canvas_entities.borrow().get(&pane).unwrap().2 {
            crate::editor::panes::PaneCanvasEntity::Markdown(entity) => entity.clone(),
            crate::editor::panes::PaneCanvasEntity::DocumentHost(_)
            | crate::editor::panes::PaneCanvasEntity::ReadOnly(_) => {
                panic!("new Markdown tab must mount a Markdown canvas")
            }
        };
        (tab, canvas.clone(), canvas.entity_id())
    });
    let pane_editor = canvas.read_with(visual, |canvas, _cx| canvas.editor());
    pane_editor.update(visual, |editor, _cx| {
        editor.set_document_dirty_for_test(true)
    });

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.handle_pane_event(
                crate::editor::panes::PaneEvent::CloseTab { pane, tab },
                Some(window),
                cx,
            );
        });
    });
    redraw(visual);
    assert!(visual.debug_bounds("tab-close-dialog").is_some());
    assert_eq!(
        editor.read_with(visual, |editor, _cx| editor.pane_close_target),
        Some((pane, tab))
    );

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_cancel_tab_close(&gpui::ClickEvent::default(), window, cx)
        });
    });
    editor.read_with(visual, |editor, cx| {
        assert!(editor.pane_close_target.is_none());
        assert!(
            editor
                .pane_workspace
                .as_ref()
                .unwrap()
                .read(cx)
                .workspace()
                .tab(pane, tab)
                .is_some()
        );
        let current = match &editor.pane_canvas_entities.borrow().get(&pane).unwrap().2 {
            crate::editor::panes::PaneCanvasEntity::Markdown(entity) => entity.entity_id(),
            crate::editor::panes::PaneCanvasEntity::DocumentHost(entity) => entity.entity_id(),
            crate::editor::panes::PaneCanvasEntity::ReadOnly(entity) => entity.entity_id(),
        };
        assert_eq!(current, canvas_id);
    });
    visual.run_until_parked();
    for _ in 0..2 {
        redraw(visual);
    }

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.handle_pane_event(
                crate::editor::panes::PaneEvent::CloseTab { pane, tab },
                Some(window),
                cx,
            );
            editor.on_discard_tab_close(&gpui::ClickEvent::default(), window, cx);
        });
    });
    visual.run_until_parked();
    redraw(visual);
    editor.read_with(visual, |editor, cx| {
        assert!(editor.pane_close_target.is_none());
        assert!(
            editor
                .pane_workspace
                .as_ref()
                .unwrap()
                .read(cx)
                .workspace()
                .tab(pane, tab)
                .is_none()
        );
    });
}

#[gpui::test]
async fn closing_one_shared_dirty_view_does_not_prompt_or_clear_the_remaining_view(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "shared\n".to_owned(), None));
    editor.update(visual, |editor, cx| {
        editor.split_pane_toward(crate::editor::panes::PaneSplitDirection::Right, cx);
    });
    visual.run_until_parked();
    redraw(visual);

    let (pane, tab, pane_editor) = editor.read_with(visual, |editor, cx| {
        let workspace = editor.pane_workspace.as_ref().unwrap().read(cx);
        let pane = workspace.workspace().focused_pane();
        let tab = workspace
            .workspace()
            .pane(pane)
            .unwrap()
            .active_tab_id()
            .unwrap();
        let pane_editor = match &editor.pane_canvas_entities.borrow().get(&pane).unwrap().2 {
            crate::editor::panes::PaneCanvasEntity::Markdown(entity) => entity.read(cx).editor(),
            crate::editor::panes::PaneCanvasEntity::DocumentHost(_)
            | crate::editor::panes::PaneCanvasEntity::ReadOnly(_) => {
                panic!("split Markdown view must mount a Markdown canvas")
            }
        };
        (pane, tab, pane_editor)
    });
    pane_editor.update(visual, |editor, _cx| {
        editor.set_document_dirty_for_test(true)
    });

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.handle_pane_event(
                crate::editor::panes::PaneEvent::CloseTab { pane, tab },
                Some(window),
                cx,
            );
        });
    });
    visual.run_until_parked();
    redraw(visual);

    editor.read_with(visual, |editor, cx| {
        assert!(editor.pane_close_target.is_none());
        let workspace = editor.pane_workspace.as_ref().unwrap().read(cx);
        assert_eq!(workspace.workspace().pane_count(), 1);
        let remaining = workspace
            .workspace()
            .pane(workspace.workspace().focused_pane())
            .unwrap()
            .active_tab()
            .unwrap();
        assert!(remaining.view().is_dirty());
    });
}
