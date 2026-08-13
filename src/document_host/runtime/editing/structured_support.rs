// @author kongweiguang

//! Delimited snapshot rebuilding and transfer cancellation.

use super::*;

impl DocumentHost {
    pub(super) fn schedule_delimited_snapshot_rebuild(&mut self, cx: &mut Context<Self>) {
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
                    let bytes: Arc<[u8]> = document.read_range(0..document.len())?.into();
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
}
