// @author kongweiguang

//! External Markdown file drops for replacing the current editor window.

use std::path::{Path, PathBuf};

use anyhow::Result;
use gpui::*;

use super::{DocumentKind, Editor, ViewMode};
use crate::i18n::I18nManager;

impl Editor {
    pub(super) fn is_markdown_file_path(path: &Path) -> bool {
        path.is_file() && crate::document_io::is_markdown_path(path)
    }

    pub(super) fn first_dropped_markdown_path(paths: &[PathBuf]) -> Option<PathBuf> {
        paths
            .iter()
            .find(|path| Self::is_markdown_file_path(path))
            .cloned()
    }

    pub(crate) fn on_external_paths_drop(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = Self::first_dropped_markdown_path(paths.paths()) else {
            let strings = cx.global::<I18nManager>().strings().clone();
            self.show_drop_open_failed_prompt(strings.drop_no_markdown_file_message, window, cx);
            return;
        };

        if crate::document_io::open_document(&path)
            .is_ok_and(|opened| matches!(opened, crate::document_io::OpenedDocument::Paged(_)))
        {
            cx.spawn(async move |_editor, cx| {
                let _ = cx.update(move |cx| {
                    if let Err(error) = crate::app_menu::open_file_in_new_window(cx, &path) {
                        eprintln!(
                            "failed to open dropped large file '{}': {error}",
                            path.display()
                        );
                    }
                });
            })
            .detach();
            return;
        }

        self.request_dropped_markdown_replace(path, window, cx);
    }

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
        self.document_epoch = self.document_epoch.wrapping_add(1);
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
        self.view_mode = ViewMode::Rendered;
        self.split_preview = None;
        self.projection_cache_task = None;
        self.projection_cache_scheduled_revision = None;
        self.split_projection_task = None;
        self.split_projection_scheduled_revision = None;
        self.source_document = gmark_document::SourceDocument::new(&markdown).into();
        let normalized = self.source_document.text();
        self.projection_cache = None;
        self.table_cells.clear();
        self.rebuild_primary_projection_from_source(cx);

        self.document_dirty = false;
        self.pending_window_edited = false;
        self.pending_window_title_refresh = true;
        self.pending_save = false;
        self.pending_save_as = false;
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
        self.last_stable_source_text = normalized;
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
        let (markdown, source_format, bytes) = self.serialized_document_bytes(cx);
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
                        this.abort_pending_drop_replace_after_save(cx);
                    });
                    return;
                }
                Ok(Err(err)) => {
                    let _ = weak_editor_for_error.update(cx, |this, cx| {
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

            if let Err(err) = gmark_document::atomic_write(&save_path, &bytes) {
                let _ = weak_editor_for_write_error.update(cx, |this, cx| {
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

            let saved_path = save_path.clone();
            let replace_result = weak_editor.update(cx, move |this, cx| {
                this.apply_successful_save(saved_path, markdown, source_format, cx);
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
