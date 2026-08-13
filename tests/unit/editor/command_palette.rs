// @author kongweiguang

use gpui::Action as _;

use super::{
    command_icon, command_search_text, display_shortcut, editing_command_for_action,
    filter_command_labels, humanize_action_name, localized_action_description,
    localized_action_label,
};

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
    assert_eq!(
        editing_command_for_action(&crate::components::InsertResource),
        Some(crate::components::EditingCommandId::Resource)
    );
    assert!(editing_command_for_action(&crate::components::SaveDocument).is_none());
}

#[gpui::test]
async fn markdown_document_menu_exposes_the_shared_resource_action(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::i18n::I18nManager::init_with_language_id(cx, "en-US");
        crate::theme::ThemeManager::init(cx);
        crate::components::init(cx);
    });
    let (editor, visual) = cx.add_window_view(|_window, cx| {
        super::Editor::from_markdown(cx, "# test\n".to_owned(), None)
    });

    visual.update(|_window, cx| {
        let menu = editor.read(cx).build_document_menu(cx);
        let resource = menu
            .items
            .iter()
            .find(|item| {
                matches!(item, gpui::MenuItem::Action { name, .. } if name.as_ref() == "Insert Resource")
            })
            .expect("Markdown menu must expose Insert Resource");
        match resource {
            gpui::MenuItem::Action { action, .. } => {
                assert!(action.as_any().is::<crate::components::InsertResource>());
            }
            _ => unreachable!("filtered to an action item"),
        }
    });
}

#[test]
fn humanizes_namespaced_camel_case_actions() {
    assert_eq!(
        humanize_action_name("gmark::SaveDocumentAs"),
        "Save Document As"
    );
    assert_eq!(humanize_action_name("plugin::open_recent"), "open recent");
}

#[gpui::test]
async fn command_labels_follow_the_selected_chinese_language(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::i18n::I18nManager::init_with_language_id(cx, "zh-CN");
        let strings = cx.global::<crate::i18n::I18nManager>().strings();
        assert_eq!(
            localized_action_label(&crate::components::CloseTab, strings, "zh-CN"),
            "关闭标签页"
        );
        assert_eq!(
            localized_action_label(&crate::components::CheckForUpdates, strings, "zh-CN"),
            "检查更新"
        );
        assert_eq!(
            localized_action_label(&crate::components::SetBulletedList, strings, "zh-CN"),
            "无序列表"
        );
        assert_eq!(
            localized_action_label(
                &crate::components::NormalizeLineEndingsCrLf,
                strings,
                "zh-CN"
            ),
            "统一为 CRLF 换行符"
        );
        assert_eq!(
            localized_action_label(&crate::components::ExportPdf, strings, "zh-CN"),
            "导出为 PDF"
        );
        assert_eq!(
            localized_action_label(&crate::components::CancelFormatting, strings, "zh-CN"),
            "取消格式化"
        );
        assert_eq!(
            localized_action_label(&crate::components::CollapseAllFolds, strings, "zh-CN"),
            "全部折叠"
        );
        assert_eq!(
            localized_action_label(&crate::components::FocusStructuredFilter, strings, "zh-CN"),
            "聚焦结构化筛选"
        );
        assert_eq!(
            localized_action_label(&crate::components::FormatDocument, strings, "zh-CN"),
            "格式化文档"
        );
        let format_label =
            localized_action_label(&crate::components::FormatDocument, strings, "zh-CN");
        assert_eq!(
            localized_action_description(
                &crate::components::FormatDocument,
                &format_label,
                "zh-CN",
            ),
            "格式化当前文档"
        );
        let columns_label =
            localized_action_label(&crate::components::FocusStructuredColumns, strings, "zh-CN");
        assert_eq!(
            localized_action_description(
                &crate::components::FocusStructuredColumns,
                &columns_label,
                "zh-CN",
            ),
            "将焦点移到结构化视图的列工具"
        );
        let resource = crate::components::InsertResource;
        let resource_label = localized_action_label(&resource, strings, "zh-CN");
        assert_eq!(resource_label, "资源");
        assert_eq!(
            localized_action_description(&resource, &resource_label, "zh-CN"),
            "选择文件并插入资源卡片"
        );
        assert_eq!(command_icon(&resource), "icon/ui/file.svg");
    });
}

#[test]
fn command_metadata_hides_action_names_and_indexes_human_aliases() {
    let action = crate::components::SetCodeBlock;
    let description = localized_action_description(&action, "代码块", "zh-CN");
    let search_text = command_search_text(&action, "代码块", &description);
    assert_eq!(description, "将当前段落转换为支持语法高亮的代码块");
    assert!(search_text.contains("code block"));
    assert!(search_text.contains("代码块"));
    assert_eq!(display_shortcut("gmark::SetCodeBlock", action.name()), "");
    assert_eq!(display_shortcut("ctrl-alt-c", action.name()), "Ctrl+Alt+C");
    assert_eq!(command_icon(&action), "icon/ui/code.svg");
    assert!(std::path::Path::new("assets/icon/ui/code.svg").is_file());

    let exit = crate::components::ExitCodeBlock;
    let exit_description = localized_action_description(&exit, "退出代码块", "zh-CN");
    let searchables = vec![
        command_search_text(&exit, "退出代码块", &exit_description),
        search_text,
    ];
    assert_eq!(filter_command_labels(&searchables, "code block")[0], 1);
    assert_eq!(command_icon(&exit), "icon/ui/code.svg");
}

#[test]
fn pane_actions_have_localized_labels_descriptions_and_icons() {
    let actions: &[(&dyn gpui::Action, &str, &str, &str, &str, &str)] = &[
        (
            &crate::components::SplitRight,
            "Split Right",
            "向右拆分",
            "Split the current pane and place a new pane on its right",
            "在当前窗格右侧创建一个新窗格",
            "icon/ui/panel-right.svg",
        ),
        (
            &crate::components::SplitDown,
            "Split Down",
            "向下拆分",
            "Split the current pane and place a new pane below it",
            "在当前窗格下方创建一个新窗格",
            "icon/ui/panel-bottom.svg",
        ),
        (
            &crate::components::ClosePane,
            "Close Pane",
            "关闭窗格",
            "Close the active pane and focus an adjacent pane",
            "关闭当前窗格，并将焦点移到相邻窗格",
            "icon/ui/close.svg",
        ),
        (
            &crate::components::FocusPaneLeft,
            "Focus Pane Left",
            "聚焦左侧窗格",
            "Focus the adjacent pane on the left",
            "将焦点移到左侧相邻窗格",
            "icon/ui/panel-left.svg",
        ),
        (
            &crate::components::FocusPaneRight,
            "Focus Pane Right",
            "聚焦右侧窗格",
            "Focus the adjacent pane on the right",
            "将焦点移到右侧相邻窗格",
            "icon/ui/panel-right.svg",
        ),
        (
            &crate::components::FocusPaneUp,
            "Focus Pane Up",
            "聚焦上方窗格",
            "Focus the adjacent pane above",
            "将焦点移到上方相邻窗格",
            "icon/ui/arrow-up.svg",
        ),
        (
            &crate::components::FocusPaneDown,
            "Focus Pane Down",
            "聚焦下方窗格",
            "Focus the adjacent pane below",
            "将焦点移到下方相邻窗格",
            "icon/ui/panel-bottom.svg",
        ),
        (
            &crate::components::MoveTabToPaneLeft,
            "Move Tab to Pane Left",
            "将标签页移至左侧窗格",
            "Move the active tab to the adjacent pane on the left",
            "将当前标签页移到左侧相邻窗格",
            "icon/ui/panel-left.svg",
        ),
        (
            &crate::components::MoveTabToPaneRight,
            "Move Tab to Pane Right",
            "将标签页移至右侧窗格",
            "Move the active tab to the adjacent pane on the right",
            "将当前标签页移到右侧相邻窗格",
            "icon/ui/panel-right.svg",
        ),
        (
            &crate::components::MoveTabToPaneUp,
            "Move Tab to Pane Up",
            "将标签页移至上方窗格",
            "Move the active tab to the adjacent pane above",
            "将当前标签页移到上方相邻窗格",
            "icon/ui/arrow-up.svg",
        ),
        (
            &crate::components::MoveTabToPaneDown,
            "Move Tab to Pane Down",
            "将标签页移至下方窗格",
            "Move the active tab to the adjacent pane below",
            "将当前标签页移到下方相邻窗格",
            "icon/ui/panel-bottom.svg",
        ),
        (
            &crate::components::BalancePanes,
            "Balance Panes",
            "平衡窗格",
            "Distribute the available space evenly across panes",
            "均衡所有窗格的可用空间",
            "icon/ui/align-center.svg",
        ),
    ];
    let english = crate::i18n::I18nStrings::en_us();
    let chinese = crate::i18n::I18nStrings::zh_cn();
    for &(action, en_label, zh_label, en_description, zh_description, icon) in actions {
        assert_eq!(localized_action_label(action, &english, "en-US"), en_label);
        assert_eq!(localized_action_label(action, &chinese, "zh-CN"), zh_label);
        assert_eq!(
            localized_action_description(action, en_label, "en-US"),
            en_description
        );
        assert_eq!(
            localized_action_description(action, zh_label, "zh-CN"),
            zh_description
        );
        assert_eq!(command_icon(action), icon);
    }
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
        crate::i18n::I18nManager::init_with_language_id(cx, "en-US");
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
        assert_eq!(quick_open.icon, "icon/ui/files.svg");
        assert!(!quick_open.description.is_empty());
        assert!(!quick_open.shortcut.contains("::"));
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
        let description = visual
            .debug_bounds("command-palette-result-description-0")
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
        assert_eq!(row.size.height, gpui::px(50.0));
        assert_eq!(icon.size, gpui::size(gpui::px(18.0), gpui::px(18.0)));
        assert!(input.left() >= dialog.left());
        assert!(input.right() <= dialog.right());
        assert!(close.left() >= dialog.left());
        assert!(close.right() <= dialog.right());
        assert!(icon.left() >= row.left());
        assert!(icon.right() <= label.left());
        assert!(label.right() <= shortcut.left());
        assert!(description.left() >= label.left());
        assert!(description.right() <= shortcut.left());
        assert!(description.top() >= label.bottom());
        assert!(shortcut.right() <= row.right());
    }
}
