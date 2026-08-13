// @author kongweiguang

use super::*;

impl Editor {
    pub(crate) fn snapshot_pane_node(
        &self,
        node: &crate::editor::panes::PaneNode,
        workspace: &crate::editor::panes::PaneWorkspace<
            gmark_document_runtime::DocumentId,
            crate::editor::panes::PaneDocumentRef,
        >,
        panes: &mut std::collections::BTreeMap<
            crate::config::workspace_session::WorkspaceSessionPaneId,
            crate::config::workspace_session::WorkspaceSessionPane,
        >,
        cx: &App,
    ) -> anyhow::Result<crate::config::workspace_session::WorkspaceSessionPaneTree> {
        match node {
            crate::editor::panes::PaneNode::Leaf(pane_id) => {
                let pane = workspace.pane(*pane_id).ok_or_else(|| {
                    anyhow::anyhow!("pane tree references missing pane {:?}", pane_id.as_uuid())
                })?;
                let active_id = pane.active_tab_id();
                let mut tabs = Vec::with_capacity(pane.tabs().len());
                let mut active_view_id = None;
                for tab in pane.tabs() {
                    let snapshot = self.snapshot_pane_tab(tab, cx)?;
                    if active_id == Some(tab.id()) {
                        active_view_id = Some(snapshot.id);
                    }
                    tabs.push(snapshot);
                }
                if active_id.is_some() && active_view_id.is_none() {
                    return Err(anyhow::anyhow!(
                        "pane {:?} active tab is missing from its tab list",
                        pane_id.as_uuid()
                    ));
                }
                let config_pane_id = crate::config::workspace_session::WorkspaceSessionPaneId::from(
                    pane_id.as_uuid(),
                );
                panes.insert(
                    config_pane_id,
                    crate::config::workspace_session::WorkspaceSessionPane::new(
                        tabs,
                        active_view_id,
                    ),
                );
                Ok(
                    crate::config::workspace_session::WorkspaceSessionPaneNode::Leaf(
                        config_pane_id,
                    ),
                )
            }
            crate::editor::panes::PaneNode::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let ratio = if ratio.is_finite() {
                    ratio.clamp(0.1, 0.9)
                } else {
                    0.5
                };
                let axis = match axis {
                    crate::editor::panes::SplitAxis::Horizontal => {
                        crate::config::workspace_session::WorkspaceSessionSplitAxis::Horizontal
                    }
                    crate::editor::panes::SplitAxis::Vertical => {
                        crate::config::workspace_session::WorkspaceSessionSplitAxis::Vertical
                    }
                };
                Ok(
                    crate::config::workspace_session::WorkspaceSessionPaneNode::Split {
                        axis,
                        ratio,
                        first: Box::new(Self::snapshot_pane_node(
                            self, first, workspace, panes, cx,
                        )?),
                        second: Box::new(Self::snapshot_pane_node(
                            self, second, workspace, panes, cx,
                        )?),
                    },
                )
            }
        }
    }

    pub(crate) fn legacy_workspace_session_tab(
        &self,
        pinned: bool,
        cx: &App,
    ) -> anyhow::Result<crate::config::workspace_session::WorkspaceSessionTab> {
        let (document_id, view_id) = if let Some(host) = self.document_host.as_ref() {
            let host = host.read(cx);
            let document_id = host
                .document_id()
                .ok_or_else(|| anyhow::anyhow!("failed to read active Host document id"))?;
            let view_id = host
                .view_id()
                .ok_or_else(|| anyhow::anyhow!("failed to read active Host view id"))?
                .uuid();
            (document_id, view_id)
        } else {
            let document_id = self
                .source_document
                .document_id()
                .map_err(|error| anyhow::anyhow!("failed to read active document id: {error}"))?;
            (document_id, self.source_document.view_id().uuid())
        };
        let path = self
            .image_preview_path
            .clone()
            .or_else(|| {
                self.file_open_failure
                    .as_ref()
                    .map(|failure| failure.path.clone())
            })
            .or_else(|| self.file_path.clone());
        let document = match path {
            Some(path) if !self.recovered_session && !path.as_os_str().is_empty() => {
                crate::config::workspace_session::WorkspaceSessionDocumentRef::File(path)
            }
            _ => crate::config::workspace_session::WorkspaceSessionDocumentRef::Recovery(
                document_id.uuid(),
            ),
        };
        let mut snapshot = crate::config::workspace_session::WorkspaceSessionTab::new(
            std::path::PathBuf::new(),
            pinned,
        );
        snapshot.id = view_id;
        snapshot.document = document;
        snapshot.state = self.pane_view_state_snapshot(cx);
        Ok(snapshot)
    }

    pub(crate) fn snapshot_workspace_session_tab(
        record: &crate::editor::tabs::TabRecord,
        source: &crate::editor::tabs::DocumentTabSnapshot,
        cx: &App,
    ) -> anyhow::Result<crate::config::workspace_session::WorkspaceSessionTab> {
        let (document_id, view_id) = if let Some(host) = source.document_host.as_ref() {
            let host = host.read(cx);
            let document_id = host
                .document_id()
                .ok_or_else(|| anyhow::anyhow!("failed to read tab Host document id"))?;
            let view_id = host
                .view_id()
                .ok_or_else(|| anyhow::anyhow!("failed to read tab Host view id"))?
                .uuid();
            (document_id, view_id)
        } else {
            let document_id = source
                .source_document
                .document_id()
                .map_err(|error| anyhow::anyhow!("failed to read tab document id: {error}"))?;
            (document_id, source.source_document.view_id().uuid())
        };
        let path = source
            .image_preview_path
            .clone()
            .or_else(|| {
                source
                    .file_open_failure
                    .as_ref()
                    .map(|failure| failure.path.clone())
            })
            .or_else(|| source.file_path.clone());
        let document = match path {
            Some(path) if !source.recovered_session && !path.as_os_str().is_empty() => {
                crate::config::workspace_session::WorkspaceSessionDocumentRef::File(path)
            }
            _ => crate::config::workspace_session::WorkspaceSessionDocumentRef::Recovery(
                document_id.uuid(),
            ),
        };
        let (selection, scroll) = if let Some(host) = source.document_host.as_ref() {
            host.read(cx).workspace_source_state()
        } else {
            (source.selection.source_selection(), source.scroll_offset)
        };
        let mut snapshot = crate::config::workspace_session::WorkspaceSessionTab::new(
            std::path::PathBuf::new(),
            record.pinned,
        );
        snapshot.id = view_id;
        snapshot.document = document;
        snapshot.state = Self::workspace_view_state(source.view_mode, selection, scroll, None);
        Ok(snapshot)
    }

    fn snapshot_pane_tab(
        &self,
        tab: &crate::editor::panes::TabView<
            gmark_document_runtime::DocumentId,
            crate::editor::panes::PaneDocumentRef,
        >,
        cx: &App,
    ) -> anyhow::Result<crate::config::workspace_session::WorkspaceSessionTab> {
        let tab_id = tab.id().as_uuid();
        let view_id = tab.view().view_id().uuid();
        if tab_id != view_id {
            return Err(anyhow::anyhow!(
                "pane tab id {tab_id} does not match runtime view instance {view_id}"
            ));
        }
        let document_id = tab.document().to_owned();
        let (_legacy_pinned, recovered, fallback_state) = self.pane_tab_metadata(document_id, cx);
        let state = tab.view().view_state_snapshot();
        let state = if state == Default::default() {
            fallback_state
        } else {
            state
        };
        let path = if let Some(path) = tab.view().host_path().cloned() {
            path
        } else if let Some(lease) = tab.view().lease() {
            let handle = lease.handle();
            let controller = handle
                .lock()
                .map_err(|error| anyhow::anyhow!("failed to lock pane document: {error}"))?;
            controller.session().file_identity.canonical_path.clone()
        } else {
            std::path::PathBuf::new()
        };
        let readonly_recovery_id = match tab.view().readonly_kind() {
            Some(crate::editor::panes::PaneReadOnlyKind::Error { message, .. }) => {
                Self::recovery_id_from_readonly_error(message)
            }
            _ => None,
        };
        let document = if let Some(recovery_id) = readonly_recovery_id {
            crate::config::workspace_session::WorkspaceSessionDocumentRef::Recovery(recovery_id)
        } else {
            match tab.view().readonly_kind() {
                Some(
                    crate::editor::panes::PaneReadOnlyKind::Image {
                        path: readonly_path,
                    }
                    | crate::editor::panes::PaneReadOnlyKind::Error {
                        path: readonly_path,
                        ..
                    },
                ) if !readonly_path.as_os_str().is_empty() => {
                    crate::config::workspace_session::WorkspaceSessionDocumentRef::File(
                        readonly_path.clone(),
                    )
                }
                _ if recovered || path.as_os_str().is_empty() => {
                    crate::config::workspace_session::WorkspaceSessionDocumentRef::Recovery(
                        document_id.uuid(),
                    )
                }
                _ => crate::config::workspace_session::WorkspaceSessionDocumentRef::File(path),
            }
        };
        let mut snapshot = crate::config::workspace_session::WorkspaceSessionTab::new(
            std::path::PathBuf::new(),
            tab.is_pinned(),
        );
        snapshot.id = view_id;
        snapshot.document = document;
        snapshot.state = state;
        Ok(snapshot)
    }

    fn recovery_id_from_readonly_error(message: &str) -> Option<uuid::Uuid> {
        let remainder = message.strip_prefix("recovery document ")?;
        let id = remainder.split_whitespace().next()?;
        uuid::Uuid::parse_str(id).ok()
    }

    fn pane_tab_metadata(
        &self,
        document_id: gmark_document_runtime::DocumentId,
        cx: &App,
    ) -> (
        bool,
        bool,
        crate::config::workspace_session::WorkspaceSessionPaneViewState,
    ) {
        let is_live_pane_document = self.pane_workspace.as_ref().is_some_and(|workspace| {
            let workspace = workspace.read(cx).workspace();
            workspace.pane_ids().into_iter().any(|pane_id| {
                workspace.pane(pane_id).is_some_and(|state| {
                    state
                        .tabs()
                        .iter()
                        .any(|tab| *tab.document() == document_id)
                })
            })
        });
        if self.source_document.document_id().ok() == Some(document_id) || is_live_pane_document {
            let pinned = self
                .tabs
                .records
                .get(self.tabs.active)
                .is_some_and(|record| record.pinned);
            let (selection, scroll) = if let Some(host) = self.document_host.as_ref() {
                host.read(cx).workspace_source_state()
            } else {
                (
                    self.last_selection_snapshot.source_selection(),
                    self.scroll_handle.offset(),
                )
            };
            let mut state = Self::workspace_view_state(
                self.view_mode,
                selection,
                scroll,
                Some(self.split_pane_ratio),
            );
            state.split_ratio = state
                .split_ratio
                .filter(|ratio| ratio.is_finite())
                .map(|ratio| ratio.clamp(0.1, 0.9));
            return (pinned, self.recovered_session, state);
        }

        for record in &self.tabs.records {
            let Some(snapshot) = record.snapshot.as_ref() else {
                continue;
            };
            if snapshot.source_document.document_id().ok() != Some(document_id) {
                continue;
            }
            let (selection, scroll) = if let Some(host) = snapshot.document_host.as_ref() {
                host.read(cx).workspace_source_state()
            } else {
                (
                    snapshot.selection.source_selection(),
                    snapshot.scroll_offset,
                )
            };
            return (
                record.pinned,
                snapshot.recovered_session,
                Self::workspace_view_state(snapshot.view_mode, selection, scroll, None),
            );
        }

        (false, false, Default::default())
    }

    fn workspace_view_state(
        view_mode: ViewMode,
        selection: gmark_document_core::SourceSelection,
        scroll: Point<Pixels>,
        split_ratio: Option<f32>,
    ) -> crate::config::workspace_session::WorkspaceSessionPaneViewState {
        crate::config::workspace_session::WorkspaceSessionPaneViewState {
            selection: Some(
                crate::config::workspace_session::WorkspaceSessionSelection::from_source_selection(
                    selection,
                ),
            ),
            scroll_x: Some(f32::from(scroll.x)),
            scroll_y: Some(f32::from(scroll.y)),
            view_mode: Some(Self::session_view_mode(view_mode).to_owned()),
            split_ratio,
            ..Default::default()
        }
    }

    pub(crate) fn apply_workspace_session_window_state(
        &self,
        session: &mut crate::config::workspace_session::WorkspaceSession,
    ) {
        session.window = self.tabs.window.clone();
        session.workspace_panel_width = self.workspace_panel_width();
        session.workspace_docked_open = Some(self.workspace_docked_open_preference());
        session.document_sidebar_width = self.document_sidebar_panel_width();
        session.document_sidebar_docked_open = Some(self.document_sidebar_docked_open_preference());
        session.split_pane_ratio = Some(self.split_pane_ratio.clamp(0.3, 0.7));
    }
}
