// @author kongweiguang

use super::*;

impl DiskSourceAdapter {
    pub(super) fn set_view_mode(&mut self, mode: LargeViewMode, cx: &mut Context<Self>) {
        if mode == LargeViewMode::Structure && self.structured_index.is_none() {
            return;
        }
        self.active_edit = None;
        self.view_mode = mode;
        cx.notify();
    }

    pub(super) fn select_markdown_table(&mut self, table: usize, cx: &mut Context<Self>) {
        let changed = match self.structured_index.as_mut() {
            Some(StructuredIndex::MarkdownTables { tables, selected })
                if table < tables.len() && table != *selected =>
            {
                *selected = table;
                true
            }
            _ => false,
        };
        if !changed {
            return;
        }
        // 视口行以表内相对序号为 key；切表必须整体失效，否则会短暂展示上一张表的同序号行。
        self.invalidate_structured_runtime();
        self.structured_filter_column = None;
        cx.notify();
    }

    pub(super) fn set_structure_error(&mut self, error: gmark_large_document::LargeDocumentError) {
        self.structure_error_byte = match &error {
            gmark_large_document::LargeDocumentError::InvalidJson { offset, .. } => Some(*offset),
            _ => None,
        };
        self.structure_error = Some(error.to_string().into());
    }

    pub(super) fn clear_structure_error(&mut self) {
        self.structure_error = None;
        self.structure_error_byte = None;
    }

    /// 结构索引、视口行、JSON 子树和筛选结果共享同一份磁盘基线。
    /// 基线变化时必须整体失效，避免后台旧任务把过期行重新发布到新文档。
    pub(super) fn invalidate_structured_runtime(&mut self) {
        self.derived_projection_generation = self.derived_projection_generation.wrapping_add(1);
        if let Some(cancellation) = self.derived_projection_cancellation.take() {
            cancellation.cancel();
        }
        self.derived_projection_snapshot = None;
        self.derived_projection_task = Task::ready(());
        self.structured_generation = self.structured_generation.wrapping_add(1);
        if let Some(cancellation) = self.structured_cancellation.take() {
            cancellation.cancel();
        }
        self.structured_pending = None;
        self.structured_rows.clear();

        self.structured_filter_generation = self.structured_filter_generation.wrapping_add(1);
        if let Some(cancellation) = self.structured_filter_cancellation.take() {
            cancellation.cancel();
        }
        self.structured_filter_running = false;
        self.structured_filtered_rows.clear();
        self.hidden_structured_columns.clear();
        self.structured_column_window_start = 0;

        self.json_expand_generation = self.json_expand_generation.wrapping_add(1);
        if let Some(cancellation) = self.json_expand_cancellation.take() {
            cancellation.cancel();
        }
        self.json_child_indexes.clear();
        self.json_expanded_nodes.clear();
        self.json_rows.clear();
    }

    pub(super) fn request_registered_projection(&mut self, cx: &mut Context<Self>) {
        let provider = self
            .active_derived_view
            .as_ref()
            .and_then(|id| {
                self.view_registry
                    .available_provider(id, &self.probe.format)
            })
            .or_else(|| {
                self.view_registry
                    .first_available_provider(&self.probe.format)
            });
        let Some(provider) = provider else {
            return;
        };
        let id = provider.descriptor().id.clone();
        self.active_derived_view = Some(id.clone());
        self.view_state.derived.entry(id.clone()).or_default();
        let Some(document) = self.document.clone() else {
            return;
        };
        if let Some(cancellation) = self.derived_projection_cancellation.take() {
            cancellation.cancel();
        }
        self.derived_projection_generation = self.derived_projection_generation.wrapping_add(1);
        let generation = self.derived_projection_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        let document_epoch = self.document_epoch;
        let revision = document.revision();
        let item_limit = provider.descriptor().max_items.unwrap_or(1_500);
        let request = DerivedProjectionRequest {
            document_epoch,
            revision,
            generation,
            root: None,
            item_limit,
        };
        let request_for_worker = request.clone();
        let cancellation = SearchCancellation::default();
        self.derived_projection_cancellation = Some(cancellation.clone());
        self.derived_projection_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    provider.build(&document, &request_for_worker, &cancellation)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.derived_projection_generation)
                    || view.active_derived_view.as_ref() != Some(&id)
                {
                    view.metrics.stale_projection_results =
                        view.metrics.stale_projection_results.saturating_add(1);
                    return;
                }
                view.derived_projection_cancellation = None;
                if let Ok(snapshot) = result
                    && request.accepts(snapshot.as_ref())
                    && !matches!(
                        snapshot.status(),
                        DerivedProjectionStatus::Failed | DerivedProjectionStatus::Cancelled
                    )
                {
                    view.derived_projection_snapshot = Some(snapshot);
                    view.metrics.projection_installs =
                        view.metrics.projection_installs.saturating_add(1);
                }
                cx.notify();
            });
        });
    }

    pub(super) fn set_structured_column_window_start(
        &mut self,
        requested: usize,
        cx: &mut Context<Self>,
    ) {
        let column_count = self
            .structured_index
            .as_ref()
            .map(|index| index.headers().len())
            .unwrap_or(0);
        let last_window =
            column_count.saturating_sub(1) / STRUCTURED_COLUMN_WINDOW * STRUCTURED_COLUMN_WINDOW;
        let start = requested.min(last_window);
        if self.structured_column_window_start == start {
            return;
        }
        // 行缓存只包含当前列窗口；换窗必须与后台 generation 一起失效，防止旧列落回。
        self.structured_column_window_start = start;
        self.structured_generation = self.structured_generation.wrapping_add(1);
        self.structured_pending = None;
        self.structured_rows.clear();
        cx.notify();
    }

    /// 撤销回磁盘基线后，结构视图应自行恢复，而不是要求用户再保存或重开文件。
    /// 构建仍放后台；generation 同时防止随后的 redo/编辑发布过期索引。
    pub(super) fn rebuild_clean_structured_index(&mut self, cx: &mut Context<Self>) {
        if self.dirty || self.structured_index.is_some() {
            return;
        }
        let Some(source) = self
            .prepared_source
            .as_ref()
            .map(|prepared| prepared.source().clone())
        else {
            return;
        };
        let Some(index) = self.index.clone() else {
            return;
        };
        let format = self.probe.format.clone();
        let cancellation = SearchCancellation::default();
        self.structured_cancellation = Some(cancellation.clone());
        self.structured_generation = self.structured_generation.wrapping_add(1);
        let generation = self.structured_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        self.structure_error = Some("Rebuilding structured view…".into());
        self.structure_error_byte = None;
        self.structured_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    build_structured_index(&source, &index, format, &cancellation)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.structured_generation) || view.dirty {
                    return;
                }
                view.structured_cancellation = None;
                match result {
                    Ok(structured) => {
                        view.structured_index = structured;
                        view.clear_structure_error();
                    }
                    Err(error) => view.set_structure_error(error),
                }
                cx.emit(DiskSourceEvent::StateChanged);
                cx.notify();
            });
        });
    }

    pub(super) fn on_horizontal_container_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(px(26.0));
        // 内层 uniform_list 负责纵向虚拟滚动；外层横向 overflow 只处理 Shift/触控板横移，
        // 避免普通鼠标滚轮同时把列推向右侧。
        if !event.modifiers.shift && f32::from(delta.y).abs() >= f32::from(delta.x).abs() {
            cx.stop_propagation();
        }
    }

    pub(super) fn max_source_window_start(&self) -> u64 {
        let indexed = self.index.as_ref().map_or(0, LineIndex::max_line_bytes);
        let selected = self
            .selected_lines
            .as_ref()
            .and_then(|selection| selection.start.try_into().ok())
            .and_then(|line| self.document.as_ref()?.line_range(line))
            .map_or(0, |range| range.end.saturating_sub(range.start));
        indexed
            .max(selected)
            .saturating_sub(MAX_RENDERED_LINE_BYTES)
    }

    pub(super) fn set_source_window_start(&mut self, start: u64, cx: &mut Context<Self>) {
        let next = start.min(self.max_source_window_start());
        if self.source_window_start == next {
            return;
        }
        // 编辑按键已即时提交到 PieceDocument；横向离开当前块时只释放有界输入窗口。
        self.active_edit = None;
        self.source_window_start = next;
        self.invalidate_source_rows();
        cx.emit(DiskSourceEvent::StateChanged);
        cx.notify();
    }

    /// 只在后台读取 viewport 与 overscan。发布时同时校验 generation、横向窗口和文档快照，
    /// 因而快速滚动或编辑后的旧磁盘结果永远不能覆盖当前画面。
    pub(super) fn request_source_rows(&mut self, visible: Range<usize>, cx: &mut Context<Self>) {
        if self.document.is_none() {
            self.provisional_anchor = Some(SourceAnchor::new(
                ((self.probe.len as u128 * visible.start as u128)
                    / self.probe.estimated_lines.max(1) as u128) as u64,
                SourceAffinity::Before,
            ));
        }
        let reader = if let Some(document) = self.document.clone() {
            SourceViewportReader::Indexed(Box::new(document))
        } else if let Some(source) = self.provisional_source.clone() {
            SourceViewportReader::Provisional {
                source,
                estimated_lines: self.probe.estimated_lines.max(1),
                encoding: self.probe.encoding.clone(),
            }
        } else {
            return;
        };
        let total = self.line_count();
        let cache_epoch = self.source_cache_epoch;
        let scrolling_forward = self
            .source_last_visible
            .as_ref()
            .is_none_or(|previous| visible.start >= previous.start);
        self.source_last_visible = Some(visible.clone());
        let (overscan_before, overscan_after) = if scrolling_forward {
            (SOURCE_OVERSCAN_ROWS, SOURCE_OVERSCAN_ROWS.saturating_mul(2))
        } else {
            (SOURCE_OVERSCAN_ROWS.saturating_mul(2), SOURCE_OVERSCAN_ROWS)
        };
        let requested = visible.start.saturating_sub(overscan_before)
            ..visible.end.saturating_add(overscan_after).min(total);
        let requested_is_cached = requested.is_empty()
            || requested
                .clone()
                .all(|line| self.source_row_epochs.get(&line) == Some(&cache_epoch));
        if let Some(pending) = self.source_pending.as_ref() {
            let disjoint = pending.end <= requested.start || requested.end <= pending.start;
            if requested_is_cached && disjoint && !self.source_cancel_in_flight {
                if let Some(cancellation) = self.source_cancellation.take() {
                    cancellation.cancel();
                }
                // 最新 viewport 已由当前 cache 完整满足：推进 generation 令后台结果过期，
                // 并在 UI 侧立即结束 task 所有权，避免无意义 completion/defer 自调度。
                self.source_generation = self.source_generation.wrapping_add(1);
                self.source_pending = None;
                self.source_queued_visible = None;
                self.source_cancel_in_flight = false;
                self.source_task = Task::ready(());
                self.metrics.viewport_cancellations =
                    self.metrics.viewport_cancellations.saturating_add(1);
                return;
            }
            // 保留最新可见范围。当前任务发布后立即补读，不依赖下一次 render 回调；
            // 远跳与当前读取完全不相交时立即取消磁盘循环，连续滚动的重叠请求则让
            // 当前任务完成，避免滚轮小步移动造成取消风暴。
            self.source_queued_visible = Some(visible);
            if !self.source_cancel_in_flight
                && disjoint
                && let Some(cancellation) = self.source_cancellation.as_ref()
            {
                cancellation.cancel();
                self.source_cancel_in_flight = true;
                self.metrics.viewport_cancellations =
                    self.metrics.viewport_cancellations.saturating_add(1);
            }
            return;
        }
        if requested_is_cached {
            self.source_cancel_in_flight = false;
            return;
        }
        self.source_generation = self.source_generation.wrapping_add(1);
        self.metrics.viewport_requests = self.metrics.viewport_requests.saturating_add(1);
        let generation = self.source_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        let window_start = self.source_window_start;
        let requested_center = requested.start.saturating_add(requested.len() / 2);
        let viewport_request = ViewportRequest::bounded(
            requested.start as u64,
            requested.len(),
            0,
            window_start,
            generation,
        );
        self.source_pending = Some(requested.clone());
        let cancellation = SearchCancellation::default();
        self.source_cancellation = Some(cancellation.clone());
        let installed_range = requested.clone();
        self.source_task = cx.spawn(async move |this, cx| {
            let rows = cx
                .background_spawn(async move {
                    match reader {
                        SourceViewportReader::Indexed(document) => document
                            .read_viewport_cancellable(&viewport_request, &cancellation)
                            .map(|snapshot| {
                                snapshot
                                    .lines
                                    .into_iter()
                                    .filter_map(|line| {
                                        Some((
                                            usize::try_from(line.line).ok()?,
                                            BoundedLineWindow::new(
                                                line.content_range,
                                                line.source_range,
                                                line.text,
                                                line.ending,
                                                line.leading_truncated,
                                                line.trailing_truncated,
                                            ),
                                        ))
                                    })
                                    .collect::<Vec<_>>()
                            }),
                        SourceViewportReader::Provisional {
                            source,
                            estimated_lines,
                            encoding,
                        } => read_provisional_source_rows(
                            &source,
                            estimated_lines,
                            requested,
                            window_start,
                            &encoding,
                            &cancellation,
                        ),
                    }
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.source_generation)
                    || view.source_cache_epoch != cache_epoch
                    || view.source_window_start != window_start
                {
                    view.metrics.stale_viewport_results =
                        view.metrics.stale_viewport_results.saturating_add(1);
                    return;
                }
                view.source_pending = None;
                view.source_cancellation = None;
                match rows {
                    Ok(rows) => {
                        view.source_cancel_in_flight = false;
                        for (line, row) in rows {
                            let row_changed = view
                                .source_rows
                                .get(&line)
                                .is_none_or(|previous| !previous.has_same_surface_text(&row));
                            if row_changed
                                && view
                                    .active_edit
                                    .as_ref()
                                    .is_none_or(|active| active.line != line)
                            {
                                view.source_row_blocks.remove(&line);
                            }
                            view.source_rows.insert(line, Arc::new(row));
                            view.source_row_epochs.insert(line, cache_epoch);
                        }
                        if !view.soak_ready_published && !view.source_rows.is_empty() {
                            view.publish_soak_ready_marker();
                            view.soak_ready_published = true;
                        }
                        // 保留相邻帧的重叠 viewport，避免程序化跳转时新旧范围互相驱逐；
                        // 超预算后只淘汰离当前请求最远的端点，缓存仍与文件大小解耦。
                        while view.source_rows.len() > MAX_SOURCE_CACHED_ROWS {
                            let first = view.source_rows.first_key_value().map(|(line, _)| *line);
                            let last = view.source_rows.last_key_value().map(|(line, _)| *line);
                            let evicted = match (first, last) {
                                (Some(first), Some(last))
                                    if requested_center.saturating_sub(first)
                                        >= last.saturating_sub(requested_center) =>
                                {
                                    first
                                }
                                (_, Some(last)) => last,
                                _ => break,
                            };
                            view.source_rows.remove(&evicted);
                            view.source_row_epochs.remove(&evicted);
                            if view
                                .active_edit
                                .as_ref()
                                .is_none_or(|active| active.line != evicted)
                            {
                                view.source_row_blocks.remove(&evicted);
                            }
                        }
                        let next_rows = view
                            .source_rows
                            .iter()
                            .filter(|(line, _)| {
                                view.source_row_epochs.get(line) == Some(&cache_epoch)
                            })
                            .map(|(line, row)| (*line, row.clone()))
                            .collect::<BTreeMap<_, _>>();
                        if !next_rows.is_empty() {
                            // 只有完整后台结果和 cache 元数据都就绪后才交换快照。
                            // pending 期间继续显示上一份 ScreenLines，正文不会退回空白帧。
                            let visible = view
                                .source_last_visible
                                .clone()
                                .unwrap_or_else(|| installed_range.clone());
                            let document_revision = view
                                .document
                                .as_ref()
                                .map_or(0, LargeDocumentAdapter::revision);
                            view.displayed_screen_lines = Arc::new(ScreenLines {
                                document_revision,
                                generation,
                                cache_epoch,
                                column_window_start: window_start,
                                visible,
                                rows: Arc::new(next_rows),
                            });
                            view.metrics.viewport_installs =
                                view.metrics.viewport_installs.saturating_add(1);
                            view.metrics.max_cached_rows = view
                                .metrics
                                .max_cached_rows
                                .max(view.displayed_screen_lines.rows.len());
                        }
                        if let Some(top_anchor) = view.displayed_screen_lines.top_source_anchor() {
                            view.view_state.source.top_byte_anchor = top_anchor;
                            view.view_state.source.line_offset_y = 0.0;
                            // provisional 逻辑行只是估算坐标。每次真实行窗口安装后保存其
                            // source byte anchor，全文索引收敛时才能回到相同正文而非 byte 0。
                            if view.document.is_none() {
                                view.provisional_anchor = Some(top_anchor);
                            }
                        }
                    }
                    Err(LargeDocumentError::Cancelled) => {}
                    Err(error) => {
                        view.source_cancel_in_flight = false;
                        view.error = Some(error.to_string().into());
                    }
                }
                let queued = view.source_queued_visible.take();
                cx.notify();
                if let Some(visible) = queued {
                    // 不在即将完成的 source_task 内覆盖并 drop 自己。TestApp 会让已取消
                    // 的后台读取立即就绪，直接递归启动下一任务会形成忙循环；defer 同时
                    // 保证生产 executor 的任务所有权边界清晰。
                    let this = cx.entity().downgrade();
                    cx.defer(move |cx| {
                        let _ = this.update(cx, |view, cx| view.request_source_rows(visible, cx));
                    });
                }
            });
        });
    }

    pub(super) fn invalidate_source_rows(&mut self) {
        self.source_generation = self.source_generation.wrapping_add(1);
        if let Some(cancellation) = self.source_cancellation.take() {
            cancellation.cancel();
        }
        self.source_cancel_in_flight = false;
        self.source_cache_epoch = self.source_cache_epoch.wrapping_add(1);
        self.source_pending = None;
        self.source_queued_visible = None;
        self.source_task = Task::ready(());
    }

    /// 生产 soak 不能把“进程还活着”误判成“大文件已打开”。只有首个 Source
    /// viewport 真正安装后才发布 marker；普通运行没有该环境变量，不产生任何 I/O。
    pub(super) fn publish_soak_ready_marker(&self) {
        let Some(marker) = std::env::var_os("GMARK_SOAK_READY_PATH").map(PathBuf::from) else {
            return;
        };
        let payload = serde_json::json!({
            "schema_version": 1,
            "process_id": std::process::id(),
            "path": self.path,
            "file_len": self.probe.len,
            "line_count": self.line_count(),
            "visible_rows": self.source_rows.len(),
            "mode": "source",
        });
        let result = (|| -> std::io::Result<()> {
            if let Some(parent) = marker.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let temporary = marker.with_extension(format!("tmp-{}", std::process::id()));
            let bytes = serde_json::to_vec_pretty(&payload).map_err(std::io::Error::other)?;
            std::fs::write(&temporary, bytes)?;
            std::fs::rename(&temporary, &marker)
        })();
        if let Err(error) = result {
            eprintln!(
                "failed to publish large-file soak readiness '{}': {error}",
                marker.display()
            );
        }
    }

    pub(super) fn anchor_source_window_for_byte(&mut self, line: u64, byte_offset: u64) {
        let Some(range) = self
            .document
            .as_ref()
            .and_then(|document| document.line_range(line))
        else {
            if self.source_window_start != 0 {
                self.source_window_start = 0;
                self.invalidate_source_rows();
            }
            return;
        };
        let relative = byte_offset
            .clamp(range.start, range.end)
            .saturating_sub(range.start);
        let next = source_window_start_for_anchor(range.end.saturating_sub(range.start), relative);
        if self.source_window_start != next {
            self.source_window_start = next;
            self.invalidate_source_rows();
        }
    }

    pub(super) fn on_source_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(px(26.0));
        let horizontal =
            event.modifiers.shift || f32::from(delta.x).abs() > f32::from(delta.y).abs();
        if !horizontal {
            // 局部 uniform_list 内部保持原生滚动；到达窗口边界后，把下一次滚轮
            // 映射回全局行并重建局部 origin，文件大小不再进入 f32 像素坐标。
            let handle = self.scroll_handle.0.borrow().base_handle.clone();
            let row_height = self.source_row_height.max(1.0);
            let local_top = (-f32::from(handle.offset().y) / row_height)
                .max(0.0)
                .floor() as usize;
            let visible_rows = (f32::from(handle.bounds().size.height) / row_height)
                .ceil()
                .max(1.0) as usize;
            let axis = f32::from(delta.y);
            let step = (axis.abs() / row_height).ceil().max(1.0) as usize;
            let at_start = local_top == 0 && self.source_list_origin > 0;
            let at_end = local_top.saturating_add(visible_rows) >= self.source_list_len()
                && self
                    .source_list_origin
                    .saturating_add(self.source_list_len())
                    < self.line_count();
            if axis > 0.0 && at_start {
                let target = self.source_list_origin.saturating_sub(step);
                self.source_list_origin = source_list_origin_for_target(self.line_count(), target);
                self.scroll_source_line_strict(target, ScrollStrategy::Top);
                cx.notify();
            } else if axis < 0.0 && at_end {
                let target = self
                    .source_list_origin
                    .saturating_add(local_top)
                    .saturating_add(step)
                    .min(self.line_count().saturating_sub(1));
                self.source_list_origin = source_list_origin_for_target(self.line_count(), target);
                self.scroll_source_line_strict(target, ScrollStrategy::Top);
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        let axis = if event.modifiers.shift {
            f32::from(delta.y)
        } else {
            f32::from(delta.x)
        };
        let byte_delta = (-axis * SOURCE_SCROLL_BYTES_PER_PIXEL).round() as i64;
        let next = shift_source_window_start(
            self.source_window_start,
            byte_delta,
            self.max_source_window_start(),
        );
        self.set_source_window_start(next, cx);
        cx.stop_propagation();
    }

    pub(super) fn on_search_input_event(
        &mut self,
        block: Entity<Block>,
        event: &BlockEvent,
        cx: &mut Context<Self>,
    ) {
        if block == self.search_input && matches!(event, BlockEvent::Changed) {
            self.schedule_search(cx);
        }
    }

    pub(super) fn on_search_host_action(
        &mut self,
        action: BlockHostAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(&action, BlockHostAction::Submit(_)) {
            self.navigate_search(1, cx);
        } else {
            self.on_line_edit_host_action(action, window, cx);
        }
    }

    pub(super) fn on_navigation_input_event(
        &mut self,
        block: Entity<Block>,
        event: &BlockEvent,
        cx: &mut Context<Self>,
    ) {
        if block != self.navigation_input || !matches!(event, BlockEvent::Changed) {
            return;
        }
        let input = block.read(cx).display_text().trim().replace(['_', ','], "");
        let Ok(value) = input.parse::<u64>() else {
            return;
        };
        let line = if let Some(document) = &self.document {
            if self.navigation_is_byte {
                document.line_for_offset(value.min(document.len()))
            } else {
                Some(
                    value
                        .saturating_sub(1)
                        .min(document.line_count().saturating_sub(1)),
                )
            }
        } else if self.navigation_is_byte {
            Some(
                ((value.min(self.probe.len) as u128 * self.probe.estimated_lines.max(1) as u128)
                    / self.probe.len.max(1) as u128) as u64,
            )
        } else {
            Some(
                value
                    .saturating_sub(1)
                    .min(self.probe.estimated_lines.saturating_sub(1)),
            )
        };
        let Some(line) = line.and_then(|line| usize::try_from(line).ok()) else {
            return;
        };
        if self.navigation_is_byte {
            if let Some(document) = &self.document {
                self.anchor_source_window_for_byte(line as u64, value.min(document.len()));
            } else {
                self.source_window_start = 0;
                self.invalidate_source_rows();
            }
        } else {
            if self.source_window_start != 0 {
                self.source_window_start = 0;
                self.invalidate_source_rows();
            }
        }
        self.view_mode = LargeViewMode::Source;
        self.select_source_lines(line..line.saturating_add(1), false);
        self.scroll_source_line(line, ScrollStrategy::Center);
        cx.emit(DiskSourceEvent::StateChanged);
        cx.notify();
    }

    pub(super) fn on_navigation_host_action(
        &mut self,
        action: BlockHostAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(&action, BlockHostAction::Submit(_)) {
            self.navigation_visible = false;
            self.active_edit = None;
            self.focus_handle.focus(window);
            cx.notify();
        } else {
            self.on_line_edit_host_action(action, window, cx);
        }
    }

    pub(super) fn on_structured_filter_input_event(
        &mut self,
        block: Entity<Block>,
        event: &BlockEvent,
        cx: &mut Context<Self>,
    ) {
        if block == self.structured_filter_input && matches!(event, BlockEvent::Changed) {
            self.schedule_structured_filter(cx);
        }
    }

    pub(super) fn schedule_structured_filter(&mut self, cx: &mut Context<Self>) {
        if let Some(cancellation) = self.structured_filter_cancellation.take() {
            cancellation.cancel();
        }
        self.structured_filter_generation = self.structured_filter_generation.wrapping_add(1);
        let generation = self.structured_filter_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        let query = self
            .structured_filter_input
            .read(cx)
            .display_text()
            .trim()
            .to_owned();
        if let Some(id) = self.active_derived_view.clone() {
            self.view_state.derived.entry(id).or_default().filter = Arc::from(query.clone());
        }
        if query.is_empty() {
            self.structured_filtered_rows.clear();
            self.structured_filter_running = false;
            self.structured_rows.clear();
            self.structured_pending = None;
            cx.notify();
            return;
        }
        let Some(StructuredIndex::Delimited(index)) = self.structured_index.clone() else {
            self.structure_error = Some("Column filtering is available for CSV/TSV".into());
            self.structure_error_byte = None;
            return;
        };
        let cancellation = SearchCancellation::default();
        self.structured_filter_cancellation = Some(cancellation.clone());
        self.structured_filter_running = true;
        let options = DelimitedFilterOptions {
            column: self.structured_filter_column,
            case_sensitive: self.search_options.case_sensitive,
            result_limit: 10_000,
        };
        self.structured_filter_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    index.filter_record_indices(&query, options, &cancellation)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.structured_filter_generation) {
                    return;
                }
                view.structured_filter_running = false;
                view.structured_filter_cancellation = None;
                view.structured_rows.clear();
                view.structured_pending = None;
                match result {
                    Ok(rows) => {
                        view.structured_filtered_rows = rows;
                        view.clear_structure_error();
                    }
                    Err(gmark_large_document::LargeDocumentError::Cancelled) => {}
                    Err(error) => view.set_structure_error(error),
                }
                cx.notify();
            });
        });
        cx.notify();
    }

    pub(super) fn schedule_search(&mut self, cx: &mut Context<Self>) {
        if let Some(cancellation) = self.search_cancellation.take() {
            cancellation.cancel();
        }
        self.search_generation = self.search_generation.wrapping_add(1);
        let generation = self.search_generation;
        let query = self.search_input.read(cx).display_text().to_owned();
        if query.is_empty() {
            self.search_results.clear();
            self.search_selected = 0;
            self.search_running = false;
            self.search_error = None;
            cx.notify();
            return;
        }
        let document = self.document.clone();
        let provisional_source = self.provisional_source.clone();
        if document.is_none() && provisional_source.is_none() {
            return;
        }
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        let options = self.search_options;
        let cancellation = SearchCancellation::default();
        self.search_cancellation = Some(cancellation.clone());
        self.search_running = true;
        self.search_error = None;
        self.search_task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(60))
                .await;
            let first_document = document.clone();
            let first_source = provisional_source.clone();
            let first_query = query.clone();
            let first_cancellation = cancellation.clone();
            let first_options = SearchOptions {
                result_limit: 1,
                ..options
            };
            let first = cx
                .background_spawn(async move {
                    search_large_reader(
                        first_document.as_ref(),
                        first_source.as_ref(),
                        &first_query,
                        first_options,
                        &first_cancellation,
                    )
                })
                .await;
            let continue_full = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.search_generation) {
                    return false;
                }
                match first {
                    Ok(matches) => {
                        view.search_results = matches;
                        view.search_selected = 0;
                        view.search_error = None;
                        if !view.search_results.is_empty() {
                            view.jump_to_search_result(cx);
                        } else {
                            view.search_running = false;
                            view.search_cancellation = None;
                        }
                    }
                    Err(gmark_large_document::LargeDocumentError::Cancelled) => {
                        return false;
                    }
                    Err(error) => {
                        view.search_running = false;
                        view.search_cancellation = None;
                        view.search_results.clear();
                        view.search_error = Some(error.to_string().into());
                    }
                }
                cx.notify();
                view.search_running && options.result_limit > 1
            });
            let Ok(true) = continue_full else {
                return;
            };
            let result = cx
                .background_spawn(async move {
                    search_large_reader(
                        document.as_ref(),
                        provisional_source.as_ref(),
                        &query,
                        options,
                        &cancellation,
                    )
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.search_generation) {
                    return;
                }
                view.search_running = false;
                view.search_cancellation = None;
                match result {
                    Ok(matches) => {
                        view.search_results = matches;
                        view.search_selected = 0;
                        view.search_error = None;
                    }
                    Err(gmark_large_document::LargeDocumentError::Cancelled) => {}
                    Err(error) => {
                        view.search_results.clear();
                        view.search_error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            });
        });
        cx.notify();
    }
}
