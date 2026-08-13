// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn tab_strip_height(&self) -> f32 {
        // Once the legacy shell has been promoted into a pane workspace, every
        // legacy tab has been migrated into the root leaf and each pane owns its
        // own Zed-style tab bar.
        if self.focus_mode || self.pane_workspace.is_some() {
            0.0
        } else {
            TAB_STRIP_HEIGHT
        }
    }

    pub(crate) fn push_closed_tab(
        &mut self,
        snapshot: DocumentTabSnapshot,
        cx: &mut Context<Self>,
    ) {
        let host_metadata = snapshot.document_host.as_ref().and_then(|host| {
            let host = host.read(cx);
            Some((host.document_id()?, host.probe().clone()))
        });
        if let Some(document_host) = snapshot.document_host.as_ref() {
            document_host.update(cx, |view, _cx| view.suspend_for_closed_tab());
        }
        let closed = match ClosedTabSnapshot::from_document_with_host(snapshot, host_metadata) {
            Ok(closed) => closed,
            Err(error) => {
                eprintln!("closed-tab snapshot capture failed: {error}");
                return;
            }
        };
        self.tabs.closed.push(closed);
        enforce_closed_tab_budget(
            &mut self.tabs.closed,
            CLOSED_TAB_LIMIT,
            CLOSED_TAB_RETAINED_BYTES_LIMIT,
        );
    }

    fn reopen_closed_tab_snapshot(
        &mut self,
        closed: ClosedTabSnapshot,
        cx: &mut Context<Self>,
    ) -> Result<DocumentTabSnapshot, String> {
        let loading = gmark_document_core::LoadingPolicy::default();
        let service = cx
            .try_global::<crate::app::document_service::DocumentService>()
            .cloned()
            .ok_or_else(|| "document service is not initialized".to_owned())?;
        match closed.source.clone() {
            ClosedDocumentSource::File { path, .. } => {
                let probe = service
                    .probe_file(&path, loading, |normalized, policy| {
                        crate::document_io::probe_document_with_policy(normalized, policy)
                    })
                    .map_err(|error| error.to_string())?;
                if crate::document_io::is_markdown_path(&path)
                    && probe.strategy == gmark_paged_document::OpenStrategy::Resident
                {
                    let limits = loading.effective_limits();
                    let shared = service
                        .open_resident_file(&path, loading, |normalized, _| {
                            crate::document_io::read_resident_text_from_probe(
                                normalized, &probe, limits,
                            )
                            .map(|opened| {
                                crate::app::document_service::ResidentMarkdownSource::from_opened(
                                    normalized, opened,
                                )
                            })
                        })
                        .map_err(|error| error.to_string())?;
                    let encoding = shared.encoding.clone();
                    let session = EditorDocumentSession::from_lease(shared.lease)
                        .map_err(|error| error.to_string())?;
                    let dirty = session.try_is_dirty().unwrap_or(true);
                    let mut snapshot = closed.into_document_with_source(session, None);
                    snapshot.file_path = Some(path);
                    snapshot.source_encoding = encoding;
                    snapshot.shared_document = true;
                    snapshot.document_dirty = dirty;
                    snapshot.saved_file_fingerprint = snapshot
                        .file_path
                        .as_deref()
                        .and_then(|path| crate::recovery::fingerprint_file(path).ok());
                    Ok(snapshot)
                } else {
                    let shared = service
                        .open_document_host(
                            &path,
                            probe.clone(),
                            loading,
                            |normalized, probe, _| {
                                let source = gmark_paged_document::FileSource::open(normalized)
                                    .map_err(|error| {
                                        anyhow::anyhow!(
                                            "failed to open '{}': {error}",
                                            normalized.display()
                                        )
                                    })?;
                                gmark_paged_document::prepare_utf8_source(
                                    source,
                                    probe.encoding.clone(),
                                )
                                .map_err(|error| {
                                    anyhow::anyhow!("failed to prepare source: {error}")
                                })
                            },
                        )
                        .map_err(|error| error.to_string())?;
                    let dirty = shared
                        .lease
                        .handle()
                        .lock()
                        .map(|controller| controller.session().dirty)
                        .unwrap_or(true);
                    let view_id = gmark_document_core::DocumentViewInstanceId::new();
                    let crate::app::document_service::SharedDocumentHostOpen {
                        lease,
                        probe,
                        file_path,
                        encoding,
                        ..
                    } = shared;
                    let handle = lease.handle();
                    let host_path = file_path.clone();
                    let host = cx.new(move |cx| {
                        crate::document_host::DocumentHost::from_shared_with_view_id_or_error(
                            host_path,
                            probe,
                            handle,
                            lease,
                            view_id,
                            crate::document_host::DocumentHostViewPresentation::default(),
                            cx,
                        )
                    });
                    let mut snapshot = closed
                        .into_document_with_source(EditorDocumentSession::shell(), Some(host));
                    snapshot.file_path = Some(path);
                    snapshot.source_encoding = encoding;
                    snapshot.shared_document = true;
                    snapshot.document_dirty = dirty;
                    snapshot.saved_file_fingerprint = snapshot
                        .file_path
                        .as_deref()
                        .and_then(|path| crate::recovery::fingerprint_file(path).ok());
                    Ok(snapshot)
                }
            }
            ClosedDocumentSource::Host { path, probe, .. } => {
                let shared = service
                    .open_document_host(&path, probe, loading, |normalized, probe, _| {
                        let source = gmark_paged_document::FileSource::open(normalized).map_err(
                            |error| {
                                anyhow::anyhow!(
                                    "failed to open '{}': {error}",
                                    normalized.display()
                                )
                            },
                        )?;
                        gmark_paged_document::prepare_utf8_source(source, probe.encoding.clone())
                            .map_err(|error| anyhow::anyhow!("failed to prepare source: {error}"))
                    })
                    .map_err(|error| error.to_string())?;
                let dirty = shared
                    .lease
                    .handle()
                    .lock()
                    .map(|controller| controller.session().dirty)
                    .unwrap_or(true);
                let view_id = gmark_document_core::DocumentViewInstanceId::new();
                let crate::app::document_service::SharedDocumentHostOpen {
                    lease,
                    probe,
                    file_path,
                    encoding,
                    ..
                } = shared;
                let handle = lease.handle();
                let host_path = file_path.clone();
                let host = cx.new(move |cx| {
                    crate::document_host::DocumentHost::from_shared_with_view_id_or_error(
                        host_path,
                        probe,
                        handle,
                        lease,
                        view_id,
                        crate::document_host::DocumentHostViewPresentation::default(),
                        cx,
                    )
                });
                let mut snapshot =
                    closed.into_document_with_source(EditorDocumentSession::shell(), Some(host));
                snapshot.file_path = Some(path);
                snapshot.source_encoding = encoding;
                snapshot.shared_document = true;
                snapshot.document_dirty = dirty;
                Ok(snapshot)
            }
            ClosedDocumentSource::Recovery {
                document_id,
                #[cfg(test)]
                journal_path,
            } => {
                let recovery_dir = {
                    #[cfg(test)]
                    if let Some(path) = journal_path.as_deref() {
                        path.parent().map(Path::to_path_buf)
                    } else {
                        None
                    }
                    #[cfg(not(test))]
                    {
                        None
                    }
                }
                .or_else(|| {
                    crate::config::AppDirs::from_system()
                        .ok()
                        .map(|dirs| dirs.recovery_dir())
                })
                .ok_or_else(|| "recovery directory is unavailable".to_owned())?;
                let recovered = crate::recovery::load_recovery_documents(&recovery_dir)
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .find(|document| {
                        uuid::Uuid::parse_str(&document.document_id)
                            .ok()
                            .map(gmark_document_runtime::DocumentId::from_uuid)
                            == Some(document_id)
                    })
                    .ok_or_else(|| "closed recovery journal is unavailable".to_owned())?;
                let source = crate::app::document_service::ResidentMarkdownSource::from_recovered(
                    recovered.source.clone(),
                    recovered.file_path.clone(),
                    recovered.source_format.clone(),
                )
                .map_err(|error| error.to_string())?;
                let shared = service
                    .open_recovery(document_id, source)
                    .map_err(|error| error.to_string())?;
                let session = EditorDocumentSession::from_lease(shared.lease)
                    .map_err(|error| error.to_string())?;
                let mut snapshot = closed.into_document_with_source(session, None);
                snapshot.file_path = recovered.file_path.clone();
                snapshot.source_encoding = shared.encoding;
                snapshot.shared_document = true;
                snapshot.document_dirty = true;
                snapshot.recovered_session = true;
                snapshot.recovery_journal = Some(Arc::new(Mutex::new(
                    crate::recovery::RecoveryJournal::resume(&recovered),
                )));
                Ok(snapshot)
            }
            ClosedDocumentSource::Image { path } => {
                let mut snapshot = Self::snapshot_for_image_preview(path);
                snapshot.shared_document = false;
                Ok(snapshot)
            }
            ClosedDocumentSource::Error { path, reason } => {
                let mut snapshot = Self::snapshot_for_file_open_failure(path, reason);
                snapshot.shared_document = false;
                Ok(snapshot)
            }
        }
    }

    pub(in crate::editor) fn request_close_tab_index(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        if index >= self.tabs.records.len() {
            return;
        }
        self.close_menu_bar(cx);
        let (dirty, lease_count) = if index == self.tabs.active {
            (self.is_document_dirty(), self.source_document.lease_count())
        } else {
            self.tabs.records[index]
                .snapshot
                .as_ref()
                .map(|snapshot| {
                    let dirty = if snapshot.document_host.is_some() {
                        snapshot.document_dirty
                    } else {
                        snapshot.source_document.is_dirty()
                    };
                    (dirty, snapshot.source_document.lease_count())
                })
                .unwrap_or((false, 0))
        };
        if dirty && lease_count <= 1 {
            if index != self.tabs.active && !self.switch_to_tab_index(index, cx) {
                return;
            }
            self.tabs.show_close_dialog = true;
            cx.notify();
            return;
        }
        self.close_tab_index_without_prompt(index, true, cx);
    }

    pub(super) fn close_tab_index_without_prompt(
        &mut self,
        index: usize,
        keep_for_restore: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if index >= self.tabs.records.len() {
            return false;
        }
        if self.tabs.records.len() == 1 {
            self.checkpoint_recovery_journal();
            let closed_id = self.tabs.records[0].id;
            self.release_render_assets_for_active_document(cx);
            let closed = self.capture_active_tab(cx);
            let _ = self.view_state.close_tab(closed_id);
            if keep_for_restore {
                self.push_closed_tab(closed, cx);
            }
            self.tabs.records[0] = TabRecord {
                id: uuid::Uuid::new_v4(),
                pinned: false,
                snapshot: None,
            };
            self.replace_document_from_markdown(String::new(), None, cx);
            self.schedule_workspace_session_save(cx);
            return true;
        }

        if index != self.tabs.active {
            let record = self.tabs.records.remove(index);
            let _ = self.view_state.close_tab(record.id);
            if index < self.tabs.active {
                self.tabs.active -= 1;
            }
            if keep_for_restore && let Some(snapshot) = record.snapshot {
                self.push_closed_tab(snapshot, cx);
            }
            self.schedule_workspace_session_save(cx);
            cx.notify();
            return true;
        }

        self.checkpoint_recovery_journal();
        let closed_id = self.tabs.records[index].id;
        self.release_render_assets_for_active_document(cx);
        let closed = self.capture_active_tab(cx);
        let _ = self.view_state.close_tab(closed_id);
        self.tabs.records.remove(index);
        self.tabs.active = index.min(self.tabs.records.len() - 1);
        let target = self.tabs.records[self.tabs.active]
            .snapshot
            .take()
            .expect("inactive tab must own a snapshot");
        if keep_for_restore {
            self.push_closed_tab(closed, cx);
        }
        self.install_tab_snapshot(target, cx);
        self.schedule_workspace_session_save(cx);
        true
    }

    pub(in crate::editor) fn close_tabs_affected_by_deleted_path(
        &mut self,
        target: &Path,
        cx: &mut Context<Self>,
    ) -> bool {
        let (mut indices, has_dirty) = self.workspace_tabs_affected_by_path(target);
        if has_dirty {
            return false;
        }
        indices.sort_unstable_by(|left, right| right.cmp(left));
        for index in indices {
            self.close_tab_index_without_prompt(index, false, cx);
        }
        true
    }

    pub(crate) fn on_close_tab_action(
        &mut self,
        _: &crate::components::CloseTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_close_tab_index(self.tabs.active, cx);
    }

    pub(crate) fn on_new_tab_action(
        &mut self,
        _: &crate::components::NewTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_untitled_tab(cx);
    }

    pub(crate) fn on_reopen_closed_tab_action(
        &mut self,
        _: &crate::components::ReopenClosedTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(closed) = self.tabs.closed.pop() else {
            return;
        };
        let snapshot = match self.reopen_closed_tab_snapshot(closed, cx) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                eprintln!("closed-tab restore failed: {error}");
                return;
            }
        };
        if !self.can_switch_tabs() {
            if let Ok(closed) = ClosedTabSnapshot::from_document(snapshot) {
                self.tabs.closed.push(closed);
            }
            return;
        }
        let current = self.capture_active_tab(cx);
        self.tabs.records[self.tabs.active].snapshot = Some(current);
        self.tabs.records.push(TabRecord {
            id: uuid::Uuid::new_v4(),
            pinned: false,
            snapshot: None,
        });
        self.tabs.active = self.tabs.records.len() - 1;
        self.install_tab_snapshot(snapshot, cx);
        self.schedule_workspace_session_save(cx);
    }

    pub(crate) fn on_previous_tab_action(
        &mut self,
        _: &crate::components::PreviousTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.records.len() < 2 {
            return;
        }
        let target = if self.tabs.active == 0 {
            self.tabs.records.len() - 1
        } else {
            self.tabs.active - 1
        };
        self.dismiss_contextual_overlays(cx);
        self.switch_to_tab_index(target, cx);
    }

    pub(crate) fn on_next_tab_action(
        &mut self,
        _: &crate::components::NextTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.records.len() < 2 {
            return;
        }
        let target = (self.tabs.active + 1) % self.tabs.records.len();
        self.dismiss_contextual_overlays(cx);
        self.switch_to_tab_index(target, cx);
    }

    pub(in crate::editor) fn on_cancel_tab_close(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tabs.show_close_dialog = false;
        self.tabs.close_after_save = false;
        self.tabs.close_others_keep = None;
        self.pane_close_target = None;
        self.pane_close_save_signal = None;
        cx.stop_propagation();
        cx.notify();
    }

    pub(in crate::editor) fn on_discard_tab_close(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tabs.show_close_dialog = false;
        self.tabs.close_after_save = false;
        if let Some((pane, tab)) = self.pane_close_target.take() {
            let Some(workspace) = self.pane_workspace.clone() else {
                self.pane_close_target = Some((pane, tab));
                self.tabs.show_close_dialog = true;
                cx.stop_propagation();
                cx.notify();
                return;
            };
            // Keep the target canvas mounted while discarding.  This preserves
            // the exact input surface on failure and lets the child Editor or
            // DocumentHost clear its recovery journal before the tab is
            // detached and dropped.
            let mounted_markdown = self.pane_canvas_entities.borrow().get(&pane).and_then(
                |(_, _, canvas)| match canvas {
                    crate::editor::panes::PaneCanvasEntity::Markdown(entity) => {
                        Some(entity.clone())
                    }
                    crate::editor::panes::PaneCanvasEntity::DocumentHost(_)
                    | crate::editor::panes::PaneCanvasEntity::ReadOnly(_) => None,
                },
            );
            let mounted_host = self.pane_canvas_entities.borrow().get(&pane).and_then(
                |(_, _, canvas)| match canvas {
                    crate::editor::panes::PaneCanvasEntity::DocumentHost(entity) => {
                        Some(entity.clone())
                    }
                    crate::editor::panes::PaneCanvasEntity::Markdown(_)
                    | crate::editor::panes::PaneCanvasEntity::ReadOnly(_) => None,
                },
            );
            let discarded = if let Some(canvas) = mounted_markdown {
                let child = canvas.read(cx).editor();
                let mut discarded = false;
                child.update(cx, |editor, _cx| {
                    let Ok(handle) = editor.source_document.handle() else {
                        return;
                    };
                    match handle.discard_current_changes_for_owned_leases(1) {
                        Ok(_) => {
                            editor.document_dirty = false;
                            discarded = true;
                        }
                        Err(error) => {
                            eprintln!("failed to discard pane markdown changes: {error}");
                        }
                    }
                });
                if discarded {
                    canvas.update(cx, |canvas, cx| canvas.checkpoint_discarded_recovery(cx));
                }
                discarded
            } else if let Some(canvas) = mounted_host {
                let host = canvas.read(cx).host();
                let mut discarded = false;
                host.update(cx, |host, cx| {
                    discarded = host.discard_unsaved_changes_for_owned_leases(1, cx);
                });
                discarded
            } else {
                workspace
                    .read(cx)
                    .workspace()
                    .tab(pane, tab)
                    .and_then(|tab| match tab.view().kind() {
                        crate::editor::panes::PaneDocumentKind::Markdown => tab
                            .view()
                            .lease()
                            .and_then(|lease| lease.handle().discard_current_changes().ok()),
                        crate::editor::panes::PaneDocumentKind::DocumentHost => tab
                            .view()
                            .host_handle()
                            .and_then(|handle| handle.discard_current_changes().ok()),
                        crate::editor::panes::PaneDocumentKind::Image
                        | crate::editor::panes::PaneDocumentKind::Error => Some(true),
                    })
                    .unwrap_or(false)
            };
            if discarded {
                self.pane_close_target = Some((pane, tab));
                if !self.close_pane_tab_now(&workspace, pane, tab, cx) {
                    self.tabs.show_close_dialog = true;
                    cx.notify();
                }
            } else {
                self.pane_close_target = Some((pane, tab));
                self.tabs.show_close_dialog = true;
                cx.stop_propagation();
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        if self.discard_current_document_changes(cx)
            && self.close_tab_index_without_prompt(self.tabs.active, false, cx)
        {
            self.advance_close_other_tabs(cx);
        }
        cx.stop_propagation();
    }

    pub(in crate::editor) fn on_save_tab_close(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tabs.show_close_dialog = false;
        if let Some((pane, tab)) = self.pane_close_target {
            self.start_pane_tab_save(pane, tab, window, cx);
            cx.stop_propagation();
            return;
        }
        self.tabs.close_after_save = true;
        self.tabs.continue_window_close_after_save = false;
        self.save_document(window, cx);
        cx.stop_propagation();
    }

    pub(in crate::editor) fn finish_pending_tab_close_after_save(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.pane_close_save_signal.is_some() && !self.is_document_dirty() {
            if let Some(signal) = self.pane_close_save_signal.as_ref() {
                signal.store(1, std::sync::atomic::Ordering::Release);
            }
            return;
        }
        if self.tabs.close_after_save && !self.is_document_dirty() {
            self.tabs.close_after_save = false;
            if self.close_tab_index_without_prompt(self.tabs.active, true, cx) {
                self.advance_close_other_tabs(cx);
            }
        }
    }

    pub(in crate::editor) fn abort_pending_tab_close_after_save(&mut self, cx: &mut Context<Self>) {
        if let Some(signal) = self.pane_close_save_signal.as_ref() {
            signal.store(2, std::sync::atomic::Ordering::Release);
        }
        if self.tabs.close_after_save {
            self.tabs.close_after_save = false;
            self.tabs.close_others_keep = None;
            cx.notify();
        }
    }
}
