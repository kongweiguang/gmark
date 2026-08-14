// @author kongweiguang

//! External file drops for opening documents in tabs or inserting resources.

use std::path::{Path, PathBuf};

use anyhow::Result;
use gpui::*;

use super::{DocumentKind, Editor, ViewMode};
use crate::i18n::I18nManager;
use crate::preferences::ResourceInsertBehavior;

#[path = "file_drop_parts/materialize.rs"]
mod materialize;
#[path = "file_drop_parts/target.rs"]
mod target;

use materialize::{
    DroppedPathKind, MAX_RESOURCE_BYTES, MAX_RESOURCE_NAME_ATTEMPTS,
    bounded_resource_candidate_path, checked_resource_input_size, checked_resource_output_size,
    classify_dropped_paths, copy_resource_without_overwrite,
};
pub(in crate::editor) use materialize::{
    ResourceCleanupGuard, ResourceDropTarget, resource_drop_target_is_current,
    resource_materialization_is_current, resource_materialization_is_current_for_tab,
    resource_materialization_is_missing,
};
use target::DroppedResourceTarget;

impl Editor {
    /// 拖放入口先捕获块拆分和文档 gate，再让后台完成 materialize，防止等待文件系统时
    /// 改动已落后的选择；完成回调只在同一 tab、block、selection、revision/epoch 下提交。
    fn insert_dropped_resource(
        &mut self,
        path: PathBuf,
        target: DroppedResourceTarget,
        cx: &mut Context<Self>,
    ) {
        let DroppedResourceTarget {
            block,
            leading,
            trailing,
            document_path,
            behavior,
            fingerprint,
        } = target;
        let weak_editor = cx.entity().downgrade();
        let expected_fingerprint = fingerprint.clone();
        let error_block = block.clone();

        cx.spawn(async move |_, cx| {
            let result = cx
                .background_spawn(async move {
                    Self::materialize_resource_with_limits(
                        "",
                        &path,
                        document_path.as_deref(),
                        behavior,
                        None,
                    )
                })
                .await;
            match result {
                Err(error) => {
                    let _ = weak_editor.update(cx, |editor, cx| {
                        if editor
                            .current_dropped_resource_target(&error_block, cx)
                            .is_some_and(|current| {
                                resource_drop_target_is_current(&expected_fingerprint, &current)
                            })
                        {
                            editor.show_image_paste_error(error, cx);
                        }
                    });
                }
                Ok((markdown, materialized)) => {
                    // 先把 guard 放进 closure 的捕获环境；WeakEntity 未执行回调时，closure
                    // 仍会被销毁并回收新副本，避免实体消失留下孤立文件。
                    let mut cleanup = ResourceCleanupGuard::new(materialized);
                    let update_result = weak_editor.update(cx, move |editor, cx| {
                        let Some(current) = editor.current_dropped_resource_target(&block, cx)
                        else {
                            return;
                        };
                        if !resource_drop_target_is_current(&fingerprint, &current) {
                            return;
                        }
                        editor.commit_dropped_resource(
                            block,
                            &fingerprint,
                            &leading,
                            markdown,
                            &mut cleanup,
                            &trailing,
                            cx,
                        );
                    });
                    let _ = update_result;
                }
            }
        })
        .detach();
    }

    /// 在 gate 通过后复用现有图片块插入语义；任何未提交分支只清理由本任务创建的副本。
    fn commit_dropped_resource(
        &mut self,
        block: Entity<super::Block>,
        fingerprint: &ResourceDropTarget,
        leading: &crate::components::InlineTextTree,
        markdown: String,
        cleanup: &mut ResourceCleanupGuard,
        trailing: &crate::components::InlineTextTree,
        cx: &mut Context<Self>,
    ) {
        let Some(current) = self.current_dropped_resource_target(&block, cx) else {
            return;
        };
        if !resource_drop_target_is_current(fingerprint, &current) {
            return;
        }
        if self.replace_cross_block_selection_with_text(
            &markdown,
            None,
            false,
            crate::components::UndoCaptureKind::NonCoalescible,
            cx,
        ) {
            cleanup.disarm();
            return;
        }
        let Some(block) = self.focusable_entity_by_id(block.entity_id()) else {
            return;
        };
        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        let can_insert_image_block = self.view_mode == ViewMode::Rendered
            && block.read(cx).kind() == crate::components::BlockKind::Paragraph
            && self.table_cell_binding(block.entity_id()).is_none()
            && !block.read(cx).uses_raw_text_editing();
        if can_insert_image_block {
            let Some(location) = self.document.find_block_location(block.entity_id()) else {
                self.finalize_pending_undo_capture(cx);
                return;
            };
            if leading.visible_len() == 0 {
                Self::set_block_title_and_kind(
                    &block,
                    crate::components::BlockKind::Paragraph,
                    crate::components::InlineTextTree::plain(markdown.clone()),
                    markdown.len(),
                    cx,
                );
                if trailing.visible_len() != 0 {
                    let trailing_block = Self::new_block(
                        cx,
                        crate::components::BlockRecord::new(
                            crate::components::BlockKind::Paragraph,
                            trailing.clone(),
                        ),
                    );
                    self.document.insert_blocks_at(
                        location.parent,
                        location.index + 1,
                        vec![trailing_block],
                        cx,
                    );
                }
                self.focus_block(block.entity_id());
                self.rebuild_image_runtimes(cx);
            } else {
                Self::set_block_title_and_kind(
                    &block,
                    crate::components::BlockKind::Paragraph,
                    leading.clone(),
                    leading.visible_len(),
                    cx,
                );
                let image_block =
                    Self::new_block(cx, crate::components::BlockRecord::paragraph(markdown));
                let mut inserted = vec![image_block.clone()];
                if trailing.visible_len() != 0 {
                    inserted.push(Self::new_block(
                        cx,
                        crate::components::BlockRecord::new(
                            crate::components::BlockKind::Paragraph,
                            trailing.clone(),
                        ),
                    ));
                }
                self.document
                    .insert_blocks_at(location.parent, location.index + 1, inserted, cx);
                self.focus_block(image_block.entity_id());
                self.rebuild_image_runtimes(cx);
            }
        } else {
            let (kind, title, cursor) = block.read_with(cx, |block, _cx| {
                let mut title = leading.clone();
                let inserted = if block.uses_raw_text_editing() || block.kind().is_code_block() {
                    crate::components::InlineTextTree::plain(markdown.clone())
                } else {
                    crate::components::InlineTextTree::from_markdown(&markdown)
                };
                title.append_tree(inserted);
                let cursor = title.visible_len();
                title.append_tree(trailing.clone());
                (block.kind(), title, cursor)
            });
            Self::set_block_title_and_kind(&block, kind, title, cursor, cx);
            if let Some(binding) = self.table_cell_binding(block.entity_id()) {
                self.sync_table_record_from_runtime(&binding.table_block, cx);
            }
            self.focus_block(block.entity_id());
            self.rebuild_image_runtimes(cx);
        }
        cleanup.disarm();
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
    }

    pub(super) fn first_dropped_openable_path(paths: &[PathBuf]) -> Option<PathBuf> {
        paths
            .iter()
            .find(|path| {
                path.is_file()
                    && (crate::document_io::is_markdown_path(path)
                        || crate::document_io::is_image_path(path))
            })
            .cloned()
    }

    pub(crate) fn on_external_paths_drop(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = paths.paths().to_vec();
        let target = self.capture_dropped_resource_target(window, cx);
        let window_handle = window.window_handle();
        let weak_editor = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let kind = cx
                .background_spawn(async move { classify_dropped_paths(&paths) })
                .await;
            let _ = cx.update_window(
                window_handle,
                move |_view: AnyView, window: &mut Window, cx: &mut App| {
                    let _ = weak_editor.update(cx, |editor, cx| match kind {
                        DroppedPathKind::Open(path) => {
                            editor.open_dropped_markdown_in_tab(path, cx)
                        }
                        DroppedPathKind::Resource(path) => {
                            if let Some(target) = target {
                                editor.insert_dropped_resource(path, target, cx)
                            } else {
                                let strings = cx.global::<I18nManager>().strings().clone();
                                editor.show_drop_open_failed_prompt(
                                    strings.drop_no_markdown_file_message,
                                    window,
                                    cx,
                                );
                            }
                        }
                        DroppedPathKind::Invalid => {
                            let strings = cx.global::<I18nManager>().strings().clone();
                            editor.show_drop_open_failed_prompt(
                                strings.drop_no_markdown_file_message,
                                window,
                                cx,
                            );
                        }
                    });
                },
            );
        })
        .detach();
    }

    /// 文件拖放与工作区导航共享同一套打开策略：已打开的路径切换到原 Tab，
    /// 新路径创建 Tab；当前文档无论是否已修改都不得被拖入文件覆盖。
    pub(in crate::editor) fn open_dropped_markdown_in_tab(
        &mut self,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.open_path_in_tab(path, cx);
    }

    #[cfg(test)]
    pub(crate) fn request_dropped_markdown_replace(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_menu_bar(cx);
        self.hide_info_dialog(cx);
        self.dismiss_contextual_overlays(cx);

        if self.is_document_dirty() {
            self.pending_drop_replace_path = Some(path);
            self.pending_drop_replace_after_save = false;
            if !self.show_drop_replace_dialog {
                self.drop_replace_restore_focus = self.document.focused_block_entity_id(window, cx);
                self.show_drop_replace_dialog = true;
                window.blur();
            }
            cx.notify();
            return;
        }

        match self.replace_document_from_path(&path, cx) {
            Ok(()) => window.set_window_edited(false),
            Err(err) => {
                self.clear_pending_workspace_navigation();
                self.show_drop_open_failed_prompt(err.to_string(), window, cx);
            }
        }
    }

    pub(super) fn replace_document_from_path(
        &mut self,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let opened = crate::document_io::read_markdown_file(path)?;
        let encoding = opened.encoding.clone();
        self.replace_document_from_markdown(opened.text, Some(path.to_path_buf()), cx);
        self.source_encoding = encoding;
        if !self.source_encoding.is_utf8() {
            self.set_view_mode(ViewMode::Preview, cx);
            self.show_encoding_conversion_dialog = true;
        }
        if !crate::document_io::is_markdown_path(path) {
            self.set_view_mode(ViewMode::Source, cx);
        }
        crate::app_menu::record_recent_file_from_editor(path, cx);
        Ok(())
    }

    pub(super) fn replace_document_from_markdown(
        &mut self,
        markdown: String,
        file_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        // A document replacement invalidates every renderer-owned generation,
        // not only standalone image-preview tiles. Cancel the old document's
        // decode tasks before advancing the epoch so completions cannot retain
        // or publish payloads into the new document.
        self.release_render_assets_for_active_document(cx);
        self.document_epoch = self.document_epoch.wrapping_add(1);
        self.reset_markdown_view_state_identity(file_path.as_deref());
        self.image_preview_path = None;
        self.source_encoding = crate::document_io::DocumentEncoding::Utf8;
        self.show_encoding_conversion_dialog = false;
        self.saved_file_fingerprint = file_path
            .as_deref()
            .and_then(|path| crate::recovery::fingerprint_file(path).ok());
        self.external_file_conflict = false;
        self.recovered_session = false;
        self.show_external_conflict_dialog = false;
        self.external_conflict_preview = None;
        self.external_conflict_restore_focus = None;
        self.allow_external_overwrite_once = false;
        self.document_kind = file_path
            .as_deref()
            .map(DocumentKind::from_path)
            .unwrap_or(DocumentKind::Markdown);
        self.file_path = file_path;
        self.image_preview_zoom = 1.0;
        self.view_mode = ViewMode::Rendered;
        self.split_preview = None;
        self.projection_cache_task = None;
        self.projection_cache_scheduled_revision = None;
        self.split_projection_task = None;
        self.split_projection_scheduled_revision = None;
        self.source_document = gmark_document::SourceDocument::new(&markdown).into();
        self.projection_cache = None;
        self.table_cells.clear();
        self.rebuild_primary_projection_from_source(cx);

        self.document_dirty = false;
        self.pending_window_edited = false;
        self.pending_window_title_refresh = true;
        self.pending_save = false;
        self.pending_save_as = false;
        self.pending_resource_insertion = None;
        self.save_task = None;
        self.save_queued = false;
        self.auto_save_task = None;
        self.pending_open_link = None;
        self.pending_close_after_save = false;
        self.close_dialog_restore_focus = None;
        self.show_unsaved_changes_dialog = false;
        self.clear_pending_drop_replace_state(cx);
        self.dismiss_contextual_overlays(cx);
        self.close_menu_bar(cx);
        self.table_axis_preview = None;
        self.table_axis_selection = None;
        self.sync_table_axis_visuals(cx);
        self.clear_cross_block_selection(cx);

        self.pending_scroll_active_block_into_view = true;
        self.pending_scroll_recheck_after_layout = true;
        self.last_scroll_viewport_size = None;
        self.scroll_handle.set_offset(point(px(0.0), px(0.0)));
        self.pending_focus = self.first_focusable_entity_id(cx);
        self.active_entity_id = self.pending_focus;

        self.undo_history.clear();
        self.redo_history.clear();
        self.pending_undo_capture = None;
        self.last_selection_snapshot = Self::empty_selection_snapshot();
        self.history_restore_in_progress = false;
        self.checkpoint_recovery_journal();
        self.refresh_stable_document_snapshot(cx);
        self.sync_workspace_after_document_path_change(cx);
        self.restart_file_watcher(cx);
        self.apply_pending_workspace_navigation(cx);
        cx.notify();
    }

    pub(crate) fn cancel_drop_replace_dialog(&mut self, cx: &mut Context<Self>) {
        self.clear_pending_workspace_navigation();
        let restore_focus = self.drop_replace_restore_focus.take();
        self.clear_pending_drop_replace_state(cx);
        if let Some(focus_id) = restore_focus {
            self.pending_focus = Some(focus_id);
            self.pending_scroll_active_block_into_view = true;
        }
        cx.notify();
    }

    pub(crate) fn discard_pending_drop_replace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.pending_drop_replace_path.take() else {
            self.clear_pending_drop_replace_state(cx);
            return;
        };

        self.clear_pending_drop_replace_state(cx);
        match self.replace_document_from_path(&path, cx) {
            Ok(()) => window.set_window_edited(false),
            Err(err) => self.show_drop_open_failed_prompt(err.to_string(), window, cx),
        }
    }

    pub(crate) fn save_and_replace_pending_drop(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_drop_replace_path.is_none() {
            self.clear_pending_drop_replace_state(cx);
            return;
        }

        self.show_drop_replace_dialog = false;
        self.pending_drop_replace_after_save = true;
        self.close_menu_bar(cx);

        if let Some(path) = self.file_path.clone() {
            if self.save_to_existing_path(&path, window, cx) {
                self.replace_after_successful_save(window, cx);
            } else {
                self.abort_pending_drop_replace_after_save(cx);
            }
            return;
        }

        self.save_via_prompt_then_replace_drop(window, cx);
        cx.notify();
    }

    pub(crate) fn on_cancel_drop_replace_dialog(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_drop_replace_dialog(cx);
    }

    pub(crate) fn on_discard_and_replace_drop(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.discard_pending_drop_replace(window, cx);
    }

    pub(crate) fn on_save_and_replace_drop(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_and_replace_pending_drop(window, cx);
    }

    fn replace_after_successful_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drop_path) = self.pending_drop_replace_path.take() else {
            self.clear_pending_drop_replace_state(cx);
            return;
        };

        self.clear_pending_drop_replace_state(cx);
        match self.replace_document_from_path(&drop_path, cx) {
            Ok(()) => window.set_window_edited(false),
            Err(err) => self.show_drop_open_failed_prompt(err.to_string(), window, cx),
        }
    }

    fn save_via_prompt_then_replace_drop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drop_path) = self.pending_drop_replace_path.clone() else {
            self.clear_pending_drop_replace_state(cx);
            return;
        };
        let (save_snapshot, source_format) = match self.prepare_background_save(cx) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => {
                self.save_queued = true;
                return;
            }
            Err(error) => {
                self.clear_pending_drop_replace_state(cx);
                self.show_drop_open_failed_prompt(error, window, cx);
                return;
            }
        };
        let markdown = save_snapshot
            .resident_baseline
            .as_ref()
            .map(gmark_document::DocumentSnapshot::text)
            .or_else(|| {
                save_snapshot
                    .read_all()
                    .ok()
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            })
            .unwrap_or_default();
        let saved_format = save_snapshot
            .source_format
            .clone()
            .unwrap_or_else(|| source_format.clone());
        let saved_revision = gmark_document::Revision::from_u64(save_snapshot.revision.0);
        let (default_dir, suggested_name) = self.save_dialog_defaults();
        let document_kind = self.document_kind;
        let prompt = cx.prompt_for_new_path(&default_dir, suggested_name.as_deref());
        let weak_editor = cx.entity().downgrade();
        let weak_editor_for_cancel = weak_editor.clone();
        let weak_editor_for_error = weak_editor.clone();
        let weak_editor_for_write_error = weak_editor.clone();
        let window_handle = window.window_handle();

        cx.spawn(async move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut save_path = match prompt.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) | Err(_) => {
                    let _ = weak_editor_for_cancel.update(cx, |this, cx| {
                        let _ = this.source_document.try_save_failed(
                            saved_revision,
                            gmark_document_runtime::SaveFailureCode::Cancelled,
                        );
                        this.abort_pending_drop_replace_after_save(cx);
                    });
                    return;
                }
                Ok(Err(err)) => {
                    let _ = weak_editor_for_error.update(cx, |this, cx| {
                        let _ = this.source_document.try_save_failed(
                            saved_revision,
                            gmark_document_runtime::SaveFailureCode::Other,
                        );
                        this.abort_pending_drop_replace_after_save(cx);
                    });
                    let detail = err.to_string();
                    let _ = cx.update_window(
                        window_handle,
                        move |_view: AnyView, window: &mut Window, cx: &mut App| {
                            let strings = cx.global::<I18nManager>().strings().clone();
                            let buttons = [strings.info_dialog_ok.as_str()];
                            let _ = window.prompt(
                                PromptLevel::Critical,
                                &strings.save_failed_title,
                                Some(&detail),
                                &buttons,
                                cx,
                            );
                        },
                    );
                    return;
                }
            };

            document_kind.apply_default_extension(&mut save_path);

            let reservation = match weak_editor
                .update(cx, |this, cx| this.reserve_save_as_target(&save_path, cx))
            {
                Ok(Ok(crate::app::document_service::SaveAsTargetReservation::Reserved(
                    reservation,
                ))) => reservation,
                Ok(Ok(crate::app::document_service::SaveAsTargetReservation::Occupied(target))) => {
                    let prompt = cx.update_window(
                        window_handle,
                        move |_view: AnyView, window: &mut Window, cx: &mut App| {
                            let strings = cx.global::<I18nManager>().strings().clone();
                            let buttons = [
                                strings.menu_open_file.as_str(),
                                strings.external_change_cancel.as_str(),
                            ];
                            window.prompt(
                                PromptLevel::Warning,
                                &strings.save_failed_title,
                                Some("目标文档已在其他视图打开；是否切换到现有文档？"),
                                &buttons,
                                cx,
                            )
                        },
                    );
                    let choice = match prompt {
                        Ok(receiver) => receiver.await.unwrap_or(1),
                        Err(_) => 1,
                    };
                    if choice == 0 {
                        let _ = weak_editor.update(cx, |this, cx| {
                            let _ = this.switch_to_shared_save_as_target(target, cx);
                            this.abort_pending_drop_replace_after_save(cx);
                        });
                    }
                    return;
                }
                Ok(Err(detail)) => {
                    let _ = weak_editor_for_write_error.update(cx, |this, cx| {
                        this.abort_pending_drop_replace_after_save(cx);
                    });
                    let _ = cx.update_window(
                        window_handle,
                        move |_view: AnyView, window: &mut Window, cx: &mut App| {
                            let strings = cx.global::<I18nManager>().strings().clone();
                            let buttons = [strings.info_dialog_ok.as_str()];
                            let _ = window.prompt(
                                PromptLevel::Critical,
                                &strings.save_failed_title,
                                Some(&detail),
                                &buttons,
                                cx,
                            );
                        },
                    );
                    return;
                }
                Err(_) => {
                    let _ = weak_editor_for_write_error.update(cx, |this, cx| {
                        this.abort_pending_drop_replace_after_save(cx);
                    });
                    return;
                }
            };

            let cancellation = gmark_paged_document::SearchCancellation::default();
            let identity = match save_snapshot.save_as_atomic_cancellable(&save_path, &cancellation)
            {
                Ok(identity) => identity,
                Err(err) => {
                    let _ = weak_editor_for_write_error.update(cx, |this, cx| {
                        let code = if matches!(
                            &err,
                            gmark_paged_document::PagedDocumentError::SourceChanged
                                | gmark_paged_document::PagedDocumentError::Persist { .. }
                        ) {
                            gmark_document_runtime::SaveFailureCode::Uncertain
                        } else {
                            gmark_document_runtime::SaveFailureCode::Other
                        };
                        let _ = this.source_document.try_save_failed(saved_revision, code);
                        this.abort_pending_drop_replace_after_save(cx);
                    });
                    let detail = err.to_string();
                    let _ = cx.update_window(
                        window_handle,
                        move |_view: AnyView, window: &mut Window, cx: &mut App| {
                            let strings = cx.global::<I18nManager>().strings().clone();
                            let buttons = [strings.info_dialog_ok.as_str()];
                            let _ = window.prompt(
                                PromptLevel::Critical,
                                &strings.save_failed_title,
                                Some(&detail),
                                &buttons,
                                cx,
                            );
                        },
                    );
                    return;
                }
            };
            let saved_path = save_path.clone();
            let replace_result = weak_editor.update(cx, move |this, cx| {
                if let Err(error) = reservation.commit() {
                    return Err(anyhow::anyhow!(error.to_string()));
                }
                let completion = this
                    .source_document
                    .try_save_succeeded(saved_revision, identity);
                if let Err(error) = completion {
                    return Err(anyhow::anyhow!(error.to_string()));
                }
                this.apply_successful_save(saved_path, markdown, saved_format, cx);
                this.pending_drop_replace_path = Some(drop_path);
                this.replace_after_successful_save_async(cx)
            });
            let _ = cx.update_window(
                window_handle,
                move |_view: AnyView, window: &mut Window, cx: &mut App| match replace_result {
                    Ok(Ok(())) => window.set_window_edited(false),
                    Ok(Err(err)) => {
                        let strings = cx.global::<I18nManager>().strings().clone();
                        let buttons = [strings.info_dialog_ok.as_str()];
                        let _ = window.prompt(
                            PromptLevel::Critical,
                            &strings.open_failed_title,
                            Some(&err.to_string()),
                            &buttons,
                            cx,
                        );
                    }
                    Err(_) => {}
                },
            );
        })
        .detach();
    }

    fn replace_after_successful_save_async(&mut self, cx: &mut Context<Self>) -> Result<()> {
        let Some(drop_path) = self.pending_drop_replace_path.take() else {
            self.clear_pending_drop_replace_state(cx);
            return Ok(());
        };

        self.clear_pending_drop_replace_state(cx);
        self.replace_document_from_path(&drop_path, cx)
    }

    fn abort_pending_drop_replace_after_save(&mut self, cx: &mut Context<Self>) {
        self.pending_drop_replace_after_save = false;
        self.show_drop_replace_dialog = false;
        self.pending_drop_replace_path = None;
        let restore_focus = self.drop_replace_restore_focus.take();
        if let Some(focus_id) = restore_focus {
            self.pending_focus = Some(focus_id);
            self.pending_scroll_active_block_into_view = true;
        }
        cx.notify();
    }

    fn clear_pending_drop_replace_state(&mut self, cx: &mut Context<Self>) {
        let had_path = self.pending_drop_replace_path.take().is_some();
        let had_dialog = self.show_drop_replace_dialog;
        let had_after_save = self.pending_drop_replace_after_save;
        let had_restore_focus = self.drop_replace_restore_focus.take().is_some();
        let had_state = had_path || had_dialog || had_after_save || had_restore_focus;
        self.show_drop_replace_dialog = false;
        self.pending_drop_replace_after_save = false;
        if had_state {
            cx.notify();
        }
    }

    fn show_drop_open_failed_prompt(
        &self,
        detail: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let strings = cx.global::<I18nManager>().strings().clone();
        let buttons = [strings.info_dialog_ok.as_str()];
        let _ = window.prompt(
            PromptLevel::Critical,
            &strings.open_failed_title,
            Some(&detail),
            &buttons,
            cx,
        );
    }
}

#[cfg(test)]
#[path = "../../tests/unit/editor/resource_materialize_limits.rs"]
mod tests;
