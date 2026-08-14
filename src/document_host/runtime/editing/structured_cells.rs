// @author kongweiguang

//! Delimited-table cell and row editing.

use super::*;

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
    pub(super) fn selected_structured_cell_text(&self) -> Option<String> {
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
    pub(super) fn current_structured_record_range(&self, baseline: &Range<u64>) -> Range<u64> {
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
                    || view.document.as_ref().map(SharedDocument::revision) != Some(base_revision)
                    || view.coordinator.pending_external_change.is_some()
                {
                    view.structured_column_progress = None;
                    return;
                }
                view.structured_cancellation = None;
                view.structured_column_progress = None;
                match result {
                    Ok(replacement) => view.install_delimited_transformation(replacement, cx),
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

    /// Apply a completed CSV/TSV transformation once and hand its immutable
    /// result to the serial worker instead of writing recovery under UI state.
    fn install_delimited_transformation(&mut self, replacement: String, cx: &mut Context<Self>) {
        self.active_edit = None;
        self.structured_cell_edit = None;
        let Some(document) = self.document.clone() else {
            return;
        };
        let base_revision = document.revision();
        let old_len = document.len();
        if let Err(error) = document.replace_range(0..old_len, replacement.as_str()) {
            self.set_structure_error(error, cx);
            return;
        }
        self.enqueue_recovery_transaction(
            &document,
            base_revision,
            0..old_len,
            &replacement,
            Some(SourceSelection::collapsed(
                replacement.len() as u64,
                SourceAffinity::After,
            )),
            recovery_view_id(self.view_mode),
            cx,
        );
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
}
