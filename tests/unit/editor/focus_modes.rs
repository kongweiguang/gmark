// @author kongweiguang

use super::{focus_row_opacity, typewriter_target_y};

#[test]
fn focus_opacity_preserves_active_row_and_keeps_headings_stronger() {
    assert_eq!(focus_row_opacity(false, false, false), 1.0);
    assert_eq!(focus_row_opacity(true, true, false), 1.0);
    assert!(focus_row_opacity(true, false, true) > focus_row_opacity(true, false, false));
}

#[test]
fn typewriter_anchor_is_stable_at_forty_five_percent() {
    assert_eq!(typewriter_target_y(100.0, 800.0), 460.0);
    assert_eq!(typewriter_target_y(0.0, -10.0), 0.0);
}

#[gpui::test]
async fn focus_and_typewriter_toggles_are_independent_and_non_mutating(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        crate::i18n::I18nManager::init(cx);
        crate::theme::ThemeManager::init(cx);
        crate::components::init(cx);
    });
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        super::Editor::from_markdown(cx, "# title\n\nbody".to_owned(), None)
    });
    editor.update(visual, |editor, cx| {
        editor.workspace.is_open = true;
        cx.notify();
    });
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("workspace-panel").is_some());
    assert!(visual.debug_bounds("status-bar").is_some());

    let before = editor.update(visual, |editor, _cx| {
        (
            editor.source_document.text(),
            editor.source_document.revision(),
        )
    });
    visual.update(|window, cx| {
        let theme = cx.global::<crate::theme::ThemeManager>().current_arc();
        let strings = cx.global::<crate::i18n::I18nManager>().strings_arc();
        editor.update(cx, |editor, cx| {
            editor.on_toggle_focus_mode_action(&crate::components::ToggleFocusMode, window, cx);
            assert!(editor.focus_mode);
            assert!(!editor.typewriter_mode);
            assert!(editor.workspace.is_open);
            assert!(
                editor
                    .render_workspace_panel(
                        &theme,
                        &strings,
                        280.0,
                        false,
                        crate::config::WorkspaceSidebarPosition::Left,
                        cx,
                    )
                    .is_none()
            );
            assert!(
                editor
                    .render_status_bar(&theme, &strings, window, cx)
                    .is_none()
            );
        });
        window.draw(cx).clear();
    });

    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_toggle_typewriter_mode_action(
                &crate::components::ToggleTypewriterMode,
                window,
                cx,
            );
            assert!(editor.focus_mode);
            assert!(editor.typewriter_mode);
            assert!(editor.pending_scroll_active_block_into_view);
        });
    });
    editor.update(visual, |editor, _cx| {
        assert_eq!(
            (
                editor.source_document.text(),
                editor.source_document.revision()
            ),
            before
        );
    });
}
