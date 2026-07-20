// @author kongweiguang

use super::*;

impl DiskSourceAdapter {
    pub(super) fn cancel_selection_transfers(&mut self) {
        self.clipboard_generation = self.clipboard_generation.wrapping_add(1);
        if let Some(cancellation) = self.clipboard_cancellation.take() {
            cancellation.cancel();
        }
        self.clipboard_task = Task::ready(());
        self.selection_export_generation = self.selection_export_generation.wrapping_add(1);
        if let Some(cancellation) = self.selection_export_cancellation.take() {
            cancellation.cancel();
        }
        self.selection_export_task = Task::ready(());
        if self
            .external_status
            .as_ref()
            .is_some_and(|status| status.as_ref() == "Copying selection…")
        {
            self.external_status = None;
        }
    }

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
            self.error = Some("Selection is larger than the 64 MiB clipboard safety limit".into());
            cx.notify();
            return;
        }
        self.start_clipboard_read(document, range, false, cx);
    }

    fn start_clipboard_read(
        &mut self,
        document: LargeDocumentAdapter,
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
        self.external_status = Some("Copying selection…".into());
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
                view.external_status = None;
                match result {
                    Ok(bytes) => {
                        view.metrics.copied_bytes = view
                            .metrics
                            .copied_bytes
                            .saturating_add(bytes.len() as u64);
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            String::from_utf8_lossy(&bytes).into_owned(),
                        ));
                        if delete_after_copy {
                            let current_revision =
                                view.document.as_ref().map(LargeDocumentAdapter::revision);
                            if current_revision == Some(revision) {
                                view.replace_source_range(range, "", cx);
                            } else {
                                view.error = Some(
                                    "Selection was copied, but the document changed before Cut could delete it"
                                        .into(),
                                );
                            }
                        } else {
                            view.error = None;
                        }
                    }
                    Err(error) => view.error = Some(error.to_string().into()),
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
            self.error = Some(error.to_string().into());
            cx.notify();
            return;
        }
        let caret = range.start.saturating_add(replacement.len() as u64);
        let selection = Some(LargeRecoverySelection::collapsed(
            caret,
            SourceAffinity::After,
        ));
        if let Some(journal) = self.recovery_journal.as_mut()
            && let Err(error) =
                journal.record_replace(range.clone(), replacement, selection, "source")
        {
            self.recovery_error = Some(error.to_string().into());
        }
        let line = next_document
            .line_for_offset(caret.min(next_document.len()))
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or_default();
        self.active_edit = None;
        self.source_drag_anchor = None;
        self.selection_anchor = Some(line);
        self.selected_lines = Some(line..line.saturating_add(1));
        self.dirty = !next_document.is_pristine();
        self.document = Some(next_document);
        self.tail_enabled = false;
        self.view_mode = LargeViewMode::Source;
        self.structured_index = None;
        self.invalidate_structured_runtime();
        // Source 是大文件编辑时的稳定模式；结构索引失效属于内部状态，
        // 不应伪装成顶部错误横幅。用户真正请求结构视图时再说明不可用原因。
        self.clear_structure_error();
        self.error = None;
        self.invalidate_source_rows();
        self.schedule_search(cx);
        cx.emit(DiskSourceEvent::StateChanged);
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
                "Selection is larger than the 64 MiB clipboard safety limit; export it instead"
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

    pub(super) fn on_export_selection(
        &mut self,
        _: &ExportSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_selection_export(false, window, cx);
    }

    pub(super) fn export_selection_as_utf8(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_selection_export(true, window, cx);
    }

    fn start_selection_export(
        &mut self,
        force_utf8: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(range) = self.selected_source_byte_range() else {
            return;
        };
        let Some(document) = self.document.clone() else {
            return;
        };
        let default_dir = self
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("selection");
        let suggested_name = if force_utf8 {
            format!("{file_name}.selection.utf8.txt")
        } else {
            format!("{file_name}.selection.txt")
        };
        let prompt = cx.prompt_for_new_path(&default_dir, Some(&suggested_name));
        let encoded_plan: Option<EncodedSavePlan> = (!force_utf8)
            .then(|| {
                self.prepared_source
                    .as_ref()
                    .and_then(PreparedUtf8Source::save_plan)
            })
            .flatten();
        if let Some(cancellation) = self.selection_export_cancellation.take() {
            cancellation.cancel();
        }
        let cancellation = SearchCancellation::default();
        self.selection_export_cancellation = Some(cancellation.clone());
        self.selection_export_generation = self.selection_export_generation.wrapping_add(1);
        let generation = self.selection_export_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        let export_bytes = range.end.saturating_sub(range.start);
        self.metrics.export_requests = self.metrics.export_requests.saturating_add(1);
        self.selection_export_task = cx.spawn(async move |this, cx| {
            let path = match prompt.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) | Err(_) => {
                    let _ = this.update(cx, |view, _cx| {
                        if task_stamp.accepts_identity(view, view.selection_export_generation) {
                            view.selection_export_cancellation = None;
                        }
                    });
                    return;
                }
                Ok(Err(error)) => {
                    let _ = this.update(cx, |view, cx| {
                        if task_stamp.accepts_identity(view, view.selection_export_generation) {
                            view.selection_export_cancellation = None;
                            view.error = Some(error.to_string().into());
                            cx.notify();
                        }
                    });
                    return;
                }
            };
            let result = cx
                .background_spawn(async move {
                    if let Some(plan) = encoded_plan {
                        let encoding = plan.encoding_name();
                        document
                            .save_encoded_range_atomic_cancellable(
                                &plan,
                                range,
                                path,
                                &cancellation,
                            )
                            .map(|_| encoding)
                    } else {
                        document
                            .save_range_atomic_cancellable(range, path, &cancellation)
                            .map(|_| "UTF-8".to_owned())
                    }
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_identity(view, view.selection_export_generation) {
                    return;
                }
                view.selection_export_cancellation = None;
                match result {
                    Ok(encoding) => {
                        view.metrics.exported_bytes =
                            view.metrics.exported_bytes.saturating_add(export_bytes);
                        view.external_status =
                            Some(format!("Selection exported as {encoding}").into());
                        view.error = None;
                    }
                    Err(LargeDocumentError::UnrepresentableEncoding { encoding }) => {
                        view.error = Some(
                            format!(
                                "Selection cannot be represented in {encoding}; use ‘Export as UTF-8…’"
                            )
                            .into(),
                        );
                    }
                    Err(error) => view.error = Some(error.to_string().into()),
                }
                cx.notify();
            });
        });
    }

    pub(super) fn on_line_edit_event(
        &mut self,
        block: Entity<Block>,
        event: &BlockEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, BlockEvent::SelectionChanged) {
            self.sync_selection_from_active_source_block(&block, cx);
            return;
        }
        if matches!(event, BlockEvent::RequestRenderedSelectAll)
            && self
                .active_edit
                .as_ref()
                .is_some_and(|active| active.block == block)
        {
            self.active_edit = None;
            self.select_source_lines(0..self.line_count(), false);
            self.sync_source_selection_visuals(cx);
            cx.notify();
            return;
        }
        if !matches!(event, BlockEvent::Changed) {
            return;
        }
        if self.saving || self.reloading {
            return;
        }
        if self
            .suppressed_line_edit_text
            .as_deref()
            .is_some_and(|expected| expected == block.read(cx).display_text())
        {
            self.suppressed_line_edit_text = None;
            return;
        }
        let Some(active) = &self.active_edit else {
            return;
        };
        if active.block != block {
            return;
        }
        if block.read(cx).marked_range.is_some() {
            // IME composition belongs to the platform input transaction. Keep the
            // transient marked text in the mounted Source Block and commit it to
            // PieceTree/recovery only when the composition is finalized; otherwise
            // every pinyin candidate update would become a separate undo step.
            cx.notify();
            return;
        }
        let text = block.read(cx).display_text().to_owned();
        let caret_in_text = block.read(cx).selected_range.end.min(text.len());
        let range = active.range.clone();
        let ending = active.ending.clone();
        let replacement = format!("{text}{ending}");
        let recovery_selection = block.read(cx).selected_range.clone();
        let recovery_selection = u64::try_from(recovery_selection.start)
            .ok()
            .zip(u64::try_from(recovery_selection.end).ok())
            .and_then(|(start, end)| {
                let start =
                    SourceAnchor::new(range.start.checked_add(start)?, SourceAffinity::Before);
                let end = SourceAnchor::new(range.start.checked_add(end)?, SourceAffinity::After);
                Some(if block.read(cx).selection_reversed {
                    LargeRecoverySelection {
                        anchor: end,
                        head: start,
                    }
                } else {
                    LargeRecoverySelection {
                        anchor: start,
                        head: end,
                    }
                })
            });
        let Some(mut next_document) = self.document.clone() else {
            return;
        };
        match next_document.replace_text(range.clone(), &replacement) {
            Ok(()) => {
                // 先在持久根的廉价快照上验证范围与 UTF-8 边界，再追加恢复记录；
                // 失败输入不得留下一个正文从未接受过的 journal 事务。
                if let Some(journal) = self.recovery_journal.as_mut()
                    && let Err(error) = journal.record_replace(
                        range.clone(),
                        &replacement,
                        recovery_selection,
                        "source",
                    )
                {
                    self.recovery_error = Some(error.to_string().into());
                }
                let reanchored = text
                    .contains(['\r', '\n'])
                    .then(|| {
                        let caret_offset = range.start.saturating_add(caret_in_text as u64);
                        let line =
                            next_document.line_for_offset(caret_offset.min(next_document.len()))?;
                        let line_range = next_document.line_range(line)?;
                        let requested = caret_offset
                            .saturating_sub(line_range.start)
                            .saturating_sub(MAX_RENDERED_LINE_BYTES / 2);
                        let windowed = read_bounded_line_window(&next_document, line, requested)
                            .ok()
                            .flatten()?;
                        let caret = usize::try_from(
                            caret_offset.saturating_sub(windowed.content_range.start),
                        )
                        .ok()?
                        .min(windowed.text.len());
                        let window_start = windowed
                            .content_range
                            .start
                            .saturating_sub(line_range.start);
                        Some((usize::try_from(line).ok()?, windowed, caret, window_start))
                    })
                    .flatten();
                if let Some((line, windowed, caret, window_start)) = reanchored {
                    let line_text = windowed.text.to_string();
                    self.source_row_blocks
                        .retain(|_, candidate| *candidate != block);
                    self.source_row_blocks.insert(line, block.clone());
                    if let Some(active) = self.active_edit.as_mut() {
                        active.line = line;
                        active.range = windowed.replace_range;
                        active.ending = windowed.ending;
                        active.leading_truncated = windowed.leading_truncated;
                        active.trailing_truncated = windowed.trailing_truncated;
                    }
                    self.source_window_start = window_start;
                    self.suppressed_line_edit_text = Some(line_text.clone());
                    block.update(cx, |block, cx| {
                        let old_len = block.display_text().len();
                        block.replace_text_in_visible_range(
                            0..old_len,
                            &line_text,
                            Some(caret..caret),
                            false,
                            cx,
                        );
                    });
                    self.selection_anchor = Some(line);
                    self.selected_lines = Some(line..line.saturating_add(1));
                    self.scroll_handle
                        .scroll_to_item(line, ScrollStrategy::Center);
                } else if let Some(active) = self.active_edit.as_mut() {
                    active.range = range.start..range.start + replacement.len() as u64;
                }
                if let Some(selection) = recovery_selection {
                    next_document.set_source_selection(selection);
                }
                let dirty = !next_document.is_pristine();
                self.document = Some(next_document);
                self.dirty = dirty;
                self.tail_enabled = false;
                self.external_status = Some("Tailing paused after the first edit".into());
                self.view_mode = LargeViewMode::Source;
                self.structured_index = None;
                self.invalidate_structured_runtime();
                self.clear_structure_error();
                self.error = None;
                self.invalidate_source_rows();
                self.schedule_search(cx);
                cx.emit(DiskSourceEvent::StateChanged);
            }
            Err(error) => self.error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    pub(crate) fn on_undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving || self.reloading {
            return;
        }
        if self
            .document
            .as_mut()
            .is_some_and(|document| document.undo())
        {
            let restored_selection = self
                .document
                .as_ref()
                .map(LargeDocumentAdapter::source_selection);
            if let Some(journal) = &self.recovery_journal
                && let Err(error) = journal.record_undo(restored_selection, "source")
            {
                self.recovery_error = Some(error.to_string().into());
            }
            self.active_edit = None;
            if let Some(selection) = restored_selection {
                self.set_source_selection(selection, cx);
            }
            self.focus_handle.focus(window);
            self.invalidate_source_rows();
            self.dirty = self
                .document
                .as_ref()
                .is_some_and(|document| !document.is_pristine());
            self.schedule_search(cx);
            if self.dirty {
                self.structured_index = None;
                self.invalidate_structured_runtime();
            } else {
                self.rebuild_clean_structured_index(cx);
            }
            cx.emit(DiskSourceEvent::StateChanged);
            cx.notify();
        }
    }

    pub(crate) fn on_redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving || self.reloading {
            return;
        }
        if self
            .document
            .as_mut()
            .is_some_and(|document| document.redo())
        {
            let restored_selection = self
                .document
                .as_ref()
                .map(LargeDocumentAdapter::source_selection);
            if let Some(journal) = &self.recovery_journal
                && let Err(error) = journal.record_redo(restored_selection, "source")
            {
                self.recovery_error = Some(error.to_string().into());
            }
            self.active_edit = None;
            if let Some(selection) = restored_selection {
                self.set_source_selection(selection, cx);
            }
            self.focus_handle.focus(window);
            self.invalidate_source_rows();
            self.dirty = self
                .document
                .as_ref()
                .is_some_and(|document| !document.is_pristine());
            self.structured_index = None;
            self.invalidate_structured_runtime();
            self.clear_structure_error();
            self.schedule_search(cx);
            cx.emit(DiskSourceEvent::StateChanged);
            cx.notify();
        }
    }

    pub(super) fn reload_from_disk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reload_from_disk_with_encoding(None, window, cx);
    }

    pub(crate) fn reopen_with_encoding(
        &mut self,
        encoding: TextEncoding,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dirty {
            self.error =
                Some("Save or undo local edits before reopening with another encoding".into());
            cx.emit(DiskSourceEvent::StateChanged);
            cx.notify();
            return;
        }
        self.reload_from_disk_with_encoding(Some(encoding), window, cx);
    }

    pub(super) fn reload_from_disk_with_encoding(
        &mut self,
        forced_encoding: Option<TextEncoding>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.saving || self.reloading {
            return;
        }
        self.cancel_selection_transfers();
        let path = self.path.clone();
        #[cfg(not(test))]
        let recovery_dir = crate::config::GmarkConfigDirs::from_system()
            .ok()
            .map(|dirs| dirs.recovery_dir());
        #[cfg(test)]
        let recovery_dir: Option<PathBuf> = None;
        let window_handle = window.window_handle();
        if let Some(cancellation) = self.index_cancellation.take() {
            cancellation.cancel();
        }
        let cancellation = SearchCancellation::default();
        self.index_cancellation = Some(cancellation.clone());
        self.index_generation = self.index_generation.wrapping_add(1);
        let task_stamp = DocumentTaskStamp::capture(self, self.index_generation);
        self.external_generation = self.external_generation.wrapping_add(1);
        self.active_edit = None;
        self.reloading = true;
        self.error = None;
        cx.emit(DiskSourceEvent::StateChanged);
        self._index_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let original = FileSource::open(&path)?;
                    let mut probe = gmark_large_document::probe_file(
                        &path,
                        gmark_large_document::ProbeOptions::default(),
                    )?;
                    if let Some(encoding) = forced_encoding {
                        probe.encoding = encoding;
                    }
                    let reopened_encoding = text_encoding_label(&probe.encoding);
                    let recovery = recovery_dir.map(|dir| {
                        LargeRecoveryJournal::create(dir, &original, probe.encoding.clone())
                    });
                    let prepared = prepare_utf8_source(original, probe.encoding.clone())?;
                    let index = LineIndex::build_cancellable(prepared.source(), &cancellation)?;
                    let document = PieceDocument::open(prepared.source().clone(), index.clone())?;
                    let structured = build_structured_index(
                        prepared.source(),
                        &index,
                        probe.format.clone(),
                        &cancellation,
                    )?;
                    Ok::<_, gmark_large_document::LargeDocumentError>((
                        probe,
                        prepared,
                        index,
                        document,
                        structured,
                        recovery,
                        reopened_encoding,
                    ))
                })
                .await;
            let reloaded = result.is_ok();
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.index_generation) {
                    view.reloading = false;
                    return;
                }
                view.index_cancellation = None;
                view.reloading = false;
                match result {
                    Ok((
                        probe,
                        prepared,
                        index,
                        document,
                        structured,
                        recovery,
                        reopened_encoding,
                    )) => {
                        if let Some(journal) = view.recovery_journal.take()
                            && let Err(error) = journal.checkpoint()
                        {
                            view.recovery_error = Some(error.to_string().into());
                        }
                        view.probe = probe;
                        view.document_epoch = view.document_epoch.wrapping_add(1);
                        view.prepared_source = Some(prepared);
                        view.provisional_source = None;
                        view.index = Some(index);
                        view.document = Some(document.into());
                        view.invalidate_source_rows();
                        view.structured_index = structured;
                        view.invalidate_structured_runtime();
                        view.active_edit = None;
                        view.dirty = false;
                        view.pending_external_change = None;
                        view.external_monitor_paused = false;
                        view.external_status =
                            Some(format!("Reopened as {reopened_encoding}").into());
                        match recovery {
                            Some(Ok(journal)) => view.recovery_journal = Some(journal),
                            Some(Err(error)) => {
                                view.recovery_error = Some(error.to_string().into())
                            }
                            None => {}
                        }
                    }
                    Err(error) => view.error = Some(error.to_string().into()),
                }
                cx.emit(DiskSourceEvent::StateChanged);
                cx.notify();
            });
            if reloaded {
                let _ = cx.update_window(
                    window_handle,
                    |_view: AnyView, window: &mut Window, _cx: &mut App| {
                        window.set_window_edited(false);
                    },
                );
            }
        });
        cx.notify();
    }

    pub(super) fn keep_local_after_external_change(&mut self, cx: &mut Context<Self>) {
        self.pending_external_change = None;
        self.external_monitor_paused = true;
        self.tail_enabled = false;
        self.external_status =
            Some("Keeping local edits · use Save As to avoid overwriting the disk version".into());
        cx.emit(DiskSourceEvent::StateChanged);
        cx.notify();
    }

    pub(crate) fn on_save_document(
        &mut self,
        _: &SaveDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.external_monitor_paused {
            self.error = Some("Disk version changed; use Save As or Reload".into());
            cx.emit(DiskSourceEvent::StateChanged);
            cx.notify();
            return;
        }
        // 保存会卸载活动行 Block；先把焦点交还宿主，保存结束后快捷键仍能继续工作。
        self.focus_handle.focus(window);
        self.start_save(self.path.clone(), false, window.window_handle(), cx);
    }

    pub(crate) fn save_as_path(
        &mut self,
        path: PathBuf,
        window_handle: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        self.start_save(path, true, window_handle, cx);
    }

    pub(super) fn start_save(
        &mut self,
        path: PathBuf,
        save_as: bool,
        window_handle: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if self.saving || self.reloading || (!self.dirty && !save_as) {
            return;
        }
        if let Some(cancellation) = self.save_cancellation.take() {
            cancellation.cancel();
        }
        self.save_generation = self.save_generation.wrapping_add(1);
        let task_stamp = DocumentTaskStamp::capture(self, self.save_generation);
        let cancellation = SearchCancellation::default();
        self.save_cancellation = Some(cancellation.clone());
        // 保存会暂时取走 document 并重建 uniform_list 的数据源。必须保留底层像素偏移；
        // 近文件尾部用 scroll_to_item 恢复会再次经过估算布局，仍可能跳动数百行。
        let save_scroll_offset = self.scroll_handle.0.borrow().base_handle.offset();
        let Some(mut document) = self.document.take() else {
            self.save_cancellation = None;
            return;
        };
        let prepared_source = self.prepared_source.take();
        let encoded_save = prepared_source
            .as_ref()
            .and_then(PreparedUtf8Source::save_plan);
        // 直接 UTF-8 的 PreparedUtf8Source 仍持有目标文件；Windows 原子替换要求
        // 所有目标句柄先关闭。编码文档的 PreparedUtf8Source 指向影子文件，失败时需保留。
        let prepared_on_error = encoded_save.as_ref().and(prepared_source);
        self.provisional_source = None;
        if let Some(cancellation) = self.search_cancellation.take() {
            cancellation.cancel();
        }
        self.search_task = Task::ready(());
        self.source_task = Task::ready(());
        self.structured_task = Task::ready(());
        self.structured_filter_task = Task::ready(());
        self.json_expand_task = Task::ready(());
        self.external_generation = self.external_generation.wrapping_add(1);
        #[cfg(not(test))]
        let recovery_dir = crate::config::GmarkConfigDirs::from_system()
            .ok()
            .map(|dirs| dirs.recovery_dir());
        #[cfg(test)]
        let recovery_dir: Option<PathBuf> = None;
        // 保存期间主动结束行编辑并阻止新编辑，避免后台保存旧快照后覆盖用户在保存中
        // 继续输入的内容。大文件保存为流式任务，状态栏会明确显示 Saving…。
        self.active_edit = None;
        self.saving = true;
        self.error = None;
        cx.emit(DiskSourceEvent::StateChanged);
        self.save_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let save_result = if let Some(plan) = encoded_save {
                        if save_as {
                            document
                                .save_encoded_atomic_as_cancellable(&plan, &path, &cancellation)
                                .map(|_| ())
                        } else {
                            document
                                .save_encoded_atomic_cancellable(&plan, &path, &cancellation)
                                .map(|_| ())
                        }
                    } else {
                        document.save_atomic_cancellable(&path, &cancellation)
                    };
                    if let Err(error) = save_result {
                        return Err((document, prepared_on_error, error));
                    }
                    // 保存后从最终磁盘内容重新建立干净基线，清除旧 undo/add buffer，并恢复结构视图。
                    let rebuild = (|| {
                        let original = FileSource::open(&path)?;
                        let probe = gmark_large_document::probe_file(
                            &path,
                            gmark_large_document::ProbeOptions::default(),
                        )?;
                        let recovery = recovery_dir.map(|dir| {
                            LargeRecoveryJournal::create(dir, &original, probe.encoding.clone())
                        });
                        let prepared = prepare_utf8_source(original, probe.encoding.clone())?;
                        let index = LineIndex::build_cancellable(prepared.source(), &cancellation)?;
                        let clean_document =
                            PieceDocument::open(prepared.source().clone(), index.clone())?;
                        let structured = build_structured_index(
                            prepared.source(),
                            &index,
                            probe.format.clone(),
                            &cancellation,
                        );
                        Ok::<_, gmark_large_document::LargeDocumentError>((
                            clean_document,
                            prepared,
                            index,
                            structured,
                            recovery,
                            probe,
                            path,
                        ))
                    })();
                    rebuild.map_err(|error| (document, prepared_on_error, error))
                })
                .await;
            let saved = result.is_ok();
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_identity(view, view.save_generation) {
                    return;
                }
                view.save_cancellation = None;
                view.saving = false;
                match result {
                    Ok((document, prepared, index, structured, recovery, probe, path)) => {
                        // 保存后的干净 PieceTree 是新的磁盘身份基线；即使 revision 从零重新
                        // 开始，也不能接受旧基线上的搜索、复制或派生 projection 结果。
                        view.document_epoch = view.document_epoch.wrapping_add(1);
                        view.cancel_selection_transfers();
                        if let Some(journal) = view.recovery_journal.take()
                            && let Err(error) = journal.checkpoint()
                        {
                            view.recovery_error = Some(error.to_string().into());
                        }
                        view.document = Some(document.into());
                        view.prepared_source = Some(prepared);
                        view.provisional_source = None;
                        view.index = Some(index);
                        view.invalidate_source_rows();
                        view.probe = probe;
                        view.scroll_handle
                            .0
                            .borrow()
                            .base_handle
                            .set_offset(save_scroll_offset);
                        view.invalidate_structured_runtime();
                        match structured {
                            Ok(structured) => {
                                view.structured_index = structured;
                                view.clear_structure_error();
                            }
                            Err(error) => {
                                view.structured_index = None;
                                view.set_structure_error(error);
                            }
                        }
                        if let Some(recovery) = recovery {
                            match recovery {
                                Ok(journal) => {
                                    view.recovery_journal = Some(journal);
                                    view.recovery_error = None;
                                }
                                Err(error) => view.recovery_error = Some(error.to_string().into()),
                            }
                        }
                        view.active_edit = None;
                        view.dirty = false;
                        if save_as {
                            view.path = path.clone();
                            view.pending_external_change = None;
                            view.external_monitor_paused = false;
                            view.external_status = None;
                            cx.emit(DiskSourceEvent::SavedAs(path));
                        }
                    }
                    Err((document, prepared, error)) => {
                        view.document = Some(document);
                        view.prepared_source = prepared;
                        view.invalidate_source_rows();
                        view.scroll_handle
                            .0
                            .borrow()
                            .base_handle
                            .set_offset(save_scroll_offset);
                        view.error = Some(error.to_string().into());
                    }
                }
                cx.emit(DiskSourceEvent::StateChanged);
                cx.notify();
            });
            if saved {
                let _ = cx.update_window(
                    window_handle,
                    |_view: AnyView, window: &mut Window, _cx: &mut App| {
                        window.set_window_edited(false);
                    },
                );
            }
        });
        cx.notify();
    }
}
