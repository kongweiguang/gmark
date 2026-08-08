// @author kongweiguang

use std::{fs, path::PathBuf};

use anyhow::Result;
use gmark_config::{
    ConfigDirs, SessionSelection, WORKSPACE_SESSION_VERSION, WorkspaceSession,
    WorkspaceSessionAffinity, WorkspaceSessionSelection, WorkspaceSessionStore,
    WorkspaceSessionTab, WorkspaceSessionWindow, WorkspaceSessionWindowState,
};
use tempfile::TempDir;
use uuid::Uuid;

fn temporary_store() -> Result<(TempDir, WorkspaceSessionStore)> {
    let temporary = TempDir::new()?;
    let store = WorkspaceSessionStore::new(ConfigDirs::from_root(temporary.path()));
    Ok((temporary, store))
}

fn tab(path: &str, pinned: bool) -> WorkspaceSessionTab {
    WorkspaceSessionTab::new(PathBuf::from(path), pinned)
}

fn session(id: Uuid, path: &str, pinned: bool) -> WorkspaceSession {
    WorkspaceSession::new(id, vec![tab(path, pinned)], 0, None)
}

fn write_registry(store: &WorkspaceSessionStore, contents: impl AsRef<[u8]>) -> Result<()> {
    fs::create_dir_all(store.dirs().root())?;
    fs::write(store.dirs().workspace_session_file(), contents)?;
    Ok(())
}

#[test]
fn reads_v1_through_v8_compatibility_samples() -> Result<()> {
    let (_temporary, store) = temporary_store()?;
    write_registry(
        &store,
        r#"{"version":1,"tabs":[{"path":"v1.md","pinned":true}],"active_index":0}"#,
    )?;
    let v1 = store.read()?;
    assert_eq!(v1.len(), 1);
    assert_eq!(v1[0].tabs[0].path, PathBuf::from("v1.md"));
    assert!(v1[0].tabs[0].pinned);

    for version in 2..=8 {
        let id = Uuid::new_v4();
        let (tab_suffix, session_suffix) = match version {
            2 => ("", ""),
            3 => (",\"view_mode\":\"split\"", ""),
            4 => (
                "",
                ",\"window\":{\"x\":12.0,\"y\":13.0,\"width\":900.0,\"height\":700.0}",
            ),
            5 => ("", ",\"workspace_panel_width\":318.0"),
            6 => ("", ",\"split_pane_ratio\":0.62"),
            7 => ("", ",\"workspace_docked_open\":false"),
            8 => (
                "",
                ",\"document_sidebar_width\":300.0,\"document_sidebar_docked_open\":true",
            ),
            _ => unreachable!(),
        };
        let selection = if version == 7 {
            ",\"selection\":{\"start\":2,\"end\":9,\"reversed\":true}"
        } else if version == 8 {
            ",\"selection\":{\"start\":2,\"end\":9,\"reversed\":false,\"anchor_affinity\":\"after\",\"head_affinity\":\"before\"}"
        } else {
            ""
        };
        let contents = format!(
            r#"{{"version":{version},"windows":[{{"id":"{id}","tabs":[{{"path":"v{version}.md","pinned":false{tab_suffix}{selection}}}],"active_index":0{session_suffix}}}]}}"#
        );
        write_registry(&store, contents)?;
        let restored = store.read()?;
        let restored_session = &restored[0];
        assert_eq!(restored_session.id, id);
        match version {
            2 => assert!(restored_session.tabs[0].view_mode.is_none()),
            3 => assert_eq!(restored_session.tabs[0].view_mode.as_deref(), Some("split")),
            4 => assert!(restored_session.window.is_some()),
            5 => assert_eq!(restored_session.workspace_panel_width, Some(318.0)),
            6 => assert_eq!(restored_session.split_pane_ratio, Some(0.62)),
            7 => {
                assert_eq!(restored_session.workspace_docked_open, Some(false));
                let selection = restored_session.tabs[0]
                    .selection
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("v7 selection missing"))?;
                let neutral = selection.selection_for_range(selection.start..selection.end);
                assert_eq!(neutral.anchor.byte_offset, 9);
                assert_eq!(neutral.anchor.affinity, WorkspaceSessionAffinity::After);
                assert_eq!(neutral.head.byte_offset, 2);
                assert_eq!(neutral.head.affinity, WorkspaceSessionAffinity::Before);
            }
            8 => {
                assert_eq!(restored_session.document_sidebar_width, Some(300.0));
                assert_eq!(restored_session.document_sidebar_docked_open, Some(true));
                let selection = restored_session.tabs[0]
                    .selection
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("v8 selection missing"))?;
                let neutral = selection.selection_for_range(selection.start..selection.end);
                assert_eq!(neutral.anchor.affinity, WorkspaceSessionAffinity::After);
                assert_eq!(neutral.head.affinity, WorkspaceSessionAffinity::Before);
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

#[test]
fn writes_current_version_after_loading_a_legacy_registry() -> Result<()> {
    let (_temporary, store) = temporary_store()?;
    let id = Uuid::new_v4();
    write_registry(
        &store,
        format!(
            r#"{{"version":2,"windows":[{{"id":"{id}","tabs":[{{"path":"old.md","pinned":false}}],"active_index":0}}]}}"#
        ),
    )?;
    let restored = store.read()?;
    store.upsert(&restored[0])?;
    let written: serde_json::Value =
        serde_json::from_slice(&fs::read(store.dirs().workspace_session_file())?)?;
    assert_eq!(
        written.get("version").and_then(serde_json::Value::as_u64),
        Some(u64::from(WORKSPACE_SESSION_VERSION))
    );
    Ok(())
}

#[test]
fn migrates_transitional_v9_without_board_data_and_rejects_board_markers() -> Result<()> {
    let (_temporary, store) = temporary_store()?;
    let id = Uuid::new_v4();
    write_registry(
        &store,
        format!(
            r#"{{"version":9,"windows":[{{"id":"{id}","tabs":[{{"path":"ordinary.md","pinned":false,"view_mode":"live"}}],"active_index":0}}]}}"#
        ),
    )?;
    let restored = store.read()?;
    assert_eq!(restored[0].tabs[0].path, PathBuf::from("ordinary.md"));
    store.upsert(&restored[0])?;
    let written: serde_json::Value =
        serde_json::from_slice(&fs::read(store.dirs().workspace_session_file())?)?;
    assert_eq!(
        written.get("version").and_then(serde_json::Value::as_u64),
        Some(u64::from(WORKSPACE_SESSION_VERSION))
    );

    write_registry(
        &store,
        format!(
            r#"{{"version":9,"windows":[{{"id":"{id}","tabs":[{{"path":"board.gboard","pinned":false,"view_mode":"board","board":{{}}}}],"active_index":0}}]}}"#
        ),
    )?;
    assert!(store.read().is_err());
    Ok(())
}

#[test]
fn normalizes_session_state_and_uses_neutral_selection_types() -> Result<()> {
    let (_temporary, store) = temporary_store()?;
    let id = Uuid::new_v4();
    let mut regular = tab("regular.md", false);
    regular.view_mode = Some("unsupported".into());
    regular.selection = Some(WorkspaceSessionSelection {
        start: 9,
        end: 2,
        reversed: true,
        anchor_affinity: None,
        head_affinity: None,
    });
    regular.scroll_x = Some(50_000_000.0);
    regular.scroll_y = Some(-50_000_000.0);
    let mut value = WorkspaceSession::new(
        id,
        vec![
            tab("   ", false),
            regular,
            tab("pinned.md", true),
            tab("regular.md", true),
        ],
        1,
        Some(PathBuf::from(" ")),
    );
    value.window = Some(WorkspaceSessionWindow {
        x: 9_000_000.0,
        y: -9_000_000.0,
        width: 100.0,
        height: 100_000.0,
        state: WorkspaceSessionWindowState::Maximized,
        display_uuid: None,
    });
    value.workspace_panel_width = Some(900.0);
    value.document_sidebar_width = Some(f32::NAN);
    value.split_pane_ratio = Some(0.9);
    store.upsert(&value)?;

    let restored = store.read()?;
    let restored = &restored[0];
    assert_eq!(restored.tabs.len(), 2);
    assert_eq!(restored.tabs[0].path, PathBuf::from("pinned.md"));
    assert_eq!(restored.tabs[1].path, PathBuf::from("regular.md"));
    assert_eq!(restored.active_index, 1);
    assert!(restored.tabs[1].view_mode.is_none());
    let selection = restored.tabs[1]
        .selection
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("selection missing after normalization"))?;
    assert_eq!((selection.start, selection.end), (2, 9));
    assert_eq!(
        selection.selection_for_range(2..9),
        SessionSelection::from_range(2..9, true)
    );
    assert_eq!(restored.tabs[1].scroll_x, Some(10_000_000.0));
    assert_eq!(restored.tabs[1].scroll_y, Some(-10_000_000.0));
    assert!(restored.workspace_root.is_none());
    assert_eq!(restored.workspace_panel_width, Some(360.0));
    assert!(restored.document_sidebar_width.is_none());
    assert_eq!(restored.split_pane_ratio, Some(0.7));
    let window = restored
        .window
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("normalized window missing"))?;
    assert_eq!((window.x, window.y), (1_000_000.0, -1_000_000.0));
    assert_eq!((window.width, window.height), (720.0, 32_768.0));
    assert_eq!(window.state, WorkspaceSessionWindowState::Maximized);
    Ok(())
}

#[test]
fn updates_windows_atomically_and_removes_paths_with_active_index_repair() -> Result<()> {
    let (_temporary, store) = temporary_store()?;
    let first_id = Uuid::new_v4();
    let second_id = Uuid::new_v4();
    let third_id = Uuid::new_v4();
    store.upsert(&session(first_id, "a.md", false))?;
    store.upsert(&session(second_id, "b.md", false))?;
    store.upsert(&session(first_id, "updated.md", false))?;
    let after_update = store.read()?;
    assert_eq!(
        after_update
            .iter()
            .map(|value| value.id)
            .collect::<Vec<_>>(),
        vec![second_id, first_id]
    );
    assert_eq!(after_update[1].tabs[0].path, PathBuf::from("updated.md"));

    store.upsert(&session(third_id, "updated.md", true))?;
    let after_duplicate = store.read()?;
    assert_eq!(
        after_duplicate
            .iter()
            .map(|value| value.id)
            .collect::<Vec<_>>(),
        vec![second_id, third_id]
    );
    assert!(after_duplicate[1].tabs[0].pinned);

    let before_invalid_update = fs::read(store.dirs().workspace_session_file())?;
    let invalid = WorkspaceSession::new(
        Uuid::new_v4(),
        (0..101)
            .map(|index| tab(&format!("{index}.md"), false))
            .collect(),
        0,
        None,
    );
    assert!(store.upsert(&invalid).is_err());
    assert_eq!(
        fs::read(store.dirs().workspace_session_file())?,
        before_invalid_update
    );

    store.remove_paths(&[PathBuf::from("updated.md")])?;
    assert_eq!(
        store
            .read()?
            .iter()
            .map(|value| value.id)
            .collect::<Vec<_>>(),
        vec![second_id]
    );
    store.remove(second_id)?;
    assert!(!store.dirs().workspace_session_file().exists());
    Ok(())
}

#[test]
fn rejects_unknown_corrupt_and_over_limit_registries() -> Result<()> {
    let (_temporary, store) = temporary_store()?;
    write_registry(&store, r#"{"version":99,"windows":[]}"#)?;
    assert!(store.read().is_err());

    write_registry(&store, "not json")?;
    assert!(store.read().is_err());

    write_registry(&store, vec![b' '; 1_048_577])?;
    assert!(store.read().is_err());

    let windows = (0..21)
        .map(|index| {
            format!(
                r#"{{"id":"{}","tabs":[{{"path":"{index}.md","pinned":false}}],"active_index":0}}"#,
                Uuid::new_v4()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    write_registry(&store, format!(r#"{{"version":8,"windows":[{windows}]}}"#))?;
    assert!(store.read().is_err());
    Ok(())
}
