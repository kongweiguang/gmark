// @author kongweiguang

use super::{editing_command_for_action, filter_command_labels, humanize_action_name};

#[test]
fn editor_actions_map_to_the_shared_editing_command_registry() {
    assert_eq!(
        editing_command_for_action(&crate::components::BoldSelection),
        Some(crate::components::EditingCommandId::Bold)
    );
    assert_eq!(
        editing_command_for_action(&crate::components::SetHeading2),
        Some(crate::components::EditingCommandId::Heading2)
    );
    assert!(editing_command_for_action(&crate::components::SaveDocument).is_none());
}

#[test]
fn humanizes_namespaced_camel_case_actions() {
    assert_eq!(
        humanize_action_name("gmark::SaveDocumentAs"),
        "Save Document As"
    );
    assert_eq!(humanize_action_name("plugin::open_recent"), "open recent");
}

#[test]
fn command_filter_prefers_prefix_then_contains_then_subsequence() {
    let labels = vec![
        "Toggle Workspace".to_owned(),
        "Save Document".to_owned(),
        "Document Save As".to_owned(),
    ];
    assert_eq!(filter_command_labels(&labels, "save"), vec![1, 2]);
    assert_eq!(filter_command_labels(&labels, "tws"), vec![0]);
}

#[gpui::test]
async fn palette_indexes_real_editor_actions_and_renders_results(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::i18n::I18nManager::init(cx);
        crate::theme::ThemeManager::init(cx);
        crate::components::init(cx);
    });
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        super::Editor::from_markdown(cx, "# test\n".to_owned(), None)
    });
    visual.simulate_resize(gpui::size(gpui::px(720.0), gpui::px(520.0)));
    visual.update(|window, cx| window.draw(cx).clear());
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_command_palette_action(&crate::components::CommandPalette, window, cx);
            let input = editor.command_palette.as_ref().unwrap().input.clone();
            input.update(cx, |input, cx| {
                input.replace_text_in_visible_range(0..0, "quick", None, false, cx);
            });
        });
    });
    visual.executor().advance_clock(super::FILTER_DEBOUNCE);
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());

    editor.update(visual, |editor, _cx| {
        let state = editor.command_palette.as_ref().unwrap();
        let commands = state
            .filtered
            .iter()
            .map(|index| &state.commands[*index])
            .collect::<Vec<_>>();
        let quick_open = commands
            .iter()
            .find(|command| command.label == "Quick Open")
            .expect("Quick Open command");
        assert_eq!(quick_open.icon, Some("icon/ui/files.svg"));
    });

    for viewport in [
        gpui::size(gpui::px(720.0), gpui::px(520.0)),
        gpui::size(gpui::px(1180.0), gpui::px(780.0)),
    ] {
        visual.simulate_resize(viewport);
        visual.update(|window, cx| window.draw(cx).clear());
        visual.update(|window, _cx| assert_eq!(window.scale_factor(), 2.0));
        let dialog = visual.debug_bounds("command-palette-dialog").unwrap();
        let input = visual.debug_bounds("command-palette-input").unwrap();
        let search_icon = visual.debug_bounds("command-palette-search-icon").unwrap();
        let close = visual.debug_bounds("command-palette-close").unwrap();
        let row = visual.debug_bounds("command-palette-result-0").unwrap();
        let icon = visual
            .debug_bounds("command-palette-result-icon-0")
            .unwrap();
        let label = visual
            .debug_bounds("command-palette-result-label-0")
            .unwrap();
        let shortcut = visual
            .debug_bounds("command-palette-result-shortcut-0")
            .unwrap();
        assert!(dialog.left() >= gpui::px(0.0));
        assert!(dialog.right() <= viewport.width);
        assert!(dialog.top() >= gpui::px(0.0));
        assert!(dialog.bottom() <= viewport.height);
        assert_eq!(input.size.height, gpui::px(40.0));
        assert_eq!(search_icon.size, gpui::size(gpui::px(16.0), gpui::px(16.0)));
        assert_eq!(close.size, gpui::size(gpui::px(28.0), gpui::px(28.0)));
        assert_eq!(row.size.height, gpui::px(34.0));
        assert_eq!(icon.size, gpui::size(gpui::px(18.0), gpui::px(18.0)));
        assert!(input.left() >= dialog.left());
        assert!(input.right() <= dialog.right());
        assert!(close.left() >= dialog.left());
        assert!(close.right() <= dialog.right());
        assert!(icon.left() >= row.left());
        assert!(icon.right() <= label.left());
        assert!(label.right() <= shortcut.left());
        assert!(shortcut.right() <= row.right());
    }
}
