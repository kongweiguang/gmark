// @author kongweiguang

//! Disk reload and encoding conversion.

use super::*;

impl DocumentHost {
    pub(super) fn reload_from_disk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reload_from_disk_with_encoding(None, window, cx);
    }

    pub(crate) fn reopen_with_encoding(
        &mut self,
        encoding: TextEncoding,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if document_dirty_state(&self.document, &self.pending_dirty) {
            self.error = Some(
                cx.global::<I18nManager>()
                    .strings()
                    .large_document_text("reopen_dirty_error")
                    .into(),
            );
            cx.emit(DocumentHostEvent::StateChanged);
            cx.notify();
            return;
        }
        self.reload_from_disk_with_encoding(Some(encoding), window, cx);
    }

    pub(super) fn reload_from_disk_with_encoding(
        &mut self,
        forced_encoding: Option<TextEncoding>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.saving || self.reloading {
            return;
        }
        self.cancel_selection_transfers();
        let path = self.path.clone();
        #[cfg(test)]
        let configured_loading = gmark_document_core::LoadingPolicy::default();
        #[cfg(not(test))]
        let configured_loading = gmark_config::read_app_preferences()
            .map(|preferences| preferences.document_loading.policy())
            .unwrap_or_default();
        let loading = if forced_encoding.is_some() {
            gmark_document_core::LoadingPolicy {
                max_resident_bytes: Some(self.probe.options.max_resident_bytes),
                force_safe_source: self.probe.force_safe_source,
            }
        } else {
            configured_loading
        };
        let loading_limits = loading.effective_limits();
        #[cfg(not(test))]
        let recovery_dir = gmark_config::GmarkConfigDirs::from_system()
            .ok()
            .map(|dirs| dirs.recovery_dir());
        #[cfg(test)]
        let recovery_dir: Option<PathBuf> = None;
        let window_handle = window.window_handle();
        if let Some(cancellation) = self.coordinator.index_cancellation.take() {
            cancellation.cancel();
        }
        let cancellation = SearchCancellation::default();
        self.coordinator.index_cancellation = Some(cancellation.clone());
        self.coordinator.index_generation = self.coordinator.index_generation.wrapping_add(1);
        let task_stamp = DocumentTaskStamp::capture(self, self.coordinator.index_generation);
        self.coordinator.external_generation = self.coordinator.external_generation.wrapping_add(1);
        self.active_edit = None;
        self.reloading = true;
        self.error = None;
        cx.emit(DocumentHostEvent::StateChanged);
        self.coordinator.index_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let original = FileSource::open(&path)?;
                    let mut probe = gmark_paged_document::probe_file(
                        &path,
                        gmark_paged_document::ProbeOptions {
                            max_resident_bytes: loading_limits.max_resident_bytes,
                            ..gmark_paged_document::ProbeOptions::default()
                        },
                    )?;
                    probe.force_safe_source = loading.force_safe_source;
                    let plan =
                        gmark_document_core::OpenPolicyResolver.resolve(loading, &probe.profile());
                    probe.strategy = match plan.backend {
                        gmark_document_core::DocumentBackendKind::Resident => {
                            OpenStrategy::Resident
                        }
                        gmark_document_core::DocumentBackendKind::Paged => OpenStrategy::Paged,
                    };
                    if let Some(encoding) = forced_encoding {
                        probe.encoding = encoding;
                    }
                    let reopened_encoding = text_encoding_label(&probe.encoding);
                    let original_for_session = original.clone();
                    let prepared = prepare_utf8_source(original, probe.encoding.clone())?;
                    let index = LineIndex::build_cancellable(prepared.source(), &cancellation)?;
                    let document = build_document_session(
                        &probe,
                        &original_for_session,
                        prepared.source().clone(),
                        index.clone(),
                        false,
                    )?;
                    let recovery = recovery_dir.as_ref().map(|dir| {
                        DocumentRecoveryJournal::create(
                            dir,
                            &original_for_session,
                            probe.encoding.clone(),
                            &document,
                        )
                    });
                    let (structure_source, structure_index, structure_bytes) =
                        structure_input_for_session(&document, &prepared, &index, &cancellation)?;
                    let structured = if derived_views_enabled(probe.strategy) {
                        build_structured_index(
                            &structure_source,
                            &structure_index,
                            probe.format.clone(),
                            &cancellation,
                            structure_bytes,
                        )?
                    } else {
                        None
                    };
                    Ok::<_, gmark_paged_document::PagedDocumentError>((
                        probe,
                        prepared,
                        index,
                        document,
                        structured,
                        recovery,
                        reopened_encoding,
                    ))
                })
                .await;
            let reloaded = result.is_ok();
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.coordinator.index_generation) {
                    view.reloading = false;
                    return;
                }
                view.coordinator.index_cancellation = None;
                view.reloading = false;
                match result {
                    Ok((
                        probe,
                        prepared,
                        index,
                        document,
                        structured,
                        recovery,
                        reopened_encoding,
                    )) => {
                        if let Some(mut journal) = view.coordinator.recovery_journal.take()
                            && let Err(error) = journal.checkpoint(&document)
                        {
                            view.coordinator.recovery_error =
                                Some(localized_document_error(&error, cx));
                        }
                        view.probe = probe;
                        view.document_epoch = view.document_epoch.wrapping_add(1);
                        view.prepared_source = Some(prepared);
                        view.provisional_source = None;
                        view.index = Some(index);
                        view.install_document_session(document);
                        view.invalidate_source_rows();
                        view.structured_index = structured;
                        view.invalidate_structured_runtime();
                        view.active_edit = None;
                        set_document_dirty_state(
                            &mut view.document,
                            &mut view.pending_dirty,
                            false,
                        );
                        view.coordinator.pending_external_change = None;
                        view.coordinator.external_monitor_paused = false;
                        view.coordinator.external_status = Some(
                            cx.global::<I18nManager>()
                                .strings()
                                .large_document_text("reopened_as_template")
                                .replace("{encoding}", &reopened_encoding)
                                .into(),
                        );
                        match recovery {
                            Some(Ok(journal)) => view.coordinator.recovery_journal = Some(journal),
                            Some(Err(error)) => {
                                view.coordinator.recovery_error =
                                    Some(localized_document_error(&error, cx))
                            }
                            None => {}
                        }
                    }
                    Err(error) => view.error = Some(localized_document_error(&error, cx)),
                }
                cx.emit(DocumentHostEvent::StateChanged);
                cx.notify();
            });
            if reloaded {
                let _ = cx.update_window(
                    window_handle,
                    |_view: AnyView, window: &mut Window, _cx: &mut App| {
                        window.set_window_edited(false);
                    },
                );
            }
        });
        cx.notify();
    }

    pub(super) fn keep_local_after_external_change(&mut self, cx: &mut Context<Self>) {
        self.coordinator.pending_external_change = None;
        self.coordinator.external_monitor_paused = true;
        self.tail_enabled = false;
        self.coordinator.external_status = Some(
            cx.global::<I18nManager>()
                .strings()
                .large_document_text("keeping_local")
                .into(),
        );
        cx.emit(DocumentHostEvent::StateChanged);
        cx.notify();
    }
}
