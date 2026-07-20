// @author kongweiguang

use super::*;

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct LargeFileMetricsSnapshot {
    pub(crate) viewport_requests: u64,
    pub(crate) viewport_installs: u64,
    pub(crate) max_cached_rows: usize,
    pub(crate) blank_frames_after_content: u64,
}

impl DiskSourceAdapter {
    pub(crate) fn from_recovery(
        path: PathBuf,
        probe: OpenProbe,
        source: FileSource,
        journal_path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let fallback_source = source.clone();
        let fallback_encoding = probe.encoding.clone();
        let mut view = Self::new(path, probe, source, cx);
        // 替换普通索引任务；Task drop 会取消尚未发布的普通打开结果，恢复日志始终胜出。
        if let Some(cancellation) = view.index_cancellation.take() {
            cancellation.cancel();
        }
        let cancellation = SearchCancellation::default();
        view.index_cancellation = Some(cancellation.clone());
        view.index_generation = view.index_generation.wrapping_add(1);
        let task_stamp = DocumentTaskStamp::capture(&view, view.index_generation);
        view._index_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    match replay_large_recovery(&journal_path) {
                        Ok(recovered) => Ok((Some(recovered), None)),
                        Err(recovery_error) => {
                            if cancellation.is_cancelled() {
                                return Err(LargeDocumentError::Cancelled);
                            }
                            let prepared = prepare_utf8_source(fallback_source, fallback_encoding)?;
                            let index =
                                LineIndex::build_cancellable(prepared.source(), &cancellation)?;
                            let document =
                                PieceDocument::open(prepared.source().clone(), index.clone())?;
                            Ok::<_, gmark_large_document::LargeDocumentError>((
                                None,
                                Some((prepared, index, document, recovery_error)),
                            ))
                        }
                    }
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.index_generation) {
                    return;
                }
                view.index_cancellation = None;
                match result {
                    Ok((Some(recovered), _)) => {
                        let selection = recovered.selection;
                        let selected_line = selection.as_ref().and_then(|selection| {
                            recovered
                                .document
                                .line_for_offset(selection.range().end)
                                .and_then(|line| usize::try_from(line).ok())
                        });
                        view.index = Some(recovered.document.line_index());
                        let mut document = LargeDocumentAdapter::from(recovered.document);
                        if let Some(selection) = selection {
                            document.set_source_selection(selection);
                        }
                        view.view_state.source.selection = document.source_selection();
                        view.view_state.source.top_byte_anchor = document.source_selection().head;
                        view.document = Some(document);
                        view.prepared_source = Some(recovered.prepared_source);
                        view.provisional_source = None;
                        view.invalidate_source_rows();
                        view.recovery_journal = Some(recovered.journal);
                        view.recovery_error = (recovered.read_status
                            == gmark_large_document::LargeRecoveryReadStatus::TruncatedTail)
                            .then(|| {
                                "Recovered the last CRC-valid edit; incomplete tail ignored".into()
                            });
                        view.structured_index = None;
                        view.invalidate_structured_runtime();
                        view.structure_error = Some(
                            "Structured view is paused until recovered edits are saved".into(),
                        );
                        view.structure_error_byte = None;
                        view.view_mode = LargeViewMode::Source;
                        view.dirty = true;
                        view.tail_enabled = false;
                        if let Some(line) = selected_line {
                            view.selection_anchor = Some(line);
                            view.selected_lines = Some(line..line.saturating_add(1));
                            view.scroll_handle
                                .scroll_to_item(line, ScrollStrategy::Center);
                        }
                    }
                    Ok((None, Some((prepared, index, document, recovery_error)))) => {
                        view.index = Some(index);
                        view.document = Some(document.into());
                        view.prepared_source = Some(prepared);
                        view.provisional_source = None;
                        view.invalidate_source_rows();
                        view.recovery_error = Some(
                            format!("Recovery conflict retained on disk: {recovery_error}").into(),
                        );
                        view.view_mode = LargeViewMode::Source;
                        view.tail_enabled = false;
                    }
                    Ok((None, None)) => {}
                    Err(error) => view.error = Some(error.to_string().into()),
                }
                cx.notify();
            });
        });
        view
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn encoding_label(&self) -> String {
        text_encoding_label(&self.probe.encoding)
    }

    pub(crate) fn cursor_position(&self, cx: &App) -> (usize, usize) {
        if let Some(active) = &self.active_edit {
            let block = active.block.read(cx);
            let offset = block.selected_range.end.min(block.display_text().len());
            let column = block.display_text()[..offset]
                .chars()
                .count()
                .saturating_add(1);
            return (active.line.saturating_add(1), column);
        }
        let line = self
            .selected_lines
            .as_ref()
            .map_or(0, |selection| selection.start)
            .saturating_add(1);
        (line, 1)
    }

    pub(super) fn accessibility_caret(&self, cx: &App) -> (u64, usize) {
        if let Some(active) = &self.active_edit {
            let block = active.block.read(cx);
            let offset = block.selected_range.end.min(block.display_text().len());
            let column = unicode_segmentation::UnicodeSegmentation::graphemes(
                &block.display_text()[..offset],
                true,
            )
            .count();
            return (active.line as u64, column);
        }
        (
            self.selected_lines
                .as_ref()
                .map_or(0, |selection| selection.start) as u64,
            0,
        )
    }

    #[cfg(test)]
    pub(crate) fn has_structure_view(&self) -> bool {
        self.structured_index.is_some()
    }

    pub(crate) fn has_registered_structure_view(&self) -> bool {
        self.structured_index.is_some()
            && self
                .active_derived_view
                .as_ref()
                .and_then(|id| {
                    self.view_registry
                        .available_provider(id, &self.probe.format)
                })
                .is_some()
    }

    pub(crate) fn show_source_view(&mut self, cx: &mut Context<Self>) {
        self.mode_notice = None;
        self.set_view_mode(LargeViewMode::Source, cx);
        cx.emit(DiskSourceEvent::StateChanged);
    }

    pub(crate) fn show_structure_view(&mut self, cx: &mut Context<Self>) {
        self.mode_notice = None;
        self.request_registered_projection(cx);
        self.set_view_mode(LargeViewMode::Structure, cx);
        cx.emit(DiskSourceEvent::StateChanged);
    }

    pub(crate) fn show_split_view(&mut self, cx: &mut Context<Self>) {
        self.mode_notice = None;
        if self.structured_index.is_some() {
            self.request_registered_projection(cx);
            self.active_edit = None;
            self.view_mode = LargeViewMode::Split;
            cx.emit(DiskSourceEvent::StateChanged);
            cx.notify();
        } else {
            self.show_source_view(cx);
        }
    }

    pub(crate) fn structure_view_active(&self) -> bool {
        self.view_mode == LargeViewMode::Structure
    }

    pub(crate) fn structured_split_active(&self) -> bool {
        self.view_mode == LargeViewMode::Split
    }

    pub(crate) fn show_mode_unavailable(&mut self, mode: &'static str, cx: &mut Context<Self>) {
        self.view_mode = LargeViewMode::Source;
        self.mode_notice = Some(
            format!(
                "{mode} needs a resident Markdown projection; Source remains available for this file size"
            )
            .into(),
        );
        cx.emit(DiskSourceEvent::StateChanged);
        cx.notify();
    }

    pub(crate) fn follow_enabled(&self) -> bool {
        self.tail_enabled
    }

    pub(crate) fn line_endings_visible(&self) -> bool {
        self.show_line_endings
    }

    pub(crate) fn toggle_follow(&mut self, cx: &mut Context<Self>) {
        if self.dirty {
            self.external_status =
                Some("Save or discard edits before enabling log following".into());
        } else {
            self.tail_enabled = !self.tail_enabled;
            self.external_status = Some(
                if self.tail_enabled {
                    "Following appended content"
                } else {
                    "Log following paused"
                }
                .into(),
            );
        }
        cx.emit(DiskSourceEvent::StateChanged);
        cx.notify();
    }

    pub(crate) fn toggle_line_endings(&mut self, cx: &mut Context<Self>) {
        self.show_line_endings = !self.show_line_endings;
        cx.emit(DiskSourceEvent::StateChanged);
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn recovered_text_for_test(&self) -> Option<Vec<u8>> {
        let document = self.document.as_ref()?;
        document.read_range(0..document.len()).ok()
    }

    #[cfg(test)]
    pub(crate) fn has_recovery_journal_for_test(&self) -> bool {
        self.recovery_journal.is_some()
    }

    #[cfg(test)]
    pub(crate) fn json_visible_rows_for_test(&self) -> Option<u64> {
        let root = self.json_root_index()?;
        Some(self.json_visible_count(&[], root))
    }

    #[cfg(test)]
    pub(crate) fn structure_view_for_test(&self) -> bool {
        self.view_mode == LargeViewMode::Structure
    }

    #[cfg(test)]
    pub(crate) fn installed_projection_for_test(&self) -> Option<(u64, u64, u64)> {
        self.derived_projection_snapshot.as_ref().map(|snapshot| {
            (
                snapshot.document_epoch(),
                snapshot.revision(),
                snapshot.generation(),
            )
        })
    }

    #[cfg(test)]
    pub(crate) fn source_view_for_test(&self) -> bool {
        self.view_mode == LargeViewMode::Source
    }

    #[cfg(test)]
    pub(crate) fn source_cache_len_for_test(&self) -> usize {
        self.source_rows.len()
    }

    #[cfg(test)]
    pub(crate) fn source_list_window_for_test(&self) -> (usize, usize, usize) {
        (
            self.source_list_origin,
            self.source_list_len(),
            self.line_count(),
        )
    }

    #[cfg(test)]
    pub(crate) fn source_row_is_current_for_test(&self, line: usize) -> bool {
        self.source_rows.contains_key(&line)
            && self.source_row_epochs.get(&line) == Some(&self.source_cache_epoch)
    }

    #[cfg(test)]
    pub(crate) fn source_row_height_for_test(&self) -> f32 {
        self.source_row_height
    }

    #[cfg(test)]
    pub(crate) fn error_for_test(&self) -> Option<String> {
        self.error.as_ref().map(ToString::to_string)
    }

    #[cfg(test)]
    pub(crate) fn scroll_top_line_for_test(&self) -> usize {
        let handle = self.scroll_handle.0.borrow().base_handle.clone();
        self.source_list_origin.saturating_add(
            (-f32::from(handle.offset().y) / self.source_row_height.max(1.0))
                .max(0.0)
                .floor() as usize,
        )
    }

    #[cfg(test)]
    pub(crate) fn scroll_to_line_for_test(&self, line: usize) {
        let local = line.saturating_sub(self.source_list_origin);
        self.scroll_handle
            .scroll_to_item(local, ScrollStrategy::Top);
    }

    #[cfg(test)]
    pub(crate) fn scroll_page_for_test(&mut self, toward_end: bool, cx: &mut Context<Self>) {
        self.scroll_page(toward_end, cx);
    }

    #[cfg(test)]
    pub(crate) fn jump_bottom_for_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.on_jump_to_bottom(&JumpToBottom, window, cx);
    }

    #[cfg(test)]
    pub(crate) fn start_drag_autoscroll_for_test(&mut self, direction: i8, cx: &mut Context<Self>) {
        self.source_drag_anchor = self
            .document
            .as_ref()
            .map(LargeDocumentAdapter::source_selection)
            .map(|selection| selection.anchor);
        self.start_source_drag_autoscroll(direction, cx);
    }

    #[cfg(test)]
    pub(crate) fn drag_autoscroll_tick_for_test(&mut self, cx: &mut Context<Self>) -> bool {
        self.source_drag_autoscroll_tick(cx)
    }

    #[cfg(test)]
    pub(crate) fn begin_line_edit_for_test(
        &mut self,
        line: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_line_edit(line, window, cx);
    }

    #[cfg(test)]
    pub(crate) fn active_edit_for_test(&self) -> Option<(usize, Entity<Block>)> {
        self.active_edit
            .as_ref()
            .map(|edit| (edit.line, edit.block.clone()))
    }

    #[cfg(test)]
    pub(crate) fn select_lines_for_test(&mut self, lines: Range<usize>) {
        self.select_source_lines(lines, false);
        self.active_edit = None;
    }

    #[cfg(test)]
    pub(crate) fn select_source_range_for_test(&mut self, range: Range<u64>, reversed: bool) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        document.set_selection(range.clone(), reversed);
        let start_line = document
            .line_for_offset(range.start)
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or_default();
        let end_line = document
            .line_for_offset(range.end.saturating_sub(1))
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or(start_line);
        self.selection_anchor = Some(if reversed { end_line } else { start_line });
        self.selected_lines = Some(start_line..end_line.saturating_add(1));
        self.active_edit = None;
    }

    #[cfg(test)]
    pub(crate) fn source_selection_for_test(&self) -> Option<SourceSelection> {
        self.document
            .as_ref()
            .map(LargeDocumentAdapter::source_selection)
    }

    #[cfg(test)]
    pub(crate) fn source_row_block_count_for_test(&self) -> usize {
        self.source_row_blocks.len()
    }

    #[cfg(test)]
    pub(crate) fn source_row_block_for_test(&self, line: usize) -> Option<Entity<Block>> {
        self.source_row_blocks.get(&line).cloned()
    }

    #[cfg(test)]
    pub(crate) fn inactive_source_row_block_for_test(&self) -> Option<(usize, Entity<Block>)> {
        let active_line = self.active_edit.as_ref().map(|active| active.line);
        self.source_row_blocks
            .iter()
            .find(|(line, _)| Some(**line) != active_line)
            .map(|(line, block)| (*line, block.clone()))
    }

    #[cfg(test)]
    pub(crate) fn screen_lines_contract_for_test(
        &self,
    ) -> (u64, u64, u64, u64, Range<usize>, usize, bool, bool) {
        let screen = &self.displayed_screen_lines;
        let epochs_match = screen
            .rows
            .keys()
            .all(|line| self.source_row_epochs.get(line) == Some(&screen.cache_epoch));
        let revision_matches = screen.document_revision
            == self
                .document
                .as_ref()
                .map_or(0, LargeDocumentAdapter::revision);
        (
            screen.document_revision,
            screen.generation,
            screen.cache_epoch,
            screen.column_window_start,
            screen.visible.clone(),
            screen.rows.len(),
            epochs_match,
            revision_matches,
        )
    }

    #[cfg(test)]
    pub(crate) fn metrics_for_test(&self) -> LargeFileMetricsSnapshot {
        let metrics = self.metrics;
        LargeFileMetricsSnapshot {
            viewport_requests: metrics.viewport_requests,
            viewport_installs: metrics.viewport_installs,
            max_cached_rows: metrics.max_cached_rows,
            blank_frames_after_content: metrics.blank_frames_after_content,
        }
    }

    #[cfg(test)]
    pub(crate) fn viewport_cancellations_for_test(&self) -> u64 {
        self.metrics.viewport_cancellations
    }

    #[cfg(test)]
    pub(crate) fn export_selection_to_path_for_test(
        &self,
        path: &Path,
        force_utf8: bool,
    ) -> Result<String, LargeDocumentError> {
        let range = self
            .selected_source_byte_range()
            .ok_or_else(|| LargeDocumentError::InvalidTransaction("missing selection".into()))?;
        let document = self
            .document
            .as_ref()
            .ok_or_else(|| LargeDocumentError::InvalidTransaction("missing document".into()))?;
        if !force_utf8
            && let Some(plan) = self
                .prepared_source
                .as_ref()
                .and_then(PreparedUtf8Source::save_plan)
        {
            let encoding = plan.encoding_name();
            document.save_encoded_range_atomic_cancellable(
                &plan,
                range,
                path,
                &SearchCancellation::default(),
            )?;
            return Ok(encoding);
        }
        document.save_range_atomic_cancellable(range, path, &SearchCancellation::default())?;
        Ok("UTF-8".to_owned())
    }

    #[cfg(test)]
    pub(crate) fn source_context_menu_open_for_test(&self) -> bool {
        self.source_context_menu.is_some()
    }

    #[cfg(test)]
    pub(crate) fn source_context_menu_is_focused_for_test(&self, window: &Window) -> bool {
        self.source_context_menu_focus_handle.is_focused(window)
    }

    #[cfg(test)]
    pub(crate) fn copy_for_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.on_copy(&Copy, window, cx);
    }

    #[cfg(test)]
    pub(crate) fn cut_for_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.on_cut(&Cut, window, cx);
    }

    #[cfg(test)]
    pub(crate) fn paste_for_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.on_paste(&Paste, window, cx);
    }

    #[cfg(test)]
    pub(crate) fn undo_for_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.on_undo(&Undo, window, cx);
    }

    #[cfg(test)]
    pub(crate) fn redo_for_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.on_redo(&Redo, window, cx);
    }

    #[cfg(test)]
    pub(crate) fn close_navigation_for_test(&mut self, cx: &mut Context<Self>) {
        self.navigation_visible = false;
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn navigation_visible_for_test(&self) -> bool {
        self.navigation_visible
    }

    #[cfg(test)]
    pub(crate) fn search_text_for_test(&self, cx: &App) -> String {
        self.search_input.read(cx).display_text().to_owned()
    }

    #[cfg(test)]
    pub(crate) fn host_is_focused_for_test(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    #[cfg(test)]
    pub(crate) fn pending_external_change_for_test(&self) -> Option<ExternalChange> {
        self.pending_external_change.clone()
    }

    #[cfg(test)]
    pub(crate) fn external_monitor_paused_for_test(&self) -> bool {
        self.external_monitor_paused
    }

    #[cfg(test)]
    pub(crate) fn keep_local_for_test(&mut self, cx: &mut Context<Self>) {
        self.keep_local_after_external_change(cx);
    }

    #[cfg(test)]
    pub(crate) fn reload_from_disk_for_test(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reload_from_disk(window, cx);
    }

    #[cfg(test)]
    pub(crate) fn expand_json_row_for_test(&mut self, display_row: u64, cx: &mut Context<Self>) {
        self.activate_json_node(display_row, cx);
    }

    #[cfg(test)]
    pub(crate) fn markdown_table_state_for_test(&self) -> Option<(usize, usize, Vec<String>, u64)> {
        let StructuredIndex::MarkdownTables { tables, selected } =
            self.structured_index.as_ref()?
        else {
            return None;
        };
        let table = tables.get(*selected)?;
        Some((
            *selected,
            tables.len(),
            table.headers().to_vec(),
            table.row_count(),
        ))
    }

    #[cfg(test)]
    pub(crate) fn structured_column_window_for_test(&self) -> (usize, usize) {
        let materialized = self
            .structured_rows
            .values()
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(0);
        (self.structured_column_window_start, materialized)
    }

    #[cfg(test)]
    pub(crate) fn jump_structured_row_to_source_for_test(
        &mut self,
        row: u64,
        cx: &mut Context<Self>,
    ) {
        self.jump_structured_row_to_source(row, cx);
    }

    #[cfg(test)]
    pub(crate) fn structure_error_for_test(&self) -> Option<(String, Option<u64>)> {
        Some((
            self.structure_error.as_ref()?.to_string(),
            self.structure_error_byte,
        ))
    }

    /// 大文件的运行状态由 Editor 的统一状态栏承载，内容视图不再绘制第二条状态栏。
    pub(crate) fn status_text(&self) -> SharedString {
        if let Some(error) = &self.error {
            return error.clone();
        }
        if self.reloading {
            return "Reloading…".into();
        }
        if self.saving {
            return "Saving…".into();
        }
        if let Some(notice) = &self.mode_notice {
            return notice.clone();
        }
        if self.index.is_none() {
            return format!(
                "{:.1} MiB · Indexing…",
                self.probe.len as f64 / (1024.0 * 1024.0)
            )
            .into();
        }
        format!(
            "{:.1} MiB · {} lines",
            self.probe.len as f64 / (1024.0 * 1024.0),
            self.line_count()
        )
        .into()
    }

    pub(crate) fn accessibility_snapshot(
        &self,
        cx: &App,
    ) -> crate::accessibility::EditorAccessibilitySnapshot {
        let title = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled")
            .to_owned();
        let lines = self
            .displayed_screen_lines
            .rows
            .iter()
            .map(|(line, row)| (*line as u64, row.text.to_string()))
            .collect();
        let error = self
            .error
            .as_ref()
            .or(self.recovery_error.as_ref())
            .or(self.structure_error.as_ref())
            .map(ToString::to_string);
        crate::accessibility::EditorAccessibilitySnapshot {
            title,
            dirty: self.dirty,
            status: self.status_text().to_string(),
            error,
            busy: self.saving || self.reloading || self.index.is_none() || self.search_running,
            search_visible: self.search_visible,
            navigation_visible: self.navigation_visible,
            caret: Some(self.accessibility_caret(cx)),
            lines,
        }
    }

    pub(crate) fn accessibility_revision(&self) -> u64 {
        use std::hash::{Hash, Hasher};

        let flags = u64::from(self.dirty)
            | (u64::from(self.saving) << 1)
            | (u64::from(self.reloading) << 2)
            | (u64::from(self.search_running) << 3)
            | (u64::from(self.search_visible) << 4)
            | (u64::from(self.navigation_visible) << 5)
            | (u64::from(self.error.is_some()) << 6)
            | (u64::from(self.structure_error.is_some()) << 7)
            | (u64::from(self.recovery_error.is_some()) << 8);
        let row_signature = self
            .displayed_screen_lines
            .rows
            .first_key_value()
            .map_or(0, |(line, _)| *line as u64)
            .wrapping_mul(31)
            .wrapping_add(
                self.displayed_screen_lines
                    .rows
                    .last_key_value()
                    .map_or(0, |(line, _)| *line as u64),
            )
            .wrapping_mul(31)
            .wrapping_add(self.displayed_screen_lines.rows.len() as u64);
        let mut message_hasher = std::collections::hash_map::DefaultHasher::new();
        self.error.hash(&mut message_hasher);
        self.structure_error.hash(&mut message_hasher);
        self.recovery_error.hash(&mut message_hasher);
        self.mode_notice.hash(&mut message_hasher);
        self.external_status.hash(&mut message_hasher);
        self.displayed_screen_lines
            .cache_epoch
            .wrapping_mul(31)
            .wrapping_add(self.displayed_screen_lines.document_revision)
            .wrapping_mul(31)
            .wrapping_add(self.displayed_screen_lines.generation)
            .wrapping_mul(31)
            .wrapping_add(self.displayed_screen_lines.column_window_start)
            .wrapping_mul(31)
            .wrapping_add(row_signature)
            .wrapping_mul(31)
            .wrapping_add(self.search_generation)
            .wrapping_mul(31)
            .wrapping_add(self.external_generation)
            .wrapping_mul(31)
            .wrapping_add(message_hasher.finish())
            .wrapping_mul(512)
            .wrapping_add(flags)
    }

    pub(crate) fn activate_accessibility_error(&mut self, cx: &mut Context<Self>) {
        if let Some(offset) = self.structure_error_byte {
            self.jump_byte_offset_to_source(offset, cx);
        }
    }

    pub(super) fn start_external_monitor(&mut self, cx: &mut Context<Self>) {
        self.external_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let snapshot = this.update(cx, |view, _cx| {
                    view.document
                        .clone()
                        .zip(view.index.clone())
                        .map(|(document, index)| {
                            let task_stamp =
                                DocumentTaskStamp::capture(view, view.external_generation);
                            (
                                document,
                                index,
                                view.path.clone(),
                                view.dirty,
                                view.tail_enabled,
                                view.external_monitor_paused,
                                task_stamp,
                                view.lifetime_cancellation.clone(),
                                view.probe.format.clone(),
                            )
                        })
                });
                let Ok(Some((
                    document,
                    index,
                    path,
                    dirty,
                    tail_enabled,
                    monitor_paused,
                    task_stamp,
                    cancellation,
                    format,
                ))) = snapshot
                else {
                    continue;
                };
                if monitor_paused {
                    continue;
                }
                let result = cx
                    .background_spawn(async move {
                        let change = document.external_change()?;
                        if matches!(change, ExternalChange::Appended { .. })
                            && !dirty
                            && tail_enabled
                        {
                            let source = FileSource::open(&path)?;
                            let previous_line_count = index.line_count();
                            let extended =
                                index.extend_for_append_cancellable(&source, &cancellation)?;
                            let structured = if matches!(format, DocumentFormat::JsonLines) {
                                validate_json_lines_from_cancellable(
                                    &source,
                                    &extended,
                                    previous_line_count.saturating_sub(1),
                                    &cancellation,
                                )
                                .map(|()| {
                                    Some(StructuredIndex::JsonLines {
                                        lines: extended.clone(),
                                        source: source.clone(),
                                        record_count: json_lines_record_count(&extended),
                                    })
                                })
                            } else {
                                build_structured_index(&source, &extended, format, &cancellation)
                            };
                            Ok::<_, gmark_large_document::LargeDocumentError>((
                                change,
                                Some((source, extended, structured)),
                            ))
                        } else {
                            Ok((change, None))
                        }
                    })
                    .await;
                let _ = this.update(cx, |view, cx| {
                    // 保存/重载可能在磁盘检查期间安装了新基线；旧结果不得覆盖新文档状态。
                    if !task_stamp.accepts_strict(view, view.external_generation) {
                        return;
                    }
                    let state_changed = !matches!(&result, Ok((ExternalChange::Unchanged, _)));
                    if state_changed {
                        view.cancel_selection_transfers();
                    }
                    match result {
                        Ok((ExternalChange::Unchanged, _)) => {}
                        Ok((
                            ExternalChange::Appended { from, to },
                            Some((source, index, structured)),
                        )) if !view.dirty && view.tail_enabled => {
                            if let Some(document) = view.document.as_mut() {
                                match document.accept_external_append(source, index.clone()) {
                                    Ok(()) => {
                                        view.index = Some(index);
                                        view.invalidate_source_rows();
                                        view.invalidate_structured_runtime();
                                        match structured {
                                            Ok(index) => {
                                                view.structured_index = index;
                                                view.clear_structure_error();
                                            }
                                            Err(error) => {
                                                view.structured_index = None;
                                                view.set_structure_error(error);
                                            }
                                        }
                                        view.external_status = Some(
                                            format!(
                                                "Following log · appended {:.1} KiB",
                                                (to - from) as f64 / 1024.0
                                            )
                                            .into(),
                                        );
                                        view.pending_external_change = None;
                                        view.schedule_search(cx);
                                        if let Some(last) = view.line_count().checked_sub(1) {
                                            view.scroll_source_line_strict(
                                                last,
                                                ScrollStrategy::Bottom,
                                            );
                                        }
                                    }
                                    Err(error) => {
                                        view.external_status = Some(error.to_string().into())
                                    }
                                }
                            }
                        }
                        Ok((change @ ExternalChange::Appended { .. }, _)) => {
                            view.pending_external_change = Some(change);
                            view.external_status = Some(
                                if view.dirty {
                                    "File grew on disk · tailing paused because this tab has edits"
                                } else {
                                    "File grew on disk · enable Follow to append new content"
                                }
                                .into(),
                            );
                        }
                        Ok((change @ ExternalChange::Truncated { .. }, _)) => {
                            view.pending_external_change = Some(change);
                            view.external_status =
                                Some("File was truncated on disk · reload required".into());
                        }
                        Ok((ExternalChange::Replaced, _)) => {
                            view.pending_external_change = Some(ExternalChange::Replaced);
                            view.external_status =
                                Some("File was replaced on disk · reload required".into());
                        }
                        Ok((ExternalChange::Modified, _)) => {
                            view.pending_external_change = Some(ExternalChange::Modified);
                            view.external_status =
                                Some("File changed on disk · reload required".into());
                        }
                        Err(error) => view.external_status = Some(error.to_string().into()),
                    }
                    if state_changed {
                        cx.emit(DiskSourceEvent::StateChanged);
                        cx.notify();
                    }
                });
            }
        });
    }
}
