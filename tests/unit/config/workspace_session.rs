// @author kongweiguang

use super::{
    WorkspaceSession, WorkspaceSessionTab, WorkspaceSessionWindow, WorkspaceSessionWindowState,
    read_workspace_sessions_with_dirs, remove_paths_from_workspace_sessions_with_dirs,
    remove_workspace_session_with_dirs, upsert_workspace_session_with_dirs,
};
use crate::config::GmarkConfigDirs;
use std::path::PathBuf;

fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("gmark-session-{name}-{}", uuid::Uuid::new_v4()))
}

fn session(id: uuid::Uuid, path: &str, pinned: bool) -> WorkspaceSession {
    WorkspaceSession::new(
        id,
        vec![WorkspaceSessionTab::new(PathBuf::from(path), pinned)],
        0,
        None,
    )
}

#[test]
fn registry_round_trips_multiple_windows_and_updates_one_in_place() {
    let root = temp_root("registry");
    let dirs = GmarkConfigDirs::from_root(&root);
    let first_id = uuid::Uuid::new_v4();
    let second_id = uuid::Uuid::new_v4();
    upsert_workspace_session_with_dirs(&session(first_id, "a.md", false), &dirs).unwrap();
    upsert_workspace_session_with_dirs(&session(second_id, "b.md", true), &dirs).unwrap();
    upsert_workspace_session_with_dirs(&session(first_id, "updated.md", false), &dirs).unwrap();

    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    assert_eq!(restored.len(), 2);
    assert_eq!(restored[0].id, second_id);
    assert_eq!(restored[1].id, first_id);
    assert_eq!(restored[1].tabs[0].path, PathBuf::from("updated.md"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn removing_one_window_preserves_the_other_and_empty_registry_removes_file() {
    let root = temp_root("remove");
    let dirs = GmarkConfigDirs::from_root(&root);
    let first_id = uuid::Uuid::new_v4();
    let second_id = uuid::Uuid::new_v4();
    upsert_workspace_session_with_dirs(&session(first_id, "a.md", false), &dirs).unwrap();
    upsert_workspace_session_with_dirs(&session(second_id, "b.md", false), &dirs).unwrap();
    remove_workspace_session_with_dirs(first_id, &dirs).unwrap();
    assert_eq!(read_workspace_sessions_with_dirs(&dirs).unwrap().len(), 1);
    remove_workspace_session_with_dirs(second_id, &dirs).unwrap();
    assert!(!dirs.workspace_session_file().exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_v1_session_is_migrated_to_registry() {
    let root = temp_root("legacy");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        dirs.workspace_session_file(),
        r#"{"version":1,"tabs":[{"path":"a.md","pinned":true}],"active_index":0}"#,
    )
    .unwrap();
    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].tabs[0].path, PathBuf::from("a.md"));
    assert!(restored[0].tabs[0].pinned);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_v2_is_migrated_with_empty_view_state() {
    let root = temp_root("legacy-v2");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::create_dir_all(&root).unwrap();
    let id = uuid::Uuid::new_v4();
    std::fs::write(
            dirs.workspace_session_file(),
            format!(
                r#"{{"version":2,"windows":[{{"id":"{id}","tabs":[{{"path":"a.md","pinned":false}}],"active_index":0}}]}}"#
            ),
        )
        .unwrap();
    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].id, id);
    assert!(restored[0].tabs[0].view_mode.is_none());
    assert!(restored[0].tabs[0].selection.is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_v3_is_migrated_with_empty_window_state() {
    let root = temp_root("legacy-v3");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::create_dir_all(&root).unwrap();
    let id = uuid::Uuid::new_v4();
    std::fs::write(
            dirs.workspace_session_file(),
            format!(
                r#"{{"version":3,"windows":[{{"id":"{id}","tabs":[{{"path":"a.md","pinned":false,"view_mode":"split"}}],"active_index":0}}]}}"#
            ),
        )
        .unwrap();
    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    assert_eq!(restored[0].tabs[0].view_mode.as_deref(), Some("split"));
    assert!(restored[0].window.is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_v4_is_migrated_with_empty_workspace_panel_width() {
    let root = temp_root("legacy-v4");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::create_dir_all(&root).unwrap();
    let id = uuid::Uuid::new_v4();
    std::fs::write(
            dirs.workspace_session_file(),
            format!(
                r#"{{"version":4,"windows":[{{"id":"{id}","tabs":[{{"path":"a.md","pinned":false}}],"active_index":0}}]}}"#
            ),
        )
        .unwrap();
    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    assert_eq!(restored[0].id, id);
    assert_eq!(restored[0].workspace_panel_width, None);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_v5_is_migrated_with_empty_split_pane_ratio() {
    let root = temp_root("legacy-v5");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::create_dir_all(&root).unwrap();
    let id = uuid::Uuid::new_v4();
    std::fs::write(
            dirs.workspace_session_file(),
            format!(
                r#"{{"version":5,"windows":[{{"id":"{id}","tabs":[{{"path":"a.md","pinned":false}}],"active_index":0,"workspace_panel_width":318.0}}]}}"#
            ),
        )
        .unwrap();
    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    assert_eq!(restored[0].id, id);
    assert_eq!(restored[0].workspace_panel_width, Some(318.0));
    assert_eq!(restored[0].split_pane_ratio, None);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_v6_is_migrated_with_empty_workspace_visibility() {
    let root = temp_root("legacy-v6");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::create_dir_all(&root).unwrap();
    let id = uuid::Uuid::new_v4();
    std::fs::write(
            dirs.workspace_session_file(),
            format!(
                r#"{{"version":6,"windows":[{{"id":"{id}","tabs":[{{"path":"a.md","pinned":false}}],"active_index":0,"workspace_panel_width":318.0,"split_pane_ratio":0.62}}]}}"#
            ),
        )
        .unwrap();

    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    assert_eq!(restored[0].workspace_panel_width, Some(318.0));
    assert_eq!(restored[0].split_pane_ratio, Some(0.62));
    assert_eq!(restored[0].workspace_docked_open, None);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_v7_selection_without_affinity_uses_directional_defaults() {
    let root = temp_root("legacy-v7-selection");
    let dirs = GmarkConfigDirs::from_root(&root);
    std::fs::create_dir_all(&root).unwrap();
    let id = uuid::Uuid::new_v4();
    std::fs::write(
        dirs.workspace_session_file(),
        format!(
            r#"{{"version":7,"windows":[{{"id":"{id}","tabs":[{{"path":"a.md","pinned":false,"selection":{{"start":2,"end":9,"reversed":true}}}}],"active_index":0}}]}}"#
        ),
    )
    .unwrap();

    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    let selection = restored[0].tabs[0].selection.as_ref().unwrap();
    let source = selection.source_selection_for_range(selection.start..selection.end);
    assert_eq!(source.anchor.byte_offset, 9);
    assert_eq!(
        source.anchor.affinity,
        gmark_large_document::SourceAffinity::After
    );
    assert_eq!(source.head.byte_offset, 2);
    assert_eq!(
        source.head.affinity,
        gmark_large_document::SourceAffinity::Before
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn workspace_docked_visibility_round_trips() {
    let root = temp_root("workspace-visibility");
    let dirs = GmarkConfigDirs::from_root(&root);
    let mut closed = session(uuid::Uuid::new_v4(), "closed.md", false);
    closed.workspace_docked_open = Some(false);
    upsert_workspace_session_with_dirs(&closed, &dirs).unwrap();

    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    assert_eq!(restored[0].workspace_docked_open, Some(false));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn workspace_panel_width_is_finite_and_bounded_before_persistence() {
    let root = temp_root("workspace-panel-width");
    let dirs = GmarkConfigDirs::from_root(&root);
    let mut bounded = session(uuid::Uuid::new_v4(), "bounded.md", false);
    bounded.workspace_panel_width = Some(900.0);
    upsert_workspace_session_with_dirs(&bounded, &dirs).unwrap();
    assert_eq!(
        read_workspace_sessions_with_dirs(&dirs).unwrap()[0].workspace_panel_width,
        Some(360.0)
    );

    let mut invalid = session(uuid::Uuid::new_v4(), "invalid.md", false);
    invalid.workspace_panel_width = Some(f32::NAN);
    upsert_workspace_session_with_dirs(&invalid, &dirs).unwrap();
    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    assert_eq!(
        restored
            .iter()
            .find(|session| session.id == invalid.id)
            .unwrap()
            .workspace_panel_width,
        None
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn split_pane_ratio_is_finite_and_bounded_before_persistence() {
    let root = temp_root("split-pane-ratio");
    let dirs = GmarkConfigDirs::from_root(&root);
    let mut bounded = session(uuid::Uuid::new_v4(), "bounded.md", false);
    bounded.split_pane_ratio = Some(0.9);
    upsert_workspace_session_with_dirs(&bounded, &dirs).unwrap();
    assert_eq!(
        read_workspace_sessions_with_dirs(&dirs).unwrap()[0].split_pane_ratio,
        Some(0.7)
    );

    let mut invalid = session(uuid::Uuid::new_v4(), "invalid.md", false);
    invalid.split_pane_ratio = Some(f32::NAN);
    upsert_workspace_session_with_dirs(&invalid, &dirs).unwrap();
    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    assert_eq!(
        restored
            .iter()
            .find(|session| session.id == invalid.id)
            .unwrap()
            .split_pane_ratio,
        None
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_view_state_is_normalized_before_persistence() {
    let root = temp_root("invalid-view-state");
    let dirs = GmarkConfigDirs::from_root(&root);
    let mut tab = WorkspaceSessionTab::new(PathBuf::from("a.md"), false);
    tab.view_mode = Some("unsupported".to_owned());
    tab.selection = Some(super::WorkspaceSessionSelection {
        start: 9,
        end: 2,
        reversed: true,
        anchor_affinity: None,
        head_affinity: None,
    });
    tab.scroll_x = Some(50_000_000.0);
    tab.scroll_y = Some(-50_000_000.0);
    let session = WorkspaceSession::new(uuid::Uuid::new_v4(), vec![tab], 0, None);
    upsert_workspace_session_with_dirs(&session, &dirs).unwrap();
    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    let tab = &restored[0].tabs[0];
    assert!(tab.view_mode.is_none());
    assert_eq!(tab.selection.as_ref().unwrap().start, 2);
    assert_eq!(tab.selection.as_ref().unwrap().end, 9);
    assert_eq!(tab.scroll_x, Some(10_000_000.0));
    assert_eq!(tab.scroll_y, Some(-10_000_000.0));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn workspace_session_round_trips_source_anchor_affinity() {
    let root = temp_root("selection-affinity");
    let dirs = GmarkConfigDirs::from_root(&root);
    let source_selection = gmark_large_document::SourceSelection {
        anchor: gmark_large_document::SourceAnchor::new(
            9,
            gmark_large_document::SourceAffinity::After,
        ),
        head: gmark_large_document::SourceAnchor::new(
            2,
            gmark_large_document::SourceAffinity::Before,
        ),
    };
    let mut tab = WorkspaceSessionTab::new(PathBuf::from("a.md"), false);
    tab.selection = Some(super::WorkspaceSessionSelection::from_source_selection(
        source_selection,
    ));
    let session = WorkspaceSession::new(uuid::Uuid::new_v4(), vec![tab], 0, None);
    upsert_workspace_session_with_dirs(&session, &dirs).unwrap();

    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    let selection = restored[0].tabs[0].selection.as_ref().unwrap();
    assert_eq!(
        selection.source_selection_for_range(selection.start..selection.end),
        source_selection
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn invalid_window_state_is_dropped_and_extreme_valid_state_is_bounded() {
    let root = temp_root("invalid-window-state");
    let dirs = GmarkConfigDirs::from_root(&root);
    let mut invalid = session(uuid::Uuid::new_v4(), "invalid.md", false);
    invalid.window = Some(WorkspaceSessionWindow {
        x: f32::NAN,
        y: 0.0,
        width: 1080.0,
        height: 720.0,
        state: WorkspaceSessionWindowState::Windowed,
        display_uuid: None,
    });
    upsert_workspace_session_with_dirs(&invalid, &dirs).unwrap();
    assert!(
        read_workspace_sessions_with_dirs(&dirs).unwrap()[0]
            .window
            .is_none()
    );

    let mut bounded = session(uuid::Uuid::new_v4(), "bounded.md", false);
    bounded.window = Some(WorkspaceSessionWindow {
        x: 9_000_000.0,
        y: -9_000_000.0,
        width: 100.0,
        height: 100_000.0,
        state: WorkspaceSessionWindowState::Maximized,
        display_uuid: None,
    });
    upsert_workspace_session_with_dirs(&bounded, &dirs).unwrap();
    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    let window = restored
        .iter()
        .find(|session| session.id == bounded.id)
        .unwrap()
        .window
        .as_ref()
        .unwrap();
    assert_eq!(window.x, 1_000_000.0);
    assert_eq!(window.y, -1_000_000.0);
    assert_eq!(window.width, 720.0);
    assert_eq!(window.height, 32_768.0);
    assert_eq!(window.state, WorkspaceSessionWindowState::Maximized);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn newest_window_owns_duplicate_path_across_registry() {
    let root = temp_root("duplicate-path");
    let dirs = GmarkConfigDirs::from_root(&root);
    let first_id = uuid::Uuid::new_v4();
    let second_id = uuid::Uuid::new_v4();
    upsert_workspace_session_with_dirs(&session(first_id, "same.md", false), &dirs).unwrap();
    upsert_workspace_session_with_dirs(&session(second_id, "same.md", true), &dirs).unwrap();
    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].id, second_id);
    assert!(restored[0].tabs[0].pinned);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_path_removal_preserves_other_tabs_and_repairs_active_index() {
    let root = temp_root("recovery-path");
    let dirs = GmarkConfigDirs::from_root(&root);
    let id = uuid::Uuid::new_v4();
    let session = WorkspaceSession::new(
        id,
        vec![
            WorkspaceSessionTab::new(PathBuf::from("recovered.md"), true),
            WorkspaceSessionTab::new(PathBuf::from("clean.md"), false),
        ],
        0,
        None,
    );
    upsert_workspace_session_with_dirs(&session, &dirs).unwrap();
    remove_paths_from_workspace_sessions_with_dirs(&[PathBuf::from("recovered.md")], &dirs)
        .unwrap();
    let restored = read_workspace_sessions_with_dirs(&dirs).unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].id, id);
    assert_eq!(restored[0].tabs[0].path, PathBuf::from("clean.md"));
    assert_eq!(restored[0].active_index, 0);
    std::fs::remove_dir_all(root).unwrap();
}
