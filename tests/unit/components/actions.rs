// @author kongweiguang

use super::{
    ShortcutCommand, normalize_shortcut_config, resolved_shortcut_keys, shortcut_conflict_for,
};
use std::collections::BTreeMap;

#[test]
fn custom_shortcut_replaces_command_defaults() {
    let mut config = BTreeMap::new();
    config.insert("save_document".to_string(), vec!["ctrl-alt-s".to_string()]);

    assert_eq!(
        resolved_shortcut_keys(&config, ShortcutCommand::SaveDocument),
        vec!["ctrl-alt-s".to_string()]
    );
}

#[test]
fn toggle_view_mode_has_default_shortcuts() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::ToggleViewMode),
        vec!["ctrl-tab".to_string(), "cmd-tab".to_string()]
    );
}

#[test]
fn toggle_workspace_has_default_shortcuts() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::ToggleWorkspace),
        vec!["cmd-b".to_string(), "ctrl-b".to_string()]
    );
}

#[test]
fn quick_open_has_default_shortcuts() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::QuickOpen),
        vec!["cmd-p".to_string(), "ctrl-p".to_string()]
    );
}

#[test]
fn command_palette_has_default_shortcuts() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::CommandPalette),
        vec!["cmd-shift-p".to_string(), "ctrl-shift-p".to_string()]
    );
}

#[test]
fn link_selection_has_standard_shortcuts() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::LinkSelection),
        vec!["cmd-k".to_string(), "ctrl-k".to_string()]
    );
}

#[test]
fn strikethrough_selection_has_standard_shortcuts() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::StrikethroughSelection),
        vec!["cmd-shift-x".to_string(), "ctrl-shift-x".to_string()]
    );
}

#[test]
fn open_folder_has_default_shortcuts() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::OpenFolder),
        vec!["cmd-shift-o".to_string(), "ctrl-shift-o".to_string()]
    );
}

#[test]
fn focus_and_typewriter_modes_have_independent_shortcuts() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::ToggleFocusMode),
        vec!["cmd-shift-f".to_string(), "ctrl-shift-f".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::ToggleTypewriterMode),
        vec!["cmd-alt-t".to_string(), "ctrl-alt-t".to_string()]
    );
}

#[test]
fn tab_lifecycle_has_standard_shortcuts() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::NewTab),
        vec![
            "cmd-n".to_string(),
            "ctrl-n".to_string(),
            "cmd-t".to_string(),
            "ctrl-t".to_string()
        ]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::NewWindow),
        vec!["cmd-shift-n".to_string(), "ctrl-shift-n".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::CloseTab),
        vec!["cmd-w".to_string(), "ctrl-w".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::ReopenClosedTab),
        vec!["cmd-shift-t".to_string(), "ctrl-shift-t".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::PreviousTab),
        vec!["cmd-pageup".to_string(), "ctrl-pageup".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::NextTab),
        vec!["cmd-pagedown".to_string(), "ctrl-pagedown".to_string()]
    );
}

#[test]
fn typora_compatible_edit_and_preferences_shortcuts_are_default() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::CopyAsMarkdown),
        vec!["cmd-shift-c".to_string(), "ctrl-shift-c".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::PasteAsPlainText),
        vec!["cmd-shift-v".to_string(), "ctrl-shift-v".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::OpenPreferences),
        vec!["cmd-,".to_string(), "ctrl-,".to_string()]
    );
}

#[test]
fn select_all_has_default_shortcuts() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::SelectAll),
        vec!["cmd-a".to_string(), "ctrl-a".to_string()]
    );
    assert!(
        shortcut_conflict_for(
            ShortcutCommand::SelectAll,
            &["cmd-a".to_string(), "ctrl-a".to_string()],
            &BTreeMap::new()
        )
        .is_none()
    );
}

#[test]
fn select_all_shortcut_can_be_customized() {
    let mut config = BTreeMap::new();
    config.insert("select_all".to_string(), vec!["ctrl-shift-a".to_string()]);

    assert_eq!(
        resolved_shortcut_keys(&config, ShortcutCommand::SelectAll),
        vec!["ctrl-shift-a".to_string()]
    );
}

#[test]
fn legacy_split_select_all_shortcut_config_maps_to_unified_command() {
    let mut config = BTreeMap::new();
    config.insert(
        "select_all_source_text".to_string(),
        vec!["ctrl-shift-a".to_string()],
    );

    assert_eq!(
        resolved_shortcut_keys(&config, ShortcutCommand::SelectAll),
        vec!["ctrl-shift-a".to_string()]
    );

    let normalized = normalize_shortcut_config(&config);
    assert_eq!(
        normalized.get("select_all"),
        Some(&vec!["ctrl-shift-a".to_string()])
    );
    assert!(!normalized.contains_key("select_all_source_text"));
    assert!(!normalized.contains_key("select_focused_block_text_rendered"));

    config.clear();
    config.insert(
        "select_focused_block_text_rendered".to_string(),
        vec!["ctrl-alt-shift-a".to_string()],
    );

    assert_eq!(
        resolved_shortcut_keys(&config, ShortcutCommand::SelectAll),
        vec!["ctrl-alt-shift-a".to_string()]
    );

    let normalized = normalize_shortcut_config(&config);
    assert_eq!(
        normalized.get("select_all"),
        Some(&vec!["ctrl-alt-shift-a".to_string()])
    );
    assert!(!normalized.contains_key("select_all_source_text"));
    assert!(!normalized.contains_key("select_focused_block_text_rendered"));
}

#[test]
fn close_and_quit_defaults_are_platform_specific() {
    #[cfg(target_os = "macos")]
    {
        assert_eq!(
            resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::CloseWindow),
            vec!["cmd-shift-w".to_string()]
        );
        assert_eq!(
            resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::QuitApplication),
            vec!["cmd-q".to_string()]
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        assert_eq!(
            resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::CloseWindow),
            vec!["ctrl-shift-w".to_string()]
        );
        assert!(
            resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::QuitApplication).is_empty()
        );
    }
}

#[test]
fn word_and_block_shortcuts_have_ctrl_and_alt_defaults() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::WordMoveLeft),
        vec!["ctrl-left".to_string(), "alt-left".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::WordDeleteBack),
        vec!["ctrl-backspace".to_string(), "alt-backspace".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::BlockUp),
        vec!["ctrl-up".to_string(), "alt-up".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::WordSelectRight),
        vec![
            "ctrl-shift-right".to_string(),
            "alt-shift-right".to_string()
        ]
    );
}

#[test]
fn page_navigation_shortcuts_have_defaults() {
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::PageUp),
        vec!["pageup".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::PageDown),
        vec!["pagedown".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::JumpToTop),
        vec!["ctrl-home".to_string(), "cmd-up".to_string()]
    );
    assert_eq!(
        resolved_shortcut_keys(&BTreeMap::new(), ShortcutCommand::JumpToBottom),
        vec!["ctrl-end".to_string(), "cmd-down".to_string()]
    );
}

#[test]
fn invalid_or_empty_shortcuts_fall_back_to_defaults() {
    let mut config = BTreeMap::new();
    config.insert("save_document".to_string(), vec!["".to_string()]);
    config.insert("open_file".to_string(), vec!["a".to_string()]);

    let normalized = normalize_shortcut_config(&config);
    assert!(!normalized.contains_key("save_document"));
    assert!(!normalized.contains_key("open_file"));
}

#[test]
fn conflicting_custom_shortcut_falls_back_to_default() {
    let mut config = BTreeMap::new();
    config.insert("copy".to_string(), vec!["ctrl-x".to_string()]);

    let normalized = normalize_shortcut_config(&config);
    assert!(!normalized.contains_key("copy"));
    assert_eq!(
        resolved_shortcut_keys(&config, ShortcutCommand::Copy),
        vec!["cmd-c".to_string(), "ctrl-c".to_string()]
    );
}

#[test]
fn detects_shortcut_conflicts_for_preferences_drafts() {
    let conflict = shortcut_conflict_for(
        ShortcutCommand::Copy,
        &["ctrl-x".to_string()],
        &BTreeMap::new(),
    )
    .expect("copy should conflict with cut");

    assert_eq!(conflict.id, "cut");
}
