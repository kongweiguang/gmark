// @author kongweiguang

//! External-change monitoring.

use super::*;

impl DocumentHost {
    pub(super) fn start_external_monitor(&mut self, cx: &mut Context<Self>) {
        self.coordinator.external_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let snapshot = this.update(cx, |view, _cx| {
                    view.document
                        .clone()
                        .zip(view.index.clone())
                        .map(|(document, index)| {
                            let task_stamp = DocumentTaskStamp::capture(
                                view,
                                view.coordinator.external_generation,
                            );
                            (
                                document,
                                index,
                                view.path.clone(),
                                document_dirty_state(&view.document, &view.pending_dirty),
                                view.tail_enabled,
                                view.coordinator.external_monitor_paused,
                                task_stamp,
                                view.coordinator.lifetime_cancellation.clone(),
                                view.probe.format.clone(),
                                derived_views_enabled(view.probe.strategy),
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
                    allow_derived_views,
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
                            let structured = if !allow_derived_views {
                                Ok(None)
                            } else if matches!(format, DocumentFormat::JsonLines) {
                                validate_json_lines_from_cancellable(
                                    &source,
                                    &extended,
                                    previous_line_count.saturating_sub(1),
                                    &cancellation,
                                )
                                .map(|()| {
                                    Some(StructuredIndex::JsonLines {
                                        lines: StructuredLines::File(extended.clone()),
                                        source: StructuredTextSource::File(source.clone()),
                                        record_count: structured_json_lines_record_count(
                                            &StructuredLines::File(extended.clone()),
                                        ),
                                    })
                                })
                            } else {
                                build_structured_index(
                                    &source,
                                    &extended,
                                    format,
                                    &cancellation,
                                    None,
                                )
                            };
                            Ok::<_, gmark_paged_document::PagedDocumentError>((
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
                    if !task_stamp.accepts_strict(view, view.coordinator.external_generation) {
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
                        )) if !document_dirty_state(&view.document, &view.pending_dirty)
                            && view.tail_enabled =>
                        {
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
                                                view.set_structure_error(error, cx);
                                            }
                                        }
                                        view.coordinator.external_status = Some(
                                            cx.global::<I18nManager>()
                                                .strings()
                                                .large_document_text("following_log_template")
                                                .replace(
                                                    "{kib}",
                                                    &format!("{:.1}", (to - from) as f64 / 1024.0),
                                                )
                                                .into(),
                                        );
                                        view.coordinator.pending_external_change = None;
                                        view.schedule_search(cx);
                                        if let Some(last) = view.line_count().checked_sub(1) {
                                            view.scroll_source_line_strict(
                                                last,
                                                ScrollStrategy::Bottom,
                                            );
                                        }
                                    }
                                    Err(error) => {
                                        view.coordinator.external_status = Some(
                                            cx.global::<I18nManager>()
                                                .strings()
                                                .large_document_error(&error)
                                                .into(),
                                        )
                                    }
                                }
                            }
                        }
                        Ok((change @ ExternalChange::Appended { .. }, _)) => {
                            view.coordinator.pending_external_change = Some(change);
                            view.coordinator.external_status = Some(
                                if document_dirty_state(&view.document, &view.pending_dirty) {
                                    cx.global::<I18nManager>()
                                        .strings()
                                        .large_document_text("disk_grew_with_edits")
                                } else {
                                    cx.global::<I18nManager>()
                                        .strings()
                                        .large_document_text("disk_grew_enable_follow")
                                }
                                .into(),
                            );
                        }
                        Ok((change @ ExternalChange::Truncated { .. }, _)) => {
                            view.coordinator.pending_external_change = Some(change);
                            view.coordinator.external_status = Some(
                                cx.global::<I18nManager>()
                                    .strings()
                                    .large_document_text("disk_truncated_reload")
                                    .into(),
                            );
                        }
                        Ok((ExternalChange::Replaced, _)) => {
                            view.coordinator.pending_external_change =
                                Some(ExternalChange::Replaced);
                            view.coordinator.external_status = Some(
                                cx.global::<I18nManager>()
                                    .strings()
                                    .large_document_text("disk_replaced_reload")
                                    .into(),
                            );
                        }
                        Ok((ExternalChange::Modified, _)) => {
                            view.coordinator.pending_external_change =
                                Some(ExternalChange::Modified);
                            view.coordinator.external_status = Some(
                                cx.global::<I18nManager>()
                                    .strings()
                                    .large_document_text("disk_changed_reload")
                                    .into(),
                            );
                        }
                        Err(error) => {
                            view.coordinator.external_status = Some(
                                cx.global::<I18nManager>()
                                    .strings()
                                    .large_document_error(&error)
                                    .into(),
                            )
                        }
                    }
                    if state_changed {
                        cx.emit(DocumentHostEvent::StateChanged);
                        cx.notify();
                    }
                });
            }
        });
    }
}
