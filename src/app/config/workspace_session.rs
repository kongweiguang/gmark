// @author kongweiguang

//! GPUI-facing compatibility facade for the domain workspace-session store.

#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
use super::AppDirs;

pub(crate) use gmark_config::{
    WorkspaceSession, WorkspaceSessionDocumentRef, WorkspaceSessionPane, WorkspaceSessionPaneId,
    WorkspaceSessionPaneNode, WorkspaceSessionPaneTree, WorkspaceSessionPaneViewState,
    WorkspaceSessionSelection, WorkspaceSessionSplitAxis, WorkspaceSessionTab,
    WorkspaceSessionWindow, WorkspaceSessionWindowState, read_workspace_sessions,
    remove_paths_from_workspace_sessions,
};

#[cfg(not(test))]
pub(crate) use gmark_config::{remove_workspace_session, upsert_workspace_session};

#[cfg(test)]
use gmark_config::WorkspaceSessionStore;

#[cfg(test)]
fn store(dirs: &AppDirs) -> WorkspaceSessionStore {
    WorkspaceSessionStore::new(dirs.clone())
}

// These explicit-directory entry points keep the existing application tests
// isolated while delegating all decoding, migration, limits, and atomic writes
// to `gmark-config`.
#[cfg(test)]
fn read_workspace_sessions_with_dirs(dirs: &AppDirs) -> anyhow::Result<Vec<WorkspaceSession>> {
    store(dirs).read()
}

#[cfg(test)]
fn upsert_workspace_session_with_dirs(
    session: &WorkspaceSession,
    dirs: &AppDirs,
) -> anyhow::Result<()> {
    store(dirs).upsert(session)
}

#[cfg(test)]
fn remove_workspace_session_with_dirs(id: uuid::Uuid, dirs: &AppDirs) -> anyhow::Result<()> {
    store(dirs).remove(id)
}

#[cfg(test)]
fn remove_paths_from_workspace_sessions_with_dirs(
    paths: &[PathBuf],
    dirs: &AppDirs,
) -> anyhow::Result<()> {
    store(dirs).remove_paths(paths)
}

#[cfg(test)]
#[path = "../../../tests/unit/config/workspace_session.rs"]
mod tests;
