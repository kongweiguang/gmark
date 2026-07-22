// @author kongweiguang

use super::*;

impl DocumentHost {
    pub(crate) fn new(
        path: PathBuf,
        probe: OpenProbe,
        source: FileSource,
        cx: &mut Context<Self>,
    ) -> Self {
        let strings = cx
            .try_global::<I18nManager>()
            .map(I18nManager::strings_arc)
            .unwrap_or_else(|| Arc::new(I18nStrings::en_us()));
        let preview_lines = source
            .read_range(0, probe.len.min(PREFIX_PREVIEW_BYTES))
            .map(|bytes| decode_provisional_bytes(&bytes, &probe.encoding, 0))
            .map(|text| {
                text.lines()
                    .map(|line| SharedString::from(line.trim_end_matches('\r').to_owned()))
                    .collect()
            })
            .unwrap_or_else(|error| {
                vec![
                    strings
                        .large_document_text("decode_first_window")
                        .replace("{error}", &error.to_string())
                        .into(),
                ]
            });
        let search_placeholder = strings.large_document_text("find_placeholder").to_owned();
        let search_input = cx.new(move |cx| {
            let mut block = Block::with_record(
                cx,
                BlockRecord::with_plain_text(BlockKind::Paragraph, String::new()),
            );
            block.set_source_raw_mode();
            block.set_input_placeholder(search_placeholder);
            block.set_host_submit_enabled(true);
            block
        });
        cx.subscribe(&search_input, Self::on_search_input_event)
            .detach();
        let navigation_placeholder = strings
            .large_document_text("go_to_line_placeholder")
            .to_owned();
        let navigation_input = cx.new(move |cx| {
            let mut block = Block::with_record(
                cx,
                BlockRecord::with_plain_text(BlockKind::Paragraph, String::new()),
            );
            block.set_source_raw_mode();
            block.set_input_placeholder(navigation_placeholder);
            block.set_host_submit_enabled(true);
            block
        });
        cx.subscribe(&navigation_input, Self::on_navigation_input_event)
            .detach();
        let structured_filter_placeholder = if probe.format == DocumentFormat::Json {
            cx.try_global::<I18nManager>()
                .map(|manager| manager.strings().json_graph_search_placeholder.clone())
                .unwrap_or_else(|| strings.json_graph_search_placeholder.clone())
        } else {
            strings.large_document_text("filter_rows").to_owned()
        };
        let structured_filter_input = cx.new(move |cx| {
            let mut block = Block::with_record(
                cx,
                BlockRecord::with_plain_text(BlockKind::Paragraph, String::new()),
            );
            block.set_source_raw_mode();
            block.set_input_placeholder(structured_filter_placeholder);
            block
        });
        let graph_edit_input = cx.new(|cx| {
            let mut block = Block::with_record(
                cx,
                BlockRecord::with_plain_text(BlockKind::Paragraph, String::new()),
            );
            block.set_source_raw_mode();
            block
        });
        let structured_cell_input = cx.new(|cx| {
            let mut block = Block::with_record(
                cx,
                BlockRecord::with_plain_text(BlockKind::Paragraph, String::new()),
            );
            block.set_compact_source_host();
            block.set_host_text_size(12.0);
            block.set_host_submit_enabled(true);
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
        // SourceBacked 只决定存储与物化策略；外层 Editor 再按格式能力决定首屏模式。
        let initial_view_mode = DocumentHostViewMode::Source;
        let lifetime_cancellation = SearchCancellation::default();
        let mut view_registry = DocumentViewRegistry::default();
        let mut selected_projection_view = None;
        let json_focused_roots = Arc::new(Mutex::new(HashMap::new()));
        if derived_views_enabled(probe.strategy) && probe.format == DocumentFormat::Json {
            let provider = JsonGraphProjectionProvider::new(json_focused_roots.clone());
            selected_projection_view = Some(provider.descriptor().id.clone());
            let registered = view_registry.register(Arc::new(provider));
            debug_assert!(registered, "built-in JSON graph view id must be unique");
        } else if derived_views_enabled(probe.strategy)
            && let Some(provider) = RegisteredStructuredProvider::for_format(&probe.format)
        {
            selected_projection_view = Some(provider.descriptor.id.clone());
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
            structured_cell_input,
            structured_cell_edit: None,
            structured_selected_cell: None,
            structured_cell_overrides: BTreeMap::new(),
            structured_cell_source_edits: Vec::new(),
            structured_context_target: None,
            structured_column_progress: None,
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
            pending_view_state: Some(DocumentViewState::default()),
            selected_projection_view,
            document_epoch: 1,
            derived_projection_generation: 0,
            derived_projection_cancellation: None,
            derived_projection_snapshot: None,
            derived_projection_error: None,
            derived_projection_error_offset: None,
            derived_projection_stale: false,
            derived_projection_root: None,
            json_focused_roots,
            graph_selected_item: None,
            graph_search_matches: Vec::new(),
            graph_search_selected: 0,
            graph_search_collapsed_before: None,
            graph_context_menu: None,
            graph_edit_target: None,
            graph_edit_input,
            graph_edit_error: None,
            graph_edit_issue: None,
            graph_edit_original: None,
            graph_state_initialized: false,
            graph_needs_fit: true,
            graph_last_viewport: None,
            graph_pan_session: None,
            graph_pending_center: None,
            graph_recenter_anchor: None,
            graph_focus_handle: cx.focus_handle(),
            json_split_ratio: 0.5,
            json_split_drag: None,
            json_split_focus_handle: cx.focus_handle(),
            derived_projection_task: Task::ready(()),
            view_mode: initial_view_mode,
            preview_lines,
            source_rows: BTreeMap::new(),
            displayed_screen_lines: Arc::new(ScreenLines::default()),
            metrics: PagedDocumentMetrics::default(),
            first_render_started: crate::perf::start(),
            source_row_blocks: BTreeMap::new(),
            source_row_epochs: BTreeMap::new(),
            source_cache_epoch: 0,
            soak_ready_published: false,
            source_pending: None,
            source_queued_visible: None,
            source_last_visible: None,
            source_list_origin: 0,
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
            search_error: None,
            mode_notice: None,
            tail_enabled,
            pending_dirty: Some(false),
            saving: false,
            reloading: false,
            error: None,
            coordinator: DocumentCoordinator::new(lifetime_cancellation),
            focus_handle: cx.focus_handle(),
            scroll_handle: UniformListScrollHandle::new(),
            structured_scroll_handle: UniformListScrollHandle::new(),
            structured_horizontal_scroll_handle: ScrollHandle::new(),
            source_window_start: 0,
            provisional_anchor: Some(SourceAnchor::new(0, SourceAffinity::Before)),
            closed_suspended: false,
            structured_task: Task::ready(()),
            structured_progress_task: Task::ready(()),
            structured_filter_task: Task::ready(()),
            json_expand_task: Task::ready(()),
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
        let recovery_dir = crate::config::GmarkConfigDirs::from_system()
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
                                PagedRecoveryJournal::create(
                                    dir,
                                    &recovery_source,
                                    encoding.clone(),
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
                            view.sync_session_active_view();
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
                                            view.sync_session_active_view();
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
                                            view.sync_session_active_view();
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

        self.coordinator.lifetime_cancellation.cancel();
        self.coordinator.external_generation = self.coordinator.external_generation.wrapping_add(1);
        self.coordinator.external_task = Task::ready(());

        self.coordinator.index_generation = self.coordinator.index_generation.wrapping_add(1);
        if let Some(cancellation) = self.coordinator.index_cancellation.take() {
            cancellation.cancel();
        }
        self.coordinator.index_task = Task::ready(());

        self.invalidate_source_rows();
        self.invalidate_structured_runtime();

        self.coordinator.search_generation = self.coordinator.search_generation.wrapping_add(1);
        if let Some(cancellation) = self.coordinator.search_cancellation.take() {
            cancellation.cancel();
        }
        self.search_running = false;
        self.coordinator.search_task = Task::ready(());

        self.coordinator.save.generation = self.coordinator.save.generation.wrapping_add(1);
        if let Some(cancellation) = self.coordinator.save.cancellation.take() {
            cancellation.cancel();
        }
        self.coordinator.save.task = Task::ready(());

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
        self.coordinator.lifetime_cancellation = SearchCancellation::default();
        if self.document.is_none() {
            self.start_initial_index(cx);
        } else if self.structured_index.is_none()
            && !document_dirty_state(&self.document, &self.pending_dirty)
        {
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
