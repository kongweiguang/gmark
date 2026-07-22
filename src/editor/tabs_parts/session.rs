// @author kongweiguang

use super::*;
use crate::editor::document_session::EditorDocumentSession;

impl Editor {
    pub(crate) fn is_document_dirty(&self) -> bool {
        let dirty = if self.document_host.is_some() {
            self.document_dirty
        } else {
            self.source_document.is_dirty()
        };
        #[cfg(test)]
        {
            // 旧 UI fixture 可能只设置边沿缓存；生产构建不会把该缓存作为正文真值。
            dirty || self.document_dirty
        }
        #[cfg(not(test))]
        dirty
    }

    #[cfg(test)]
    pub(crate) fn set_document_dirty_for_test(&mut self, dirty: bool) {
        self.document_dirty = dirty;
        if dirty {
            self.source_document.mark_dirty();
        } else {
            self.source_document.mark_persisted();
        }
    }

    pub(in crate::editor) fn dismiss_tab_context_menu(&mut self) -> bool {
        let dismissed = self.tabs.context_menu.take().is_some();
        if dismissed {
            self.context_menu_keyboard_item = None;
            self.context_menu_keyboard_submenu_item = None;
        }
        dismissed
    }

    pub(in crate::editor) fn tab_context_menu_info(&self) -> Option<(usize, bool, bool)> {
        let menu = self.tabs.context_menu.as_ref()?;
        let pinned = self
            .tabs
            .records
            .get(menu.index)
            .is_some_and(|record| record.pinned);
        Some((menu.index, pinned, self.tabs.records.len() > 1))
    }

    pub(in crate::editor) fn workspace_session_snapshot(
        &self,
        cx: &App,
    ) -> crate::config::workspace_session::WorkspaceSession {
        let active_path = self.file_path.as_ref();
        let mut active_index = 0usize;
        let mut tabs = Vec::new();
        for (index, record) in self.tabs.records.iter().enumerate() {
            let path = if index == self.tabs.active {
                active_path
            } else {
                record
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.file_path.as_ref())
            };
            let Some(path) = path else {
                continue;
            };
            if index == self.tabs.active {
                active_index = tabs.len();
            }
            let mut tab = crate::config::workspace_session::WorkspaceSessionTab::new(
                path.clone(),
                record.pinned,
            );
            let (view_mode, selection, scroll_offset) = if index == self.tabs.active {
                if let Some(host) = self.document_host.as_ref() {
                    let (selection, scroll) = host.read(cx).workspace_source_state();
                    (self.view_mode, selection, scroll)
                } else {
                    (
                        self.view_mode,
                        self.last_selection_snapshot.source_selection(),
                        self.scroll_handle.offset(),
                    )
                }
            } else {
                let snapshot = record
                    .snapshot
                    .as_ref()
                    .expect("inactive tab must own a snapshot");
                if let Some(host) = snapshot.document_host.as_ref() {
                    let (selection, scroll) = host.read(cx).workspace_source_state();
                    (snapshot.view_mode, selection, scroll)
                } else {
                    (
                        snapshot.view_mode,
                        snapshot.selection.source_selection(),
                        snapshot.scroll_offset,
                    )
                }
            };
            tab.view_mode = Some(Self::session_view_mode(view_mode).to_owned());
            tab.selection = Some(
                crate::config::workspace_session::WorkspaceSessionSelection::from_source_selection(
                    selection,
                ),
            );
            tab.scroll_x = Some(f32::from(scroll_offset.x));
            tab.scroll_y = Some(f32::from(scroll_offset.y));
            tabs.push(tab);
        }
        let mut session = crate::config::workspace_session::WorkspaceSession::new(
            self.tabs.session_id,
            tabs,
            active_index,
            self.explicit_workspace_root(),
        );
        session.window = self.tabs.window.clone();
        session.workspace_panel_width = self.workspace_panel_width();
        session.workspace_docked_open = Some(self.workspace_docked_open_preference());
        session.split_pane_ratio = Some(self.split_pane_ratio.clamp(0.3, 0.7));
        session
    }

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
            let session = self.workspace_session_snapshot(cx);
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
            let session = self.workspace_session_snapshot(cx);
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
        self.split_pane_ratio = split_pane_ratio
            .filter(|ratio| ratio.is_finite())
            .map_or(0.5, |ratio| ratio.clamp(0.3, 0.7));
        self.split_resize_session = None;
        let target = active_index.min(self.tabs.records.len().saturating_sub(1));
        self.switch_to_tab_index(target, cx);
        self.schedule_workspace_session_save(cx);
        cx.notify();
    }

    pub(super) fn session_view_mode(mode: ViewMode) -> &'static str {
        match mode {
            ViewMode::Rendered => "live",
            ViewMode::Source => "source",
            ViewMode::Preview => "preview",
            ViewMode::Split => "split",
        }
    }

    pub(super) fn restored_view_mode(mode: Option<&str>) -> ViewMode {
        match mode.map(str::to_ascii_lowercase).as_deref() {
            Some("source") => ViewMode::Source,
            Some("preview" | "structure") => ViewMode::Preview,
            Some("split") => ViewMode::Split,
            Some("live" | "rendered") => ViewMode::Rendered,
            _ => ViewMode::Rendered,
        }
    }

    pub(super) fn restored_selection(
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

    pub(in crate::editor) fn mark_explicit_window_close(&mut self, explicit: bool) {
        self.tabs.remove_session_after_window_close = explicit;
    }

    pub(in crate::editor) fn cancel_explicit_window_close(&mut self) {
        self.tabs.remove_session_after_window_close = false;
    }

    pub(in crate::editor) fn remove_workspace_session_for_explicit_close(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.tabs.remove_session_after_window_close {
            return;
        }
        self.tabs.remove_session_after_window_close = false;
        #[cfg(test)]
        {
            let _ = cx;
        }
        #[cfg(not(test))]
        {
            let id = self.tabs.session_id;
            let generation = self.tabs.session_generation.wrapping_add(1);
            self.tabs.session_generation = generation;
            SESSION_WRITE_GENERATIONS
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .expect("workspace session generation lock poisoned")
                .insert(id, generation);
            cx.background_spawn(async move {
                let _guard = SESSION_WRITE_LOCK
                    .lock()
                    .map_err(|_| anyhow::anyhow!("workspace session write lock poisoned"))?;
                crate::config::workspace_session::remove_workspace_session(id)
            })
            .detach();
        }
    }

    pub(in crate::editor) fn remove_workspace_session_after_final_save(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.pending_close_after_save {
            let should_remove = self.tabs.remove_session_after_window_close;
            self.remove_workspace_session_for_explicit_close(cx);
            return should_remove;
        }
        false
    }

    pub(crate) fn install_detached_tab(&mut self, detached: DetachedTab, cx: &mut Context<Self>) {
        self.install_tab_snapshot(detached.snapshot, cx);
        self.schedule_workspace_session_save(cx);
    }

    pub(super) fn detach_tab_by_id(
        &mut self,
        id: uuid::Uuid,
        cx: &mut Context<Self>,
    ) -> Option<DetachedTab> {
        if self.tabs.records.len() < 2 || !self.can_switch_tabs() {
            return None;
        }
        let index = self
            .tabs
            .records
            .iter()
            .position(|record| record.id == id)?;
        let snapshot = if index == self.tabs.active {
            let snapshot = self.capture_active_tab(cx);
            self.tabs.records.remove(index);
            self.tabs.active = index.min(self.tabs.records.len() - 1);
            let target = self.tabs.records[self.tabs.active]
                .snapshot
                .take()
                .expect("inactive tab must own a snapshot");
            self.install_tab_snapshot(target, cx);
            snapshot
        } else {
            self.tabs.records.remove(index).snapshot?
        };
        if index < self.tabs.active {
            self.tabs.active -= 1;
        }
        self.schedule_workspace_session_save(cx);
        cx.notify();
        Some(DetachedTab { snapshot })
    }

    pub(super) fn can_switch_tabs(&self) -> bool {
        self.save_task.is_none() && self.export_task.is_none()
    }

    pub(super) fn capture_active_tab(&mut self, cx: &mut Context<Self>) -> DocumentTabSnapshot {
        if matches!(self.view_mode, ViewMode::Source | ViewMode::Split) {
            let source = self.document.raw_source_text(cx);
            self.sync_source_document_from_projection(&source);
        }
        let selection = self.capture_source_selection_snapshot(cx);

        // 文档后台任务绝不能跨标签完成后污染另一份活动状态。保存和导出在入口被阻止，
        // 其余 debounce/task 可安全取消并在目标标签恢复后重新调度。
        self.auto_save_task = None;
        self.spellcheck_task = None;
        self.projection_cache_task = None;
        self.split_projection_task = None;
        self.recovery_task = None;
        self.file_watch_task = None;
        self.file_watch_guard = None;

        DocumentTabSnapshot {
            document_host: self.document_host.take(),
            source_document: mem::replace(
                &mut self.source_document,
                EditorDocumentSession::new(SourceDocument::new("")),
            ),
            source_encoding: mem::replace(
                &mut self.source_encoding,
                crate::document_io::DocumentEncoding::Utf8,
            ),
            document_kind: self.document_kind,
            file_path: self.file_path.take(),
            file_open_failure: self.file_open_failure.take(),
            saved_file_fingerprint: self.saved_file_fingerprint.take(),
            document_dirty: self.is_document_dirty(),
            view_mode: self.view_mode,
            selection,
            scroll_offset: self.scroll_handle.offset(),
            undo_history: mem::take(&mut self.undo_history),
            redo_history: mem::take(&mut self.redo_history),
            pending_undo_capture: self.pending_undo_capture.take(),
            virtual_undo_selections: mem::take(&mut self.virtual_undo_selections),
            virtual_redo_selections: mem::take(&mut self.virtual_redo_selections),
            pending_virtual_undo_selection: self.pending_virtual_undo_selection.take(),
            last_stable_source_text: mem::take(&mut self.last_stable_source_text),
            recovery_journal: self.recovery_journal.take(),
            external_file_conflict: self.external_file_conflict,
            recovered_session: self.recovered_session,
            show_encoding_conversion_dialog: self.show_encoding_conversion_dialog,
            external_conflict_preview: self.external_conflict_preview.take(),
            allow_external_overwrite_once: self.allow_external_overwrite_once,
        }
    }

    pub(super) fn install_tab_snapshot(
        &mut self,
        snapshot: DocumentTabSnapshot,
        cx: &mut Context<Self>,
    ) {
        self.accessibility_revision = None;
        self.document_host = snapshot.document_host.clone();
        if let Some(document_host) = self.document_host.as_ref() {
            document_host.update(cx, |view, cx| view.resume_after_closed_tab(cx));
        }
        let source = snapshot.source_document.text();
        let target_mode = snapshot.view_mode;
        let target_path = snapshot.file_path.clone();
        // 搜索结果导航属于即将安装的目标标签。replace_document 会提前消费该请求，
        // 因此先暂存，待快照选择恢复完毕后再执行，避免目标行被旧光标覆盖。
        let pending_navigation = self.take_pending_workspace_navigation();
        self.replace_document_from_markdown(source, target_path, cx);

        self.source_document = snapshot.source_document;
        self.source_encoding = snapshot.source_encoding;
        self.document_kind = snapshot.document_kind;
        self.file_open_failure = snapshot.file_open_failure;
        self.saved_file_fingerprint = snapshot.saved_file_fingerprint;
        self.document_dirty = snapshot.document_dirty;
        if snapshot.document_dirty {
            self.source_document.mark_dirty();
        } else {
            self.source_document.mark_persisted();
        }
        self.pending_window_edited = snapshot.document_dirty;
        self.pending_window_title_refresh = true;
        self.undo_history = snapshot.undo_history;
        self.redo_history = snapshot.redo_history;
        self.pending_undo_capture = snapshot.pending_undo_capture;
        self.virtual_undo_selections = snapshot.virtual_undo_selections;
        self.virtual_redo_selections = snapshot.virtual_redo_selections;
        self.pending_virtual_undo_selection = snapshot.pending_virtual_undo_selection;
        self.last_stable_source_text = snapshot.last_stable_source_text;
        self.recovery_journal = snapshot.recovery_journal;
        self.external_file_conflict = snapshot.external_file_conflict;
        self.recovered_session = snapshot.recovered_session;
        self.show_encoding_conversion_dialog = snapshot.show_encoding_conversion_dialog;
        self.external_conflict_preview = snapshot.external_conflict_preview;
        self.allow_external_overwrite_once = snapshot.allow_external_overwrite_once;

        // replace_document 建出的 projection 属于临时 revision；重新发布目标 Rope 的 snapshot，
        // 确保虚拟 surface、Split cache 和保存 revision 使用同一个真值。
        self.projection_cache = None;
        self.rebuild_primary_projection_from_source(cx);
        if target_mode != ViewMode::Rendered {
            self.set_view_mode(target_mode, cx);
        }
        self.apply_selection_snapshot_in_current_mode(&snapshot.selection, cx);
        self.last_selection_snapshot = snapshot.selection;
        self.scroll_handle.set_offset(snapshot.scroll_offset);
        self.pending_scroll_active_block_into_view = false;
        self.pending_scroll_recheck_after_layout = false;
        self.restore_pending_workspace_navigation(pending_navigation);
        self.apply_pending_workspace_navigation(cx);
        self.restart_file_watcher(cx);
        if self.is_document_dirty() {
            self.schedule_recovery_journal(cx);
            self.schedule_auto_save(cx);
        }
        cx.notify();
    }

    pub(in crate::editor) fn switch_to_tab_index(
        &mut self,
        target: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if target == self.tabs.active
            || target >= self.tabs.records.len()
            || !self.can_switch_tabs()
        {
            return false;
        }
        let Some(target_snapshot) = self.tabs.records[target].snapshot.take() else {
            return false;
        };
        let current_snapshot = self.capture_active_tab(cx);
        self.tabs.records[self.tabs.active].snapshot = Some(current_snapshot);
        self.tabs.active = target;
        self.install_tab_snapshot(target_snapshot, cx);
        if self.find_panel.is_some() {
            self.schedule_find(cx);
        }
        self.schedule_workspace_session_save(cx);
        true
    }

    pub(super) fn tab_index_for_path(&self, path: &Path) -> Option<usize> {
        let active_path = self.file_path.as_deref();
        self.tabs
            .records
            .iter()
            .enumerate()
            .find_map(|(index, record)| {
                let candidate = if index == self.tabs.active {
                    active_path
                } else {
                    record
                        .snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.file_path.as_deref())
                };
                (candidate == Some(path)).then_some(index)
            })
    }

    pub(in crate::editor) fn open_path_in_tab(
        &mut self,
        path: PathBuf,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.tab_index_for_path(&path) {
            self.switch_to_tab_index(index, cx);
            return;
        }
        if !self.can_switch_tabs() {
            return;
        }
        self.tabs.open_generation = self.tabs.open_generation.wrapping_add(1);
        let generation = self.tabs.open_generation;
        self.tabs.open_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let read_path = path.clone();
            let opened = cx
                .background_spawn(async move { crate::document_io::open_document(&read_path) })
                .await;
            let _ = this.update(cx, |editor, cx| {
                if editor.tabs.open_generation != generation {
                    return;
                }
                editor.tabs.open_task = None;
                match opened {
                    Ok(crate::document_io::OpenedDocument::Resident(opened)) => {
                        editor.install_new_tab(opened, path, cx)
                    }
                    Ok(
                        crate::document_io::OpenedDocument::ResidentFormat(probe)
                        | crate::document_io::OpenedDocument::Paged(probe),
                    ) => match gmark_paged_document::FileSource::open(&path) {
                        Ok(source) => editor.install_new_source_backed_tab(path, probe, source, cx),
                        Err(error) => {
                            editor.install_file_open_failure_tab(path, error.to_string(), cx)
                        }
                    },
                    Err(error) => editor.install_file_open_failure_tab(path, error.to_string(), cx),
                }
            });
        }));
    }
}
