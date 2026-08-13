// @author kongweiguang

//! GPUI 无关的 Gmark 配置、偏好与工作区会话持久化契约。

#![forbid(unsafe_code)]

mod dirs;
mod installation_id;
mod persistence;
mod preferences;
mod preferences_storage;
mod recent_files;
mod workspace_codec;
mod workspace_session;

pub use dirs::AppDirs;
pub use installation_id::{
    load_or_create_installation_id, load_or_create_installation_id_with_dirs,
};
pub use preferences::{
    AccessibilityOverride, AppPreferences, AutoSavePreference, DEFAULT_LANGUAGE_ID,
    DocumentLoadingPreferences, ImagePasteBehavior, Preferences, ResolvedVisualPreferences,
    ResourceInsertBehavior, ShortcutConfig, StartupOpenPreference, StatusBarButton,
    StatusBarPreferences, SystemVisualPreferences, ThemeAppearance, ThemePalette,
    VisualAccessibilityPreferences,
};
pub use preferences_storage::{
    load_or_create_app_preferences, load_or_create_app_preferences_with_dirs, read_app_preferences,
    read_app_preferences_with_dirs, save_app_preferences, save_app_preferences_with_dirs,
};
pub use recent_files::{
    RECENT_FILES_LIMIT, read_recent_files, read_recent_files_with_dirs, record_recent_file,
    record_recent_file_with_dirs, remove_recent_file, remove_recent_file_with_dirs,
};
pub use workspace_session::{
    SessionAnchor, SessionSelection, WORKSPACE_SESSION_VERSION, WorkspaceSession,
    WorkspaceSessionAffinity, WorkspaceSessionDocumentRef, WorkspaceSessionHistoryEntry,
    WorkspaceSessionMarkdownFold, WorkspaceSessionPane, WorkspaceSessionPaneId,
    WorkspaceSessionPaneNode, WorkspaceSessionPaneTree, WorkspaceSessionPaneViewState,
    WorkspaceSessionSelection, WorkspaceSessionSplitAxis, WorkspaceSessionStore,
    WorkspaceSessionTab, WorkspaceSessionTableLayout, WorkspaceSessionWindow,
    WorkspaceSessionWindowState, read_workspace_sessions, remove_paths_from_workspace_sessions,
    remove_workspace_session, upsert_workspace_session,
};
