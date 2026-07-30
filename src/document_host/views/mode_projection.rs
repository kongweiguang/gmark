// @author kongweiguang

//! Derived projection scheduling and clean index rebuilding.

use super::*;

impl DocumentHost {
    pub(super) fn request_registered_projection(&mut self, cx: &mut Context<Self>) {
        let provider = self
            .selected_projection_view
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
        self.selected_projection_view = Some(id.clone());
        document_view_state_mut(&mut self.document, &mut self.tab_view_state)
            .derived
            .entry(id.clone())
            .or_default();
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
            root: self
                .derived_projection_root
                .as_ref()
                .map(|root| SourceLocator::new(root.source.range.clone())),
            item_limit,
        };
        if let Some(root) = self.derived_projection_root.clone()
            && let Ok(mut roots) = self.json_focused_roots.lock()
        {
            roots.clear();
            roots.insert((document_epoch, generation), root);
        }
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
                    || view.selected_projection_view.as_ref() != Some(&id)
                {
                    view.metrics.stale_projection_results =
                        view.metrics.stale_projection_results.saturating_add(1);
                    let profile = view.probe.profile();
                    let plan = session_plan(&profile, &view.probe, view.probe.strategy, false);
                    crate::perf::emit_document_value(
                        "document_projection_cancelled",
                        view.metrics.stale_projection_results,
                        &profile.format,
                        &plan,
                    );
                    return;
                }
                view.derived_projection_cancellation = None;
                match result {
                    Ok(snapshot)
                        if request.accepts(snapshot.as_ref())
                            && !matches!(
                                snapshot.status(),
                                DerivedProjectionStatus::Failed
                                    | DerivedProjectionStatus::Cancelled
                            ) =>
                    {
                        view.derived_projection_snapshot = Some(snapshot);
                        view.derived_projection_error = None;
                        view.derived_projection_error_offset = None;
                        view.derived_projection_stale = false;
                        view.metrics.projection_installs =
                            view.metrics.projection_installs.saturating_add(1);
                    }
                    Err(error @ ProjectionError::InvalidJson { offset, .. }) => {
                        let strings = cx.global::<I18nManager>().strings();
                        let location = view.document.as_ref().and_then(|document| {
                            let line = document.line_for_offset(offset.min(document.len()))?;
                            let line_start = document.line_range(line)?.start;
                            Some((line + 1, offset.saturating_sub(line_start) + 1))
                        });
                        view.derived_projection_error = Some(match location {
                            Some((line, column)) => strings
                                .large_document_text("error_invalid_json_location")
                                .replace("{line}", &line.to_string())
                                .replace("{column}", &column.to_string())
                                .into(),
                            None => error.to_string().into(),
                        });
                        view.derived_projection_error_offset = Some(offset);
                        view.derived_projection_stale = view.derived_projection_snapshot.is_some();
                    }
                    Err(ProjectionError::Cancelled | ProjectionError::SourceChanged) => {}
                    Err(error) => {
                        view.derived_projection_error = Some(error.to_string().into());
                        view.derived_projection_error_offset = None;
                        view.derived_projection_stale = view.derived_projection_snapshot.is_some();
                    }
                    _ => {}
                }
                cx.notify();
            });
        });
    }

    /// JSON 图更新与 Source 输入解耦；连续按键只保留最后一个 revision 的后台构建。
    pub(super) fn schedule_json_graph_projection(&mut self, cx: &mut Context<Self>) {
        if self.probe.format != DocumentFormat::Json {
            return;
        }
        self.derived_projection_generation = self.derived_projection_generation.wrapping_add(1);
        if let Some(cancellation) = self.derived_projection_cancellation.take() {
            cancellation.cancel();
        }
        let generation = self.derived_projection_generation;
        self.derived_projection_task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.derived_projection_generation == generation {
                    view.request_registered_projection(cx);
                }
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
        if !derived_views_enabled(self.probe.strategy)
            || document_dirty_state(&self.document, &self.pending_dirty)
            || self.structured_index.is_some()
        {
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
        self.structure_error = Some(
            cx.global::<I18nManager>()
                .strings()
                .large_document_text("rebuilding_structured")
                .into(),
        );
        self.structure_error_byte = None;
        self.structured_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    build_structured_index(&source, &index, format, &cancellation, None)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.structured_generation)
                    || document_dirty_state(&view.document, &view.pending_dirty)
                {
                    return;
                }
                view.structured_cancellation = None;
                match result {
                    Ok(structured) => {
                        view.structured_index = structured;
                        view.clear_structure_error();
                    }
                    Err(error) => view.set_structure_error(error, cx),
                }
                cx.emit(DocumentHostEvent::StateChanged);
                cx.notify();
            });
        });
    }
}
