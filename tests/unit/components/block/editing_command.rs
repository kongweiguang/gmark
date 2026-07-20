// @author kongweiguang

use super::*;

#[test]
fn ranking_prefers_prefixes_and_supports_chinese_and_pinyin() {
    assert_eq!(
        filter_commands(&SLASH_COMMANDS, "h2"),
        vec![EditingCommandId::Heading2]
    );
    assert_eq!(
        filter_commands(&SLASH_COMMANDS, "表格"),
        vec![EditingCommandId::Table]
    );
    assert_eq!(
        filter_commands(&SLASH_COMMANDS, "dmk"),
        vec![EditingCommandId::CodeBlock]
    );
    assert_eq!(filter_commands(&SLASH_COMMANDS, "missing"), Vec::new());
}

#[test]
fn context_prevents_partial_or_invalid_inline_commands() {
    let cross_block = EditingContext {
        selection: EditingSelectionContext::AcrossBlocks,
        ..EditingContext::default()
    };
    assert!(EditingCommandId::Bold.is_available(cross_block));
    assert!(!EditingCommandId::Link.is_available(cross_block));
    assert!(!EditingCommandId::MoveBlockUp.is_available(cross_block));
}

#[test]
fn structural_edges_disable_only_the_invalid_move_direction() {
    let first = EditingContext {
        sibling_index: 0,
        sibling_count: 2,
        ..EditingContext::default()
    };
    assert!(!EditingCommandId::MoveBlockUp.is_available(first));
    assert!(EditingCommandId::MoveBlockDown.is_available(first));

    let last = EditingContext {
        sibling_index: 1,
        ..first
    };
    assert!(EditingCommandId::MoveBlockUp.is_available(last));
    assert!(!EditingCommandId::MoveBlockDown.is_available(last));

    let only = EditingContext {
        sibling_index: 0,
        sibling_count: 1,
        ..first
    };
    assert!(!EditingCommandId::MoveBlockUp.is_available(only));
    assert!(!EditingCommandId::MoveBlockDown.is_available(only));
    assert!(EditingCommandId::DuplicateBlock.is_available(only));
    assert!(EditingCommandId::DeleteBlock.is_available(only));
}

#[test]
fn block_actions_and_context_menu_share_all_insert_commands() {
    for command in INSERT_COMMANDS {
        assert!(BLOCK_MENU_COMMANDS.contains(&command));
    }
    let code_context = EditingContext {
        block: EditingBlockContext::Code,
        sibling_count: 1,
        ..EditingContext::default()
    };
    for command in INSERT_COMMANDS {
        assert!(command.is_available(code_context), "{command:?}");
    }
}

#[test]
fn every_command_has_stable_metadata() {
    for command in SLASH_COMMANDS.into_iter().chain(INLINE_COMMANDS) {
        let descriptor = command.descriptor();
        assert_eq!(descriptor.id, command);
        assert!(!descriptor.localization_key.is_empty());
        assert!(descriptor.icon_path.starts_with("icon/ui/"));
        assert!(descriptor.icon_path.ends_with(".svg"));
        assert!(!descriptor.aliases.is_empty());
        if matches!(
            command,
            EditingCommandId::Bold
                | EditingCommandId::Italic
                | EditingCommandId::Underline
                | EditingCommandId::Strikethrough
                | EditingCommandId::InlineCode
                | EditingCommandId::Link
        ) {
            assert!(descriptor.shortcut.is_some());
        }
    }
}

#[test]
fn recent_commands_ignore_unknowns_deduplicate_and_keep_five() {
    let ids = vec![
        "missing".to_owned(),
        "heading_1".to_owned(),
        "heading_1".to_owned(),
        "table".to_owned(),
        "image".to_owned(),
        "math".to_owned(),
        "horizontal_rule".to_owned(),
        "quote".to_owned(),
        "bold".to_owned(),
    ];
    assert_eq!(
        normalized_recent_commands(&ids),
        vec![
            EditingCommandId::Heading1,
            EditingCommandId::Table,
            EditingCommandId::Image,
            EditingCommandId::Math,
            EditingCommandId::HorizontalRule,
        ]
    );

    let mut recent = vec![
        EditingCommandId::Heading1,
        EditingCommandId::Table,
        EditingCommandId::Image,
        EditingCommandId::Math,
        EditingCommandId::HorizontalRule,
    ];
    assert!(record_recent_command(&mut recent, EditingCommandId::Image));
    assert_eq!(recent[0], EditingCommandId::Image);
    assert_eq!(recent.len(), 5);
    assert!(!record_recent_command(&mut recent, EditingCommandId::Bold));
}
