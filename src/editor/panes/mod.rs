// @author kongweiguang

//! Pure pane-tree state used by the editor integration layer.
//!
//! The model remains pure; the view and canvas adapters are the GPUI-facing
//! integration layer used by the root editor window.

use std::path::PathBuf;

use gmark_paged_document::OpenProbe;
use gpui::{App, Context, Entity};

use crate::document_host::DetachedDocumentHostView;
use crate::editor::document_session::EditorDocumentSession;

mod canvas;
mod controller;
mod model;
mod readonly;
mod view;

pub use canvas::*;
pub use controller::*;
pub use model::*;
pub use readonly::*;
pub use view::*;

/// Runtime canvas mounted for an active pane leaf.  The enum keeps the
/// markdown and source-backed host paths explicit while allowing one factory
/// slot in [`PaneWorkspaceView`] to return either Render entity.
pub(crate) enum PaneCanvasEntity {
    Markdown(Entity<PaneEditorCanvas>),
    DocumentHost(Entity<PaneDocumentHostCanvas>),
    ReadOnly(Entity<PaneReadOnlyCanvas>),
}

impl PaneCanvasEntity {
    pub(crate) fn markdown_editor(&self, cx: &App) -> Option<Entity<crate::editor::Editor>> {
        match self {
            Self::Markdown(canvas) => Some(canvas.read(cx).editor()),
            Self::DocumentHost(_) | Self::ReadOnly(_) => None,
        }
    }

    pub(crate) fn document_host(
        &self,
        cx: &App,
    ) -> Option<Entity<crate::document_host::DocumentHost>> {
        match self {
            Self::Markdown(_) | Self::ReadOnly(_) => None,
            Self::DocumentHost(canvas) => Some(canvas.read(cx).host()),
        }
    }
}

/// Crate-facing bridge; the canvas constructor itself stays private to the
/// pane module so it is not a general-purpose window-shell factory.
pub(crate) fn create_pane_editor_canvas(
    cx: &mut Context<PaneEditorCanvas>,
    session: EditorDocumentSession,
    file_path: Option<PathBuf>,
    pane_tab_id: uuid::Uuid,
    view_state: PaneViewStateSnapshot,
) -> PaneEditorCanvas {
    PaneEditorCanvas::new(cx, session, file_path, pane_tab_id, view_state)
}

pub(crate) fn create_pane_document_host_canvas(
    cx: &mut Context<PaneDocumentHostCanvas>,
    path: PathBuf,
    probe: OpenProbe,
    detached: DetachedDocumentHostView,
) -> PaneDocumentHostCanvas {
    PaneDocumentHostCanvas::new(cx, path, probe, detached)
}

pub(crate) fn create_pane_readonly_canvas(
    cx: &mut Context<PaneReadOnlyCanvas>,
    kind: PaneReadOnlyKind,
) -> PaneReadOnlyCanvas {
    PaneReadOnlyCanvas::new(kind, cx)
}
