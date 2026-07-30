// @author kongweiguang

//! Initial indexing and session installation.

use super::*;

impl DocumentHost {
    /// 初次打开和关闭标签后的恢复共用同一条索引管线。任务结果由 document epoch、
    /// revision 与 generation 三重门禁，关闭期间取消的旧 worker 永远不能重新安装。
    pub(super) fn start_initial_index(&mut self, cx: &mut Context<Self>) {
        if self.document.is_some() || self.coordinator.index_cancellation.is_some() {
            return;
        }
        let Some(worker_source) = self.provisional_source.clone() else {
            self.error = Some(
                cx.global::<I18nManager>()
                    .strings()
                    .large_document_text("source_backend_unavailable")
                    .into(),
            );
            return;
        };
        let probe = self.probe.clone();
        #[cfg(not(test))]
        let recovery_dir = gmark_config::GmarkConfigDirs::from_system()
            .ok()
            .map(|dirs| dirs.recovery_dir());
        #[cfg(test)]
        let recovery_dir: Option<PathBuf> = None;
        let index_cache_dir = ProjectDirs::from("com", "kongweiguang", "gmark")
            .map(|dirs| dirs.cache_dir().join("large-document-indexes"));
        let index_cancellation = SearchCancellation::default();
        let index_worker_cancellation = index_cancellation.clone();
        self.coordinator.index_cancellation = Some(index_cancellation);
        self.coordinator.index_generation = self.coordinator.index_generation.wrapping_add(1);
        let task_stamp = DocumentTaskStamp::capture(self, self.coordinator.index_generation);
        self.coordinator.index_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let path = worker_source.path().to_path_buf();
                    let options = probe.options;
                    let force_safe_source = probe.force_safe_source;
                    let mut candidate_probe = probe;
                    for attempt in 0..3 {
                        // Probe 与完整读取之间可能发生替换或增长。任何 identity 变化都用
                        // 原 ProbeOptions 重新规划，并重开稳定句柄。
                        let mut worker_source = FileSource::open(&path)?;
                        if worker_source.identity()? != candidate_probe.identity {
                            candidate_probe = gmark_paged_document::probe_file(&path, options)?;
                            candidate_probe.force_safe_source = force_safe_source;
                            worker_source = FileSource::open(&path)?;
                            if worker_source.identity()? != candidate_probe.identity {
                                if attempt < 2 {
                                    continue;
                                }
                                return Err(PagedDocumentError::SourceChanged);
                            }
                        }
                        let probe = candidate_probe.clone();
                        let build = (|| {
                            let direct_utf8 = matches!(probe.encoding, TextEncoding::Utf8 { .. });
                            let encoding = probe.encoding.clone();
                            let recovery_source = worker_source.clone();
                            let prepared = prepare_utf8_source(worker_source, encoding.clone())?;
                            let source = prepared.source().clone();
                            let index = if direct_utf8 {
                                if let Some(cache_dir) = index_cache_dir.as_ref() {
                                    LineIndex::build_cached_cancellable(
                                        &source,
                                        cache_dir,
                                        &index_worker_cancellation,
                                    )?
                                } else {
                                    LineIndex::build_cancellable(
                                        &source,
                                        &index_worker_cancellation,
                                    )?
                                }
                            } else {
                                LineIndex::build_cancellable(&source, &index_worker_cancellation)?
                            };
                            let document = build_document_session(
                                &probe,
                                &recovery_source,
                                source,
                                index.clone(),
                                false,
                            )?;
                            let (structure_source, structure_index, structure_bytes) =
                                structure_input_for_session(
                                    &document,
                                    &prepared,
                                    &index,
                                    &index_worker_cancellation,
                                )?;
                            let recovery = recovery_dir.as_ref().map(|dir| {
                                DocumentRecoveryJournal::create(
                                    dir,
                                    &recovery_source,
                                    encoding.clone(),
                                    &document,
                                )
                            });
                            Ok::<_, PagedDocumentError>((
                                probe,
                                index,
                                document,
                                prepared,
                                recovery,
                                structure_source,
                                structure_index,
                                structure_bytes,
                            ))
                        })();
                        match build {
                            Err(PagedDocumentError::SourceChanged) if attempt < 2 => {
                                candidate_probe = gmark_paged_document::probe_file(&path, options)?;
                                candidate_probe.force_safe_source = force_safe_source;
                            }
                            result => return result,
                        }
                    }
                    Err(PagedDocumentError::SourceChanged)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.coordinator.index_generation) {
                    return;
                }
                view.coordinator.index_cancellation = None;
                match result {
                    Ok((
                        probe,
                        index,
                        document,
                        prepared,
                        recovery,
                        structure_source,
                        structure_index,
                        structure_bytes,
                    )) => {
                        let strategy_changed = view.probe.strategy != probe.strategy;
                        view.probe = probe;
                        let format = view.probe.format.clone();
                        if view.probe.strategy == OpenStrategy::Paged {
                            view.view_mode = DocumentHostViewMode::Source;
                            view.sync_tab_active_view();
                        }
                        let anchor_line = view
                            .provisional_anchor
                            .and_then(|anchor| document.line_for_offset(anchor.byte_offset))
                            .and_then(|line| usize::try_from(line).ok());
                        view.index = Some(index);
                        view.install_document_session(document);
                        view.prepared_source = Some(prepared);
                        view.provisional_source = None;
                        view.provisional_anchor = None;
                        view.invalidate_source_rows();
                        if let Some(line) = anchor_line {
                            view.scroll_source_line(line, ScrollStrategy::Top);
                        }
                        if let Some(recovery) = recovery {
                            match recovery {
                                Ok(journal) => view.coordinator.recovery_journal = Some(journal),
                                Err(error) => {
                                    view.coordinator.recovery_error = Some(
                                        cx.global::<I18nManager>()
                                            .strings()
                                            .large_document_error(&error)
                                            .into(),
                                    )
                                }
                            }
                        }
                        view.schedule_search(cx);
                        if !derived_views_enabled(view.probe.strategy) {
                            // Paged 安全模式只安装 Source，不启动任何需要全文结构化扫描的任务。
                            view.clear_structure_error();
                            if strategy_changed {
                                cx.emit(DocumentHostEvent::ViewModeChanged(
                                    DocumentHostMode::Source,
                                ));
                            }
                            cx.emit(DocumentHostEvent::StateChanged);
                            cx.notify();
                            return;
                        }
                        if format == DocumentFormat::Json
                            && view.view_mode != DocumentHostViewMode::Source
                        {
                            view.request_registered_projection(cx);
                        }
                        if format == DocumentFormat::Json {
                            view.clear_structure_error();
                            cx.emit(DocumentHostEvent::StateChanged);
                            cx.notify();
                            return;
                        }
                        view.structured_generation = view.structured_generation.wrapping_add(1);
                        let generation = view.structured_generation;
                        let structured_task_stamp = DocumentTaskStamp::capture(view, generation);
                        if !matches!(format, DocumentFormat::PlainText) {
                            view.structure_error = Some(
                                cx.global::<I18nManager>()
                                    .strings()
                                    .large_document_text("indexing_structured")
                                    .into(),
                            );
                            view.structure_error_byte = None;
                        }
                        let structure_cancellation = SearchCancellation::default();
                        view.structured_cancellation = Some(structure_cancellation.clone());
                        view.structured_task = cx.spawn(async move |this, cx| {
                            let structured = cx
                                .background_spawn(async move {
                                    build_structured_index(
                                        &structure_source,
                                        &structure_index,
                                        format,
                                        &structure_cancellation,
                                        structure_bytes,
                                    )
                                })
                                .await;
                            let _ = this.update(cx, |view, cx| {
                                if !structured_task_stamp
                                    .accepts_strict(view, view.structured_generation)
                                    || document_dirty_state(&view.document, &view.pending_dirty)
                                {
                                    return;
                                }
                                view.structured_cancellation = None;
                                match structured {
                                    Ok(Some(structured)) => {
                                        view.structured_index = Some(structured);
                                        view.clear_structure_error();
                                    }
                                    Ok(None) => {
                                        view.clear_structure_error();
                                        if matches!(
                                            view.view_mode,
                                            DocumentHostViewMode::Live
                                                | DocumentHostViewMode::Structure
                                        ) {
                                            view.view_mode = DocumentHostViewMode::Source;
                                            view.sync_tab_active_view();
                                        }
                                    }
                                    Err(error) => {
                                        view.set_structure_error(error, cx);
                                        if matches!(
                                            view.view_mode,
                                            DocumentHostViewMode::Live
                                                | DocumentHostViewMode::Structure
                                        ) {
                                            view.view_mode = DocumentHostViewMode::Source;
                                            view.sync_tab_active_view();
                                        }
                                    }
                                }
                                cx.emit(DocumentHostEvent::StateChanged);
                                cx.notify();
                            });
                        });
                    }
                    Err(error) => {
                        view.error = Some(
                            cx.global::<I18nManager>()
                                .strings()
                                .large_document_error(&error)
                                .into(),
                        )
                    }
                }
                cx.emit(DocumentHostEvent::StateChanged);
                cx.notify();
            });
        });
    }
}
