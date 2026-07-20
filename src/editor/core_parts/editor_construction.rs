// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) const HISTORY_LIMIT: usize = 200;
    pub(in crate::editor) const HISTORY_COALESCE_WINDOW: Duration = Duration::from_millis(1_000);
    pub(in crate::editor) const SPLIT_PROJECTION_DEBOUNCE: Duration = Duration::from_millis(24);
    /// 大文档后台投影必须等待连续输入停顿，避免上一 revision 的全量行切分抢占下一按键。
    pub(in crate::editor) const VIRTUAL_PROJECTION_DEBOUNCE: Duration = Duration::from_millis(750);
    pub(in crate::editor) const RENDERED_SELECT_ALL_CYCLE_WINDOW: Duration =
        Duration::from_millis(750);
    /// 超过该区域数时，全量 GPUI Entity 已明显越过启动与内存 SLO。
    pub(in crate::editor) const VIRTUAL_SURFACE_REGION_THRESHOLD: usize = 8_192;

    pub(in crate::editor) fn should_virtualize_projection(
        projection: &PreparedSplitProjection,
    ) -> bool {
        projection.regions.len() >= Self::VIRTUAL_SURFACE_REGION_THRESHOLD
    }

    // reason: platform menu and tests construct untitled editors; remove only with that compatibility entrypoint.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn from_markdown(
        cx: &mut Context<Self>,
        markdown: String,
        file_path: Option<PathBuf>,
    ) -> Self {
        Self::from_markdown_internal(cx, markdown, file_path, false)
    }

    pub(crate) fn from_opened_markdown(
        cx: &mut Context<Self>,
        opened: crate::document_io::OpenedMarkdown,
        file_path: Option<PathBuf>,
    ) -> Self {
        let requires_conversion = !opened.encoding.is_utf8();
        let mut editor = Self::from_markdown_internal(cx, opened.text, file_path, false);
        editor.source_encoding = opened.encoding;
        if requires_conversion {
            editor.set_view_mode(ViewMode::Preview, cx);
            editor.show_encoding_conversion_dialog = true;
        }
        editor
    }

    pub(crate) fn from_large_file(
        cx: &mut Context<Self>,
        path: PathBuf,
        probe: gmark_large_document::OpenProbe,
        source: gmark_large_document::FileSource,
    ) -> Self {
        let mut editor = Self::from_markdown_internal(cx, String::new(), Some(path.clone()), false);
        let large_file =
            cx.new(move |cx| crate::large_file::DiskSourceAdapter::new(path, probe, source, cx));
        Self::subscribe_disk_source_adapter(&large_file, cx);
        editor.source_surface = SourceSurface::disk(large_file);
        editor.view_mode = ViewMode::Source;
        editor.pending_focus = None;
        editor.active_entity_id = None;
        editor.restart_file_watcher(cx);
        editor
    }

    pub(crate) fn from_large_recovery(
        cx: &mut Context<Self>,
        path: PathBuf,
        probe: gmark_large_document::OpenProbe,
        source: gmark_large_document::FileSource,
        journal_path: PathBuf,
    ) -> Self {
        let mut editor = Self::from_markdown_internal(cx, String::new(), Some(path.clone()), false);
        let large_file = cx.new(move |cx| {
            crate::large_file::DiskSourceAdapter::from_recovery(
                path,
                probe,
                source,
                journal_path,
                cx,
            )
        });
        Self::subscribe_disk_source_adapter(&large_file, cx);
        editor.source_surface = SourceSurface::disk(large_file);
        editor.view_mode = ViewMode::Source;
        editor.document_dirty = true;
        editor.pending_window_edited = true;
        editor.pending_focus = None;
        editor.active_entity_id = None;
        editor.restart_file_watcher(cx);
        editor
    }

    pub(crate) fn install_accessibility_bridge(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.accessibility_bridge.is_some() {
            return;
        }
        let snapshot = self.accessibility_snapshot(cx);
        self.accessibility_revision = Some(self.current_accessibility_revision(cx));
        let Some((bridge, mut wake)) =
            crate::accessibility::AccessibilityBridge::new(window, snapshot)
        else {
            return;
        };
        self.accessibility_bridge = Some(bridge);
        // 平台 action handler 可能运行在非 GPUI 线程；无界 channel 只负责唤醒一帧，
        // 真正动作仍回到现有 Editor action 路径和窗口线程，不引入空闲轮询。
        self.accessibility_wake_task = Some(cx.spawn(async move |this, cx| {
            while wake.next().await.is_some() {
                let Ok(()) = this.update(cx, |_editor, cx| cx.notify()) else {
                    break;
                };
            }
        }));
    }

    pub(in crate::editor) fn accessibility_snapshot(
        &self,
        cx: &App,
    ) -> crate::accessibility::EditorAccessibilitySnapshot {
        if let Some(large_file) = self.source_surface.as_ref() {
            return large_file.read(cx).accessibility_snapshot(cx);
        }
        let title = self
            .file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled")
            .to_owned();
        let lines = self
            .source_document
            .text()
            .lines()
            .take(512)
            .enumerate()
            .map(|(line, text)| (line as u64, text.to_owned()))
            .collect();
        crate::accessibility::EditorAccessibilitySnapshot {
            title,
            dirty: self.document_dirty,
            status: if self.document_dirty {
                "Modified".to_owned()
            } else {
                "Saved".to_owned()
            },
            error: self
                .external_file_conflict
                .then(|| "File changed on disk".to_owned()),
            busy: self.save_task.is_some() || self.export_in_progress,
            search_visible: self.find_panel.is_some(),
            navigation_visible: false,
            caret: None,
            lines,
        }
    }

    pub(in crate::editor) fn current_accessibility_revision(&self, cx: &App) -> u64 {
        if let Some(large_file) = self.source_surface.as_ref() {
            return large_file.read(cx).accessibility_revision();
        }
        let flags = u64::from(self.document_dirty)
            | (u64::from(self.find_panel.is_some()) << 1)
            | (u64::from(self.external_file_conflict) << 2)
            | (u64::from(self.save_task.is_some()) << 3)
            | (u64::from(self.export_in_progress) << 4);
        self.source_document
            .revision()
            .get()
            .wrapping_mul(32)
            .wrapping_add(flags)
    }

    pub(crate) fn subscribe_disk_source_adapter(
        view: &Entity<crate::large_file::DiskSourceAdapter>,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe(view, |editor, _, event, cx| match event {
            crate::large_file::DiskSourceEvent::SavedAs(path) => {
                editor.file_path = Some(path.clone());
                editor.saved_file_fingerprint = crate::recovery::fingerprint_file(path).ok();
                editor.document_dirty = false;
                editor.pending_window_edited = false;
                editor.schedule_workspace_session_save(cx);
                cx.notify();
            }
            crate::large_file::DiskSourceEvent::StateChanged => cx.notify(),
        })
        .detach();
    }

    pub(crate) fn from_recovered(
        cx: &mut Context<Self>,
        recovered: crate::recovery::RecoveredDocument,
    ) -> Self {
        let journal = Arc::new(Mutex::new(crate::recovery::RecoveryJournal::resume(
            &recovered,
        )));
        let target_mode = match recovered.view_mode.as_str() {
            "source" => ViewMode::Source,
            "split" => ViewMode::Split,
            "preview" => ViewMode::Preview,
            _ => ViewMode::Rendered,
        };
        let selection =
            UndoSelectionSnapshot::from_source_selection(recovered.selection.source_selection());
        let base_file_changed = recovered.base_file_changed;
        if recovered.read_status == crate::recovery::RecoveryReadStatus::TruncatedTail {
            eprintln!(
                "recovery journal '{}' had a corrupt tail; restored the last CRC-valid record",
                recovered.journal_path.display()
            );
        }
        if recovered.base_file_changed {
            eprintln!(
                "recovered document base changed externally: {}",
                recovered.file_path.as_deref().map_or_else(
                    || "<untitled>".to_owned(),
                    |path| path.display().to_string()
                )
            );
        }

        let mut editor =
            Self::from_markdown_internal(cx, recovered.source, recovered.file_path, false);
        assert!(
            editor
                .source_document
                .restore_source_format(recovered.source_format),
            "恢复日志中的源码格式必须与恢复文本一致"
        );
        editor.recovery_journal = Some(journal);
        editor.external_file_conflict = base_file_changed;
        editor.recovered_session = true;
        if target_mode != ViewMode::Rendered {
            editor.set_view_mode(target_mode, cx);
        }
        editor.apply_selection_snapshot_in_current_mode(&selection, cx);
        editor.last_selection_snapshot = selection;
        editor.document_dirty = true;
        editor.pending_window_edited = true;
        editor.pending_window_title_refresh = true;
        editor
    }

    #[cfg(test)]
    pub(super) fn from_markdown_virtualized(
        cx: &mut Context<Self>,
        markdown: String,
        file_path: Option<PathBuf>,
    ) -> Self {
        Self::from_markdown_internal(cx, markdown, file_path, true)
    }

    fn from_markdown_internal(
        cx: &mut Context<Self>,
        markdown: String,
        file_path: Option<PathBuf>,
        force_virtual_surface: bool,
    ) -> Self {
        let construction_started = perf::start();
        let source_document = SourceDocument::new(&markdown);
        let normalized = source_document.text();
        let saved_file_fingerprint = file_path
            .as_deref()
            .and_then(|path| crate::recovery::fingerprint_file(path).ok());
        #[cfg(not(test))]
        let recovery_journal = crate::config::GmarkConfigDirs::from_system()
            .and_then(|dirs| {
                crate::recovery::RecoveryJournal::create(
                    &dirs.recovery_dir(),
                    file_path.clone(),
                    markdown.clone(),
                )
            })
            .map(|journal| Arc::new(Mutex::new(journal)))
            .map_err(|error| eprintln!("failed to initialize recovery journal: {error}"))
            .ok();
        #[cfg(test)]
        let recovery_journal = None;
        let projection = Arc::new(PreparedSplitProjection::from_snapshot_adaptive(
            source_document.snapshot(),
            Self::VIRTUAL_SURFACE_REGION_THRESHOLD,
        ));
        let virtual_surface =
            (force_virtual_surface || Self::should_virtualize_projection(&projection)).then(|| {
                let mut surface = VirtualSurfaceState::new(Arc::clone(&projection));
                let initial_window = surface.desired_window(0.0, 720.0, 800.0, Some(0));
                surface.reconcile_mounts(initial_window, cx);
                surface
            });
        let mut roots = if let Some(surface) = virtual_surface.as_ref() {
            surface.viewport_roots()
        } else {
            Self::build_blocks_from_projection_reusing(cx, &projection, &mut HashMap::new())
        };
        if roots.is_empty() {
            roots.push(Self::new_block(cx, BlockRecord::paragraph(String::new())));
        }

        let mut document = DocumentTree::new(roots);
        document.rebuild_metadata_and_snapshot(cx);
        let mut status_bar = StatusBarState::default();
        status_bar.set_word_count(
            source_document.revision(),
            status_bar::count_characters(&normalized),
        );
        let pending_focus = document
            .root_blocks()
            .iter()
            .find(|block| {
                let block = block.read(cx);
                block.kind() != BlockKind::Comment && !block.record.is_yaml_frontmatter()
            })
            .or_else(|| document.first_root())
            .map(|block| block.entity_id());

        let mut editor = Self {
            accessibility_bridge: None,
            accessibility_wake_task: None,
            accessibility_revision: None,
            source_surface: SourceSurface::resident(),
            source_document,
            source_encoding: crate::document_io::DocumentEncoding::Utf8,
            document_epoch: 0,
            projection_cache: Some(projection),
            document,
            split_preview: None,
            split_pane_ratio: 0.5,
            split_resize_session: None,
            split_divider_focus_handle: cx.focus_handle(),
            document_toolbar_focus_handles: std::array::from_fn(|_| cx.focus_handle()),
            table_cells: HashMap::new(),
            view_mode: ViewMode::Rendered,
            pending_focus,
            active_entity_id: pending_focus,
            pending_scroll_active_block_into_view: true,
            pending_scroll_recheck_after_layout: true,
            pending_save: false,
            pending_save_as: false,
            save_task: None,
            save_queued: false,
            auto_save_task: None,
            spellcheck_task: None,
            export_task: None,
            export_cancel: None,
            export_in_progress: false,
            export_cancel_requested: false,
            pending_open_link: None,
            pending_window_edited: false,
            pending_window_title_refresh: false,
            document_dirty: false,
            file_path,
            saved_file_fingerprint,
            file_watch_guard: None,
            file_watch_task: None,
            external_file_conflict: false,
            recovered_session: false,
            show_external_conflict_dialog: false,
            show_encoding_conversion_dialog: false,
            external_conflict_preview: None,
            external_conflict_restore_focus: None,
            allow_external_overwrite_once: false,
            scroll_handle: ScrollHandle::new(),
            last_scroll_viewport_size: None,
            prev_visible_block_ids: Vec::new(),
            row_stride_cache: HashMap::new(),
            render_row_cache: None,
            prev_render_window: None,
            close_guard_installed: false,
            show_unsaved_changes_dialog: false,
            pending_close_after_save: false,
            close_dialog_restore_focus: None,
            pending_drop_replace_path: None,
            show_drop_replace_dialog: false,
            pending_drop_replace_after_save: false,
            drop_replace_restore_focus: None,
            info_dialog: None,
            update_check_in_progress: false,
            workspace: WorkspaceState::default(),
            tabs: tabs::TabState::new(),
            focus_mode: false,
            typewriter_mode: false,
            status_bar,
            context_menu: None,
            context_menu_keyboard_item: None,
            context_menu_keyboard_submenu_item: None,
            context_menu_scroll_handle: ScrollHandle::new(),
            command_palette: None,
            find_panel: None,
            table_insert_dialog: None,
            context_menu_submenu_close_task: None,
            table_axis_preview: None,
            table_axis_selection: None,
            cross_block_selection: None,
            cross_block_drag: None,
            rendered_select_all_cycle: None,
            // 桌面 Markdown 编辑器的高频导航需始终可见；G 启动器仍可由用户手动收纳。
            menu_bar_expanded: true,
            menu_window_activation_subscription: None,
            menu_bar_open: None,
            menu_submenu_open: None,
            menu_keyboard_item: None,
            menu_keyboard_submenu_item: None,
            menu_bar_hovered: false,
            menu_panel_hovered: false,
            menu_submenu_panel_hovered: false,
            menu_submenu_bridge_hovered: false,
            menu_close_task: None,
            scrollbar_hovered: false,
            scrollbar_thumb_hovered: false,
            scrollbar_visible_until: Instant::now(),
            scrollbar_fade_task: None,
            split_preview_scrollbar_hovered: false,
            split_preview_scrollbar_visible_until: Instant::now(),
            split_preview_scrollbar_fade_task: None,
            scroll_recheck_task: None,
            projection_cache_task: None,
            projection_cache_scheduled_revision: None,
            split_projection_task: None,
            split_projection_scheduled_revision: None,
            recovery_journal,
            recovery_task: None,
            recovery_generation: 0,
            scrollbar_drag: None,
            split_preview_scrollbar_drag: None,
            undo_history: Vec::new(),
            redo_history: Vec::new(),
            pending_undo_capture: None,
            virtual_undo_selections: Vec::new(),
            virtual_redo_selections: Vec::new(),
            pending_virtual_undo_selection: None,
            last_selection_snapshot: Self::empty_selection_snapshot(),
            last_stable_source_text: normalized,
            pending_dirty_source: None,
            history_restore_in_progress: false,
            image_reference_definitions: Arc::default(),
            link_reference_definitions: Arc::default(),
            footnote_registry: Arc::default(),
            pending_virtual_global_runtime_refresh: false,
            pending_virtual_footnote_focus: None,
            pending_virtual_footnote_backref_focus: None,
            virtual_surface,
            first_render_started: construction_started,
            pending_input_trace: None,
        };
        if editor.virtual_surface.is_some() {
            editor.rebuild_virtual_table_runtimes(cx);
            let source = editor.source_document.text();
            editor.rebuild_runtime_context_from_markdown(&source, cx);
        } else {
            editor.rebuild_table_runtimes(cx);
        }
        editor.pending_focus = editor.first_focusable_entity_id(cx);
        editor.active_entity_id = editor.pending_focus;
        editor.refresh_stable_document_snapshot(cx);
        editor.restart_file_watcher(cx);
        editor.schedule_active_block_spellcheck(cx);
        if let Some(started) = construction_started {
            perf::emit(
                "editor_construct",
                started,
                Some(editor.source_document.len()),
                Some(true),
                None,
            );
        }
        editor
    }
}
