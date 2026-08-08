// @author kongweiguang

use super::*;

#[test]
fn saved_tabs_start_from_latest_snapshot_but_do_not_broadcast_changes() {
    let path = PathBuf::from("notes.md");
    let first_id = Uuid::new_v4();
    let second_id = Uuid::new_v4();
    let mut manager = MarkdownViewStateManager::default();
    manager.open_tab(MarkdownTabIdentity::saved(&path, first_id));
    manager
        .state_for_tab_mut(first_id)
        .expect("first tab")
        .collapsed_headings
        .insert("heading/one#0".to_owned(), true);
    manager.close_tab(first_id);
    let second = manager.open_tab(MarkdownTabIdentity::saved(&path, second_id));
    assert_eq!(second.collapsed_headings.get("heading/one#0"), Some(&true));
    manager
        .state_for_tab_mut(second_id)
        .expect("second tab")
        .collapsed_headings
        .insert("heading/two#0".to_owned(), true);
    assert!(
        !manager
            .state_for_tab(first_id)
            .is_some_and(|state| state.collapsed_headings.contains_key("heading/two#0"))
    );
}

#[test]
fn new_duplicate_tab_reads_latest_snapshot_without_broadcasting_to_existing_tabs() {
    let path = PathBuf::from("notes.md");
    let first_id = Uuid::new_v4();
    let second_id = Uuid::new_v4();
    let third_id = Uuid::new_v4();
    let mut manager = MarkdownViewStateManager::default();

    manager.open_tab(MarkdownTabIdentity::saved(&path, first_id));
    manager.set_heading_collapsed(first_id, "heading/one#0", true);
    let second = manager.open_tab(MarkdownTabIdentity::saved(&path, second_id));
    assert_eq!(second.collapsed_headings.get("heading/one#0"), Some(&true));

    manager.set_heading_collapsed(second_id, "heading/two#0", true);
    assert!(
        !manager
            .state_for_tab(first_id)
            .is_some_and(|state| state.collapsed_headings.contains_key("heading/two#0"))
    );
    let third = manager.open_tab(MarkdownTabIdentity::saved(&path, third_id));
    assert_eq!(third.collapsed_headings.get("heading/two#0"), Some(&true));
}

#[test]
fn keys_are_normalized_and_capacity_is_bounded() {
    assert_eq!(
        heading_view_key(&["Chapter  One"], " Hello   World ", 2),
        "heading/chapter one/hello world#2"
    );
    assert_eq!(table_view_key(&[], &["A", "B"], 0), "table//a|b#0");
    let mut manager = MarkdownViewStateManager::with_capacity(1);
    manager.open_tab(MarkdownTabIdentity::untitled(Uuid::new_v4()));
    manager.open_tab(MarkdownTabIdentity::untitled(Uuid::new_v4()));
    assert!(manager.tabs.len() <= 1);
}

#[test]
fn rekey_preserves_current_tab_state_without_broadcasting_to_old_path() {
    let old_path = PathBuf::from("old.md");
    let new_path = PathBuf::from("new.md");
    let tab_id = Uuid::new_v4();
    let mut manager = MarkdownViewStateManager::default();
    manager.open_tab(MarkdownTabIdentity::saved(&old_path, tab_id));
    manager.set_heading_collapsed(tab_id, "heading/one#0", true);

    let state = manager.rekey_tab(tab_id, MarkdownTabIdentity::saved(&new_path, tab_id));
    assert_eq!(state.collapsed_headings.get("heading/one#0"), Some(&true));
    assert_eq!(manager.state_for_tab(tab_id), Some(&state));

    let reopened_id = Uuid::new_v4();
    let old_state = manager.open_tab(MarkdownTabIdentity::saved(&old_path, reopened_id));
    assert!(old_state.collapsed_headings.is_empty());
}

#[test]
fn saved_identity_uses_one_absolute_key_before_the_file_exists() {
    let relative = PathBuf::from("target/gmark-uncommitted-note.md");
    let absolute = std::path::absolute(&relative).expect("absolute test path");
    let first_id = Uuid::new_v4();
    let second_id = Uuid::new_v4();
    let mut manager = MarkdownViewStateManager::default();

    manager.open_tab(MarkdownTabIdentity::saved(&relative, first_id));
    manager.set_heading_collapsed(first_id, "heading/one#0", true);
    manager.close_tab(first_id);

    let reopened = manager.open_tab(MarkdownTabIdentity::saved(&absolute, second_id));
    assert_eq!(
        reopened.collapsed_headings.get("heading/one#0"),
        Some(&true)
    );
}

#[test]
fn shared_store_is_process_scoped_but_keeps_existing_tabs_isolated() {
    let path = PathBuf::from("shared.md");
    let first_id = Uuid::new_v4();
    let second_id = Uuid::new_v4();
    let first_store = SharedMarkdownViewState::default();
    let second_store = first_store.clone();

    first_store.open_tab(MarkdownTabIdentity::saved(&path, first_id));
    first_store.set_heading_collapsed(first_id, "heading/one#0", true);
    let second = second_store.open_tab(MarkdownTabIdentity::saved(&path, second_id));
    assert_eq!(second.collapsed_headings.get("heading/one#0"), Some(&true));

    second_store.set_heading_collapsed(second_id, "heading/two#0", true);
    assert!(
        !first_store
            .state_for_tab(first_id)
            .is_some_and(|state| state.collapsed_headings.contains_key("heading/two#0"))
    );
}
