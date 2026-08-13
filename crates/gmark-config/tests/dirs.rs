// @author kongweiguang

use std::fs;

use anyhow::Result;
use directories::BaseDirs;
use gmark_config::{
    AppDirs, WorkspaceSessionStore, load_or_create_installation_id_with_dirs,
    read_recent_files_with_dirs,
};
#[cfg(unix)]
use gmark_config::{
    WorkspaceSession, WorkspaceSessionPane, WorkspaceSessionTab, record_recent_file_with_dirs,
};
use tempfile::TempDir;
#[cfg(unix)]
use uuid::Uuid;

#[test]
fn ui_check_root_maps_each_artifact_to_its_own_root_without_creating() -> Result<()> {
    let temporary = TempDir::new()?;
    let root = temporary.path().join("ui-check");
    let dirs = AppDirs::from_system_with_override(Some(root.clone()))?;

    assert_eq!(dirs.config_root(), root.join("config"));
    assert_eq!(dirs.state_root(), root.join("state"));
    assert_eq!(dirs.cache_root(), root.join("cache"));
    assert_eq!(dirs.runtime_root(), root.join("runtime"));
    assert_eq!(dirs.config_toml_file(), root.join("config/config.toml"));
    assert_eq!(dirs.languages_dir(), root.join("config/languages"));
    assert_eq!(dirs.history_file(), root.join("state/.history"));
    assert_eq!(
        dirs.workspace_session_file(),
        root.join("state/workspace-session.json")
    );
    assert_eq!(
        dirs.workspace_session_pre_v10_file(),
        root.join("state/workspace-session.pre-v10.json")
    );
    assert_eq!(
        dirs.installation_id_file(),
        root.join("state/installation-id")
    );
    assert_eq!(dirs.recovery_dir(), root.join("state/recovery"));
    assert_eq!(dirs.crash_reports_dir(), root.join("state/crash-reports"));
    assert_eq!(dirs.updates_dir(), root.join("cache/updates"));
    assert_eq!(
        dirs.large_document_indexes_dir(),
        root.join("cache/large-document-indexes")
    );
    assert_eq!(dirs.latex_svg_dir(), root.join("cache/latex-svg"));
    assert_eq!(dirs.mermaid_svg_dir(), root.join("cache/mermaid-svg"));
    assert_eq!(
        dirs.instance_lock_file(),
        root.join("runtime/instance.lock")
    );
    assert!(!root.exists());
    Ok(())
}

#[test]
fn ui_check_override_rejects_empty_and_relative_paths() {
    assert!(AppDirs::from_system_with_override(Some(std::path::PathBuf::new())).is_err());
    assert!(
        AppDirs::from_system_with_override(Some(std::path::PathBuf::from("relative-root",)))
            .is_err()
    );
}

#[test]
fn system_constructor_uses_dot_gmark_under_the_user_home() -> Result<()> {
    let dirs = AppDirs::from_system_with_override(None)?;
    let base = BaseDirs::new().ok_or_else(|| anyhow::anyhow!("home directory unavailable"))?;
    let expected = base.home_dir().join(".gmark");
    assert_eq!(dirs.config_root(), expected.join("config"));
    assert_eq!(dirs.state_root(), expected.join("state"));
    assert_eq!(dirs.cache_root(), expected.join("cache"));
    assert_eq!(dirs.runtime_root(), expected.join("runtime"));
    Ok(())
}

#[test]
fn roots_are_created_on_demand_and_state_files_are_private_on_unix() -> Result<()> {
    let temporary = TempDir::new()?;
    let dirs = AppDirs::from_system_with_override(Some(temporary.path().join("ui-check")))?;
    dirs.ensure_config_root()?;
    dirs.ensure_state_parent(&dirs.workspace_session_file())?;
    dirs.ensure_cache_parent(&dirs.large_document_indexes_dir())?;
    dirs.ensure_runtime_parent(&dirs.instance_lock_file())?;

    assert!(dirs.config_root().is_dir());
    assert!(dirs.state_root().is_dir());
    assert!(dirs.cache_root().is_dir());
    assert!(dirs.runtime_root().is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            fs::metadata(dirs.state_root())?.permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(dirs.runtime_root())?.permissions().mode() & 0o777,
            0o700
        );
    }
    Ok(())
}

#[test]
fn non_directory_roots_are_rejected() -> Result<()> {
    let temporary = TempDir::new()?;
    let conflict = temporary.path().join("state");
    fs::write(&conflict, b"not a directory")?;
    let dirs = AppDirs::from_roots(
        temporary.path().join("config"),
        conflict,
        temporary.path().join("cache"),
        temporary.path().join("runtime"),
    );
    assert!(dirs.validate_state_root().is_err());
    assert!(dirs.ensure_state_root().is_err());
    Ok(())
}

#[test]
fn ui_check_root_does_not_read_or_write_legacy_single_root_files() -> Result<()> {
    let temporary = TempDir::new()?;
    let legacy_root = temporary.path().join("legacy");
    fs::create_dir_all(&legacy_root)?;
    // These canaries are deliberately invalid for the legacy readers. Any
    // accidental fallback would fail the operation instead of looking empty.
    let old_history = b"\xfflegacy.md\n";
    let old_session = br#"{"version":"legacy-canary"}"#;
    let old_installation_id = "legacy-installation-id-canary\n".to_owned();
    fs::write(legacy_root.join(".history"), old_history)?;
    fs::write(legacy_root.join("workspace-session.json"), old_session)?;
    fs::write(legacy_root.join("installation-id"), &old_installation_id)?;

    let dirs = AppDirs::from_system_with_override(Some(legacy_root.clone()))?;
    assert!(read_recent_files_with_dirs(&dirs)?.is_empty());
    let new_installation_id = load_or_create_installation_id_with_dirs(&dirs)?;
    assert_ne!(new_installation_id.to_string(), old_installation_id.trim());
    assert!(WorkspaceSessionStore::new(dirs.clone()).read()?.is_empty());

    assert_eq!(fs::read(legacy_root.join(".history"))?, old_history);
    assert_eq!(
        fs::read(legacy_root.join("workspace-session.json"))?,
        old_session
    );
    assert_eq!(
        fs::read_to_string(legacy_root.join("installation-id"))?,
        old_installation_id
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn sensitive_state_files_are_created_with_mode_0600() -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let temporary = TempDir::new()?;
    let dirs = AppDirs::from_ui_check_root(temporary.path().join("ui-check"));
    record_recent_file_with_dirs(std::path::Path::new("history.md"), &dirs)?;
    let history_mode = fs::metadata(dirs.history_file())?.permissions().mode() & 0o777;
    assert_eq!(history_mode, 0o600);

    let _ = load_or_create_installation_id_with_dirs(&dirs)?;
    let installation_mode = fs::metadata(dirs.installation_id_file())?
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(installation_mode, 0o600);

    let store = WorkspaceSessionStore::new(dirs.clone());
    let mut session = WorkspaceSession::single_pane(Uuid::new_v4(), None);
    let pane_id = session.focused_pane;
    let tab = WorkspaceSessionTab::new("session.md".into(), false);
    let tab_id = tab.id;
    session
        .panes
        .insert(pane_id, WorkspaceSessionPane::new(vec![tab], Some(tab_id)));
    store.upsert(&session)?;
    let session_mode = fs::metadata(dirs.workspace_session_file())?
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(session_mode, 0o600);
    Ok(())
}

#[cfg(unix)]
#[test]
fn symbolic_link_roots_are_rejected() -> Result<()> {
    use std::os::unix::fs::symlink;

    let temporary = TempDir::new()?;
    let target = temporary.path().join("target");
    let link = temporary.path().join("state");
    fs::create_dir(&target)?;
    symlink(&target, &link)?;
    let dirs = AppDirs::from_roots(
        temporary.path().join("config"),
        link,
        temporary.path().join("cache"),
        temporary.path().join("runtime"),
    );
    assert!(dirs.validate_state_root().is_err());
    assert!(dirs.ensure_state_root().is_err());
    Ok(())
}
