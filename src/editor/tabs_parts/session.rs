// @author kongweiguang

use super::*;
use crate::editor::document_session::EditorDocumentSession;
use serde_json::{Value, json};

#[path = "session_parts/persistence.rs"]
mod persistence;
#[path = "session_parts/restore.rs"]
mod restore;
#[path = "session_parts/snapshot.rs"]
mod snapshot;
#[path = "session_parts/tab_state.rs"]
mod tab_state;
#[path = "session_parts/view_state.rs"]
mod view_state;
