// @author kongweiguang

use super::*;

impl Editor {
    /// 原子替换后持久化同步失败时，磁盘可能已经是新内容，但不能宣称保存成功。
    /// 刷新 fingerprint 允许用户重试，同时保留 dirty 与恢复 journal。
    fn apply_uncertain_save_baseline(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let path_changed = self.file_path.as_ref() != Some(&path);
        self.saved_file_fingerprint = crate::recovery::fingerprint_file(&path).ok();
        self.external_file_conflict = false;
        self.allow_external_overwrite_once = false;
        self.file_path = Some(path);
        if path_changed {
            self.restart_file_watcher(cx);
        }
        self.document_dirty = true;
        self.pending_window_edited = true;
        self.pending_window_title_refresh = true;
        self.schedule_recovery_journal(cx);
        self.schedule_auto_save(cx);
        self.sync_workspace_after_document_path_change(cx);
        cx.notify();
    }

    fn save_existing_path_in_background(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dispatch_started = super::perf::start();
        if self.save_task.is_some() {
            self.save_queued = true;
            return;
        }

        let overwrite = std::mem::take(&mut self.allow_external_overwrite_once);
        if self.external_file_conflict && !overwrite {
            self.present_external_file_conflict(&path, window, cx);
            return;
        }
        let expected_fingerprint = self.saved_file_fingerprint.clone();
        let conflict_restore_focus = if self.external_conflict_restore_focus.is_none()
            && self.close_dialog_restore_focus.is_none()
        {
            self.document.focused_block_entity_id(window, cx)
        } else {
            None
        };
        let (snapshot, source_format) = self.prepare_background_save(cx);
        let revision = snapshot.revision();
        let document_epoch = self.document_epoch;
        let byte_len = snapshot.len();
        let window_handle = window.window_handle();
        let should_close_after_save = self.pending_close_after_save;
        let worker_path = path.clone();
        let worker_format = source_format.clone();

        self.save_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let outcome = cx
                .background_spawn(async move {
                    let serialize_started = super::perf::start();
                    let Some((source, bytes)) =
                        snapshot.formatted_text_and_bytes(worker_format.clone())
                    else {
                        return ExistingSaveOutcome::Failed {
                            detail: "source format no longer matches save snapshot".to_owned(),
                            target_may_have_changed: false,
                        };
                    };
                    if let Some(started) = serialize_started {
                        super::perf::emit(
                            "save_serialize",
                            started,
                            Some(bytes.len()),
                            Some(true),
                            None,
                        );
                    }

                    if !overwrite
                        && let Some(expected) = expected_fingerprint
                        && crate::recovery::fingerprint_file(&worker_path)
                            .map(|current| current != expected)
                            .unwrap_or(true)
                    {
                        let (disk, disk_bytes, disk_error) = match std::fs::read(&worker_path) {
                            Ok(bytes) => (
                                String::from_utf8_lossy(&bytes).into_owned(),
                                bytes.len(),
                                None,
                            ),
                            Err(error) => (String::new(), 0, Some(error.to_string())),
                        };
                        return ExistingSaveOutcome::Conflict(build_external_conflict_preview(
                            &worker_path,
                            &source,
                            &disk,
                            disk_bytes,
                            disk_error,
                        ));
                    }

                    let write_started = super::perf::start();
                    let result = atomic_write(&worker_path, &bytes);
                    if let Some(started) = write_started {
                        let detail = result.as_ref().err().map(|error| error.stage().to_string());
                        super::perf::emit(
                            "save_atomic_write",
                            started,
                            Some(bytes.len()),
                            Some(result.is_ok()),
                            detail.as_deref(),
                        );
                    }
                    match result {
                        Ok(()) => ExistingSaveOutcome::Saved {
                            source,
                            source_format: worker_format,
                            revision,
                        },
                        Err(error) => ExistingSaveOutcome::Failed {
                            detail: error.to_string(),
                            target_may_have_changed: error.target_may_have_changed(),
                        },
                    }
                })
                .await;

            let mut saved_current_revision = false;
            let mut conflict = false;
            let mut error = None;
            let _ = this.update(cx, |editor, cx| {
                editor.save_task = None;
                match outcome {
                    ExistingSaveOutcome::Saved {
                        source,
                        source_format,
                        revision,
                    } => {
                        saved_current_revision = editor.apply_background_save_success(
                            path,
                            source,
                            source_format,
                            revision,
                            document_epoch,
                            cx,
                        );
                        if should_close_after_save && !saved_current_revision {
                            editor.abort_pending_close_after_save(cx);
                        }
                    }
                    ExistingSaveOutcome::Conflict(preview) => {
                        editor.external_file_conflict = true;
                        editor.external_conflict_preview = Some(preview);
                        if editor.external_conflict_restore_focus.is_none() {
                            editor.external_conflict_restore_focus = conflict_restore_focus;
                        }
                        editor.show_external_conflict_dialog = true;
                        editor.close_menu_bar(cx);
                        editor.hide_info_dialog(cx);
                        conflict = true;
                        if should_close_after_save {
                            editor.pending_close_after_save = true;
                        }
                        cx.notify();
                    }
                    ExistingSaveOutcome::Failed {
                        detail,
                        target_may_have_changed,
                    } => {
                        editor.abort_pending_tab_close_after_save(cx);
                        editor.abort_window_close_tab_sequence(cx);
                        if should_close_after_save {
                            editor.abort_pending_close_after_save(cx);
                        }
                        if target_may_have_changed && editor.document_epoch == document_epoch {
                            editor.apply_uncertain_save_baseline(path, cx);
                        }
                        error = Some(detail);
                    }
                }
                if std::mem::take(&mut editor.save_queued) && editor.document_dirty {
                    editor.pending_save = true;
                    cx.notify();
                }
            });

            let _ = cx.update_window(
                window_handle,
                move |_view: AnyView, window: &mut Window, cx: &mut App| {
                    if saved_current_revision {
                        window.set_window_edited(false);
                        if should_close_after_save {
                            window.remove_window();
                        }
                    } else if conflict {
                        window.blur();
                    } else if let Some(detail) = error {
                        let strings = cx.global::<I18nManager>().strings().clone();
                        let buttons = [strings.info_dialog_ok.as_str()];
                        let _ = window.prompt(
                            PromptLevel::Critical,
                            &strings.save_failed_title,
                            Some(&detail),
                            &buttons,
                            cx,
                        );
                    }
                },
            );
        }));

        if let Some(started) = dispatch_started {
            super::perf::emit(
                "save_background_dispatch",
                started,
                Some(byte_len),
                Some(true),
                None,
            );
        }
    }

    pub(in crate::editor) fn save_to_existing_path(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.existing_path_has_external_change(path) {
            self.present_external_file_conflict(path, window, cx);
            return false;
        }
        let serialize_started = super::perf::start();
        let (markdown, source_format, bytes) = self.serialized_document_bytes(cx);
        if let Some(started) = serialize_started {
            super::perf::emit(
                "save_serialize",
                started,
                Some(bytes.len()),
                Some(true),
                None,
            );
        }
        let write_started = super::perf::start();
        let result = atomic_write(path, &bytes);
        if let Some(started) = write_started {
            let detail = result.as_ref().err().map(|error| error.stage().to_string());
            super::perf::emit(
                "save_atomic_write",
                started,
                Some(bytes.len()),
                Some(result.is_ok()),
                detail.as_deref(),
            );
        }
        match result {
            Ok(_) => {
                self.apply_successful_save(path.to_path_buf(), markdown, source_format, cx);
                window.set_window_edited(false);
                true
            }
            Err(err) => {
                if err.target_may_have_changed() {
                    self.apply_uncertain_save_baseline(path.to_path_buf(), cx);
                }
                let detail = err.to_string();
                let strings = cx.global::<I18nManager>().strings().clone();
                let buttons = [strings.info_dialog_ok.as_str()];
                let _ = window.prompt(
                    PromptLevel::Critical,
                    &strings.save_failed_title,
                    Some(&detail),
                    &buttons,
                    cx,
                );
                false
            }
        }
    }

    fn existing_path_has_external_change(&mut self, path: &Path) -> bool {
        if std::mem::take(&mut self.allow_external_overwrite_once) {
            return false;
        }
        if self.external_file_conflict {
            return true;
        }
        let Some(expected) = self.saved_file_fingerprint.as_ref() else {
            return false;
        };
        let changed = crate::recovery::fingerprint_file(path)
            .map(|current| current != *expected)
            .unwrap_or(true);
        self.external_file_conflict = changed;
        changed
    }

    pub(in crate::editor) fn present_external_file_conflict(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let local = self.serialized_document_text(cx);
        let (disk, disk_bytes, disk_error) = match std::fs::read(path) {
            Ok(bytes) => (
                String::from_utf8_lossy(&bytes).into_owned(),
                bytes.len(),
                None,
            ),
            Err(error) => (String::new(), 0, Some(error.to_string())),
        };
        self.external_conflict_preview = Some(build_external_conflict_preview(
            path, &local, &disk, disk_bytes, disk_error,
        ));
        if self.external_conflict_restore_focus.is_none()
            && self.close_dialog_restore_focus.is_none()
        {
            self.external_conflict_restore_focus =
                self.document.focused_block_entity_id(window, cx);
        }
        self.show_external_conflict_dialog = true;
        self.close_menu_bar(cx);
        self.hide_info_dialog(cx);
        window.blur();
        cx.notify();
    }

    fn hide_external_file_conflict(&mut self, cx: &mut Context<Self>) {
        self.show_external_conflict_dialog = false;
        self.external_conflict_preview = None;
        cx.notify();
    }

    pub(crate) fn on_cancel_external_conflict(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_external_conflict(cx);
    }

    pub(in crate::editor) fn cancel_external_conflict(&mut self, cx: &mut Context<Self>) {
        self.hide_external_file_conflict(cx);
        self.abort_window_close_tab_sequence(cx);
        if self.pending_close_after_save {
            self.abort_pending_close_after_save(cx);
            self.external_conflict_restore_focus = None;
            return;
        }
        if let Some(entity_id) = self.external_conflict_restore_focus.take() {
            self.pending_focus = Some(entity_id);
            self.pending_scroll_active_block_into_view = true;
        }
    }

    pub(crate) fn on_save_as_external_conflict(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hide_external_file_conflict(cx);
        self.external_conflict_restore_focus = None;
        self.pending_save_as = true;
        cx.notify();
    }

    pub(crate) fn on_overwrite_external_conflict(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hide_external_file_conflict(cx);
        self.external_conflict_restore_focus = None;
        self.allow_external_overwrite_once = true;
        self.pending_save = true;
        cx.notify();
    }

    pub(crate) fn on_reload_external_conflict(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.file_path.clone() else {
            self.hide_external_file_conflict(cx);
            self.external_conflict_restore_focus = None;
            return;
        };
        self.pending_close_after_save = false;
        self.abort_window_close_tab_sequence(cx);
        self.hide_external_file_conflict(cx);
        self.external_conflict_restore_focus = None;
        match self.replace_document_from_path(&path, cx) {
            Ok(()) => window.set_window_edited(false),
            Err(error) => {
                let strings = cx.global::<I18nManager>().strings().clone();
                let buttons = [strings.info_dialog_ok.as_str()];
                let _ = window.prompt(
                    PromptLevel::Critical,
                    &strings.open_failed_title,
                    Some(&error.to_string()),
                    &buttons,
                    cx,
                );
            }
        }
    }

    fn save_document_via_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (snapshot, source_format) = self.prepare_background_save(cx);
        let saved_revision = snapshot.revision();
        let saved_document_epoch = self.document_epoch;
        let (default_dir, suggested_name) = self.save_dialog_defaults();
        let prompt = cx.prompt_for_new_path(&default_dir, suggested_name.as_deref());
        let weak_editor = cx.entity().downgrade();
        let weak_editor_for_cancel = weak_editor.clone();
        let weak_editor_for_error = weak_editor.clone();
        let window_handle = window.window_handle();
        let should_close_after_save = self.pending_close_after_save;

        cx.spawn(async move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut path = match prompt.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) | Err(_) => {
                    let _ = weak_editor_for_cancel.update(cx, |this, cx| {
                        this.abort_pending_tab_close_after_save(cx);
                        this.abort_window_close_tab_sequence(cx);
                        if should_close_after_save {
                            this.abort_pending_close_after_save(cx);
                        }
                    });
                    return;
                }
                Ok(Err(err)) => {
                    let _ = weak_editor_for_error.update(cx, |this, cx| {
                        this.abort_pending_tab_close_after_save(cx);
                        this.abort_window_close_tab_sequence(cx);
                        if should_close_after_save {
                            this.abort_pending_close_after_save(cx);
                        }
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

            if path.extension().is_none() {
                path.set_extension("md");
            }

            let worker_format = source_format.clone();
            let result = cx
                .background_spawn(async move {
                    let serialize_started = super::perf::start();
                    let Some((source, bytes)) =
                        snapshot.formatted_text_and_bytes(worker_format.clone())
                    else {
                        return Err((
                            path,
                            "source format no longer matches save snapshot".to_owned(),
                            false,
                        ));
                    };
                    if let Some(started) = serialize_started {
                        super::perf::emit(
                            "save_serialize",
                            started,
                            Some(bytes.len()),
                            Some(true),
                            None,
                        );
                    }
                    let write_started = super::perf::start();
                    let write_result = atomic_write(&path, &bytes);
                    if let Some(started) = write_started {
                        let detail = write_result
                            .as_ref()
                            .err()
                            .map(|error| error.stage().to_string());
                        super::perf::emit(
                            "save_atomic_write",
                            started,
                            Some(bytes.len()),
                            Some(write_result.is_ok()),
                            detail.as_deref(),
                        );
                    }
                    match write_result {
                        Ok(()) => Ok((path, source, worker_format)),
                        Err(error) => {
                            let target_may_have_changed = error.target_may_have_changed();
                            Err((path, error.to_string(), target_may_have_changed))
                        }
                    }
                })
                .await;

            let (path, source, source_format) = match result {
                Ok(saved) => saved,
                Err((failed_path, detail, target_may_have_changed)) => {
                    let _ = weak_editor.update(cx, |this, cx| {
                        this.abort_pending_tab_close_after_save(cx);
                        this.abort_window_close_tab_sequence(cx);
                        if should_close_after_save {
                            this.abort_pending_close_after_save(cx);
                        }
                        if target_may_have_changed && this.document_epoch == saved_document_epoch {
                            this.apply_uncertain_save_baseline(failed_path, cx);
                        }
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
            };
            let saved_current_revision = weak_editor
                .update(cx, move |this, cx| {
                    let saved_current = this.apply_background_save_success(
                        path,
                        source,
                        source_format,
                        saved_revision,
                        saved_document_epoch,
                        cx,
                    );
                    if !saved_current {
                        this.abort_pending_tab_close_after_save(cx);
                        this.abort_window_close_tab_sequence(cx);
                    }
                    if should_close_after_save && !saved_current {
                        this.abort_pending_close_after_save(cx);
                    }
                    saved_current
                })
                .unwrap_or(false);
            let _ = cx.update_window(
                window_handle,
                move |_view: AnyView, window: &mut Window, _cx: &mut App| {
                    if saved_current_revision {
                        window.set_window_edited(false);
                    }
                    if should_close_after_save && saved_current_revision {
                        window.remove_window();
                    }
                },
            );
        })
        .detach();
    }

    pub(crate) fn save_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.source_encoding.is_utf8() {
            self.request_encoding_conversion(cx);
            return;
        }
        if let Some(path) = self.file_path.clone() {
            self.save_existing_path_in_background(path, window, cx);
            return;
        }

        self.save_document_via_prompt(window, cx);
    }

    pub(crate) fn save_document_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.source_surface.is_some() {
            self.save_large_document_via_prompt(window, cx);
            return;
        }
        if !self.source_encoding.is_utf8() {
            self.request_encoding_conversion(cx);
            return;
        }
        self.save_document_via_prompt(window, cx);
    }

    fn save_large_document_via_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(large_file) = self.source_surface.disk_view_cloned() else {
            return;
        };
        let (default_dir, suggested_name) = self.save_dialog_defaults();
        let prompt = cx.prompt_for_new_path(&default_dir, suggested_name.as_deref());
        let window_handle = window.window_handle();
        let weak_editor = cx.entity().downgrade();
        cx.spawn(async move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut path = match prompt.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) | Err(_) => {
                    let _ = weak_editor.update(cx, |editor, cx| {
                        editor.abort_pending_tab_close_after_save(cx);
                        editor.abort_window_close_tab_sequence(cx);
                        editor.abort_pending_close_after_save(cx);
                    });
                    return;
                }
                Ok(Err(error)) => {
                    let detail = error.to_string();
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
            if path.extension().is_none() {
                path.set_extension("md");
            }
            let _ = large_file.update(cx, move |view, cx| {
                view.save_as_path(path, window_handle, cx);
            });
        })
        .detach();
    }
}
