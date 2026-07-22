// @author kongweiguang

//! Window-level editor state such as scrolling, mode switching, and menus.

use super::*;

impl Editor {
    pub(super) fn scrollbar_geometry(
        viewport_height: f32,
        max_scroll_y: f32,
        current_scroll_y: f32,
    ) -> ScrollbarGeometry {
        let track_height = viewport_height.max(20.0);
        let content_height = viewport_height + max_scroll_y;
        let thumb_height = if max_scroll_y > 0.5 {
            (track_height * (viewport_height / content_height)).clamp(28.0, track_height)
        } else {
            track_height
        };
        let progress = if max_scroll_y > 0.0 {
            current_scroll_y.clamp(0.0, max_scroll_y) / max_scroll_y
        } else {
            0.0
        };
        let thumb_top = (track_height - thumb_height).max(0.0) * progress;
        ScrollbarGeometry {
            track_height,
            thumb_height,
            thumb_top,
            max_scroll_y,
        }
    }

    pub(super) fn scroll_offset_for_thumb_top(
        thumb_top: f32,
        track_height: f32,
        thumb_height: f32,
        max_scroll_y: f32,
    ) -> f32 {
        if max_scroll_y <= 0.0 {
            return 0.0;
        }

        let travel = (track_height - thumb_height).max(0.0);
        if travel <= 0.0 {
            return 0.0;
        }

        let progress = (thumb_top / travel).clamp(0.0, 1.0);
        max_scroll_y * progress
    }

    /// Picks the contiguous run of rows to mount; the culled runs become two
    /// spacers and the focused row stays mounted. `strides[i]` is row `i`'s
    /// footprint (height plus trailing gap); being scroll-invariant, their running
    /// sum places each row against a band from the current scroll offset.
    /// Unmeasured rows use a lower-bound estimate. The caller must extend the run
    /// to its measurement frontier before painting, so a restored deep offset
    /// cannot land beyond the estimated document height. Pure, so it is unit-tested
    /// headlessly.
    pub(super) fn rendered_window(
        strides: &[f32],
        scroll_y: f32,
        viewport_height: f32,
        overdraw: f32,
        focus_row: Option<usize>,
    ) -> RenderWindow {
        let n = strides.len();
        if n == 0 {
            return RenderWindow {
                run_start: 0,
                run_end: 0,
                top_h: 0.0,
                bottom_h: 0.0,
            };
        }

        let band_top = scroll_y - overdraw;
        let band_bottom = scroll_y + viewport_height + overdraw;

        let mut run_start = n;
        let mut run_end = 0usize;
        let mut top_of_start = 0.0f32;
        let mut bottom_of_end = 0.0f32;
        let mut cursor = 0.0f32;
        for (index, &stride) in strides.iter().enumerate() {
            let top = cursor;
            let bottom = cursor + stride.max(0.0);
            if bottom >= band_top && top <= band_bottom {
                if index < run_start {
                    run_start = index;
                    top_of_start = top;
                }
                run_end = index + 1;
                bottom_of_end = bottom;
            }
            cursor = bottom;
        }
        let total = cursor;

        // Nothing hit the band (float edge, or estimate short of scroll): mount
        // the last row so the viewport never lands on a spacer.
        if run_start >= run_end {
            run_start = n - 1;
            run_end = n;
            top_of_start = total - strides[n - 1].max(0.0);
            bottom_of_end = total;
        }

        // Keep the focused row mounted; GPUI blurs an unmounted caret. Reaching a
        // far focus row widens the run, but autoscroll makes that rare.
        if let Some(focus_row) = focus_row {
            let focus_row = focus_row.min(n - 1);
            if focus_row < run_start {
                run_start = focus_row;
                top_of_start = strides[..focus_row].iter().map(|s| s.max(0.0)).sum();
            }
            if focus_row + 1 > run_end {
                run_end = focus_row + 1;
                bottom_of_end = strides[..=focus_row].iter().map(|s| s.max(0.0)).sum();
            }
        }

        RenderWindow {
            run_start,
            run_end,
            top_h: top_of_start.max(0.0),
            bottom_h: (total - bottom_of_end).max(0.0),
        }
    }

    /// 未测量行的最小高度只适合裁剪已知前缀；恢复到较深滚动位置时，必须从首个
    /// 未测量行连续挂载到目标窗口，避免低估总高后只渲染末行并留下整屏空白。
    pub(super) fn include_render_measurement_frontier(
        mut window: RenderWindow,
        strides: &[f32],
        measurement_frontier: usize,
    ) -> RenderWindow {
        let frontier = measurement_frontier.min(strides.len());
        if frontier < window.run_start {
            window.run_start = frontier;
            window.top_h = strides[..frontier]
                .iter()
                .map(|stride| stride.max(0.0))
                .sum();
        }
        window
    }

    /// 小文档完整挂载可避免滚动与行高学习之间的空白帧；超过阈值后才启用裁剪。
    /// 只有恢复偏移已经落在估算总高之外时才扩展到测量前沿；普通深滚动必须保持
    /// 有界窗口，否则一个未测量的首行会让数百行被同时挂载。
    pub(super) fn rendered_document_window(
        strides: &[f32],
        scroll_y: f32,
        viewport_height: f32,
        overdraw: f32,
        measurement_frontier: usize,
        restoring_deep_offset: bool,
        virtualization_threshold: usize,
    ) -> RenderWindow {
        if strides.len() < virtualization_threshold {
            return RenderWindow {
                run_start: 0,
                run_end: strides.len(),
                top_h: 0.0,
                bottom_h: 0.0,
            };
        }
        let window = Self::rendered_window(strides, scroll_y, viewport_height, overdraw, None);
        let estimated_total = strides.iter().map(|stride| stride.max(0.0)).sum::<f32>();
        if restoring_deep_offset && scroll_y > estimated_total {
            Self::include_render_measurement_frontier(window, strides, measurement_frontier)
        } else {
            window
        }
    }

    /// Builds the OS window title, including the dirty marker when the
    /// document has unsaved changes.
    pub(super) fn window_title(
        file_path: Option<&Path>,
        is_dirty: bool,
        strings: &crate::i18n::I18nStrings,
    ) -> String {
        let base_title = if let Some(path) = file_path {
            format!(
                "gmark - {}",
                path.file_name().map_or_else(
                    || path.to_string_lossy().to_string(),
                    |name| name.to_string_lossy().to_string()
                )
            )
        } else {
            "gmark".to_string()
        };

        if is_dirty && !strings.dirty_title_marker.is_empty() {
            format!("{} {}", strings.dirty_title_marker, base_title)
        } else {
            base_title
        }
    }

    pub(crate) fn on_toggle_view_mode_action(
        &mut self,
        _: &crate::components::ToggleViewMode,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_view_mode_from_ui(cx);
    }

    pub(super) fn toggle_view_mode_from_ui(&mut self, cx: &mut Context<Self>) {
        self.end_block_pointer_selection_sessions(cx);
        self.last_selection_snapshot = self.capture_source_selection_snapshot(cx);
        self.toggle_view_mode(cx);
    }

    pub(crate) fn on_undo(
        &mut self,
        action: &crate::components::Undo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(document_host) = self.document_host.clone() {
            document_host.update(cx, |document_host, cx| {
                document_host.on_undo(action, window, cx);
            });
            return;
        }
        self.undo_document(cx);
    }

    pub(crate) fn on_redo(
        &mut self,
        action: &crate::components::Redo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(document_host) = self.document_host.clone() {
            document_host.update(cx, |document_host, cx| {
                document_host.on_redo(action, window, cx);
            });
            return;
        }
        self.redo_document(cx);
    }

    pub(crate) fn on_save_document(
        &mut self,
        action: &crate::components::SaveDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(document_host) = self.document_host.clone() {
            document_host.update(cx, |document_host, cx| {
                document_host.on_save_document(action, window, cx);
            });
            return;
        }
        self.request_save_document(cx);
    }

    pub(crate) fn on_save_document_as(
        &mut self,
        _: &crate::components::SaveDocumentAs,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_save_document_as(cx);
    }

    pub(crate) fn on_normalize_line_endings_lf(
        &mut self,
        _: &crate::components::NormalizeLineEndingsLf,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.normalize_line_endings(gmark_document::LineEnding::Lf, cx);
    }

    pub(crate) fn on_normalize_line_endings_crlf(
        &mut self,
        _: &crate::components::NormalizeLineEndingsCrLf,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.normalize_line_endings(gmark_document::LineEnding::CrLf, cx);
    }

    pub(crate) fn on_normalize_line_endings_cr(
        &mut self,
        _: &crate::components::NormalizeLineEndingsCr,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.normalize_line_endings(gmark_document::LineEnding::Cr, cx);
    }

    pub(crate) fn on_export_html(
        &mut self,
        _: &crate::components::ExportHtml,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_document_via_prompt(crate::export::ExportFormat::Html, window, cx);
    }

    pub(crate) fn on_export_pdf(
        &mut self,
        _: &crate::components::ExportPdf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_document_via_prompt(crate::export::ExportFormat::Pdf, window, cx);
    }

    pub(crate) fn on_export_image(
        &mut self,
        _: &crate::components::ExportImage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_document_via_prompt(crate::export::ExportFormat::Png, window, cx);
    }

    pub(crate) fn on_quit_application(
        &mut self,
        _: &crate::components::QuitApplication,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::app_menu::request_quit_application(cx);
    }

    pub(crate) fn on_close_window(
        &mut self,
        _: &crate::components::CloseWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_close_current_window(window, cx);
    }

    pub(crate) fn on_install_cli_tool(
        &mut self,
        _: &crate::components::InstallCliTool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::app_menu::install_cli_tool(cx);
    }

    pub(crate) fn on_uninstall_cli_tool(
        &mut self,
        _: &crate::components::UninstallCliTool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::app_menu::uninstall_cli_tool(cx);
    }

    pub(crate) fn toggle_view_mode(&mut self, cx: &mut Context<Self>) {
        let tabular_document = self
            .document_host
            .as_ref()
            .is_some_and(|view| view.read(cx).supports_tabular_modes());
        let target = match self.view_mode {
            ViewMode::Rendered | ViewMode::Preview => ViewMode::Source,
            ViewMode::Source if tabular_document => ViewMode::Preview,
            ViewMode::Source => ViewMode::Rendered,
            ViewMode::Split if tabular_document => ViewMode::Source,
            ViewMode::Split => ViewMode::Rendered,
        };
        self.set_view_mode(target, cx);
    }

    pub(crate) fn set_view_mode(&mut self, target: ViewMode, cx: &mut Context<Self>) {
        if let Some(document_host) = self.document_host.clone() {
            let json_document = document_host.read(cx).is_json_document();
            let delimited_document = document_host.read(cx).is_delimited_document();
            let tabular_document = json_document || delimited_document;
            if delimited_document
                && target == ViewMode::Rendered
                && !document_host.read(cx).source_is_utf8()
            {
                self.request_encoding_conversion(cx);
                return;
            }
            let target = if json_document && target == ViewMode::Rendered {
                ViewMode::Preview
            } else {
                target
            };
            let split_ratio = self.split_pane_ratio;
            document_host.update(cx, |view, cx| {
                if tabular_document {
                    view.set_json_split_ratio(split_ratio, cx);
                    match target {
                        ViewMode::Source => view.show_source_view(cx),
                        ViewMode::Split => view.show_split_view(cx),
                        ViewMode::Preview => view.show_structure_view(cx),
                        ViewMode::Rendered if delimited_document => view.show_live_view(cx),
                        ViewMode::Rendered => view.show_structure_view(cx),
                    }
                } else {
                    match target {
                        ViewMode::Source => view.show_source_view(cx),
                        ViewMode::Rendered => view.show_mode_unavailable("Live", cx),
                        ViewMode::Split => view.show_mode_unavailable("Split", cx),
                        ViewMode::Preview => view.show_mode_unavailable("Preview", cx),
                    }
                }
            });
            if tabular_document {
                self.view_mode = target;
                self.status_bar.format_overflow_open = false;
                self.schedule_workspace_session_save(cx);
                cx.notify();
                return;
            }
            // 大文件增强尚未产生 resident Markdown projection 时，模式控件必须保持
            // Source 选中，不能让 Live/Preview 标签与实际源码画布相互矛盾。
            self.view_mode = ViewMode::Source;
            self.status_bar.format_overflow_open = false;
            cx.notify();
            return;
        }
        if target != ViewMode::Preview && !self.source_encoding.is_utf8() {
            self.request_encoding_conversion(cx);
            return;
        }
        if self.view_mode == target {
            return;
        }

        self.end_block_pointer_selection_sessions(cx);
        let selection_snapshot = if self.view_mode == ViewMode::Preview {
            self.last_selection_snapshot
        } else {
            self.capture_source_selection_snapshot(cx)
        };
        self.clear_cross_block_selection(cx);
        self.rendered_select_all_cycle = None;
        if target == ViewMode::Preview {
            self.projection_cache_task = None;
            self.projection_cache_scheduled_revision = None;
        }
        if target == ViewMode::Split {
            self.enter_split_view(cx);
        } else if self.view_mode == ViewMode::Split {
            self.exit_split_view(target, cx);
        } else {
            match (self.view_mode, target) {
                (ViewMode::Source, ViewMode::Rendered | ViewMode::Preview) => {
                    self.rebuild_primary_projection_from_source(cx);
                }
                (ViewMode::Rendered | ViewMode::Preview, ViewMode::Source) => {
                    // 切换视图不能触发源码规范化；Source 视图直接读取 Rope 真值。
                    let markdown = self.source_document.text();
                    let block = Self::new_block(cx, BlockRecord::paragraph(markdown));
                    block.update(cx, |block, _cx| block.set_source_document_mode());
                    self.document.replace_roots(vec![block], cx);
                    self.table_cells.clear();
                    self.virtual_surface = None;
                }
                (ViewMode::Rendered, ViewMode::Preview)
                | (ViewMode::Preview, ViewMode::Rendered) => {}
                _ => return,
            }
        }

        self.view_mode = target;
        self.render_row_cache = None;
        self.set_projection_read_only(target == ViewMode::Preview, cx);

        if target == ViewMode::Preview {
            self.last_selection_snapshot = selection_snapshot;
            self.pending_focus = None;
            self.active_entity_id = None;
        } else {
            self.apply_selection_snapshot_in_current_mode(&selection_snapshot, cx);
        }
        self.pending_scroll_active_block_into_view = true;
        self.pending_scroll_recheck_after_layout = true;
        self.last_scroll_viewport_size = None;
        self.pending_window_title_refresh = true;
        self.close_dialog_restore_focus = None;
        self.table_axis_preview = None;
        self.table_axis_selection = None;
        self.dismiss_contextual_overlays(cx);
        self.sync_table_axis_visuals(cx);
        self.refresh_stable_document_snapshot(cx);
        cx.notify();
    }

    /// 将当前投影及原生表格单元格统一切换为只读或可编辑状态。
    pub(super) fn set_projection_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        Self::set_document_read_only(&self.document, &self.table_cells, read_only, cx);
    }

    fn set_document_read_only(
        document: &DocumentTree,
        table_cells: &HashMap<EntityId, TableCellBinding>,
        read_only: bool,
        cx: &mut Context<Self>,
    ) {
        let mut blocks: Vec<Entity<Block>> = document
            .visible_blocks()
            .iter()
            .map(|visible| visible.entity.clone())
            .collect();
        blocks.extend(table_cells.values().map(|binding| binding.cell.clone()));
        blocks.sort_by_key(Entity::entity_id);
        blocks.dedup_by_key(|block| block.entity_id());
        for block in blocks {
            block.update(cx, move |block, cx| {
                block.set_read_only(read_only || block.record.is_yaml_frontmatter());
                cx.notify();
            });
        }
    }

    fn source_view_document(&self, cx: &mut Context<Self>) -> DocumentTree {
        let block = Self::new_block(cx, BlockRecord::paragraph(self.source_document.text()));
        block.update(cx, |block, _cx| block.set_source_document_mode());
        let mut document = DocumentTree::new(vec![block]);
        document.rebuild_metadata_and_snapshot(cx);
        document
    }

    /// 返回当前 SourceDocument revision 的共享纯投影；旧缓存只作为增量基线。
    fn prepare_current_projection(&mut self) -> Arc<PreparedSplitProjection> {
        self.projection_cache_task = None;
        self.projection_cache_scheduled_revision = None;
        let snapshot = self.source_document.snapshot();
        if let Some(cached) = self.projection_cache.as_ref()
            && cached.revision == snapshot.revision()
        {
            return Arc::clone(cached);
        }
        let prepared = Arc::new(if let Some(previous) = self.projection_cache.as_deref() {
            if self.virtual_surface.is_some() {
                PreparedSplitProjection::from_snapshot_incremental_regions_only(snapshot, previous)
            } else {
                PreparedSplitProjection::from_snapshot_incremental(snapshot, previous)
            }
        } else {
            PreparedSplitProjection::from_snapshot_adaptive(
                snapshot,
                Self::VIRTUAL_SURFACE_REGION_THRESHOLD,
            )
        });
        self.projection_cache = Some(Arc::clone(&prepared));
        prepared
    }

    pub(super) fn rebuild_primary_projection_from_source(&mut self, cx: &mut Context<Self>) {
        self.rebuild_primary_projection_from_source_internal(false, cx);
    }

    pub(super) fn rebuild_primary_projection_from_source_reusing(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.rebuild_primary_projection_from_source_internal(true, cx);
    }

    fn rebuild_primary_projection_from_source_internal(
        &mut self,
        reuse_entities: bool,
        cx: &mut Context<Self>,
    ) {
        let prepared = self.prepare_current_projection();
        if Self::should_virtualize_projection(&prepared) {
            let mut surface = VirtualSurfaceState::new(Arc::clone(&prepared));
            let viewport_height = f32::from(self.scroll_handle.bounds().size.height.max(px(720.0)));
            let scroll_y = (-f32::from(self.scroll_handle.offset().y)).max(0.0);
            let target = surface.desired_window(scroll_y, viewport_height, 800.0, None);
            surface.reconcile_mounts(target, cx);
            let mut roots = surface.viewport_roots();
            if roots.is_empty() {
                roots.push(Self::new_block(cx, BlockRecord::paragraph(String::new())));
            }
            self.virtual_surface = Some(surface);
            self.document.replace_roots(roots, cx);
            self.row_stride_cache.clear();
            self.render_row_cache = None;
            self.rebuild_virtual_table_runtimes(cx);
            return;
        }
        self.virtual_surface = None;
        let mut reusable = if reuse_entities {
            self.document
                .visible_blocks()
                .iter()
                .map(|visible| (visible.entity.read(cx).record.id, visible.entity.clone()))
                .collect::<HashMap<_, _>>()
        } else {
            HashMap::new()
        };
        let mut roots = Self::build_blocks_from_projection_reusing(cx, &prepared, &mut reusable);
        if roots.is_empty() {
            roots.push(Self::new_block(cx, BlockRecord::paragraph(String::new())));
        }
        self.document.replace_roots(roots, cx);
        let current_entity_ids = self
            .document
            .visible_blocks()
            .iter()
            .map(|visible| visible.entity.entity_id())
            .collect::<std::collections::HashSet<_>>();
        self.row_stride_cache
            .retain(|entity_id, _| current_entity_ids.contains(entity_id));
        self.rebuild_table_runtimes(cx);
    }

    fn enter_split_view(&mut self, cx: &mut Context<Self>) {
        self.projection_cache_task = None;
        self.projection_cache_scheduled_revision = None;
        self.split_projection_task = None;
        self.split_projection_scheduled_revision = None;
        self.virtual_surface = None;
        match self.view_mode {
            ViewMode::Rendered | ViewMode::Preview => {
                let source_document = self.source_view_document(cx);
                self.document = source_document;
                self.table_cells.clear();
                self.split_preview = None;
                // 首个 Split 树必须与 Prepared IR 共用记录 ID，后续 revision 才能复用 Entity。
                self.rebuild_split_preview_projection(cx);
            }
            ViewMode::Source => {
                self.rebuild_split_preview_projection(cx);
            }
            ViewMode::Split => unreachable!(),
        }
    }

    fn exit_split_view(&mut self, target: ViewMode, cx: &mut Context<Self>) {
        self.split_resize_session = None;
        self.split_preview_scrollbar_drag = None;
        self.split_preview_scrollbar_hovered = false;
        self.split_preview_scrollbar_fade_task = None;
        self.split_preview_scrollbar_visible_until = Instant::now();
        self.split_projection_task = None;
        self.split_projection_scheduled_revision = None;
        match target {
            ViewMode::Source => {
                self.split_preview = None;
                self.table_cells.clear();
            }
            ViewMode::Rendered | ViewMode::Preview => {
                let projection_is_current = self
                    .split_preview
                    .as_ref()
                    .is_some_and(|state| state.revision == self.source_document.revision());
                if !projection_is_current {
                    self.rebuild_split_preview_projection(cx);
                }
                let should_virtualize = self
                    .projection_cache
                    .as_deref()
                    .is_some_and(Self::should_virtualize_projection);
                if should_virtualize {
                    // Split 右侧当前仍是全量只读树；返回 Live/Preview 时必须恢复
                    // Rope 驱动的虚拟 surface，不能让一次模式切换永久放大全量 Entity。
                    self.split_preview = None;
                    self.table_cells.clear();
                    self.rebuild_primary_projection_from_source(cx);
                    return;
                }
                let state = self
                    .split_preview
                    .take()
                    .expect("Split 模式必须持有右侧预览投影");
                self.document = state.document;
                self.table_cells = state.table_cells;
            }
            ViewMode::Split => unreachable!(),
        }
    }

    /// 根据当前源码重建 Split 右侧投影；解析结果不反向覆盖 SourceDocument。
    pub(super) fn rebuild_split_preview_projection(&mut self, cx: &mut Context<Self>) {
        let prepared = self.prepare_current_projection();
        self.install_split_preview_projection(prepared, cx);
    }

    fn install_split_preview_projection(
        &mut self,
        prepared: Arc<PreparedSplitProjection>,
        cx: &mut Context<Self>,
    ) {
        debug_assert!(prepared.reused_prefix_regions <= prepared.regions.len());
        self.projection_cache = Some(Arc::clone(&prepared));
        let mut previous_state = self.split_preview.take();
        let scroll_handle = previous_state
            .as_ref()
            .map(|state| state.scroll_handle.clone())
            .unwrap_or_default();
        let scroll_driver = previous_state
            .as_ref()
            .and_then(|state| state.scroll_driver);

        if Self::should_virtualize_projection(&prepared) {
            let viewport_height = f32::from(scroll_handle.bounds().size.height.max(px(720.0)));
            let scroll_y = (-f32::from(scroll_handle.offset().y)).max(0.0);
            let mut surface = previous_state
                .as_mut()
                .and_then(|state| state.virtual_surface.take())
                .unwrap_or_else(|| VirtualSurfaceState::new(Arc::clone(&prepared)));
            if surface.projection_revision() == prepared.revision {
                let target = surface.desired_window(scroll_y, viewport_height, 800.0, None);
                surface.reconcile_mounts(target, cx);
            } else {
                surface.replace_projection(
                    Arc::clone(&prepared),
                    scroll_y,
                    viewport_height,
                    800.0,
                    None,
                    cx,
                );
            }
            let mut roots = surface.viewport_roots();
            if roots.is_empty() {
                roots.push(Self::new_block(cx, BlockRecord::paragraph(String::new())));
            }
            let mut document = DocumentTree::new(roots);
            document.rebuild_metadata_and_snapshot(cx);
            self.split_preview = Some(SplitPreviewState {
                document,
                virtual_surface: Some(surface),
                table_cells: HashMap::new(),
                source_ranges: HashMap::new(),
                scroll_handle,
                scroll_driver,
                row_stride_cache: HashMap::new(),
                previous_visible_ids: Vec::new(),
                previous_render_window: None,
                revision: prepared.revision,
            });
            self.refresh_split_virtual_preview_runtime(cx);
            return;
        }

        let mut reusable_entities = previous_state
            .as_ref()
            .map(|state| {
                state
                    .document
                    .visible_blocks()
                    .iter()
                    .map(|visible| (visible.entity.read(cx).record.id, visible.entity.clone()))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let mut roots =
            Self::build_blocks_from_projection_reusing(cx, &prepared, &mut reusable_entities);
        if roots.is_empty() {
            roots.push(Self::new_block(cx, BlockRecord::paragraph(String::new())));
        }
        let mut preview_document = DocumentTree::new(roots);
        preview_document.rebuild_metadata_and_snapshot(cx);

        // 复用现有 runtime 构建器，但事务完成后恢复左侧 Source 文档所有权。
        let source_document = std::mem::replace(&mut self.document, preview_document);
        let source_cells = std::mem::take(&mut self.table_cells);
        self.rebuild_table_runtimes(cx);
        let (_, source_ranges) = self.build_source_target_mappings_with_block_ranges(cx);
        let preview_document = std::mem::replace(&mut self.document, source_document);
        let preview_cells = std::mem::replace(&mut self.table_cells, source_cells);
        Self::set_document_read_only(&preview_document, &preview_cells, true, cx);

        let mut row_stride_cache = previous_state
            .as_ref()
            .map(|state| state.row_stride_cache.clone())
            .unwrap_or_default();
        let current_entity_ids = preview_document
            .visible_blocks()
            .iter()
            .map(|visible| visible.entity.entity_id())
            .collect::<std::collections::HashSet<_>>();
        row_stride_cache.retain(|entity_id, _| current_entity_ids.contains(entity_id));
        let previous_visible_ids = previous_state
            .as_ref()
            .map(|state| state.previous_visible_ids.clone())
            .unwrap_or_default();
        let previous_render_window = previous_state
            .as_ref()
            .and_then(|state| state.previous_render_window);
        self.split_preview = Some(SplitPreviewState {
            document: preview_document,
            virtual_surface: None,
            table_cells: preview_cells,
            source_ranges,
            scroll_handle,
            scroll_driver,
            row_stride_cache,
            previous_visible_ids,
            previous_render_window,
            revision: prepared.revision,
        });
    }
}

#[path = "window_state_parts/controller.rs"]
mod controller;
