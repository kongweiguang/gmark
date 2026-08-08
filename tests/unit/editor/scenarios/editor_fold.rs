// @author kongweiguang

#[gpui::test]
async fn rendered_accessibility_snapshot_exposes_fold_targets(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let source = "# Heading\n\nbody\n\n> [!NOTE]\n> callout body";
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, source.to_owned(), None));
    visual.simulate_resize(size(px(720.0), px(520.0)));
    redraw(visual);

    let folds = editor.update_in(visual, |editor, window, cx| {
        editor.sync_rendered_view_state(window, cx);
        editor.accessibility_snapshot(cx).folds
    });
    assert!(folds.iter().any(|fold| {
        matches!(
            fold.target.as_ref(),
            Some(crate::accessibility::AccessibilityFoldTarget::Rendered { heading: true, .. })
        )
    }));
    assert!(folds.iter().any(|fold| {
        matches!(
            fold.target.as_ref(),
            Some(crate::accessibility::AccessibilityFoldTarget::Rendered { heading: false, .. })
        )
    }));
}

#[gpui::test]
async fn rendered_heading_fold_button_stays_on_the_title_row(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(
            cx,
            "# This deliberately long rendered heading wraps across multiple lines without moving its content anchor\n\nbody"
                .to_owned(),
            None,
        )
    });
    visual.simulate_resize(size(px(720.0), px(520.0)));
    redraw(visual);

    let first_line_height = visual.update(|_window, cx| {
        let typography = &cx
            .global::<crate::theme::ThemeManager>()
            .current()
            .typography;
        typography.h1_size * typography.text_line_height
    });

    let body = editor.read_with(visual, |editor, cx| {
        editor
            .document
            .flatten_visible_blocks()
            .into_iter()
            .find(|visible| visible.entity.read(cx).display_text() == "body")
            .and_then(|visible| visible.entity.read(cx).last_bounds)
            .expect("body content bounds")
    });

    let button = visual
        .debug_bounds("heading-fold-button")
        .expect("heading fold button");
    let title = visual
        .debug_bounds("heading-fold-title")
        .expect("heading title");
    let actions = visual
        .debug_bounds("focused-block-context-actions")
        .expect("focused heading block actions");

    assert_eq!(button.size, size(px(18.0), px(18.0)));
    assert!(actions.right() <= button.left());
    assert!(button.right() <= title.left());
    assert!(
        f32::from(title.left() - body.left()).abs() <= 0.5,
        "fold controls must stay in the gutter without shifting the heading content edge; title={title:?}, body={body:?}"
    );
    assert!(button.top() < title.bottom() && button.bottom() > title.top());
    assert!(f32::from(title.size.height) > first_line_height * 1.5);
    let first_line_center = title.top() + px(first_line_height / 2.0);
    let center_delta = f32::from(button.center().y - first_line_center).abs();
    assert!(
        center_delta <= 1.0,
        "fold button must align to the first visual line of a wrapped heading; button={button:?}, title={title:?}"
    );
    let actions_center_delta = f32::from(actions.center().y - first_line_center).abs();
    assert!(
        actions_center_delta <= 1.0,
        "block actions must align to the current heading line; actions={actions:?}, title={title:?}"
    );
}

#[gpui::test]
async fn collapsing_rendered_fold_moves_pending_focus_to_its_owner(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let source = "# Heading\n\nbody";
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, source.to_owned(), None));
    visual.simulate_resize(size(px(720.0), px(520.0)));
    redraw(visual);

    let (key, owner_id) = editor.update_in(visual, |editor, window, cx| {
        editor.sync_rendered_view_state(window, cx);
        let owner = editor
            .document
            .flatten_visible_blocks()
            .into_iter()
            .find(|visible| visible.entity.read(cx).presentation_fold_heading)
            .expect("heading fold owner");
        let key = owner
            .entity
            .read(cx)
            .presentation_fold_key
            .as_ref()
            .expect("heading fold key")
            .to_string();
        editor.pending_focus = None;
        (key, owner.entity.entity_id())
    });

    editor.update(visual, |editor, cx| {
        editor.toggle_rendered_collapse(&key, true, cx);
        assert_eq!(editor.pending_focus, Some(owner_id));
    });
}

#[gpui::test]
async fn restoring_a_collapsed_fold_never_leaves_focus_in_hidden_body(cx: &mut TestAppContext) {
    init_editor_test_app(cx);
    let source = "# Heading\n\nbody";
    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, source.to_owned(), None));
    visual.simulate_resize(size(px(720.0), px(520.0)));
    redraw(visual);

    editor.update_in(visual, |editor, window, cx| {
        editor.sync_rendered_view_state(window, cx);
        let visible = editor.document.flatten_visible_blocks();
        let owner = visible
            .iter()
            .find(|block| block.entity.read(cx).presentation_fold_heading)
            .expect("heading fold owner")
            .entity
            .clone();
        let body = visible
            .iter()
            .find(|block| block.entity.read(cx).display_text() == "body")
            .expect("heading body")
            .entity
            .clone();
        let key = owner
            .read(cx)
            .presentation_fold_key
            .as_ref()
            .expect("heading fold key")
            .to_string();
        editor.ensure_markdown_view_state();
        editor
            .view_state
            .set_heading_collapsed(editor.tabs.active_id(), key, true);
        editor.pending_focus = None;
        body.read(cx).focus_handle.focus(window);
        editor.sync_rendered_view_state(window, cx);

        assert!(body.read(cx).presentation_hidden);
        assert_eq!(editor.pending_focus, Some(owner.entity_id()));
    });
}
