// @author kongweiguang

use super::*;
use crate::document_runtime::map_persistence_error;
use std::io::Write as _;

impl DocumentHost {
    pub(super) fn begin_structured_cell_edit(
        &mut self,
        record: Option<u64>,
        column: usize,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view_mode != DocumentHostViewMode::Live || !self.is_delimited_document() {
            return;
        }
        self.structured_cell_edit = Some(StructuredCellEdit { record, column });
        let host = cx.entity().downgrade();
        self.structured_cell_input.update(cx, move |input, cx| {
            input.set_host_action_handler(move |action, window, cx| {
                let _ = host.update(cx, |view, cx| {
                    view.on_structured_cell_host_action(action, window, cx)
                });
            });
            let len = input.display_text().len();
            input.replace_text_in_visible_range(0..len, &value, None, false, cx);
            input.focus_handle.focus(window);
        });
        cx.notify();
    }

    pub(super) fn select_structured_cell(
        &mut self,
        target: StructuredCellEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view_mode == DocumentHostViewMode::Source {
            return;
        }
        if self
            .structured_cell_edit
            .is_some_and(|editing| editing != target)
        {
            // 点击另一格会先把焦点交还表格；必须在旧编辑器失焦前提交其权威文本，
            // 否则下一次渲染只会重新读取索引中的旧值，造成用户输入静默丢失。
            let value = self
                .structured_cell_input
                .read(cx)
                .display_text()
                .to_owned();
            self.commit_structured_cell_edit(value, cx);
        }
        self.structured_selected_cell = Some(target);
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub(super) fn on_structured_table_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = self.structured_selected_cell else {
            return;
        };
        match event.keystroke.key.as_str() {
            "enter" => {
                if let Some(value) = self.structured_cell_value(selected) {
                    self.begin_structured_cell_edit(
                        selected.record,
                        selected.column,
                        value,
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
            }
            "tab" => {
                let Some(StructuredIndex::Delimited(index)) = self.structured_index.as_ref() else {
                    return;
                };
                let columns = index.column_count().max(1);
                let slots = columns.saturating_mul(index.record_count() as usize + 1);
                if slots == 0 {
                    return;
                }
                let current = selected.record.map_or(selected.column, |record| {
                    columns.saturating_add(record as usize * columns + selected.column)
                });
                let next = if event.keystroke.modifiers.shift {
                    (current + slots - 1) % slots
                } else {
                    (current + 1) % slots
                };
                self.structured_selected_cell = Some(if next < columns {
                    StructuredCellEdit {
                        record: None,
                        column: next,
                    }
                } else {
                    StructuredCellEdit {
                        record: Some(((next - columns) / columns) as u64),
                        column: (next - columns) % columns,
                    }
                });
                cx.stop_propagation();
                cx.notify();
            }
            "escape" => {
                self.structured_cell_edit = None;
                cx.stop_propagation();
                cx.notify();
            }
            _ => {}
        }
    }

    fn structured_cell_value(&self, target: StructuredCellEdit) -> Option<String> {
        if let Some(value) = self.structured_cell_overrides.get(&target) {
            return Some(value.clone());
        }
        let StructuredIndex::Delimited(index) = self.structured_index.as_ref()? else {
            return None;
        };
        if let Some(record) = target.record {
            index
                .read_records(record, 1)
                .ok()?
                .pop()?
                .fields
                .get(target.column)
                .cloned()
                .or_else(|| Some(String::new()))
        } else {
            index.headers().get(target.column).cloned()
        }
    }

    /// 派生预览只复制用户实际选中的单元格；CSV 使用索引读取完整字段，
    /// 其余结构视图使用当前受限视口快照，避免一次复制触发无界文件扫描。
    fn selected_structured_cell_text(&self) -> Option<String> {
        let target = self.structured_selected_cell?;
        if matches!(self.structured_index, Some(StructuredIndex::Delimited(_))) {
            return self.structured_cell_value(target);
        }
        if target.record.is_none() {
            return self
                .structured_index
                .as_ref()?
                .headers()
                .get(target.column)
                .cloned();
        }
        let record = target.record?;
        self.structured_rows
            .values()
            .find(|row| row.index == record)
            .or_else(|| self.json_rows.values().find(|row| row.index == record))
            .and_then(|row| {
                target
                    .column
                    .checked_sub(row.column_start)
                    .map(|index| (row, index))
            })
            .and_then(|(row, index)| row.cells.get(index))
            .cloned()
    }

    fn on_structured_cell_host_action(
        &mut self,
        action: BlockHostAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            BlockHostAction::Submit(value) => {
                self.commit_structured_cell_edit(value.to_string(), cx);
                self.focus_handle.focus(window);
            }
            BlockHostAction::DismissTransientUi => {
                self.structured_cell_edit = None;
                self.focus_handle.focus(window);
                cx.notify();
            }
            _ => {}
        }
    }

    fn commit_structured_cell_edit(&mut self, value: String, cx: &mut Context<Self>) {
        let Some(target) = self.structured_cell_edit.take() else {
            return;
        };
        let Some(StructuredIndex::Delimited(index)) = self.structured_index.as_ref() else {
            return;
        };
        let record = if let Some(record) = target.record {
            index
                .read_records(record, 1)
                .ok()
                .and_then(|mut rows| rows.pop())
        } else {
            index.read_header().ok().flatten()
        };
        let Some(mut record) = record else {
            return;
        };
        let baseline_range = record.byte_range.clone();
        for (edited, override_value) in &self.structured_cell_overrides {
            if edited.record == target.record {
                record
                    .fields
                    .resize(index.column_count().max(edited.column + 1), String::new());
                record.fields[edited.column] = override_value.clone();
            }
        }
        record
            .fields
            .resize(index.column_count().max(target.column + 1), String::new());
        record.fields[target.column] = value.clone();
        let current_range = self.current_structured_record_range(&baseline_range);
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let terminator = document
            .read_range(current_range.clone())
            .ok()
            .map(|bytes| delimited_record_terminator(&bytes))
            .unwrap_or("\n");
        let replacement = serialize_delimited_record(&record.fields, index.delimiter(), terminator);
        if self.replace_delimited_table_source_range(baseline_range.clone(), &replacement, cx) {
            self.structured_cell_overrides.insert(target, value);
        }
    }

    /// 结构索引中的区间属于本轮连续编辑开始前的基线。后台重建完成前只需累加
    /// 之前整条记录替换造成的偏移，即可继续安全编辑相邻行或同一行的其他列。
    fn current_structured_record_range(&self, baseline: &Range<u64>) -> Range<u64> {
        let mut shift_before = 0i128;
        let mut shift_inside = 0i128;
        for (edited, delta) in &self.structured_cell_source_edits {
            if edited.end <= baseline.start {
                shift_before += i128::from(*delta);
            } else if edited == baseline {
                shift_inside += i128::from(*delta);
            }
        }
        let shift = |value: u64, delta: i128| {
            if delta >= 0 {
                value.saturating_add(u64::try_from(delta).unwrap_or(u64::MAX))
            } else {
                value.saturating_sub(u64::try_from(-delta).unwrap_or(u64::MAX))
            }
        };
        shift(baseline.start, shift_before)
            ..shift(baseline.end, shift_before.saturating_add(shift_inside))
    }

    pub(super) fn insert_delimited_row(&mut self, before: u64, cx: &mut Context<Self>) {
        let Some(StructuredIndex::Delimited(index)) = self.structured_index.as_ref() else {
            return;
        };
        let count = index.record_count();
        let before = before.min(count);
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let fields = vec![String::new(); index.column_count().max(1)];
        if document.is_empty() && index.column_count() == 0 {
            let replacement = format!(
                "{}{}",
                serialize_delimited_record(
                    &[cx.global::<I18nManager>()
                        .strings()
                        .large_document_text("default_column_template")
                        .replace("{number}", "1")],
                    index.delimiter(),
                    "\n",
                ),
                serialize_delimited_record(&fields, index.delimiter(), "")
            );
            self.replace_delimited_table_source_range(0..0, &replacement, cx);
            return;
        }
        let (offset, prefix, terminator) = if before < count {
            let Some(row) = index
                .read_records(before, 1)
                .ok()
                .and_then(|mut rows| rows.pop())
            else {
                return;
            };
            let current_range = self.current_structured_record_range(&row.byte_range);
            let terminator = document
                .read_range(current_range)
                .ok()
                .map(|bytes| delimited_record_terminator(&bytes))
                .unwrap_or("\n");
            (row.byte_range.start, "", terminator)
        } else {
            let len = document.len();
            let trailing = (len > 0)
                .then(|| document.read_range(len.saturating_sub(2)..len).ok())
                .flatten()
                .unwrap_or_default();
            if trailing.ends_with(b"\n") || trailing.ends_with(b"\r") {
                (len, "", delimited_record_terminator(&trailing))
            } else if len > 0 {
                (len, "\n", "")
            } else {
                (0, "", "")
            }
        };
        let mut replacement = prefix.to_owned();
        replacement.push_str(&serialize_delimited_record(
            &fields,
            index.delimiter(),
            terminator,
        ));
        self.replace_delimited_table_source_range(offset..offset, &replacement, cx);
    }

    pub(super) fn delete_delimited_row(&mut self, record: u64, cx: &mut Context<Self>) {
        let Some(StructuredIndex::Delimited(index)) = self.structured_index.as_ref() else {
            return;
        };
        let Some(row) = index
            .read_records(record, 1)
            .ok()
            .and_then(|mut rows| rows.pop())
        else {
            return;
        };
        self.replace_delimited_table_source_range(row.byte_range, "", cx);
    }

    pub(super) fn transform_delimited_column(
        &mut self,
        edit: DelimitedEdit,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.document.clone() else {
            return;
        };
        let DocumentFormat::Delimited { delimiter } = self.probe.format else {
            return;
        };
        if let Some(cancellation) = self.structured_cancellation.take() {
            cancellation.cancel();
        }
        self.structured_generation = self.structured_generation.wrapping_add(1);
        let generation = self.structured_generation;
        let base_revision = document.revision();
        let total = self
            .structured_index
            .as_ref()
            .map_or(0, |index| index.row_count().saturating_add(1));
        let progress = Arc::new(AtomicU64::new(0));
        self.structured_column_progress = Some((progress.clone(), total));
        let cancellation = SearchCancellation::default();
        self.structured_cancellation = Some(cancellation.clone());
        self.structure_error = Some(
            cx.global::<I18nManager>()
                .strings()
                .large_document_text("updating_columns")
                .into(),
        );
        self.structured_progress_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                let running = this
                    .update(cx, |view, cx| {
                        let running = view.structured_column_progress.is_some();
                        if running {
                            cx.notify();
                        }
                        running
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        });
        self.structured_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    transform_delimited_adapter(document, delimiter, edit, &cancellation, &progress)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.structured_generation != generation
                    || view.document.as_ref().map(DocumentSession::revision) != Some(base_revision)
                    || view.coordinator.pending_external_change.is_some()
                {
                    view.structured_column_progress = None;
                    return;
                }
                view.structured_cancellation = None;
                view.structured_column_progress = None;
                match result {
                    Ok(document) => view.install_delimited_transformation(document, cx),
                    Err(PagedDocumentError::Cancelled) => {}
                    Err(error) => view.set_structure_error(error, cx),
                }
            });
        });
    }

    pub(super) fn cancel_delimited_column_transform(&mut self, cx: &mut Context<Self>) {
        if let Some(cancellation) = self.structured_cancellation.take() {
            cancellation.cancel();
        }
        self.structured_generation = self.structured_generation.wrapping_add(1);
        self.structured_column_progress = None;
        self.clear_structure_error();
        cx.notify();
    }

    fn install_delimited_transformation(
        &mut self,
        mut next_document: DocumentSession,
        cx: &mut Context<Self>,
    ) {
        self.active_edit = None;
        self.structured_cell_edit = None;
        next_document.dirty = !next_document.is_pristine();
        self.install_document_session(next_document);
        self.tail_enabled = false;
        let preserve_live_table = matches!(
            self.view_mode,
            DocumentHostViewMode::Live | DocumentHostViewMode::Split
        ) && self.structured_index.is_some();
        if preserve_live_table {
            // 列变换已在新文档中完成；旧表格只负责撑住当前帧，直到新索引和可见行
            // 原子安装，期间不能退回 Source。
            self.structured_pending = None;
            self.structured_cell_overrides.clear();
            self.structured_cell_source_edits.clear();
            self.hidden_structured_columns.clear();
            self.structured_column_window_start = 0;
        } else {
            self.structured_index = None;
            self.invalidate_structured_runtime();
        }
        self.invalidate_source_rows();
        self.schedule_search(cx);
        self.schedule_delimited_snapshot_rebuild(cx);
        if preserve_live_table {
            self.clear_structure_error();
        }
        cx.emit(DocumentHostEvent::StateChanged);
        cx.notify();
    }

    fn schedule_delimited_snapshot_rebuild(&mut self, cx: &mut Context<Self>) {
        if !self.is_delimited_document() {
            return;
        }
        if let Some(cancellation) = self.structured_cancellation.take() {
            cancellation.cancel();
        }
        let Some(document) = self.document.clone() else {
            return;
        };
        let DocumentFormat::Delimited { delimiter } = self.probe.format else {
            return;
        };
        self.structured_generation = self.structured_generation.wrapping_add(1);
        let generation = self.structured_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        let cancellation = SearchCancellation::default();
        let loaded_rows = self.structured_rows.keys().copied().collect::<Vec<_>>();
        let visible_columns = self.structured_column_window_start
            ..self
                .structured_column_window_start
                .saturating_add(STRUCTURED_COLUMN_WINDOW);
        self.structured_cancellation = Some(cancellation.clone());
        self.structure_error = Some(
            cx.global::<I18nManager>()
                .strings()
                .large_document_text("refreshing_table")
                .into(),
        );
        self.structured_task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(200))
                .await;
            let result = cx
                .background_spawn(async move {
                    if cancellation.is_cancelled() {
                        return Err(PagedDocumentError::Cancelled);
                    }
                    let bytes: Arc<[u8]> = document
                        .snapshot()
                        .read_range(0..document.len())
                        .map_err(|error| PagedDocumentError::InvalidTransaction(error.to_string()))?
                        .into();
                    let index = DelimitedIndex::build_snapshot_cancellable(
                        bytes,
                        DelimitedIndexOptions {
                            delimiter,
                            ..DelimitedIndexOptions::default()
                        },
                        &cancellation,
                    )?;
                    let index = StructuredIndex::Delimited(index);
                    let mut refreshed_rows = BTreeMap::new();
                    for row in loaded_rows {
                        if cancellation.is_cancelled() {
                            return Err(PagedDocumentError::Cancelled);
                        }
                        refreshed_rows.extend(
                            index
                                .read_rows(row, 1, visible_columns.clone())?
                                .into_iter()
                                .map(|row| (row.index, row)),
                        );
                    }
                    Ok::<_, PagedDocumentError>((index, refreshed_rows))
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.structured_generation) {
                    return;
                }
                view.structured_cancellation = None;
                match result {
                    Ok((index, refreshed_rows)) => {
                        view.structured_index = Some(index);
                        // 新索引与当前视口行必须同一帧安装；先清空再异步读取会让一次
                        // 普通单元格点击短暂退回“加载中”，形成整表闪烁。
                        view.structured_rows = refreshed_rows;
                        view.structured_pending = None;
                        view.structured_cell_overrides.clear();
                        view.structured_cell_source_edits.clear();
                        view.clear_structure_error();
                    }
                    Err(PagedDocumentError::Cancelled) => {}
                    Err(error) => view.set_structure_error(error, cx),
                }
                cx.emit(DocumentHostEvent::StateChanged);
                cx.notify();
            });
        });
    }

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
        let copying_zh = I18nStrings::zh_cn().large_document_text("copying_selection");
        let copying_en = I18nStrings::en_us().large_document_text("copying_selection");
        if self
            .coordinator
            .external_status
            .as_ref()
            .is_some_and(|status| {
                status.as_ref() == copying_zh.as_str() || status.as_ref() == copying_en.as_str()
            })
        {
            self.coordinator.external_status = None;
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
        self.install_source_replacement(next_document, range, replacement, true, true, cx);
        true
    }

    /// CSV 表格操作在旧索引追平前继续使用其基线坐标；源码 transaction 成功后
    /// 同步记录长度变化，让紧接着发生的单元格、行操作仍能命中当前正文。
    fn replace_delimited_table_source_range(
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
        self.install_source_replacement(next_document, range, replacement, true, false, cx);
        true
    }

    fn install_source_replacement(
        &mut self,
        mut next_document: DocumentSession,
        range: Range<u64>,
        replacement: &str,
        preserve_view: bool,
        preserve_structure: bool,
        cx: &mut Context<Self>,
    ) {
        let caret = range.start.saturating_add(replacement.len() as u64);
        let selection = Some(PagedRecoverySelection::collapsed(
            caret,
            SourceAffinity::After,
        ));
        if let Some(journal) = self.coordinator.recovery_journal.as_mut()
            && let Err(error) = record_recovery_transaction(
                journal,
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
            self.sync_session_active_view();
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
                Ok(Err(_error)) => {
                    let _ = this.update(cx, |view, cx| {
                        if task_stamp.accepts_identity(view, view.selection_export_generation) {
                            view.selection_export_cancellation = None;
                            view.error = Some(
                                cx.global::<I18nManager>()
                                    .strings()
                                    .large_document_text("error_export_selection")
                                    .into(),
                            );
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
                        view.coordinator.external_status = Some(
                            cx.global::<I18nManager>()
                                .strings()
                                .large_document_text("selection_exported_template")
                                .replace("{encoding}", &encoding)
                                .into(),
                        );
                        view.error = None;
                    }
                    Err(PagedDocumentError::UnrepresentableEncoding { encoding }) => {
                        view.error = Some(
                            cx.global::<I18nManager>()
                                .strings()
                                .large_document_text("selection_encoding_error_template")
                                .replace("{encoding}", &encoding)
                                .into(),
                        );
                    }
                    Err(error) => view.error = Some(localized_document_error(&error, cx)),
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
                    PagedRecoverySelection {
                        anchor: end,
                        head: start,
                    }
                } else {
                    PagedRecoverySelection {
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
                if let Some(journal) = self.coordinator.recovery_journal.as_mut()
                    && let Err(error) = record_recovery_transaction(
                        journal,
                        next_document.revision().saturating_sub(1),
                        range.clone(),
                        replacement.as_str(),
                        recovery_selection,
                        DocumentViewId::source(),
                    )
                {
                    self.coordinator.recovery_error = Some(error.to_string().into());
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
                next_document.dirty = dirty;
                self.install_document_session(next_document);
                self.tail_enabled = false;
                self.coordinator.external_status = Some(
                    cx.global::<I18nManager>()
                        .strings()
                        .large_document_text("tailing_paused_after_edit")
                        .into(),
                );
                let preserve_json_split = self.probe.format == DocumentFormat::Json
                    && self.view_mode == DocumentHostViewMode::Split;
                if !preserve_json_split {
                    self.view_mode = DocumentHostViewMode::Source;
                    self.sync_session_active_view();
                }
                self.structured_index = None;
                self.invalidate_structured_runtime();
                self.clear_structure_error();
                self.error = None;
                self.invalidate_source_rows();
                self.schedule_search(cx);
                self.schedule_json_graph_projection(cx);
                cx.emit(DocumentHostEvent::StateChanged);
            }
            Err(error) => self.error = Some(localized_document_error(&error, cx)),
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
                .map(DocumentSession::source_selection);
            if let Some(journal) = self.coordinator.recovery_journal.as_mut()
                && let Err(error) = journal.record(&RecoveryRecord {
                    action: RecoveryAction::Undo,
                    selection: restored_selection,
                    view_id: DocumentViewId::source(),
                })
            {
                self.coordinator.recovery_error = Some(error.to_string().into());
            }
            self.active_edit = None;
            if let Some(selection) = restored_selection {
                self.set_source_selection(selection, cx);
            }
            self.focus_handle.focus(window);
            self.invalidate_source_rows();
            let dirty = self
                .document
                .as_ref()
                .is_some_and(|document| !document.is_pristine());
            set_document_dirty_state(&mut self.document, &mut self.pending_dirty, dirty);
            self.schedule_search(cx);
            let preserve_live_table = self.is_delimited_document()
                && matches!(
                    self.view_mode,
                    DocumentHostViewMode::Live | DocumentHostViewMode::Split
                )
                && self.structured_index.is_some();
            if preserve_live_table {
                self.structured_pending = None;
                self.structured_cell_overrides.clear();
                self.structured_cell_source_edits.clear();
                self.schedule_delimited_snapshot_rebuild(cx);
                self.clear_structure_error();
            } else if dirty {
                self.structured_index = None;
                self.invalidate_structured_runtime();
            } else {
                self.rebuild_clean_structured_index(cx);
            }
            self.schedule_json_graph_projection(cx);
            if dirty && !preserve_live_table {
                self.schedule_delimited_snapshot_rebuild(cx);
            }
            cx.emit(DocumentHostEvent::StateChanged);
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
                .map(DocumentSession::source_selection);
            if let Some(journal) = self.coordinator.recovery_journal.as_mut()
                && let Err(error) = journal.record(&RecoveryRecord {
                    action: RecoveryAction::Redo,
                    selection: restored_selection,
                    view_id: DocumentViewId::source(),
                })
            {
                self.coordinator.recovery_error = Some(error.to_string().into());
            }
            self.active_edit = None;
            if let Some(selection) = restored_selection {
                self.set_source_selection(selection, cx);
            }
            self.focus_handle.focus(window);
            self.invalidate_source_rows();
            let dirty = self
                .document
                .as_ref()
                .is_some_and(|document| !document.is_pristine());
            set_document_dirty_state(&mut self.document, &mut self.pending_dirty, dirty);
            let preserve_live_table = self.is_delimited_document()
                && matches!(
                    self.view_mode,
                    DocumentHostViewMode::Live | DocumentHostViewMode::Split
                )
                && self.structured_index.is_some();
            if preserve_live_table {
                self.structured_pending = None;
                self.structured_cell_overrides.clear();
                self.structured_cell_source_edits.clear();
            } else {
                self.structured_index = None;
                self.invalidate_structured_runtime();
            }
            self.clear_structure_error();
            self.schedule_search(cx);
            self.schedule_json_graph_projection(cx);
            self.schedule_delimited_snapshot_rebuild(cx);
            if preserve_live_table {
                self.clear_structure_error();
            }
            cx.emit(DocumentHostEvent::StateChanged);
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
        if document_dirty_state(&self.document, &self.pending_dirty) {
            self.error = Some(
                cx.global::<I18nManager>()
                    .strings()
                    .large_document_text("reopen_dirty_error")
                    .into(),
            );
            cx.emit(DocumentHostEvent::StateChanged);
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
        #[cfg(test)]
        let configured_loading = gmark_document_core::LoadingPolicy::default();
        #[cfg(not(test))]
        let configured_loading = crate::config::read_app_preferences()
            .map(|preferences| preferences.document_loading.policy())
            .unwrap_or_default();
        let loading = if forced_encoding.is_some() {
            gmark_document_core::LoadingPolicy {
                max_resident_bytes: Some(self.probe.options.max_resident_bytes),
                max_resident_lines: Some(self.probe.options.max_resident_lines),
                max_structural_units: Some(self.probe.options.max_structural_units),
                force_safe_source: self.probe.force_safe_source,
                ..gmark_document_core::LoadingPolicy::default()
            }
        } else {
            configured_loading
        };
        let loading_limits = loading.effective_limits();
        #[cfg(not(test))]
        let recovery_dir = crate::config::GmarkConfigDirs::from_system()
            .ok()
            .map(|dirs| dirs.recovery_dir());
        #[cfg(test)]
        let recovery_dir: Option<PathBuf> = None;
        let window_handle = window.window_handle();
        if let Some(cancellation) = self.coordinator.index_cancellation.take() {
            cancellation.cancel();
        }
        let cancellation = SearchCancellation::default();
        self.coordinator.index_cancellation = Some(cancellation.clone());
        self.coordinator.index_generation = self.coordinator.index_generation.wrapping_add(1);
        let task_stamp = DocumentTaskStamp::capture(self, self.coordinator.index_generation);
        self.coordinator.external_generation = self.coordinator.external_generation.wrapping_add(1);
        self.active_edit = None;
        self.reloading = true;
        self.error = None;
        cx.emit(DocumentHostEvent::StateChanged);
        self.coordinator.index_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let original = FileSource::open(&path)?;
                    let mut probe = gmark_paged_document::probe_file(
                        &path,
                        gmark_paged_document::ProbeOptions {
                            max_resident_bytes: loading_limits.max_resident_bytes,
                            max_resident_lines: loading_limits.max_resident_lines,
                            max_structural_units: loading_limits.max_structural_units,
                            ..gmark_paged_document::ProbeOptions::default()
                        },
                    )?;
                    probe.force_safe_source = loading.force_safe_source;
                    let plan =
                        gmark_document_core::OpenPolicyResolver.resolve(loading, &probe.profile());
                    probe.strategy = match plan.backend {
                        gmark_document_core::DocumentBackendKind::Resident => {
                            OpenStrategy::Resident
                        }
                        gmark_document_core::DocumentBackendKind::Paged => OpenStrategy::Paged,
                    };
                    if let Some(encoding) = forced_encoding {
                        probe.encoding = encoding;
                    }
                    let reopened_encoding = text_encoding_label(&probe.encoding);
                    let recovery = recovery_dir.map(|dir| {
                        PagedRecoveryJournal::create(dir, &original, probe.encoding.clone())
                    });
                    let original_for_session = original.clone();
                    let prepared = prepare_utf8_source(original, probe.encoding.clone())?;
                    let index = LineIndex::build_cancellable(prepared.source(), &cancellation)?;
                    let document = build_document_session(
                        &probe,
                        &original_for_session,
                        prepared.source().clone(),
                        index.clone(),
                        false,
                    )?;
                    let (structure_source, structure_index, structure_bytes) =
                        structure_input_for_session(&document, &prepared, &index, &cancellation)?;
                    let structured = if derived_views_enabled(probe.strategy) {
                        build_structured_index(
                            &structure_source,
                            &structure_index,
                            probe.format.clone(),
                            &cancellation,
                            structure_bytes,
                        )?
                    } else {
                        None
                    };
                    Ok::<_, gmark_paged_document::PagedDocumentError>((
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
                if !task_stamp.accepts_strict(view, view.coordinator.index_generation) {
                    view.reloading = false;
                    return;
                }
                view.coordinator.index_cancellation = None;
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
                        if let Some(journal) = view.coordinator.recovery_journal.take()
                            && let Err(error) = journal.checkpoint()
                        {
                            view.coordinator.recovery_error =
                                Some(localized_document_error(&error, cx));
                        }
                        view.probe = probe;
                        view.document_epoch = view.document_epoch.wrapping_add(1);
                        view.prepared_source = Some(prepared);
                        view.provisional_source = None;
                        view.index = Some(index);
                        view.install_document_session(document);
                        view.invalidate_source_rows();
                        view.structured_index = structured;
                        view.invalidate_structured_runtime();
                        view.active_edit = None;
                        set_document_dirty_state(
                            &mut view.document,
                            &mut view.pending_dirty,
                            false,
                        );
                        view.coordinator.pending_external_change = None;
                        view.coordinator.external_monitor_paused = false;
                        view.coordinator.external_status = Some(
                            cx.global::<I18nManager>()
                                .strings()
                                .large_document_text("reopened_as_template")
                                .replace("{encoding}", &reopened_encoding)
                                .into(),
                        );
                        match recovery {
                            Some(Ok(journal)) => view.coordinator.recovery_journal = Some(journal),
                            Some(Err(error)) => {
                                view.coordinator.recovery_error =
                                    Some(localized_document_error(&error, cx))
                            }
                            None => {}
                        }
                    }
                    Err(error) => view.error = Some(localized_document_error(&error, cx)),
                }
                cx.emit(DocumentHostEvent::StateChanged);
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
        self.coordinator.pending_external_change = None;
        self.coordinator.external_monitor_paused = true;
        self.tail_enabled = false;
        self.coordinator.external_status = Some(
            cx.global::<I18nManager>()
                .strings()
                .large_document_text("keeping_local")
                .into(),
        );
        cx.emit(DocumentHostEvent::StateChanged);
        cx.notify();
    }

    pub(crate) fn on_save_document(
        &mut self,
        _: &SaveDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.coordinator.external_monitor_paused {
            self.error = Some(
                cx.global::<I18nManager>()
                    .strings()
                    .large_document_text("disk_changed_save_as_reload")
                    .into(),
            );
            cx.emit(DocumentHostEvent::StateChanged);
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
        if self.saving
            || self.reloading
            || (!document_dirty_state(&self.document, &self.pending_dirty) && !save_as)
        {
            return;
        }
        if let Some(cancellation) = self.coordinator.save.cancellation.take() {
            cancellation.cancel();
        }
        self.coordinator.save.generation = self.coordinator.save.generation.wrapping_add(1);
        let task_stamp = DocumentTaskStamp::capture(self, self.coordinator.save.generation);
        let save_started = crate::perf::start();
        let open_strategy = self.probe.strategy;
        let probe_options = self.probe.options;
        let force_safe_source = self.probe.force_safe_source;
        let save_profile = self.probe.profile();
        let save_plan = session_plan(&save_profile, &self.probe, open_strategy, false);
        let cancellation = SearchCancellation::default();
        self.coordinator.save.cancellation = Some(cancellation.clone());
        // 保存会暂时取走 document 并重建 uniform_list 的数据源。必须保留底层像素偏移；
        // 近文件尾部用 scroll_to_item 恢复会再次经过估算布局，仍可能跳动数百行。
        let save_scroll_offset = self.scroll_handle.0.borrow().base_handle.offset();
        let Some(mut document) = self.take_document_session() else {
            self.coordinator.save.cancellation = None;
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
        if let Some(cancellation) = self.coordinator.search_cancellation.take() {
            cancellation.cancel();
        }
        self.coordinator.search_task = Task::ready(());
        self.coordinator.source_task = Task::ready(());
        self.structured_task = Task::ready(());
        self.structured_filter_task = Task::ready(());
        self.json_expand_task = Task::ready(());
        self.coordinator.external_generation = self.coordinator.external_generation.wrapping_add(1);
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
        cx.emit(DocumentHostEvent::StateChanged);
        self.coordinator.save.task = cx.spawn(async move |this, cx| {
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
                        return Err((document, prepared_on_error, map_persistence_error(error)));
                    }
                    // 保存后从最终磁盘内容重新建立干净基线，清除旧 undo/add buffer，并恢复结构视图。
                    let rebuild = (|| {
                        let original = FileSource::open(&path)?;
                        let mut probe = gmark_paged_document::probe_file(&path, probe_options)?;
                        // 当前会话不因保存后的大小变化热迁移；重新打开时才重新执行策略。
                        probe.strategy = open_strategy;
                        probe.force_safe_source = force_safe_source;
                        let recovery = recovery_dir.map(|dir| {
                            PagedRecoveryJournal::create(dir, &original, probe.encoding.clone())
                        });
                        let original_for_session = original.clone();
                        let prepared = prepare_utf8_source(original, probe.encoding.clone())?;
                        let index = LineIndex::build_cancellable(prepared.source(), &cancellation)?;
                        let clean_document = build_document_session(
                            &probe,
                            &original_for_session,
                            prepared.source().clone(),
                            index.clone(),
                            true,
                        )?;
                        verify_saved_session_readback(&document, &clean_document, &cancellation)?;
                        let (structure_source, structure_index, structure_bytes) =
                            structure_input_for_session(
                                &clean_document,
                                &prepared,
                                &index,
                                &cancellation,
                            )?;
                        let structured = if derived_views_enabled(probe.strategy) {
                            build_structured_index(
                                &structure_source,
                                &structure_index,
                                probe.format.clone(),
                                &cancellation,
                                structure_bytes,
                            )
                        } else {
                            Ok(None)
                        };
                        Ok::<_, gmark_paged_document::PagedDocumentError>((
                            clean_document,
                            prepared,
                            index,
                            structured,
                            recovery,
                            probe,
                            path,
                        ))
                    })();
                    rebuild.map_err(|error| {
                        (document, prepared_on_error, map_persistence_error(error))
                    })
                })
                .await;
            let saved = result.is_ok();
            if let Some(started) = save_started {
                crate::perf::emit_document(
                    "document_save",
                    started,
                    usize::try_from(save_profile.len).ok(),
                    Some(saved),
                    &save_profile.format,
                    &save_plan,
                    Some(if save_as { "save_as" } else { "save" }),
                );
            }
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_identity(view, view.coordinator.save.generation) {
                    return;
                }
                view.coordinator.save.cancellation = None;
                view.saving = false;
                match result {
                    Ok((document, prepared, index, structured, recovery, probe, path)) => {
                        // 保存后的干净 PieceTree 是新的磁盘身份基线；即使 revision 从零重新
                        // 开始，也不能接受旧基线上的搜索、复制或派生 projection 结果。
                        view.document_epoch = view.document_epoch.wrapping_add(1);
                        view.cancel_selection_transfers();
                        if let Some(journal) = view.coordinator.recovery_journal.take()
                            && let Err(error) = journal.checkpoint()
                        {
                            view.coordinator.recovery_error =
                                Some(localized_document_error(&error, cx));
                        }
                        view.install_document_session(document);
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
                                view.set_structure_error(error, cx);
                            }
                        }
                        if let Some(recovery) = recovery {
                            match recovery {
                                Ok(journal) => {
                                    view.coordinator.recovery_journal = Some(journal);
                                    view.coordinator.recovery_error = None;
                                }
                                Err(error) => {
                                    view.coordinator.recovery_error =
                                        Some(localized_document_error(&error, cx))
                                }
                            }
                        }
                        view.active_edit = None;
                        set_document_dirty_state(
                            &mut view.document,
                            &mut view.pending_dirty,
                            false,
                        );
                        if save_as {
                            view.path = path.clone();
                            view.coordinator.pending_external_change = None;
                            view.coordinator.external_monitor_paused = false;
                            view.coordinator.external_status = None;
                            cx.emit(DocumentHostEvent::SavedAs(path));
                        }
                    }
                    Err((document, prepared, error)) => {
                        view.install_document_session(document);
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
                cx.emit(DocumentHostEvent::StateChanged);
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

fn delimited_record_terminator(bytes: &[u8]) -> &'static str {
    if bytes.ends_with(b"\r\n") {
        "\r\n"
    } else if bytes.ends_with(b"\n") {
        "\n"
    } else if bytes.ends_with(b"\r") {
        "\r"
    } else {
        ""
    }
}

fn transform_delimited_adapter(
    mut document: DocumentSession,
    delimiter: u8,
    edit: DelimitedEdit,
    cancellation: &SearchCancellation,
    progress: &AtomicU64,
) -> Result<DocumentSession, PagedDocumentError> {
    let resident_source =
        document.store.kind() == gmark_document_core::DocumentBackendKind::Resident;
    let (column, header) = match edit {
        DelimitedEdit::InsertColumn { before, header } => (before, Some(header)),
        DelimitedEdit::DeleteColumn { column } => (column, None),
        _ => {
            return Err(PagedDocumentError::InvalidTransaction(
                "column worker received a non-column edit".into(),
            ));
        }
    };
    let mut input = tempfile::NamedTempFile::new().map_err(|source| PagedDocumentError::Io {
        path: std::env::temp_dir(),
        source,
    })?;
    document.write_to_cancellable(input.as_file_mut(), cancellation)?;
    input
        .as_file_mut()
        .sync_all()
        .map_err(|source| PagedDocumentError::Io {
            path: input.path().to_path_buf(),
            source,
        })?;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_path(input.path())
        .map_err(|source| PagedDocumentError::Io {
            path: input.path().to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, source),
        })?;
    let bytes = FileSource::open(input.path())?;
    let source_len = bytes.identity()?.len;
    let mut output = tempfile::NamedTempFile::new().map_err(|source| PagedDocumentError::Io {
        path: std::env::temp_dir(),
        source,
    })?;
    let output_path = output.path().to_path_buf();
    let mut record = csv::ByteRecord::new();
    let mut physical = 0u64;
    loop {
        if physical.is_multiple_of(1_024) && cancellation.is_cancelled() {
            return Err(PagedDocumentError::Cancelled);
        }
        let start = reader.position().byte();
        if !reader
            .read_byte_record(&mut record)
            .map_err(|source| PagedDocumentError::Io {
                path: input.path().to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, source),
            })?
        {
            break;
        }
        let end = reader.position().byte();
        let raw_end = if end < source_len {
            (end + 1).min(source_len)
        } else {
            end
        };
        let raw = bytes.read_range(start, raw_end)?;
        let terminator = if resident_source {
            "\n"
        } else {
            delimited_record_terminator(&raw)
        };
        let mut fields = record
            .iter()
            .map(|field| String::from_utf8_lossy(field).into_owned())
            .collect::<Vec<_>>();
        if let Some(header) = &header {
            fields.insert(
                column.min(fields.len()),
                if physical == 0 {
                    header.clone()
                } else {
                    String::new()
                },
            );
        } else if column < fields.len() {
            fields.remove(column);
        }
        output
            .write_all(serialize_delimited_record(&fields, delimiter, terminator).as_bytes())
            .map_err(|source| PagedDocumentError::Io {
                path: output_path.clone(),
                source,
            })?;
        physical += 1;
        progress.store(physical, Ordering::Relaxed);
    }
    if physical == 0
        && let Some(header) = &header
    {
        output
            .write_all(
                serialize_delimited_record(std::slice::from_ref(header), delimiter, "").as_bytes(),
            )
            .map_err(|source| PagedDocumentError::Io {
                path: output_path.clone(),
                source,
            })?;
    }
    output
        .as_file_mut()
        .sync_all()
        .map_err(|source| PagedDocumentError::Io {
            path: output_path.clone(),
            source,
        })?;
    let output_reader = output.reopen().map_err(|source| PagedDocumentError::Io {
        path: output_path,
        source,
    })?;
    document.replace_text_reader(0..document.len(), output_reader)?;
    Ok(document)
}
