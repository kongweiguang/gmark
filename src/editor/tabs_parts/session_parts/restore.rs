// @author kongweiguang

use super::*;

impl Editor {
    pub(crate) fn apply_workspace_session_view_state(
        &mut self,
        state: &crate::config::workspace_session::WorkspaceSessionPaneViewState,
        cx: &mut Context<Self>,
    ) {
        let mode = if self.source_encoding.is_utf8() {
            Self::restored_view_mode(state.view_mode.as_deref())
        } else {
            ViewMode::Preview
        };
        if mode != ViewMode::Rendered {
            self.set_view_mode(mode, cx);
        }
        let selection =
            Self::restored_selection(&self.source_document.text(), state.selection.as_ref());
        self.apply_selection_snapshot_in_current_mode(&selection, cx);
        self.last_selection_snapshot = selection;
        self.scroll_handle.set_offset(point(
            px(state.scroll_x.unwrap_or_default()),
            px(state.scroll_y.unwrap_or_default()),
        ));
    }

    /// Rebuild the host-owned presentation DTO from the neutral persisted pane
    /// state.  The conversion intentionally carries metadata only; the host
    /// constructor receives the service lease and remains the sole body owner.
    pub(crate) fn host_presentation_from_workspace_state(
        state: &crate::config::workspace_session::WorkspaceSessionPaneViewState,
    ) -> crate::document_host::DocumentHostViewPresentation {
        let current = Self::host_view_presentation_state(state);
        let back = state
            .back
            .iter()
            .filter_map(Self::host_history_presentation_state)
            .collect::<Vec<_>>();
        let forward = state
            .forward
            .iter()
            .filter_map(Self::host_history_presentation_state)
            .collect::<Vec<_>>();
        crate::document_host::DocumentHostViewPresentation::bounded(current, back, forward)
    }

    fn host_history_presentation_state(
        value: &serde_json::Value,
    ) -> Option<crate::document_host::ViewPresentationState> {
        let object = value.as_object()?;
        let state = crate::config::workspace_session::WorkspaceSessionPaneViewState {
            selection: object
                .get("selection")
                .and_then(|value| serde_json::from_value(value.clone()).ok()),
            scroll_x: object
                .get("scroll_x")
                .and_then(serde_json::Value::as_f64)
                .map(|v| v as f32),
            scroll_y: object
                .get("scroll_y")
                .and_then(serde_json::Value::as_f64)
                .map(|v| v as f32),
            view_mode: object
                .get("view_mode")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            split_ratio: object
                .get("split_ratio")
                .and_then(serde_json::Value::as_f64)
                .map(|v| v as f32),
            markdown_fold: None,
            markdown_folds: object
                .get("markdown_folds")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default(),
            table_layout: object.get("table_layout").cloned(),
            forward: Vec::new(),
            back: Vec::new(),
        };
        Some(Self::host_view_presentation_state(&state))
    }

    fn host_view_presentation_state(
        state: &crate::config::workspace_session::WorkspaceSessionPaneViewState,
    ) -> crate::document_host::ViewPresentationState {
        let mut presentation = crate::document_host::ViewPresentationState {
            view_mode: match state.view_mode.as_deref() {
                Some("live") => crate::document_host::DocumentHostViewMode::Live,
                Some("preview") => crate::document_host::DocumentHostViewMode::Structure,
                Some("split") => crate::document_host::DocumentHostViewMode::Split,
                _ => crate::document_host::DocumentHostViewMode::Source,
            },
            ..Default::default()
        };
        presentation.source_scroll_y = state.scroll_y.unwrap_or_default();
        presentation.structured_scroll_y = state.scroll_y.unwrap_or_default();
        presentation.structured_scroll_x = state.scroll_x.unwrap_or_default();
        presentation.json_split_ratio = state
            .split_ratio
            .filter(|ratio| ratio.is_finite())
            .map_or(0.5, |ratio| ratio.clamp(0.1, 0.9));
        if let Some(selection) = state.selection.as_ref() {
            let end = selection.end.max(selection.start);
            presentation.tab_view_state.source.selection =
                selection.source_selection_for_range(selection.start..end);
        }
        for fold in state
            .markdown_fold
            .iter()
            .chain(state.markdown_folds.iter())
        {
            let Some(object) = fold.as_object() else {
                continue;
            };
            if object
                .get("collapsed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true)
            {
                if let Some(line) = object.get("line").and_then(serde_json::Value::as_u64) {
                    presentation.source_collapsed_folds.insert(line);
                }
            }
        }
        if let Some(layout) = state
            .table_layout
            .as_ref()
            .and_then(serde_json::Value::as_object)
        {
            presentation.structured_filter_query = layout
                .get("filter_query")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            presentation.structured_filter_column = layout
                .get("filter_column")
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| usize::try_from(value).ok());
            if let Some(columns) = layout
                .get("hidden_columns")
                .and_then(serde_json::Value::as_array)
            {
                presentation.hidden_structured_columns.extend(
                    columns
                        .iter()
                        .filter_map(serde_json::Value::as_u64)
                        .filter_map(|value| usize::try_from(value).ok()),
                );
            }
            presentation.structured_column_window_start = layout
                .get("column_window_start")
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or_default();
            presentation.structured_selected_cell = layout
                .get("selected_cell")
                .and_then(serde_json::Value::as_array)
                .and_then(|values| {
                    (values.len() == 2).then(|| {
                        (
                            values[0].as_u64(),
                            values[1]
                                .as_u64()
                                .and_then(|value| usize::try_from(value).ok())
                                .unwrap_or_default(),
                        )
                    })
                });
        }
        presentation
    }

    pub(crate) fn runtime_pane_node(
        node: &crate::config::workspace_session::WorkspaceSessionPaneNode,
    ) -> anyhow::Result<crate::editor::panes::PaneNode> {
        match node {
            crate::config::workspace_session::WorkspaceSessionPaneNode::Leaf(id) => {
                Ok(crate::editor::panes::PaneNode::Leaf(
                    crate::editor::panes::PaneId::from_uuid(id.as_uuid()),
                ))
            }
            crate::config::workspace_session::WorkspaceSessionPaneNode::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let ratio = if ratio.is_finite() {
                    ratio.clamp(0.1, 0.9)
                } else {
                    return Err(anyhow::anyhow!("pane split ratio is not finite"));
                };
                let axis = match axis {
                    crate::config::workspace_session::WorkspaceSessionSplitAxis::Horizontal => {
                        crate::editor::panes::SplitAxis::Horizontal
                    }
                    crate::config::workspace_session::WorkspaceSessionSplitAxis::Vertical => {
                        crate::editor::panes::SplitAxis::Vertical
                    }
                };
                Ok(crate::editor::panes::PaneNode::Split {
                    axis,
                    ratio,
                    first: Box::new(Self::runtime_pane_node(first)?),
                    second: Box::new(Self::runtime_pane_node(second)?),
                })
            }
        }
    }

    pub(super) fn session_view_mode(mode: ViewMode) -> &'static str {
        match mode {
            ViewMode::Rendered => "live",
            ViewMode::Source => "source",
            ViewMode::Preview => "preview",
            ViewMode::Split => "split",
        }
    }

    pub(crate) fn restored_view_mode(mode: Option<&str>) -> ViewMode {
        match mode.map(str::to_ascii_lowercase).as_deref() {
            Some("source") => ViewMode::Source,
            Some("preview" | "structure") => ViewMode::Preview,
            Some("split") => ViewMode::Split,
            Some("live" | "rendered") => ViewMode::Rendered,
            _ => ViewMode::Rendered,
        }
    }

    pub(crate) fn restored_selection(
        source: &str,
        selection: Option<&crate::config::workspace_session::WorkspaceSessionSelection>,
    ) -> UndoSelectionSnapshot {
        let Some(selection) = selection else {
            return Self::empty_selection_snapshot();
        };
        let clamp = |offset: usize| {
            let mut offset = offset.min(source.len());
            while offset > 0 && !source.is_char_boundary(offset) {
                offset -= 1;
            }
            offset
        };
        let start = clamp(selection.start);
        let end = clamp(selection.end).max(start);
        UndoSelectionSnapshot::from_source_selection(
            selection.source_selection_for_range(start..end),
        )
    }

    pub(super) fn apply_restored_snapshot_state(
        snapshot: &mut DocumentTabSnapshot,
        tab: &RestoredTab,
        cx: &mut Context<Self>,
    ) {
        if snapshot.document_host.is_some() {
            snapshot.view_mode = match &tab.opened {
                crate::document_io::OpenedDocument::ResidentFormat(probe)
                    if matches!(
                        probe.format,
                        gmark_document_core::DocumentFormat::Json
                            | gmark_document_core::DocumentFormat::Delimited { .. }
                    ) =>
                {
                    let restored = Self::restored_view_mode(tab.view_mode.as_deref());
                    match (&probe.format, restored) {
                        (gmark_document_core::DocumentFormat::Json, ViewMode::Rendered) => {
                            ViewMode::Preview
                        }
                        (_, mode) => mode,
                    }
                }
                crate::document_io::OpenedDocument::ResidentFormat(_)
                | crate::document_io::OpenedDocument::Paged(_) => ViewMode::Source,
                crate::document_io::OpenedDocument::Resident(_) => snapshot.view_mode,
                crate::document_io::OpenedDocument::Image => ViewMode::Preview,
            };
            if let Some(host) = snapshot.document_host.as_ref() {
                let selection = tab
                    .selection
                    .as_ref()
                    .map(|selection| {
                        selection.source_selection_for_range(selection.start..selection.end)
                    })
                    .unwrap_or_default();
                host.update(cx, |host, cx| {
                    host.restore_workspace_source_state(
                        selection,
                        tab.scroll_y.unwrap_or_default(),
                        cx,
                    )
                });
            }
            return;
        }
        snapshot.view_mode = if snapshot.source_encoding.is_utf8() {
            Self::restored_view_mode(tab.view_mode.as_deref())
        } else {
            ViewMode::Preview
        };
        snapshot.selection =
            Self::restored_selection(&snapshot.source_document.text(), tab.selection.as_ref());
        snapshot.scroll_offset = point(
            px(tab.scroll_x.unwrap_or_default()),
            px(tab.scroll_y.unwrap_or_default()),
        );
    }

    pub(super) fn apply_restored_tab_state(&mut self, tab: &RestoredTab, cx: &mut Context<Self>) {
        let mode = if self.source_encoding.is_utf8() {
            Self::restored_view_mode(tab.view_mode.as_deref())
        } else {
            ViewMode::Preview
        };
        if mode != ViewMode::Rendered {
            self.set_view_mode(mode, cx);
        }
        let selection =
            Self::restored_selection(&self.source_document.text(), tab.selection.as_ref());
        self.apply_selection_snapshot_in_current_mode(&selection, cx);
        self.last_selection_snapshot = selection;
        self.scroll_handle.set_offset(point(
            px(tab.scroll_x.unwrap_or_default()),
            px(tab.scroll_y.unwrap_or_default()),
        ));
    }
}
