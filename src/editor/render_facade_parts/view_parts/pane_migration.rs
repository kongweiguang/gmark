// @author kongweiguang

// 把窗格迁移与工作区准备隔离，保持渲染 facade 只负责组装入口。

use super::*;

impl Editor {
    pub(super) fn pane_tab_from_legacy_snapshot(
        snapshot: &mut DocumentTabSnapshot,
        pinned: bool,
        cx: &mut Context<Self>,
    ) -> Option<
        crate::editor::panes::TabView<
            gmark_document_runtime::DocumentId,
            crate::editor::panes::PaneDocumentRef,
        >,
    > {
        let mut view_state = crate::editor::panes::PaneViewStateSnapshot {
            selection: Some(
                crate::config::workspace_session::WorkspaceSessionSelection::from_source_selection(
                    snapshot.selection.source_selection(),
                ),
            ),
            scroll_x: Some(f32::from(snapshot.scroll_offset.x)),
            scroll_y: Some(f32::from(snapshot.scroll_offset.y)),
            view_mode: Some(
                match snapshot.view_mode {
                    ViewMode::Rendered => "live",
                    ViewMode::Source => "source",
                    ViewMode::Preview => "preview",
                    ViewMode::Split => "split",
                }
                .to_owned(),
            ),
            ..Default::default()
        };

        let (tab_id, document_id, document) =
            if let Some(path) = snapshot.image_preview_path.clone() {
                let document_id = snapshot.source_document.document_id().ok()?;
                let view_id = snapshot.source_document.view_id();
                let tab_id = crate::editor::panes::TabId::from_uuid(view_id.uuid());
                let title = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Image".to_owned());
                let document = crate::editor::panes::PaneDocumentRef::from_image(
                    document_id,
                    path,
                    view_id,
                    title,
                );
                (tab_id, document_id, document)
            } else if let Some(failure) = snapshot.file_open_failure.clone() {
                let document_id = snapshot.source_document.document_id().ok()?;
                let view_id = snapshot.source_document.view_id();
                let tab_id = crate::editor::panes::TabId::from_uuid(view_id.uuid());
                let title = failure
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Document error".to_owned());
                let document = crate::editor::panes::PaneDocumentRef::from_error(
                    document_id,
                    failure.path,
                    failure.reason,
                    view_id,
                    title,
                );
                (tab_id, document_id, document)
            } else if let Some(host) = snapshot.document_host.as_ref().cloned() {
                let path = snapshot
                    .file_path
                    .clone()
                    .unwrap_or_else(|| host.read(cx).path().to_path_buf());
                let probe = host.read(cx).probe().clone();
                let detached = host.update(cx, |host, cx| host.detach_view(cx))?;
                let document_id = detached
                    .handle()
                    .lock()
                    .ok()
                    .map(|controller| controller.document_id())?;
                let tab_id = crate::editor::panes::TabId::from_uuid(detached.view_id().uuid());
                let title = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| snapshot.document_kind.untitled_name().to_owned());
                let document = crate::editor::panes::PaneDocumentRef::from_detached_host(
                    document_id,
                    detached,
                    path,
                    probe,
                    title,
                );
                // DocumentHost already carries its complete presentation snapshot.
                view_state = document.view_state_snapshot();
                (tab_id, document_id, document)
            } else {
                let document_id = snapshot.source_document.document_id().ok()?;
                let lease = snapshot.source_document.lease_arc()?;
                let view_id = snapshot.source_document.view_id();
                let tab_id = crate::editor::panes::TabId::from_uuid(view_id.uuid());
                let title = snapshot
                    .file_path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| snapshot.document_kind.untitled_name().to_owned());
                let document =
                    crate::editor::panes::PaneDocumentRef::from_lease_arc_with_title_and_path(
                        document_id,
                        lease,
                        view_id,
                        title,
                        snapshot.file_path.clone(),
                    );
                (tab_id, document_id, document)
            };
        document.set_view_state_snapshot(view_state);
        Some(crate::editor::panes::TabView::with_pinned(
            tab_id,
            document_id,
            document,
            pinned,
        ))
    }

    pub(super) fn restore_legacy_host_snapshots(
        prepared: &[(
            usize,
            crate::editor::panes::TabView<
                gmark_document_runtime::DocumentId,
                crate::editor::panes::PaneDocumentRef,
            >,
        )],
        records: &mut [TabRecord],
        cx: &mut Context<Self>,
    ) {
        for (index, tab) in prepared {
            let document = tab.view();
            if document.kind() != crate::editor::panes::PaneDocumentKind::DocumentHost {
                continue;
            }
            let Some(detached) = document.take_detached_host() else {
                continue;
            };
            let Some(path) = document.host_path().cloned() else {
                continue;
            };
            let Some(probe) = document.host_probe().cloned() else {
                continue;
            };
            if let Some(snapshot) = records
                .get_mut(*index)
                .and_then(|record| record.snapshot.as_mut())
            {
                snapshot.document_host = Some(cx.new(move |cx| {
                    crate::document_host::DocumentHost::from_detached(path, probe, detached, cx)
                }));
            }
        }
    }

    pub(super) fn migrate_legacy_tabs_into_pane(
        &mut self,
        workspace: &mut crate::editor::panes::PaneWorkspace<
            gmark_document_runtime::DocumentId,
            crate::editor::panes::PaneDocumentRef,
        >,
        pane: crate::editor::panes::PaneId,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.tabs.records.len() <= 1 {
            return true;
        }
        let active = self.tabs.active;
        let mut prepared = Vec::new();
        for (index, record) in self.tabs.records.iter_mut().enumerate() {
            if index == active {
                continue;
            }
            let Some(snapshot) = record.snapshot.as_mut() else {
                Self::restore_legacy_host_snapshots(&prepared, &mut self.tabs.records, cx);
                return false;
            };
            let Some(tab) = Self::pane_tab_from_legacy_snapshot(snapshot, record.pinned, cx) else {
                Self::restore_legacy_host_snapshots(&prepared, &mut self.tabs.records, cx);
                return false;
            };
            prepared.push((index, tab));
        }

        let mut candidate = workspace.clone();
        for (_, tab) in &prepared {
            if candidate.insert_tab(pane, tab.clone()).is_err() {
                Self::restore_legacy_host_snapshots(&prepared, &mut self.tabs.records, cx);
                return false;
            }
        }
        *workspace = candidate;
        self.tabs.records = vec![self.tabs.records.remove(active)];
        self.tabs.records[0].snapshot = None;
        self.tabs.active = 0;
        true
    }

    pub(super) fn restore_root_runtime_after_failed_pane_migration(
        &mut self,
        workspace: &crate::editor::panes::PaneWorkspace<
            gmark_document_runtime::DocumentId,
            crate::editor::panes::PaneDocumentRef,
        >,
        pane: crate::editor::panes::PaneId,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = workspace
            .pane(pane)
            .and_then(|state| state.active_tab())
            .map(|tab| tab.view().clone())
        else {
            return;
        };

        match document.kind() {
            crate::editor::panes::PaneDocumentKind::Markdown => {
                let Some(lease) = document.lease_arc().cloned() else {
                    return;
                };
                if let Ok(session) =
                    EditorDocumentSession::from_lease_arc_with_view_id(lease, document.view_id())
                {
                    self.source_document = session;
                }
            }
            crate::editor::panes::PaneDocumentKind::DocumentHost => {
                let Some(detached) = document.take_detached_host() else {
                    return;
                };
                let Some(path) = document.host_path().cloned() else {
                    let _ = document.put_detached_host(detached);
                    return;
                };
                let Some(probe) = document.host_probe().cloned() else {
                    let _ = document.put_detached_host(detached);
                    return;
                };
                self.document_host = Some(cx.new(move |cx| {
                    crate::document_host::DocumentHost::from_detached(path, probe, detached, cx)
                }));
            }
            crate::editor::panes::PaneDocumentKind::Image
            | crate::editor::panes::PaneDocumentKind::Error => {}
        }
    }

    pub(in crate::editor) fn new_document_tab_in_pane(
        &mut self,
        pane: crate::editor::panes::PaneId,
        document_kind: DocumentKind,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(workspace) = self.pane_workspace.clone() else {
            return false;
        };
        if workspace.read(cx).workspace().pane(pane).is_none() {
            return false;
        }

        let logical_path = std::path::PathBuf::from(document_kind.untitled_name());
        let (tab_id, document_id, document) =
            if let Some(format) = document_kind.document_host_format() {
                let initial_source = document_kind.initial_source();
                let host = cx.new({
                    let logical_path = logical_path.clone();
                    move |cx| {
                        crate::document_host::DocumentHost::new_untitled(
                            logical_path,
                            format,
                            initial_source,
                            cx,
                        )
                    }
                });
                let Some(document_id) = host.read(cx).document_id() else {
                    return false;
                };
                let path = host.read(cx).path().to_path_buf();
                let probe = host.read(cx).probe().clone();
                let Some(detached) = host.update(cx, |host, cx| host.detach_view(cx)) else {
                    return false;
                };
                let tab_id = crate::editor::panes::TabId::from_uuid(detached.view_id().uuid());
                let title = document_kind.untitled_name().to_owned();
                let document = crate::editor::panes::PaneDocumentRef::from_detached_host(
                    document_id,
                    detached,
                    path,
                    probe,
                    title,
                );
                (tab_id, document_id, document)
            } else {
                let session = EditorDocumentSession::new(gmark_document::SourceDocument::new(
                    document_kind.initial_source(),
                ));
                let Ok(document_id) = session.document_id() else {
                    return false;
                };
                let Some(lease) = session.lease_arc() else {
                    return false;
                };
                let view_id = session.view_id();
                let tab_id = crate::editor::panes::TabId::from_uuid(view_id.uuid());
                let document =
                    crate::editor::panes::PaneDocumentRef::from_lease_arc_with_title_and_path(
                        document_id,
                        lease,
                        view_id,
                        document_kind.untitled_name(),
                        None,
                    );
                let view_state = crate::editor::panes::PaneViewStateSnapshot {
                    view_mode: Some(
                        match document_kind.initial_view_mode() {
                            ViewMode::Rendered => "live",
                            ViewMode::Source => "source",
                            ViewMode::Preview => "preview",
                            ViewMode::Split => "split",
                        }
                        .to_owned(),
                    ),
                    ..Default::default()
                };
                document.set_view_state_snapshot(view_state);
                (tab_id, document_id, document)
            };

        let inserted = workspace
            .update(cx, |workspace, _cx| {
                workspace.workspace_mut().insert_tab(
                    pane,
                    crate::editor::panes::TabView::new(tab_id, document_id, document),
                )?;
                workspace.workspace_mut().set_active_tab(pane, tab_id)
            })
            .is_ok();
        if inserted {
            self.sync_pane_canvas_entities(cx);
            self.schedule_workspace_session_save(cx);
            cx.notify();
        }
        inserted
    }

    pub(super) fn ensure_pane_workspace(&mut self, cx: &mut Context<Self>) {
        if self.pane_workspace.is_none() {
            let root_view_state = self.pane_view_state_snapshot(cx);
            let pane_id = crate::editor::panes::PaneId::new();
            let (tab_id, document_id, document) = if let Some(path) =
                self.image_preview_path.clone()
            {
                let Ok(document_id) = self.source_document.document_id() else {
                    return;
                };
                let view_id = self.source_document.view_id();
                let tab_id = crate::editor::panes::TabId::from_uuid(view_id.uuid());
                let title = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Image".to_owned());
                let document = crate::editor::panes::PaneDocumentRef::from_image(
                    document_id,
                    path,
                    view_id,
                    title,
                );
                (tab_id, document_id, document)
            } else if let Some(failure) = self.file_open_failure.clone() {
                let Ok(document_id) = self.source_document.document_id() else {
                    return;
                };
                let view_id = self.source_document.view_id();
                let tab_id = crate::editor::panes::TabId::from_uuid(view_id.uuid());
                let title = failure
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Document error".to_owned());
                let document = crate::editor::panes::PaneDocumentRef::from_error(
                    document_id,
                    failure.path,
                    failure.reason,
                    view_id,
                    title,
                );
                (tab_id, document_id, document)
            } else if let Some(host_entity) = self.document_host.as_ref().cloned() {
                let Some(detached) = host_entity.update(cx, |host, cx| host.detach_view(cx)) else {
                    return;
                };
                let Ok(document_id) = detached
                    .handle()
                    .lock()
                    .map(|controller| controller.document_id())
                else {
                    return;
                };
                let Some(path) = self.pane_host_path.clone() else {
                    return;
                };
                let Some(probe) = self.pane_host_probe.clone() else {
                    return;
                };
                let tab_id = crate::editor::panes::TabId::from_uuid(detached.view_id().uuid());
                let title = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Document".to_owned());
                let document = crate::editor::panes::PaneDocumentRef::from_detached_host(
                    document_id,
                    detached,
                    path,
                    probe,
                    title,
                );
                (tab_id, document_id, document)
            } else {
                // Transfer the root Markdown view lease into the first
                // pane.  The shell keeps a tiny empty compatibility
                // adapter for legacy menu code, so one pane does not
                // create a second lease over the actual document.
                let session =
                    std::mem::replace(&mut self.source_document, EditorDocumentSession::shell());
                let tab_id = crate::editor::panes::TabId::from_uuid(session.view_id().uuid());
                let Ok(document_id) = session.document_id() else {
                    return;
                };
                let Some(lease) = session.lease_arc() else {
                    return;
                };
                let title = self
                    .file_path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| self.document_kind.untitled_name().to_owned());
                let document =
                    crate::editor::panes::PaneDocumentRef::from_lease_arc_with_title_and_path(
                        document_id,
                        lease,
                        session.view_id(),
                        title,
                        self.file_path.clone(),
                    );
                document.set_view_state_snapshot(root_view_state);
                let _ = session;
                (tab_id, document_id, document)
            };
            let mut workspace = crate::editor::panes::PaneWorkspace::with_root_id(pane_id);
            if workspace
                .insert_tab(
                    pane_id,
                    crate::editor::panes::TabView::new(tab_id, document_id, document),
                )
                .is_err()
            {
                return;
            }
            if !self.migrate_legacy_tabs_into_pane(&mut workspace, pane_id, cx) {
                self.restore_root_runtime_after_failed_pane_migration(&workspace, pane_id, cx);
                return;
            }
            self.file_watch_task = None;
            self.file_watch_guard = None;
            self.shared_event_task = None;
            let editor = cx.entity().downgrade();
            let editor_for_persistence = editor.clone();
            let controller =
                crate::editor::panes::PaneWorkspaceController::new(move |event, window, cx| {
                    let _ = editor.update(cx, |editor, cx| {
                        editor.handle_pane_event(event, Some(window), cx)
                    });
                })
                .with_workspace_changed(move |cx| {
                    let _ = editor_for_persistence.update(cx, |editor, cx| {
                        editor.schedule_workspace_session_save(cx);
                    });
                });
            let entity =
                cx.new(|_| crate::editor::panes::PaneWorkspaceView::new(workspace, controller));
            self.pane_workspace = Some(entity);
        }
        self.sync_pane_canvas_entities(cx);
    }
}
