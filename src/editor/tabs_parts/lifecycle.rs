// @author kongweiguang

use super::*;
use crate::editor::document_session::EditorDocumentSession;

impl Editor {
    pub(super) fn install_new_tab(
        &mut self,
        opened: crate::document_io::OpenedMarkdown,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if !self.can_switch_tabs() {
            return;
        }
        let current = self.capture_active_tab(cx);
        self.tabs.records[self.tabs.active].snapshot = Some(current);
        let snapshot = Self::snapshot_for_opened_document(opened, path);
        self.tabs.records.push(TabRecord {
            id: uuid::Uuid::new_v4(),
            pinned: false,
            snapshot: None,
        });
        self.tabs.active = self.tabs.records.len() - 1;
        self.install_tab_snapshot(snapshot, cx);
        self.schedule_workspace_session_save(cx);
    }

    pub(in crate::editor) fn install_new_source_backed_tab(
        &mut self,
        path: PathBuf,
        probe: gmark_paged_document::OpenProbe,
        source: gmark_paged_document::FileSource,
        cx: &mut Context<Self>,
    ) {
        if !self.can_switch_tabs() {
            return;
        }
        let structured_preview = probe.strategy == gmark_paged_document::OpenStrategy::Resident
            && matches!(
                probe.format,
                gmark_document_core::DocumentFormat::Json
                    | gmark_document_core::DocumentFormat::Delimited { .. }
            );
        let current = self.capture_active_tab(cx);
        self.tabs.records[self.tabs.active].snapshot = Some(current);
        let mut snapshot = Self::snapshot_for_untitled_document(DocumentKind::from_path(&path));
        snapshot.file_path = Some(path.clone());
        snapshot.saved_file_fingerprint = crate::recovery::fingerprint_file(&path).ok();
        snapshot.recovery_journal = None;
        snapshot.view_mode = if structured_preview {
            ViewMode::Preview
        } else {
            ViewMode::Source
        };
        let source_backed_view =
            cx.new(move |cx| crate::document_host::DocumentHost::new(path, probe, source, cx));
        Self::subscribe_document_host(&source_backed_view, cx);
        snapshot.document_host = Some(source_backed_view);
        self.tabs.records.push(TabRecord {
            id: uuid::Uuid::new_v4(),
            pinned: false,
            snapshot: None,
        });
        self.tabs.active = self.tabs.records.len() - 1;
        self.install_tab_snapshot(snapshot, cx);
        self.schedule_workspace_session_save(cx);
    }

    pub(super) fn install_file_open_failure_tab(
        &mut self,
        path: PathBuf,
        reason: String,
        cx: &mut Context<Self>,
    ) {
        let snapshot = Self::snapshot_for_file_open_failure(path, reason);
        self.new_tab_from_snapshot(snapshot, cx);
    }

    pub(crate) fn install_initial_file_open_failure(
        &mut self,
        path: PathBuf,
        reason: String,
        cx: &mut Context<Self>,
    ) {
        let snapshot = Self::snapshot_for_file_open_failure(path, reason);
        self.install_tab_snapshot(snapshot, cx);
        self.schedule_workspace_session_save(cx);
    }

    fn snapshot_for_file_open_failure(path: PathBuf, reason: String) -> DocumentTabSnapshot {
        let mut snapshot = Self::snapshot_for_untitled_document(DocumentKind::from_path(&path));
        snapshot.file_path = Some(path.clone());
        snapshot.file_open_failure = Some(FileOpenFailure {
            path: path.clone(),
            reason,
            action_error: None,
        });
        snapshot.saved_file_fingerprint = crate::recovery::fingerprint_file(&path).ok();
        snapshot.recovery_journal = None;
        snapshot.view_mode = ViewMode::Source;
        snapshot
    }

    pub(super) fn snapshot_for_restored_document(
        tab: &RestoredTab,
        cx: &mut Context<Self>,
    ) -> Option<DocumentTabSnapshot> {
        match &tab.opened {
            crate::document_io::OpenedDocument::Resident(opened) => Some(
                Self::snapshot_for_opened_document(opened.clone(), tab.path.clone()),
            ),
            crate::document_io::OpenedDocument::ResidentFormat(probe)
            | crate::document_io::OpenedDocument::Paged(probe) => {
                let source = gmark_paged_document::FileSource::open(&tab.path).ok()?;
                let mut snapshot =
                    Self::snapshot_for_untitled_document(DocumentKind::from_path(&tab.path));
                snapshot.file_path = Some(tab.path.clone());
                snapshot.saved_file_fingerprint = crate::recovery::fingerprint_file(&tab.path).ok();
                snapshot.recovery_journal = None;
                snapshot.view_mode = if probe.strategy
                    == gmark_paged_document::OpenStrategy::Resident
                    && matches!(
                        probe.format,
                        gmark_document_core::DocumentFormat::Json
                            | gmark_document_core::DocumentFormat::Delimited { .. }
                    ) {
                    ViewMode::Preview
                } else {
                    ViewMode::Source
                };
                let path = tab.path.clone();
                let probe = probe.clone();
                let document_host = cx.new(move |cx| {
                    crate::document_host::DocumentHost::new(path, probe, source, cx)
                });
                Self::subscribe_document_host(&document_host, cx);
                snapshot.document_host = Some(document_host);
                Some(snapshot)
            }
        }
    }

    pub(super) fn snapshot_for_opened_document(
        opened: crate::document_io::OpenedMarkdown,
        path: PathBuf,
    ) -> DocumentTabSnapshot {
        let source_document = EditorDocumentSession::new_with_open_context(
            SourceDocument::new(&opened.text),
            opened.loading_limits,
            opened.text_encoding.clone(),
            opened.file_identity.clone(),
        );
        let source = source_document.text();
        #[cfg(not(test))]
        let recovery_journal = crate::config::GmarkConfigDirs::from_system()
            .and_then(|dirs| {
                crate::recovery::RecoveryJournal::create(
                    &dirs.recovery_dir(),
                    Some(path.clone()),
                    source.clone(),
                )
            })
            .map(|journal| Arc::new(Mutex::new(journal)))
            .ok();
        #[cfg(test)]
        let recovery_journal = None;
        let requires_conversion = !opened.encoding.is_utf8();
        DocumentTabSnapshot {
            document_host: None,
            source_document,
            source_encoding: opened.encoding,
            document_kind: DocumentKind::from_path(&path),
            file_path: Some(path.clone()),
            file_open_failure: None,
            saved_file_fingerprint: crate::recovery::fingerprint_file(&path).ok(),
            document_dirty: false,
            view_mode: if requires_conversion {
                ViewMode::Preview
            } else {
                ViewMode::Rendered
            },
            selection: UndoSelectionSnapshot::collapsed(
                0,
                gmark_document_core::SourceAffinity::Before,
            ),
            scroll_offset: point(px(0.0), px(0.0)),
            undo_history: Vec::new(),
            redo_history: Vec::new(),
            pending_undo_capture: None,
            virtual_undo_selections: Vec::new(),
            virtual_redo_selections: Vec::new(),
            pending_virtual_undo_selection: None,
            last_stable_source_text: source,
            recovery_journal,
            external_file_conflict: false,
            recovered_session: false,
            show_encoding_conversion_dialog: requires_conversion,
            external_conflict_preview: None,
            allow_external_overwrite_once: false,
        }
    }

    pub(super) fn snapshot_for_untitled_document(
        document_kind: DocumentKind,
    ) -> DocumentTabSnapshot {
        let source = document_kind.initial_source();
        let source_document = EditorDocumentSession::new(SourceDocument::new(source));
        #[cfg(not(test))]
        let recovery_journal = crate::config::GmarkConfigDirs::from_system()
            .and_then(|dirs| {
                crate::recovery::RecoveryJournal::create(
                    &dirs.recovery_dir(),
                    None,
                    source.to_owned(),
                )
            })
            .map(|journal| Arc::new(Mutex::new(journal)))
            .ok();
        #[cfg(test)]
        let recovery_journal = None;
        DocumentTabSnapshot {
            document_host: None,
            source_document,
            source_encoding: crate::document_io::DocumentEncoding::Utf8,
            document_kind,
            file_path: None,
            file_open_failure: None,
            saved_file_fingerprint: None,
            document_dirty: false,
            view_mode: document_kind.initial_view_mode(),
            selection: UndoSelectionSnapshot::collapsed(
                0,
                gmark_document_core::SourceAffinity::Before,
            ),
            scroll_offset: point(px(0.0), px(0.0)),
            undo_history: Vec::new(),
            redo_history: Vec::new(),
            pending_undo_capture: None,
            virtual_undo_selections: Vec::new(),
            virtual_redo_selections: Vec::new(),
            pending_virtual_undo_selection: None,
            last_stable_source_text: source.to_owned(),
            recovery_journal,
            external_file_conflict: false,
            recovered_session: false,
            show_encoding_conversion_dialog: false,
            external_conflict_preview: None,
            allow_external_overwrite_once: false,
        }
    }

    pub(crate) fn new_untitled_tab(&mut self, cx: &mut Context<Self>) -> bool {
        self.new_document_tab(DocumentKind::Markdown, cx)
    }

    pub(super) fn new_untyped_tab(&mut self, cx: &mut Context<Self>) -> bool {
        self.new_document_tab(DocumentKind::Unspecified, cx)
    }

    pub(super) fn new_document_tab(
        &mut self,
        document_kind: DocumentKind,
        cx: &mut Context<Self>,
    ) -> bool {
        self.new_tab_from_snapshot(Self::snapshot_for_untitled_document(document_kind), cx)
    }

    fn new_tab_from_snapshot(
        &mut self,
        snapshot: DocumentTabSnapshot,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.can_switch_tabs() {
            return false;
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
        true
    }

    pub(super) fn snapshot_for_recovered_document(
        recovered: crate::recovery::RecoveredDocument,
    ) -> DocumentTabSnapshot {
        if recovered.read_status == crate::recovery::RecoveryReadStatus::TruncatedTail {
            eprintln!(
                "recovery journal '{}' had a corrupt tail; restored the last CRC-valid record",
                recovered.journal_path.display()
            );
        }
        if recovered.base_file_changed {
            eprintln!(
                "recovered document base changed externally: {}",
                recovered.file_path.as_deref().map_or_else(
                    || "<untitled>".to_owned(),
                    |path| path.display().to_string()
                )
            );
        }
        let selection =
            UndoSelectionSnapshot::from_source_selection(recovered.selection.source_selection());
        let view_mode = match recovered.view_mode.as_str() {
            "source" => ViewMode::Source,
            "split" => ViewMode::Split,
            "preview" => ViewMode::Preview,
            _ => ViewMode::Rendered,
        };
        let file_path = recovered.file_path.clone();
        let source = recovered.source.clone();
        let mut source_document = EditorDocumentSession::new(SourceDocument::new(&source));
        assert!(
            source_document.restore_source_format(recovered.source_format.clone()),
            "恢复日志中的源码格式必须与恢复文本一致"
        );
        source_document.mark_dirty();
        DocumentTabSnapshot {
            document_host: None,
            source_document,
            source_encoding: crate::document_io::DocumentEncoding::Utf8,
            document_kind: file_path
                .as_deref()
                .map(DocumentKind::from_path)
                .unwrap_or(DocumentKind::Markdown),
            saved_file_fingerprint: file_path
                .as_deref()
                .and_then(|path| crate::recovery::fingerprint_file(path).ok()),
            file_path,
            file_open_failure: None,
            document_dirty: true,
            view_mode,
            selection,
            scroll_offset: point(px(0.0), px(0.0)),
            undo_history: Vec::new(),
            redo_history: Vec::new(),
            pending_undo_capture: None,
            virtual_undo_selections: Vec::new(),
            virtual_redo_selections: Vec::new(),
            pending_virtual_undo_selection: None,
            last_stable_source_text: source,
            recovery_journal: Some(Arc::new(Mutex::new(
                crate::recovery::RecoveryJournal::resume(&recovered),
            ))),
            external_file_conflict: recovered.base_file_changed,
            recovered_session: true,
            show_encoding_conversion_dialog: false,
            external_conflict_preview: None,
            allow_external_overwrite_once: false,
        }
    }

    pub(crate) fn append_recovered_tabs(
        &mut self,
        recovered: Vec<crate::recovery::RecoveredDocument>,
        cx: &mut Context<Self>,
    ) {
        for recovered in recovered {
            self.tabs.records.push(TabRecord {
                id: uuid::Uuid::new_v4(),
                pinned: false,
                snapshot: Some(Self::snapshot_for_recovered_document(recovered)),
            });
        }
        self.schedule_workspace_session_save(cx);
        cx.notify();
    }

    pub(in crate::editor) fn tab_strip_height(&self) -> f32 {
        if self.focus_mode {
            0.0
        } else {
            TAB_STRIP_HEIGHT
        }
    }

    pub(super) fn push_closed_tab(
        &mut self,
        snapshot: DocumentTabSnapshot,
        cx: &mut Context<Self>,
    ) {
        if let Some(document_host) = snapshot.document_host.as_ref() {
            document_host.update(cx, |view, _cx| view.suspend_for_closed_tab());
        }
        self.tabs.closed.push(snapshot);
        if self.tabs.closed.len() > CLOSED_TAB_LIMIT {
            self.tabs.closed.remove(0);
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
        let dirty = if index == self.tabs.active {
            self.is_document_dirty()
        } else {
            self.tabs.records[index]
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.document_dirty)
        };
        if dirty {
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
            let closed = self.capture_active_tab(cx);
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
        let closed = self.capture_active_tab(cx);
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
        let Some(snapshot) = self.tabs.closed.pop() else {
            return;
        };
        if !self.can_switch_tabs() {
            self.tabs.closed.push(snapshot);
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
        self.checkpoint_recovery_journal();
        self.document_dirty = false;
        if self.close_tab_index_without_prompt(self.tabs.active, false, cx) {
            self.advance_close_other_tabs(cx);
        }
    }

    pub(in crate::editor) fn on_save_tab_close(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tabs.show_close_dialog = false;
        self.tabs.close_after_save = true;
        self.tabs.continue_window_close_after_save = false;
        self.save_document(window, cx);
    }

    pub(in crate::editor) fn finish_pending_tab_close_after_save(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.close_after_save && !self.is_document_dirty() {
            self.tabs.close_after_save = false;
            if self.close_tab_index_without_prompt(self.tabs.active, true, cx) {
                self.advance_close_other_tabs(cx);
            }
        }
    }

    pub(in crate::editor) fn abort_pending_tab_close_after_save(&mut self, cx: &mut Context<Self>) {
        if self.tabs.close_after_save {
            self.tabs.close_after_save = false;
            self.tabs.close_others_keep = None;
            cx.notify();
        }
    }

    pub(super) fn pinned_tab_count(&self) -> usize {
        self.tabs
            .records
            .iter()
            .take_while(|record| record.pinned)
            .count()
    }

    pub(in crate::editor) fn toggle_pin_tab(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if index >= self.tabs.records.len() {
            return false;
        }
        let active_id = self.tabs.records[self.tabs.active].id;
        self.tabs.records[index].pinned = !self.tabs.records[index].pinned;
        // 固定标签始终构成稳定前缀；稳定排序保留每个分区内用户定义的视觉顺序。
        self.tabs.records.sort_by_key(|record| !record.pinned);
        self.tabs.active = self
            .tabs
            .records
            .iter()
            .position(|record| record.id == active_id)
            .expect("active tab must survive pin reorder");
        self.schedule_workspace_session_save(cx);
        cx.notify();
        true
    }

    pub(super) fn reorder_tab(
        &mut self,
        source: usize,
        target: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if source >= self.tabs.records.len()
            || target >= self.tabs.records.len()
            || source == target
        {
            return false;
        }
        let active_id = self.tabs.records[self.tabs.active].id;
        let source_pinned = self.tabs.records[source].pinned;
        let pinned_count = self.pinned_tab_count();
        let allowed = if source_pinned {
            0..pinned_count
        } else {
            pinned_count..self.tabs.records.len()
        };
        let target = target.clamp(allowed.start, allowed.end.saturating_sub(1));
        if source == target {
            return false;
        }
        let record = self.tabs.records.remove(source);
        self.tabs.records.insert(target, record);
        self.tabs.active = self
            .tabs
            .records
            .iter()
            .position(|record| record.id == active_id)
            .expect("active tab must survive drag reorder");
        self.schedule_workspace_session_save(cx);
        cx.notify();
        true
    }

    pub(in crate::editor) fn request_close_other_tabs(
        &mut self,
        keep_index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(keep) = self.tabs.records.get(keep_index).map(|record| record.id) else {
            return;
        };
        self.tabs.close_others_keep = Some(keep);
        self.advance_close_other_tabs(cx);
    }

    pub(super) fn advance_close_other_tabs(&mut self, cx: &mut Context<Self>) {
        let Some(keep) = self.tabs.close_others_keep else {
            return;
        };
        loop {
            let Some(index) = self
                .tabs
                .records
                .iter()
                .position(|record| record.id != keep)
            else {
                self.tabs.close_others_keep = None;
                if let Some(keep_index) = self
                    .tabs
                    .records
                    .iter()
                    .position(|record| record.id == keep)
                {
                    self.switch_to_tab_index(keep_index, cx);
                }
                cx.notify();
                return;
            };
            let before = self.tabs.records.len();
            self.request_close_tab_index(index, cx);
            if self.tabs.show_close_dialog || self.tabs.records.len() == before {
                return;
            }
        }
    }

    pub(super) fn dirty_tab_index_except_active(&self) -> Option<usize> {
        self.tabs
            .records
            .iter()
            .enumerate()
            .find_map(|(index, record)| {
                (index != self.tabs.active
                    && record
                        .snapshot
                        .as_ref()
                        .is_some_and(|snapshot| snapshot.document_dirty))
                .then_some(index)
            })
    }

    pub(in crate::editor) fn activate_dirty_tab_for_window_close(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.is_document_dirty() {
            return true;
        }
        self.dirty_tab_index_except_active()
            .is_some_and(|index| self.switch_to_tab_index(index, cx))
    }

    /// 返回 true 表示当前保存完成后可以直接关闭窗口；false 表示仍需逐个处理后台 dirty 标签。
    pub(in crate::editor) fn prepare_window_close_save(&mut self) -> bool {
        let has_more_dirty_tabs = self.dirty_tab_index_except_active().is_some();
        self.tabs.continue_window_close_after_save = has_more_dirty_tabs;
        self.tabs.close_after_save = false;
        !has_more_dirty_tabs
    }

    pub(in crate::editor) fn continue_window_close_after_save(&mut self, cx: &mut Context<Self>) {
        if !self.tabs.continue_window_close_after_save {
            return;
        }
        self.tabs.continue_window_close_after_save = false;
        if self.activate_dirty_tab_for_window_close(cx) {
            self.show_unsaved_changes_dialog = true;
            self.close_dialog_restore_focus = None;
            cx.notify();
        }
    }

    pub(in crate::editor) fn abort_window_close_tab_sequence(&mut self, cx: &mut Context<Self>) {
        self.cancel_explicit_window_close();
        if self.tabs.continue_window_close_after_save {
            self.tabs.continue_window_close_after_save = false;
            cx.notify();
        }
    }

    pub(super) fn ensure_tab_strip_focus_handles(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (Vec<FocusHandle>, FocusHandle) {
        let live_ids: HashSet<_> = self.tabs.records.iter().map(|record| record.id).collect();
        self.tabs
            .focus_handles
            .retain(|id, _| live_ids.contains(id));
        let handles = self
            .tabs
            .records
            .iter()
            .map(|record| {
                self.tabs
                    .focus_handles
                    .entry(record.id)
                    .or_insert_with(|| cx.focus_handle())
                    .clone()
            })
            .collect();
        let new_tab = self
            .tabs
            .new_tab_focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        (handles, new_tab)
    }

    pub(super) fn focus_tab_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(record) = self.tabs.records.get(index) else {
            return;
        };
        // 标签栏局部导航必须留在 tablist；鼠标点击和全局 Next/Previous 仍沿用编辑器焦点恢复。
        self.pending_focus = None;
        self.tabs
            .focus_handles
            .entry(record.id)
            .or_insert_with(|| cx.focus_handle())
            .focus(window);
    }
}
