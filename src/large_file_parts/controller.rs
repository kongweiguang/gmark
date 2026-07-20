// @author kongweiguang

use super::*;

impl DiskSourceAdapter {
    pub(crate) fn new(
        path: PathBuf,
        probe: OpenProbe,
        source: FileSource,
        cx: &mut Context<Self>,
    ) -> Self {
        let preview_lines = source
            .read_range(0, probe.len.min(PREFIX_PREVIEW_BYTES))
            .map(|bytes| decode_provisional_bytes(&bytes, &probe.encoding, 0))
            .map(|text| {
                text.lines()
                    .map(|line| SharedString::from(line.trim_end_matches('\r').to_owned()))
                    .collect()
            })
            .unwrap_or_else(|error| vec![format!("Unable to decode first window: {error}").into()]);
        let search_input = cx.new(move |cx| {
            let mut block = Block::with_record(
                cx,
                BlockRecord::with_plain_text(BlockKind::Paragraph, String::new()),
            );
            block.set_source_raw_mode();
            block.set_input_placeholder("Find in large file");
            block.set_host_submit_enabled(true);
            block
        });
        cx.subscribe(&search_input, Self::on_search_input_event)
            .detach();
        let navigation_input = cx.new(move |cx| {
            let mut block = Block::with_record(
                cx,
                BlockRecord::with_plain_text(BlockKind::Paragraph, String::new()),
            );
            block.set_source_raw_mode();
            block.set_input_placeholder("Go to line");
            block.set_host_submit_enabled(true);
            block
        });
        cx.subscribe(&navigation_input, Self::on_navigation_input_event)
            .detach();
        let structured_filter_input = cx.new(|cx| {
            let mut block = Block::with_record(
                cx,
                BlockRecord::with_plain_text(BlockKind::Paragraph, String::new()),
            );
            block.set_source_raw_mode();
            block.set_input_placeholder("Filter rows");
            block
        });
        cx.subscribe(
            &structured_filter_input,
            Self::on_structured_filter_input_event,
        )
        .detach();
        let tail_enabled = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("log"));
        // 大文件只替换存储与物化策略，不能改变用户进入文档后的主路径。
        // 结构索引是后台渐进增强；无论文件类型，首屏都必须落在普通 Source 语义。
        let initial_view_mode = LargeViewMode::Source;
        let lifetime_cancellation = SearchCancellation::default();
        let mut view_registry = DocumentViewRegistry::default();
        let mut active_derived_view = None;
        if let Some(provider) = RegisteredStructuredProvider::for_format(&probe.format) {
            active_derived_view = Some(provider.descriptor.id.clone());
            let registered = view_registry.register(Arc::new(provider));
            debug_assert!(registered, "built-in derived view id must be unique");
        }
        let mut view = Self {
            path,
            probe,
            index: None,
            document: None,
            prepared_source: None,
            provisional_source: Some(source),
            structured_index: None,
            structured_rows: BTreeMap::new(),
            structured_pending: None,
            structured_generation: 0,
            structured_cancellation: None,
            structure_error: None,
            structure_error_byte: None,
            structured_filter_input,
            structured_filter_column: None,
            structured_filtered_rows: Vec::new(),
            structured_filter_generation: 0,
            structured_filter_cancellation: None,
            structured_filter_running: false,
            hidden_structured_columns: BTreeSet::new(),
            structured_column_window_start: 0,
            json_child_indexes: BTreeMap::new(),
            json_expanded_nodes: BTreeSet::new(),
            json_rows: BTreeMap::new(),
            json_expand_generation: 0,
            json_expand_cancellation: None,
            view_registry,
            view_state: DocumentViewState::default(),
            active_derived_view,
            document_epoch: 1,
            derived_projection_generation: 0,
            derived_projection_cancellation: None,
            derived_projection_snapshot: None,
            derived_projection_task: Task::ready(()),
            view_mode: initial_view_mode,
            preview_lines,
            source_rows: BTreeMap::new(),
            displayed_screen_lines: Arc::new(ScreenLines::default()),
            metrics: LargeFileMetrics::default(),
            source_row_blocks: BTreeMap::new(),
            source_row_epochs: BTreeMap::new(),
            source_cache_epoch: 0,
            soak_ready_published: false,
            source_pending: None,
            source_queued_visible: None,
            source_last_visible: None,
            source_list_origin: 0,
            source_generation: 0,
            source_cancellation: None,
            source_cancel_in_flight: false,
            source_row_height: FALLBACK_SOURCE_ROW_HEIGHT,
            active_edit: None,
            suppressed_line_edit_text: None,
            selection_anchor: None,
            selected_lines: None,
            source_drag_anchor: None,
            source_drag_autoscroll_direction: 0,
            source_drag_autoscroll_task: Task::ready(()),
            source_context_menu: None,
            source_context_menu_focus_handle: cx.focus_handle(),
            search_input,
            search_visible: false,
            navigation_input,
            navigation_visible: false,
            navigation_is_byte: false,
            show_line_endings: false,
            search_options: SearchOptions::default(),
            search_results: Vec::new(),
            search_selected: 0,
            search_running: false,
            search_generation: 0,
            search_cancellation: None,
            search_error: None,
            mode_notice: None,
            external_status: None,
            pending_external_change: None,
            external_monitor_paused: false,
            external_generation: 0,
            index_generation: 0,
            index_cancellation: None,
            save_generation: 0,
            save_cancellation: None,
            tail_enabled,
            dirty: false,
            saving: false,
            reloading: false,
            error: None,
            recovery_journal: None,
            recovery_error: None,
            focus_handle: cx.focus_handle(),
            scroll_handle: UniformListScrollHandle::new(),
            source_window_start: 0,
            provisional_anchor: Some(SourceAnchor::new(0, SourceAffinity::Before)),
            closed_suspended: false,
            lifetime_cancellation,
            _index_task: Task::ready(()),
            source_task: Task::ready(()),
            structured_task: Task::ready(()),
            structured_filter_task: Task::ready(()),
            json_expand_task: Task::ready(()),
            search_task: Task::ready(()),
            external_task: Task::ready(()),
            save_task: Task::ready(()),
            clipboard_generation: 0,
            clipboard_cancellation: None,
            clipboard_task: Task::ready(()),
            selection_export_generation: 0,
            selection_export_cancellation: None,
            selection_export_task: Task::ready(()),
        };
        view.start_initial_index(cx);
        view.start_external_monitor(cx);
        view
    }

    /// 初次打开和关闭标签后的恢复共用同一条索引管线。任务结果由 document epoch、
    /// revision 与 generation 三重门禁，关闭期间取消的旧 worker 永远不能重新安装。
    fn start_initial_index(&mut self, cx: &mut Context<Self>) {
        if self.document.is_some() || self.index_cancellation.is_some() {
            return;
        }
        let Some(worker_source) = self.provisional_source.clone() else {
            self.error = Some("Source backend is unavailable for indexing".into());
            return;
        };
        let direct_utf8 = matches!(self.probe.encoding, TextEncoding::Utf8 { .. });
        let format = self.probe.format.clone();
        let encoding = self.probe.encoding.clone();
        #[cfg(not(test))]
        let recovery_dir = crate::config::GmarkConfigDirs::from_system()
            .ok()
            .map(|dirs| dirs.recovery_dir());
        #[cfg(test)]
        let recovery_dir: Option<PathBuf> = None;
        let recovery_source = worker_source.clone();
        let index_cache_dir = ProjectDirs::from("com", "kongweiguang", "gmark")
            .map(|dirs| dirs.cache_dir().join("large-document-indexes"));
        let index_cancellation = SearchCancellation::default();
        let index_worker_cancellation = index_cancellation.clone();
        self.index_cancellation = Some(index_cancellation);
        self.index_generation = self.index_generation.wrapping_add(1);
        let task_stamp = DocumentTaskStamp::capture(self, self.index_generation);
        self._index_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    // Recovery stores the source encoding independently of the editable UTF-8
                    // shadow, so retain the probe value for both operations.
                    let prepared = prepare_utf8_source(worker_source, encoding.clone())?;
                    let source = prepared.source().clone();
                    let index = if direct_utf8 {
                        if let Some(cache_dir) = index_cache_dir {
                            LineIndex::build_cached_cancellable(
                                &source,
                                cache_dir,
                                &index_worker_cancellation,
                            )?
                        } else {
                            LineIndex::build_cancellable(&source, &index_worker_cancellation)?
                        }
                    } else {
                        LineIndex::build_cancellable(&source, &index_worker_cancellation)?
                    };
                    let document = PieceDocument::open(source, index.clone())?;
                    let recovery = recovery_dir.map(|dir| {
                        LargeRecoveryJournal::create(dir, &recovery_source, encoding.clone())
                    });
                    Ok::<_, gmark_large_document::LargeDocumentError>((
                        index, document, prepared, recovery,
                    ))
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.index_generation) {
                    return;
                }
                view.index_cancellation = None;
                match result {
                    Ok((index, document, prepared, recovery)) => {
                        let structure_source = prepared.source().clone();
                        let structure_index = index.clone();
                        let anchor_line = view
                            .provisional_anchor
                            .and_then(|anchor| document.line_for_offset(anchor.byte_offset))
                            .and_then(|line| usize::try_from(line).ok());
                        view.index = Some(index);
                        view.document = Some(document.into());
                        view.prepared_source = Some(prepared);
                        view.provisional_source = None;
                        view.provisional_anchor = None;
                        view.invalidate_source_rows();
                        if let Some(line) = anchor_line {
                            view.scroll_source_line(line, ScrollStrategy::Top);
                        }
                        if let Some(recovery) = recovery {
                            match recovery {
                                Ok(journal) => view.recovery_journal = Some(journal),
                                Err(error) => view.recovery_error = Some(error.to_string().into()),
                            }
                        }
                        view.schedule_search(cx);
                        view.structured_generation = view.structured_generation.wrapping_add(1);
                        let generation = view.structured_generation;
                        let structured_task_stamp = DocumentTaskStamp::capture(view, generation);
                        if !matches!(format, DocumentFormat::PlainText) {
                            view.structure_error = Some("Indexing structured view…".into());
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
                                    )
                                })
                                .await;
                            let _ = this.update(cx, |view, cx| {
                                if !structured_task_stamp
                                    .accepts_strict(view, view.structured_generation)
                                    || view.dirty
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
                                        if view.view_mode == LargeViewMode::Structure {
                                            view.view_mode = LargeViewMode::Source;
                                        }
                                    }
                                    Err(error) => {
                                        view.set_structure_error(error);
                                        if view.view_mode == LargeViewMode::Structure {
                                            view.view_mode = LargeViewMode::Source;
                                        }
                                    }
                                }
                                cx.emit(DiskSourceEvent::StateChanged);
                                cx.notify();
                            });
                        });
                    }
                    Err(error) => view.error = Some(error.to_string().into()),
                }
                cx.emit(DiskSourceEvent::StateChanged);
                cx.notify();
            });
        });
    }

    /// 关闭的标签会暂存在 reopen 栈中，所以不能依赖 Entity Drop 释放任务。
    /// 所有 worker 在这里显式取消并推进代次；保留的 PieceTree、selection 与 ViewState
    /// 仍是纯内存状态，重新打开时可以安全恢复。
    pub(crate) fn suspend_for_closed_tab(&mut self) {
        if self.closed_suspended {
            return;
        }
        debug_assert!(!self.saving && !self.reloading);
        self.closed_suspended = true;
        self.document_epoch = self.document_epoch.wrapping_add(1);

        self.lifetime_cancellation.cancel();
        self.external_generation = self.external_generation.wrapping_add(1);
        self.external_task = Task::ready(());

        self.index_generation = self.index_generation.wrapping_add(1);
        if let Some(cancellation) = self.index_cancellation.take() {
            cancellation.cancel();
        }
        self._index_task = Task::ready(());

        self.invalidate_source_rows();
        self.invalidate_structured_runtime();

        self.search_generation = self.search_generation.wrapping_add(1);
        if let Some(cancellation) = self.search_cancellation.take() {
            cancellation.cancel();
        }
        self.search_running = false;
        self.search_task = Task::ready(());

        self.save_generation = self.save_generation.wrapping_add(1);
        if let Some(cancellation) = self.save_cancellation.take() {
            cancellation.cancel();
        }
        self.save_task = Task::ready(());

        self.source_drag_anchor = None;
        self.source_drag_autoscroll_direction = 0;
        self.source_drag_autoscroll_task = Task::ready(());
        self.cancel_selection_transfers();
    }

    /// 只恢复关闭标签时被挂起的任务。普通标签切换仍保留既有实体，不会重复启动 monitor。
    pub(crate) fn resume_after_closed_tab(&mut self, cx: &mut Context<Self>) {
        if !self.closed_suspended {
            return;
        }
        self.closed_suspended = false;
        self.lifetime_cancellation = SearchCancellation::default();
        if self.document.is_none() {
            self.start_initial_index(cx);
        } else if self.structured_index.is_none() && !self.dirty {
            self.rebuild_clean_structured_index(cx);
        }
        self.start_external_monitor(cx);
        if !self.search_input.read(cx).display_text().is_empty() {
            self.schedule_search(cx);
        }
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn is_closed_suspended_for_test(&self) -> bool {
        self.closed_suspended
    }
}
