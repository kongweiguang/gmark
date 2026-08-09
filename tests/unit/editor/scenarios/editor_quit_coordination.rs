// @author kongweiguang

#[gpui::test]
async fn quit_application_allows_clean_editor_windows_to_quit(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let (first_editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "first".to_string(), None));
    let _first_window = activate_visual_window(cx);

    let (second_editor, cx) = cx
        .cx
        .add_window_view(|_window, cx| Editor::from_markdown(cx, "second".to_string(), None));
    let _second_window = activate_visual_window(cx);

    assert_eq!(cx.cx.windows().len(), 2);

    cx.cx.update(|cx| {
        crate::app_menu::dispatch_menu_action(&QuitApplication, cx);
    });
    cx.run_until_parked();

    first_editor.read_with(cx, |editor, _cx| {
        assert!(!editor.show_unsaved_changes_dialog);
    });
    second_editor.read_with(cx, |editor, _cx| {
        assert!(!editor.show_unsaved_changes_dialog);
    });
}
#[gpui::test]
async fn quit_requested_from_editor_callback_is_deferred_without_reentrant_window_update(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);

    let (editor, visual) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "clean".to_owned(), None));

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            // This is the production failure path: the shortcut/action
            // callback is already updating the current Editor when it
            // requests application quit.  The coordinator must only schedule
            // work for the next GPUI turn, so the current window handle
            // remains valid until then.
            editor.on_quit_application(&QuitApplication, window, cx);
            assert_eq!(
                crate::app_menu::QuitCoordinator::phase(cx),
                crate::app_menu::QuitPhase::Scheduled
            );
            assert_eq!(
                crate::app_menu::request_quit_application(cx),
                crate::app_menu::QuitRequestOutcome::AlreadyInProgress
            );
        });
    });

    cx.run_until_parked();

    let outcome = cx.update(|cx| crate::app_menu::QuitCoordinator::last_outcome(cx));
    assert_eq!(outcome, Some(crate::app_menu::QuitRequestOutcome::Approved));
}

#[gpui::test]
async fn quit_application_prompts_dirty_editor_without_quitting(cx: &mut TestAppContext) {
    init_editor_test_app(cx);

    let (first_editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "first".to_string(), None));
    let first_window = activate_visual_window(cx);

    let (second_editor, cx) = cx
        .cx
        .add_window_view(|_window, cx| Editor::from_markdown(cx, "second".to_string(), None));
    let second_window = activate_visual_window(cx);

    second_editor.update(cx, |editor, cx| editor.mark_dirty(cx));
    assert_eq!(cx.cx.windows().len(), 2);

    cx.cx.update(|cx| {
        crate::app_menu::dispatch_menu_action(&QuitApplication, cx);
    });
    cx.run_until_parked();

    let open_windows = cx.cx.windows();
    assert_eq!(open_windows.len(), 2);
    assert!(
        open_windows
            .iter()
            .any(|window| window.window_id() == first_window.window_id())
    );
    assert!(
        open_windows
            .iter()
            .any(|window| window.window_id() == second_window.window_id())
    );
    first_editor.read_with(cx, |editor, _cx| {
        assert!(!editor.show_unsaved_changes_dialog);
    });
    second_editor.read_with(cx, |editor, _cx| {
        assert!(editor.show_unsaved_changes_dialog);
    });
}

#[gpui::test]
async fn update_quit_cancels_when_an_external_conflict_is_already_unresolved(
    cx: &mut TestAppContext,
) {
    init_editor_test_app(cx);
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        Editor::from_markdown(cx, "conflicted".to_owned(), None)
    });

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.show_external_conflict_dialog = true;
            assert_eq!(
                crate::app_menu::request_update_quit_application(cx),
                crate::app_menu::QuitRequestOutcome::Scheduled
            );
            assert!(!editor.on_window_should_close_for_quit(window, cx));
            assert_eq!(
                crate::app_menu::QuitCoordinator::phase(cx),
                crate::app_menu::QuitPhase::Idle
            );
            assert_eq!(
                crate::app_menu::QuitCoordinator::last_outcome(cx),
                Some(crate::app_menu::QuitRequestOutcome::Aborted)
            );
        });
    });
}
