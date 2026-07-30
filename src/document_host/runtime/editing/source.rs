// @author kongweiguang

//! Source selection, clipboard, and replacement operations.

use super::*;

impl DocumentHost {
    pub(super) fn selected_source_byte_range(&self) -> Option<Range<u64>> {
        if let Some(document) = self.document.as_ref() {
            let range = document.source_selection().range();
            return (!range.is_empty()).then_some(range);
        }
        None
    }

    pub(super) fn select_source_lines(&mut self, lines: Range<usize>, reversed: bool) {
        self.selection_anchor = if reversed {
            lines.end.checked_sub(1)
        } else {
            Some(lines.start)
        };
        self.selected_lines = Some(lines.clone());
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let Some(start) = document
            .line_range(lines.start as u64)
            .map(|range| range.start)
        else {
            return;
        };
        let Some(end) = lines
            .end
            .checked_sub(1)
            .and_then(|line| document.line_range(line as u64))
            .map(|range| range.end)
        else {
            return;
        };
        document.set_selection(start..end, reversed);
    }

    pub(super) fn on_select_all(
        &mut self,
        _: &SelectAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_edit.is_some() {
            return;
        }
        let line_count = self.line_count();
        if line_count == 0 {
            return;
        }
        self.select_source_lines(0..line_count, false);
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub(super) fn on_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.view_mode != DocumentHostViewMode::Source {
            if self.is_json_document()
                && let Some(range) = self
                    .graph_selected_item
                    .as_ref()
                    .and_then(|item| self.resolve_json_graph_edit_target(item))
                    .map(|target| target.range)
                && let Some(document) = self.document.clone()
            {
                if selection_transfer_for_len(range.end.saturating_sub(range.start))
                    == SelectionTransfer::ExportFile
                {
                    self.error = Some(
                        cx.global::<I18nManager>()
                            .strings()
                            .large_document_text("clipboard_limit")
                            .into(),
                    );
                    cx.notify();
                    return;
                }
                self.start_clipboard_read(document, range, false, cx);
                return;
            }
            if let Some(text) = self.selected_structured_cell_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.error = None;
                cx.notify();
                return;
            }
        }
        let Some(lines) = self.selected_lines.clone() else {
            return;
        };
        if lines.len() <= 1 && self.active_edit.is_some() {
            return;
        }
        let Some(document) = self.document.clone() else {
            // 首屏索引尚未完成时，单个可见行已经是稳定的解码快照，复制不应失效。
            // 多行仍等待精确行坐标，避免把估算行窗口拼成错误正文。
            if lines.len() == 1
                && let Some(row) = self.displayed_screen_lines.row(lines.start)
            {
                cx.write_to_clipboard(ClipboardItem::new_string(row.text.to_string()));
                self.error = None;
                cx.notify();
            }
            return;
        };
        let Some(range) = self.selected_source_byte_range() else {
            return;
        };
        if selection_transfer_for_len(range.end.saturating_sub(range.start))
            == SelectionTransfer::ExportFile
        {
            self.error = Some(
                cx.global::<I18nManager>()
                    .strings()
                    .large_document_text("clipboard_limit")
                    .into(),
            );
            cx.notify();
            return;
        }
        self.start_clipboard_read(document, range, false, cx);
    }

    fn start_clipboard_read(
        &mut self,
        document: DocumentSession,
        range: Range<u64>,
        delete_after_copy: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(cancellation) = self.clipboard_cancellation.take() {
            cancellation.cancel();
        }
        self.clipboard_generation = self.clipboard_generation.wrapping_add(1);
        let generation = self.clipboard_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        let revision = document.revision();
        let read_range = range.clone();
        self.metrics.copy_requests = self.metrics.copy_requests.saturating_add(1);
        let cancellation = SearchCancellation::default();
        self.clipboard_cancellation = Some(cancellation.clone());
        self.coordinator.external_status = Some(
            cx.global::<I18nManager>()
                .strings()
                .large_document_text("copying_selection")
                .into(),
        );
        self.clipboard_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    document.read_range_cancellable(read_range, &cancellation)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_identity(view, view.clipboard_generation) {
                    return;
                }
                view.clipboard_cancellation = None;
                view.coordinator.external_status = None;
                match result {
                    Ok(bytes) => {
                        view.metrics.copied_bytes =
                            view.metrics.copied_bytes.saturating_add(bytes.len() as u64);
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            String::from_utf8_lossy(&bytes).into_owned(),
                        ));
                        if delete_after_copy {
                            let current_revision =
                                view.document.as_ref().map(DocumentSession::revision);
                            if current_revision == Some(revision) {
                                view.replace_source_range(range, "", cx);
                            } else {
                                view.error = Some(
                                    cx.global::<I18nManager>()
                                        .strings()
                                        .large_document_text("cut_revision_changed")
                                        .into(),
                                );
                            }
                        } else {
                            view.error = None;
                        }
                    }
                    Err(error) => view.error = Some(localized_document_error(&error, cx)),
                }
                cx.notify();
            });
        });
        cx.notify();
    }

    pub(super) fn delete_selected_source(&mut self, cx: &mut Context<Self>) {
        if self.saving || self.reloading {
            return;
        }
        if self
            .selected_lines
            .as_ref()
            .is_none_or(|lines| lines.len() <= 1 && self.active_edit.is_some())
        {
            return;
        }
        let Some(range) = self.selected_source_byte_range() else {
            return;
        };
        self.replace_source_range(range, "", cx);
    }

    fn replace_source_range(
        &mut self,
        range: Range<u64>,
        replacement: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(mut next_document) = self.document.clone() else {
            return;
        };
        if let Err(error) = next_document.replace_text(range.clone(), replacement) {
            self.error = Some(localized_document_error(&error, cx));
            cx.notify();
            return;
        }
        let preserve_view = (self.probe.format == DocumentFormat::Json
            && self.view_mode == DocumentHostViewMode::Split)
            || (self.is_delimited_document()
                && matches!(
                    self.view_mode,
                    DocumentHostViewMode::Live | DocumentHostViewMode::Split
                ));
        self.install_source_replacement(
            next_document,
            range,
            replacement,
            preserve_view,
            false,
            false,
            cx,
        );
    }

    fn replace_structured_cell_source_range(
        &mut self,
        range: Range<u64>,
        replacement: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut next_document) = self.document.clone() else {
            return false;
        };
        if let Err(error) = next_document.replace_text(range.clone(), replacement) {
            self.error = Some(localized_document_error(&error, cx));
            cx.notify();
            return false;
        }
        self.install_source_replacement(next_document, range, replacement, true, true, false, cx);
        true
    }

    /// CSV 表格操作在旧索引追平前继续使用其基线坐标；源码 transaction 成功后
    /// 同步记录长度变化，让紧接着发生的单元格、行操作仍能命中当前正文。
    pub(super) fn replace_delimited_table_source_range(
        &mut self,
        baseline_range: Range<u64>,
        replacement: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let current_range = self.current_structured_record_range(&baseline_range);
        let delta = i64::try_from(replacement.len())
            .unwrap_or(i64::MAX)
            .saturating_sub(
                i64::try_from(current_range.end - current_range.start).unwrap_or(i64::MAX),
            );
        if !self.replace_structured_cell_source_range(current_range, replacement, cx) {
            return false;
        }
        self.structured_cell_source_edits
            .push((baseline_range, delta));
        true
    }

    pub(super) fn replace_source_range_from_graph(
        &mut self,
        base_revision: u64,
        range: Range<u64>,
        replacement: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut next_document) = self.document.clone() else {
            return false;
        };
        let transaction = Transaction {
            base_revision: gmark_document_core::DocumentRevision(base_revision),
            edits: vec![SourceEdit {
                range: range.clone(),
                replacement: Arc::from(replacement),
            }],
        };
        if let Err(error) = next_document.apply_source_transaction(&transaction) {
            self.graph_edit_error = Some(localized_document_error(&error, cx));
            cx.notify();
            return false;
        }
        self.install_source_replacement(next_document, range, replacement, true, false, false, cx);
        true
    }

    pub(super) fn install_source_replacement(
        &mut self,
        mut next_document: DocumentSession,
        range: Range<u64>,
        replacement: &str,
        preserve_view: bool,
        preserve_structure: bool,
        preserve_folds: bool,
        cx: &mut Context<Self>,
    ) {
        if !preserve_folds && let Some(document) = self.document.as_ref() {
            let start_line = document
                .line_for_offset(range.start.min(document.len()))
                .and_then(|line| usize::try_from(line).ok())
                .unwrap_or_default();
            let end_line = document
                .line_for_offset(range.end.min(document.len()))
                .and_then(|line| usize::try_from(line).ok())
                .unwrap_or(start_line);
            self.fold_projection.apply_source_edit(
                range.clone(),
                start_line,
                end_line,
                replacement,
            );
        }
        let caret = range.start.saturating_add(replacement.len() as u64);
        let selection = Some(SourceSelection::collapsed(caret, SourceAffinity::After));
        if let Some(journal) = self.coordinator.recovery_journal.as_mut()
            && let Err(error) = record_recovery_transaction(
                journal,
                &next_document,
                next_document.revision().saturating_sub(1),
                range.clone(),
                replacement,
                selection,
                recovery_view_id(self.view_mode),
            )
        {
            self.coordinator.recovery_error = Some(error.to_string().into());
        }
        let line = next_document
            .line_for_offset(caret.min(next_document.len()))
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or_default();
        self.active_edit = None;
        self.source_drag_anchor = None;
        self.selection_anchor = Some(line);
        self.selected_lines = Some(line..line.saturating_add(1));
        next_document.dirty = !next_document.is_pristine();
        self.install_document_session(next_document);
        self.tail_enabled = false;
        if !preserve_view {
            self.view_mode = DocumentHostViewMode::Source;
            self.sync_tab_active_view();
        }
        if !preserve_structure {
            self.structured_index = None;
            self.invalidate_structured_runtime();
        }
        // Source 是大文件编辑时的稳定模式；结构索引失效属于内部状态，
        // 不应伪装成顶部错误横幅。用户真正请求结构视图时再说明不可用原因。
        self.clear_structure_error();
        self.error = None;
        self.invalidate_source_rows();
        self.schedule_search(cx);
        self.derived_projection_stale = self.derived_projection_snapshot.is_some();
        self.schedule_json_graph_projection(cx);
        self.schedule_delimited_snapshot_rebuild(cx);
        if preserve_structure {
            // 单格编辑期间旧索引与覆盖值仍可用，后台追平不应显示成整表刷新状态。
            self.clear_structure_error();
        }
        cx.emit(DocumentHostEvent::StateChanged);
        cx.notify();
    }

    pub(super) fn on_paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        // 聚焦行由 Block 的 EntityInputHandler 处理；宿主只处理跨行或卸载选区。
        if self.active_edit.is_some() || self.saving || self.reloading {
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let Some(range) = self
            .document
            .as_ref()
            .map(|document| document.source_selection().range())
        else {
            return;
        };
        self.replace_source_range(range, &text, cx);
    }

    pub(super) fn on_cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if self.saving || self.reloading || self.active_edit.is_some() {
            return;
        }
        let Some(range) = self.selected_source_byte_range() else {
            return;
        };
        if selection_transfer_for_len(range.end.saturating_sub(range.start))
            == SelectionTransfer::ExportFile
        {
            self.error = Some(
                cx.global::<I18nManager>()
                    .strings()
                    .large_document_text("selection_export_limit")
                    .into(),
            );
            cx.notify();
            return;
        }
        let Some(document) = self.document.clone() else {
            return;
        };
        self.start_clipboard_read(document, range, true, cx);
    }

    pub(super) fn on_delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_selected_source(cx);
    }

    pub(super) fn on_delete_back(
        &mut self,
        _: &DeleteBack,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_selected_source(cx);
    }
}
