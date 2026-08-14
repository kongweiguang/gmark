// @author kongweiguang

//! Editor and recovery window construction.

use super::*;

use crate::app::document_service::{
    DocumentService, ResidentMarkdownSource, SharedDocumentHostOpen, SharedResidentOpen,
};

/// A canonical workspace tab keeps the service-owned runtime open alive while
/// the Editor rebuilds its pane tree. Markdown/recovery tabs use the resident
/// adapter; structured and paged text use the host adapter. The enum is kept at
/// the app boundary so restore never copies document bodies into a second tab.
pub(crate) enum WorkspaceSessionRestoredOpen {
    Resident(SharedResidentOpen),
    Host(SharedDocumentHostOpen),
    Image { path: PathBuf },
    Error { path: PathBuf, message: String },
}
// Keep window construction and workspace restoration in bounded modules while
// preserving the existing app-menu re-exports and call paths.
#[path = "windows_parts/open.rs"]
mod open;
#[path = "windows_parts/session.rs"]
mod session;

#[cfg(test)]
pub(crate) use open::open_recovered_editor_window;
pub(crate) use open::{
    PreparedPagedRecovery, PreparedRecoveredDocument, open_detached_tab_window, open_editor_window,
    open_file_in_new_window, open_file_in_safe_source_window, open_paged_recovery_window,
    open_prepared_paged_recovery_window, open_prepared_recovered_editor_tabs_window,
    open_recovered_editor_tabs_window, prepare_paged_recovery, prepare_recovered_document,
};
pub(crate) use session::open_workspace_session_window;
