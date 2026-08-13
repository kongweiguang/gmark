// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn install_workspace_session_window_observer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.window_bounds_subscription.is_some() {
            return;
        }
        self.capture_workspace_session_window(window, cx);
        self.tabs.window_bounds_subscription =
            Some(cx.observe_window_bounds(window, |editor, window, cx| {
                if editor.capture_workspace_session_window(window, cx) {
                    editor.schedule_workspace_session_save(cx);
                }
            }));
    }

    pub(super) fn capture_workspace_session_window(
        &mut self,
        window: &Window,
        cx: &Context<Self>,
    ) -> bool {
        let bounds = window.window_bounds();
        let (state, bounds) = match bounds {
            WindowBounds::Windowed(bounds) => (
                crate::config::workspace_session::WorkspaceSessionWindowState::Windowed,
                bounds,
            ),
            WindowBounds::Maximized(bounds) => (
                crate::config::workspace_session::WorkspaceSessionWindowState::Maximized,
                bounds,
            ),
            WindowBounds::Fullscreen(bounds) => (
                crate::config::workspace_session::WorkspaceSessionWindowState::Fullscreen,
                bounds,
            ),
        };
        let captured = crate::config::workspace_session::WorkspaceSessionWindow {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
            state,
            display_uuid: window.display(cx).and_then(|display| display.uuid().ok()),
        };
        if self.tabs.window.as_ref() == Some(&captured) {
            return false;
        }
        self.tabs.window = Some(captured);
        true
    }

    pub(in crate::editor) fn schedule_workspace_session_save(&mut self, cx: &mut Context<Self>) {
        #[cfg(test)]
        {
            let _ = cx;
        }
        #[cfg(not(test))]
        {
            let generation = self.tabs.session_generation.wrapping_add(1);
            self.tabs.session_generation = generation;
            let session = match self.workspace_session_snapshot_result(cx) {
                Ok(session) => session,
                Err(error) => {
                    eprintln!("failed to capture workspace session: {error}");
                    return;
                }
            };
            SESSION_WRITE_GENERATIONS
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .expect("workspace session generation lock poisoned")
                .insert(session.id, generation);
            self.tabs.session_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let result = cx
                    .background_spawn(async move {
                        // 原子 rename 只保证单次写完整；串行锁与持锁后的 generation 校验
                        // 共同阻止较旧窗口任务在新状态之后完成并覆盖磁盘。
                        let _guard = SESSION_WRITE_LOCK.lock().map_err(|_| {
                            anyhow::anyhow!("workspace session write lock poisoned")
                        })?;
                        let is_current = SESSION_WRITE_GENERATIONS
                            .get_or_init(|| Mutex::new(HashMap::new()))
                            .lock()
                            .map_err(|_| {
                                anyhow::anyhow!("workspace session generation lock poisoned")
                            })?
                            .get(&session.id)
                            .copied()
                            == Some(generation);
                        if !is_current {
                            return Ok(());
                        }
                        crate::config::workspace_session::upsert_workspace_session(&session)
                    })
                    .await;
                let _ = this.update(cx, |editor, _cx| {
                    if editor.tabs.session_generation == generation {
                        editor.tabs.session_task = None;
                    }
                    if let Err(error) = result {
                        eprintln!("failed to persist workspace session: {error}");
                    }
                });
            }));
        }
    }

    pub(in crate::editor) fn sync_workspace_session_view_state(&mut self, cx: &mut Context<Self>) {
        let (selection, scroll) = self.document_host.as_ref().map_or_else(
            || {
                (
                    self.last_selection_snapshot.source_selection(),
                    self.scroll_handle.offset(),
                )
            },
            |host| host.read(cx).workspace_source_state(),
        );
        let range = selection.range();
        let signature = SessionViewSignature {
            tab_id: self.tabs.records[self.tabs.active].id,
            mode: match self.view_mode {
                ViewMode::Rendered => 0,
                ViewMode::Source => 1,
                ViewMode::Preview => 2,
                ViewMode::Split => 3,
            },
            selection_start: usize::try_from(range.start).unwrap_or(usize::MAX),
            selection_end: usize::try_from(range.end).unwrap_or(usize::MAX),
            selection_reversed: selection.reversed(),
            scroll_x_bits: f32::from(scroll.x).to_bits(),
            scroll_y_bits: f32::from(scroll.y).to_bits(),
        };
        if self.tabs.last_session_view_signature != Some(signature) {
            self.tabs.last_session_view_signature = Some(signature);
            self.schedule_workspace_session_save(cx);
        }
    }

    pub(in crate::editor) fn persist_workspace_session_before_quit(&self, cx: &App) {
        #[cfg(test)]
        let _ = cx;
        #[cfg(not(test))]
        {
            let session = match self.workspace_session_snapshot_result(cx) {
                Ok(session) => session,
                Err(error) => {
                    eprintln!("failed to capture workspace session before quit: {error}");
                    return;
                }
            };
            let result = SESSION_WRITE_LOCK
                .lock()
                .map_err(|_| anyhow::anyhow!("workspace session write lock poisoned"))
                .and_then(|_guard| {
                    crate::config::workspace_session::upsert_workspace_session(&session)
                });
            if let Err(error) = result {
                eprintln!("failed to flush workspace session before quit: {error}");
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn restore_tab_session(
        &mut self,
        session_id: uuid::Uuid,
        restored: Vec<RestoredTab>,
        active_index: usize,
        workspace_root: Option<PathBuf>,
        workspace_panel_width: Option<f32>,
        workspace_docked_open: Option<bool>,
        split_pane_ratio: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        self.restore_tab_session_with_sidebars(
            session_id,
            restored,
            active_index,
            workspace_root,
            workspace_panel_width,
            workspace_docked_open,
            None,
            None,
            split_pane_ratio,
            cx,
        );
    }

    pub(crate) fn restore_tab_session_with_sidebars(
        &mut self,
        session_id: uuid::Uuid,
        restored: Vec<RestoredTab>,
        active_index: usize,
        workspace_root: Option<PathBuf>,
        workspace_panel_width: Option<f32>,
        workspace_docked_open: Option<bool>,
        document_sidebar_width: Option<f32>,
        document_sidebar_docked_open: Option<bool>,
        split_pane_ratio: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        let Some(first) = restored.first() else {
            return;
        };
        self.tabs.session_id = session_id;
        self.tabs.records[0].pinned = first.pinned;
        match &first.opened {
            crate::document_io::OpenedDocument::Resident(_) => {
                self.apply_restored_tab_state(first, cx);
            }
            crate::document_io::OpenedDocument::ResidentFormat(probe)
                if matches!(
                    probe.format,
                    gmark_document_core::DocumentFormat::Json
                        | gmark_document_core::DocumentFormat::Delimited { .. }
                ) =>
            {
                let restored = Self::restored_view_mode(first.view_mode.as_deref());
                let mode = match (&probe.format, restored) {
                    (gmark_document_core::DocumentFormat::Json, ViewMode::Rendered) => {
                        ViewMode::Preview
                    }
                    (_, mode) => mode,
                };
                self.set_view_mode(mode, cx);
            }
            crate::document_io::OpenedDocument::ResidentFormat(_)
            | crate::document_io::OpenedDocument::Paged(_) => {
                self.set_view_mode(ViewMode::Source, cx);
            }
            crate::document_io::OpenedDocument::Image => {
                self.set_view_mode(ViewMode::Preview, cx);
            }
        }
        if let Some(host) = self.document_host.clone() {
            let selection = first
                .selection
                .as_ref()
                .map(|selection| {
                    selection.source_selection_for_range(selection.start..selection.end)
                })
                .unwrap_or_default();
            host.update(cx, |host, cx| {
                host.restore_workspace_source_state(
                    selection,
                    first.scroll_y.unwrap_or_default(),
                    cx,
                )
            });
        }
        for tab in restored.into_iter().skip(1) {
            let Some(mut snapshot) = Self::snapshot_for_restored_document(&tab, cx) else {
                continue;
            };
            Self::apply_restored_snapshot_state(&mut snapshot, &tab, cx);
            self.tabs.records.push(TabRecord {
                id: uuid::Uuid::new_v4(),
                pinned: tab.pinned,
                snapshot: Some(snapshot),
            });
        }
        if let Some(root) = workspace_root {
            self.restore_explicit_workspace_root(root, cx);
        }
        self.restore_workspace_panel_width(workspace_panel_width);
        self.restore_workspace_docked_open_preference(workspace_docked_open);
        self.restore_document_sidebar_panel_width(document_sidebar_width);
        self.restore_document_sidebar_docked_open_preference(document_sidebar_docked_open);
        self.split_pane_ratio = split_pane_ratio
            .filter(|ratio| ratio.is_finite())
            .map_or(0.5, |ratio| ratio.clamp(0.3, 0.7));
        self.split_resize_session = None;
        let target = active_index.min(self.tabs.records.len().saturating_sub(1));
        self.switch_to_tab_index(target, cx);
        self.schedule_workspace_session_save(cx);
        cx.notify();
    }

    /// Restore a canonical v10 pane tree after the window shell has been
    /// initialized.  The legacy tab strip is populated first for the shell's
    /// close/save lifecycle; the pane model then becomes the authoritative
    /// document/view inventory.  Every pane tab uses the persisted view UUID
    /// for both `TabId` and `PaneDocumentRef::view_id` so a subsequent capture
    /// cannot silently write a different identity.
    pub(crate) fn restore_canonical_workspace_session(
        &mut self,
        session: crate::config::workspace_session::WorkspaceSession,
        restored: Vec<(
            crate::config::workspace_session::WorkspaceSessionTab,
            Option<crate::app::app_menu::WorkspaceSessionRestoredOpen>,
        )>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        if restored.is_empty() {
            return Err(anyhow::anyhow!("workspace session has no restorable tabs"));
        }
        let first_tab = restored
            .first()
            .map(|(tab, _)| tab)
            .ok_or_else(|| anyhow::anyhow!("workspace session has no first tab"))?;
        self.tabs.session_id = session.id;
        let first_record = self
            .tabs
            .records
            .first_mut()
            .ok_or_else(|| anyhow::anyhow!("editor has no shell tab for workspace restore"))?;
        first_record.pinned = first_tab.pinned;
        self.apply_workspace_session_view_state(&first_tab.state, cx);
        if let Some(root) = session.workspace_root.clone() {
            self.restore_explicit_workspace_root(root, cx);
        }
        self.restore_workspace_panel_width(session.workspace_panel_width);
        self.restore_workspace_docked_open_preference(session.workspace_docked_open);
        self.restore_document_sidebar_panel_width(session.document_sidebar_width);
        self.restore_document_sidebar_docked_open_preference(session.document_sidebar_docked_open);
        self.split_pane_ratio = session
            .split_pane_ratio
            .filter(|ratio| ratio.is_finite())
            .map_or(0.5, |ratio| ratio.clamp(0.3, 0.7));
        self.split_resize_session = None;

        let first_restored_id = first_tab.id;
        let mut restored_by_id = restored
            .into_iter()
            .map(|(tab, shared)| (tab.id, (tab, shared)))
            .collect::<std::collections::HashMap<_, _>>();
        let mut source_document = Some(std::mem::replace(
            &mut self.source_document,
            EditorDocumentSession::shell(),
        ));
        let mut pane_states = std::collections::BTreeMap::new();

        for (pane_id, pane) in &session.panes {
            let mut tabs = Vec::with_capacity(pane.tabs.len());
            for tab in &pane.tabs {
                let (_restored_tab, shared) = restored_by_id
                    .remove(&tab.id)
                    .ok_or_else(|| anyhow::anyhow!("pane tab {} was not opened", tab.id))?;
                let view_id = gmark_document_core::DocumentViewInstanceId::from_uuid(tab.id);
                let readonly = match shared.as_ref() {
                    Some(crate::app::app_menu::WorkspaceSessionRestoredOpen::Image { path }) => {
                        Some((path.clone(), None))
                    }
                    Some(crate::app::app_menu::WorkspaceSessionRestoredOpen::Error {
                        path,
                        message,
                    }) => Some((path.clone(), Some(message.clone()))),
                    _ => None,
                };
                let (document_id, document) = if let Some((path, message)) = readonly {
                    // Images and failed opens deliberately remain in the
                    // canonical pane tree as lease-free read-only tabs. A
                    // malformed session still fails above before any model
                    // is installed, while an individual I/O failure is
                    // represented by the error canvas instead of dropping
                    // the persisted tab.
                    let document_id = gmark_document_runtime::DocumentId::new();
                    let title = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .filter(|title| !title.is_empty())
                        .unwrap_or_else(|| "document".to_owned());
                    let document = if let Some(message) = message {
                        crate::editor::panes::PaneDocumentRef::from_error(
                            document_id,
                            path,
                            message,
                            view_id,
                            title,
                        )
                    } else {
                        crate::editor::panes::PaneDocumentRef::from_image(
                            document_id,
                            path,
                            view_id,
                            title,
                        )
                    };
                    document.set_view_state_snapshot(tab.state.clone());
                    if first_restored_id == tab.id {
                        // The temporary shell editor has an empty source
                        // lease when the first persisted tab is readonly;
                        // drop it so the canonical pane tree owns the only
                        // restored tab and the source-consumed invariant
                        // remains valid.
                        source_document.take();
                    }
                    (document_id, document)
                } else if first_restored_id == tab.id && self.document_host.is_none() {
                    let source = source_document.take().ok_or_else(|| {
                        anyhow::anyhow!("workspace source view was consumed twice")
                    })?;
                    // The first Editor view is constructed before the canonical
                    // pane tree is installed. Rebind that already-open lease to
                    // the persisted view identity so capture cannot emit a new
                    // random UUID after restore. Dropping the provisional view
                    // unregisters it from the same Controller.
                    let document_session = if source.view_id() == view_id {
                        source
                    } else {
                        let handle = source.handle().map_err(|error| {
                            anyhow::anyhow!("source view has no handle: {error}")
                        })?;
                        let exact =
                            EditorDocumentSession::from_handle_with_view_id(handle, view_id)
                                .map_err(|error| {
                                    anyhow::anyhow!(
                                        "failed to restore persisted source view: {error}"
                                    )
                                })?;
                        drop(source);
                        exact
                    };
                    let document_id = document_session.document_id().map_err(|error| {
                        anyhow::anyhow!("failed to read pane document id: {error}")
                    })?;
                    let lease = document_session.lease_arc().ok_or_else(|| {
                        anyhow::anyhow!("restored source view has no document lease")
                    })?;
                    let path = tab.document.file_path().map(Path::to_path_buf);
                    let title = path
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Untitled.md".to_owned());
                    let document =
                        crate::editor::panes::PaneDocumentRef::from_lease_arc_with_title_and_path(
                            document_id,
                            lease,
                            view_id,
                            title,
                            path,
                        );
                    document.set_view_state_snapshot(tab.state.clone());
                    (document_id, document)
                } else if first_restored_id == tab.id {
                    let entity = self
                        .document_host
                        .take()
                        .ok_or_else(|| anyhow::anyhow!("workspace host view was consumed twice"))?;
                    let detached = entity
                        .update(cx, |host, host_cx| host.detach_view(host_cx))
                        .ok_or_else(|| {
                            anyhow::anyhow!("workspace host view could not be detached")
                        })?;
                    let document_id = detached
                        .handle()
                        .lock()
                        .map_err(|error| {
                            anyhow::anyhow!("failed to read host document id: {error}")
                        })?
                        .document_id();
                    let path = self
                        .pane_host_path
                        .take()
                        .or_else(|| tab.document.file_path().map(Path::to_path_buf))
                        .ok_or_else(|| anyhow::anyhow!("workspace host tab has no file path"))?;
                    let probe = self
                        .pane_host_probe
                        .take()
                        .ok_or_else(|| anyhow::anyhow!("workspace host tab has no probe"))?;
                    let title = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "document".to_owned());
                    let document = crate::editor::panes::PaneDocumentRef::from_detached_host(
                        document_id,
                        detached,
                        path,
                        probe,
                        title,
                    );
                    document.set_view_state_snapshot(tab.state.clone());
                    source_document.take();
                    (document_id, document)
                } else {
                    let shared = shared.ok_or_else(|| {
                        anyhow::anyhow!("pane tab {} is missing a shared document lease", tab.id)
                    })?;
                    match shared {
                        crate::app::app_menu::WorkspaceSessionRestoredOpen::Resident(shared) => {
                            let document_session = EditorDocumentSession::from_lease_with_view_id(
                                shared.lease,
                                view_id,
                            )
                            .map_err(|error| {
                                anyhow::anyhow!("failed to create pane document session: {error}")
                            })?;
                            let document_id = document_session.document_id().map_err(|error| {
                                anyhow::anyhow!("failed to read pane document id: {error}")
                            })?;
                            let lease = document_session.lease_arc().ok_or_else(|| {
                                anyhow::anyhow!("shared source view has no document lease")
                            })?;
                            let path = tab.document.file_path().map(Path::to_path_buf);
                            let title = path
                                .as_ref()
                                .and_then(|path| path.file_name())
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "Untitled.md".to_owned());
                            let document = crate::editor::panes::PaneDocumentRef::from_lease_arc_with_title_and_path(
                                document_id,
                                lease,
                                view_id,
                                title,
                                path,
                            );
                            document.set_view_state_snapshot(tab.state.clone());
                            (document_id, document)
                        }
                        crate::app::app_menu::WorkspaceSessionRestoredOpen::Host(shared) => {
                            let path = tab
                                .document
                                .file_path()
                                .ok_or_else(|| anyhow::anyhow!("host tab has no file path"))?
                                .to_path_buf();
                            let probe = shared.probe.clone();
                            let document_id = shared.document_id;
                            let presentation =
                                Self::host_presentation_from_workspace_state(&tab.state);
                            let handle = shared.lease.handle();
                            if view_id.uuid().is_nil() {
                                return Err(anyhow::anyhow!(
                                    "host tab {} has a nil persisted view id",
                                    tab.id
                                ));
                            }
                            if handle
                                .lock()
                                .map_err(|error| {
                                    anyhow::anyhow!("failed to inspect host view registry: {error}")
                                })?
                                .view_selection(view_id)
                                .is_some()
                            {
                                return Err(anyhow::anyhow!(
                                    "host tab {} persisted view id is already registered",
                                    tab.id
                                ));
                            }
                            let detached =
                                crate::document_host::DetachedDocumentHostView::from_shared_with_view_id(
                                   handle,
                                    shared.lease,
                                    view_id,
                                    presentation,
                                )
                                .map_err(|error| {
                                    anyhow::anyhow!(
                                        "restored host view could not be prepared: {error}"
                                    )
                                })?;
                            let document =
                                crate::editor::panes::PaneDocumentRef::from_detached_host(
                                    document_id,
                                    detached,
                                    path,
                                    probe,
                                    tab.document
                                        .file_path()
                                        .and_then(|path| path.file_name())
                                        .map(|name| name.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| "document".to_owned()),
                                );
                            document.set_view_state_snapshot(tab.state.clone());
                            (document_id, document)
                        }
                        crate::app::app_menu::WorkspaceSessionRestoredOpen::Image { path } => {
                            let document_id = gmark_document_runtime::DocumentId::new();
                            let title = path
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .filter(|title| !title.is_empty())
                                .unwrap_or_else(|| "document".to_owned());
                            let document = crate::editor::panes::PaneDocumentRef::from_image(
                                document_id,
                                path,
                                view_id,
                                title,
                            );
                            document.set_view_state_snapshot(tab.state.clone());
                            (document_id, document)
                        }
                        crate::app::app_menu::WorkspaceSessionRestoredOpen::Error {
                            path,
                            message,
                        } => {
                            let document_id = gmark_document_runtime::DocumentId::new();
                            let title = path
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .filter(|title| !title.is_empty())
                                .unwrap_or_else(|| "document".to_owned());
                            let document = crate::editor::panes::PaneDocumentRef::from_error(
                                document_id,
                                path,
                                message,
                                view_id,
                                title,
                            );
                            document.set_view_state_snapshot(tab.state.clone());
                            (document_id, document)
                        }
                    }
                };
                let runtime_tab_id = crate::editor::panes::TabId::from_uuid(tab.id);
                tabs.push(crate::editor::panes::TabView::with_pinned(
                    runtime_tab_id,
                    document_id,
                    document,
                    tab.pinned,
                ));
            }
            let mut state = crate::editor::panes::PaneState::with_tabs(tabs);
            if let Some(active_tab) = pane.active_tab {
                state
                    .set_active_tab(crate::editor::panes::TabId::from_uuid(active_tab))
                    .map_err(|error| anyhow::anyhow!("invalid active pane tab: {error}"))?;
            }
            pane_states.insert(
                crate::editor::panes::PaneId::from_uuid(pane_id.as_uuid()),
                state,
            );
        }
        if source_document.is_some() {
            return Err(anyhow::anyhow!(
                "workspace session root does not contain its first opened tab"
            ));
        }
        if !restored_by_id.is_empty() {
            return Err(anyhow::anyhow!(
                "workspace session contains tabs outside its pane tree"
            ));
        }
        let root = Self::runtime_pane_node(&session.root)?;
        let focused = crate::editor::panes::PaneId::from_uuid(session.focused_pane.as_uuid());
        let workspace = crate::editor::panes::PaneWorkspace::from_parts(root, pane_states, focused)
            .map_err(|error| anyhow::anyhow!("invalid workspace pane tree: {error}"))?;
        let editor = cx.entity().downgrade();
        let controller =
            crate::editor::panes::PaneWorkspaceController::new(move |event, window, cx| {
                let _ = editor.update(cx, |editor, cx| {
                    editor.handle_pane_event(event, Some(window), cx)
                });
            });
        self.pane_workspace =
            Some(cx.new(|_| crate::editor::panes::PaneWorkspaceView::new(workspace, controller)));
        self.file_watch_task = None;
        self.file_watch_guard = None;
        self.shared_event_task = None;
        cx.notify();
        Ok(())
    }
}
