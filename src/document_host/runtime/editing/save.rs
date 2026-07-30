// @author kongweiguang

//! Atomic save and post-save session reconciliation.

use super::coordinator::map_persistence_error;
use super::*;
use std::io::Write as _;

impl DocumentHost {
    pub(crate) fn on_save_document(
        &mut self,
        _: &SaveDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.coordinator.external_monitor_paused {
            self.error = Some(
                cx.global::<I18nManager>()
                    .strings()
                    .large_document_text("disk_changed_save_as_reload")
                    .into(),
            );
            cx.emit(DocumentHostEvent::StateChanged);
            cx.notify();
            return;
        }
        // 保存会卸载活动行 Block；先把焦点交还宿主，保存结束后快捷键仍能继续工作。
        self.focus_handle.focus(window);
        if crate::source_tools::format_on_save_for_file(
            &self.path,
            crate::preferences::EditorSettings::format_on_save(cx),
        ) && self.probe.strategy != OpenStrategy::Paged
        {
            self.start_format_before_save(window.window_handle(), cx);
            return;
        }
        self.start_save(self.path.clone(), false, window.window_handle(), cx);
    }

    pub(crate) fn save_as_path(
        &mut self,
        path: PathBuf,
        window_handle: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        self.start_save(path, true, window_handle, cx);
    }

    pub(super) fn start_save(
        &mut self,
        path: PathBuf,
        save_as: bool,
        window_handle: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if self.saving
            || self.reloading
            || (!document_dirty_state(&self.document, &self.pending_dirty) && !save_as)
        {
            return;
        }
        if let Some(cancellation) = self.coordinator.save.cancellation.take() {
            cancellation.cancel();
        }
        self.coordinator.save.generation = self.coordinator.save.generation.wrapping_add(1);
        let task_stamp = DocumentTaskStamp::capture(self, self.coordinator.save.generation);
        let save_started = crate::perf::start();
        let open_strategy = self.probe.strategy;
        let probe_options = self.probe.options;
        let force_safe_source = self.probe.force_safe_source;
        let save_profile = self.probe.profile();
        let save_plan = session_plan(&save_profile, &self.probe, open_strategy, false);
        let cancellation = SearchCancellation::default();
        self.coordinator.save.cancellation = Some(cancellation.clone());
        // 保存会暂时取走 document 并重建 uniform_list 的数据源。必须保留底层像素偏移；
        // 近文件尾部用 scroll_to_item 恢复会再次经过估算布局，仍可能跳动数百行。
        let save_scroll_offset = self.scroll_handle.0.borrow().base_handle.offset();
        let Some(mut document) = self.take_document_session() else {
            self.coordinator.save.cancellation = None;
            return;
        };
        let prepared_source = self.prepared_source.take();
        let encoded_save = prepared_source
            .as_ref()
            .and_then(PreparedUtf8Source::save_plan);
        // 直接 UTF-8 的 PreparedUtf8Source 仍持有目标文件；Windows 原子替换要求
        // 所有目标句柄先关闭。编码文档的 PreparedUtf8Source 指向影子文件，失败时需保留。
        let prepared_on_error = encoded_save.as_ref().and(prepared_source);
        self.provisional_source = None;
        if let Some(cancellation) = self.coordinator.search_cancellation.take() {
            cancellation.cancel();
        }
        self.coordinator.search_task = Task::ready(());
        self.coordinator.source_task = Task::ready(());
        self.structured_task = Task::ready(());
        self.structured_filter_task = Task::ready(());
        self.json_expand_task = Task::ready(());
        self.coordinator.external_generation = self.coordinator.external_generation.wrapping_add(1);
        #[cfg(not(test))]
        let recovery_dir = gmark_config::GmarkConfigDirs::from_system()
            .ok()
            .map(|dirs| dirs.recovery_dir());
        #[cfg(test)]
        let recovery_dir: Option<PathBuf> = None;
        // 保存期间主动结束行编辑并阻止新编辑，避免后台保存旧快照后覆盖用户在保存中
        // 继续输入的内容。大文件保存为流式任务，状态栏会明确显示 Saving…。
        self.active_edit = None;
        self.saving = true;
        self.error = None;
        cx.emit(DocumentHostEvent::StateChanged);
        self.coordinator.save.task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let save_result = if let Some(plan) = encoded_save {
                        if save_as {
                            document
                                .save_encoded_atomic_as_cancellable(&plan, &path, &cancellation)
                                .map(|_| ())
                        } else {
                            document
                                .save_encoded_atomic_cancellable(&plan, &path, &cancellation)
                                .map(|_| ())
                        }
                    } else {
                        document.save_atomic_cancellable(&path, &cancellation)
                    };
                    if let Err(error) = save_result {
                        return Err((document, prepared_on_error, map_persistence_error(error)));
                    }
                    // 保存后从最终磁盘内容重新建立干净基线，清除旧 undo/add buffer，并恢复结构视图。
                    let rebuild = (|| {
                        let original = FileSource::open(&path)?;
                        let mut probe = gmark_paged_document::probe_file(&path, probe_options)?;
                        // 当前会话不因保存后的大小变化热迁移；重新打开时才重新执行策略。
                        probe.strategy = open_strategy;
                        probe.force_safe_source = force_safe_source;
                        let original_for_session = original.clone();
                        let prepared = prepare_utf8_source(original, probe.encoding.clone())?;
                        let index = LineIndex::build_cancellable(prepared.source(), &cancellation)?;
                        let clean_document = build_document_session(
                            &probe,
                            &original_for_session,
                            prepared.source().clone(),
                            index.clone(),
                            true,
                        )?;
                        let recovery = recovery_dir.as_ref().map(|dir| {
                            DocumentRecoveryJournal::create(
                                dir,
                                &original_for_session,
                                probe.encoding.clone(),
                                &clean_document,
                            )
                        });
                        verify_saved_session_readback(&document, &clean_document, &cancellation)?;
                        let (structure_source, structure_index, structure_bytes) =
                            structure_input_for_session(
                                &clean_document,
                                &prepared,
                                &index,
                                &cancellation,
                            )?;
                        let structured = if derived_views_enabled(probe.strategy) {
                            build_structured_index(
                                &structure_source,
                                &structure_index,
                                probe.format.clone(),
                                &cancellation,
                                structure_bytes,
                            )
                        } else {
                            Ok(None)
                        };
                        Ok::<_, gmark_paged_document::PagedDocumentError>((
                            clean_document,
                            prepared,
                            index,
                            structured,
                            recovery,
                            probe,
                            path,
                        ))
                    })();
                    rebuild.map_err(|error| {
                        (document, prepared_on_error, map_persistence_error(error))
                    })
                })
                .await;
            let saved = result.is_ok();
            if let Some(started) = save_started {
                crate::perf::emit_document(
                    "document_save",
                    started,
                    usize::try_from(save_profile.len).ok(),
                    Some(saved),
                    &save_profile.format,
                    &save_plan,
                    Some(if save_as { "save_as" } else { "save" }),
                );
            }
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_identity(view, view.coordinator.save.generation) {
                    return;
                }
                view.coordinator.save.cancellation = None;
                view.saving = false;
                match result {
                    Ok((document, prepared, index, structured, recovery, probe, path)) => {
                        // 保存后的干净 PieceTree 是新的磁盘身份基线；即使 revision 从零重新
                        // 开始，也不能接受旧基线上的搜索、复制或派生 projection 结果。
                        view.document_epoch = view.document_epoch.wrapping_add(1);
                        view.cancel_selection_transfers();
                        if let Some(mut journal) = view.coordinator.recovery_journal.take()
                            && let Err(error) = journal.checkpoint(&document)
                        {
                            view.coordinator.recovery_error =
                                Some(localized_document_error(&error, cx));
                        }
                        view.install_document_session(document);
                        view.prepared_source = Some(prepared);
                        view.provisional_source = None;
                        view.index = Some(index);
                        view.invalidate_source_rows();
                        view.probe = probe;
                        view.scroll_handle
                            .0
                            .borrow()
                            .base_handle
                            .set_offset(save_scroll_offset);
                        view.invalidate_structured_runtime();
                        match structured {
                            Ok(structured) => {
                                view.structured_index = structured;
                                view.clear_structure_error();
                            }
                            Err(error) => {
                                view.structured_index = None;
                                view.set_structure_error(error, cx);
                            }
                        }
                        if let Some(recovery) = recovery {
                            match recovery {
                                Ok(journal) => {
                                    view.coordinator.recovery_journal = Some(journal);
                                    view.coordinator.recovery_error = None;
                                }
                                Err(error) => {
                                    view.coordinator.recovery_error =
                                        Some(localized_document_error(&error, cx))
                                }
                            }
                        }
                        view.active_edit = None;
                        set_document_dirty_state(
                            &mut view.document,
                            &mut view.pending_dirty,
                            false,
                        );
                        if save_as {
                            view.path = path.clone();
                            view.coordinator.pending_external_change = None;
                            view.coordinator.external_monitor_paused = false;
                            view.coordinator.external_status = None;
                            cx.emit(DocumentHostEvent::SavedAs(path));
                        }
                    }
                    Err((document, prepared, error)) => {
                        view.install_document_session(document);
                        view.prepared_source = prepared;
                        view.invalidate_source_rows();
                        view.scroll_handle
                            .0
                            .borrow()
                            .base_handle
                            .set_offset(save_scroll_offset);
                        view.error = Some(error.to_string().into());
                    }
                }
                cx.emit(DocumentHostEvent::StateChanged);
                cx.notify();
            });
            if saved {
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
}

pub(super) fn delimited_record_terminator(bytes: &[u8]) -> &'static str {
    if bytes.ends_with(b"\r\n") {
        "\r\n"
    } else if bytes.ends_with(b"\n") {
        "\n"
    } else if bytes.ends_with(b"\r") {
        "\r"
    } else {
        ""
    }
}

pub(super) fn transform_delimited_adapter(
    mut document: DocumentSession,
    delimiter: u8,
    edit: DelimitedEdit,
    cancellation: &SearchCancellation,
    progress: &AtomicU64,
) -> Result<DocumentSession, PagedDocumentError> {
    let resident_source =
        document.store.kind() == gmark_document_core::DocumentBackendKind::Resident;
    let (column, header) = match edit {
        DelimitedEdit::InsertColumn { before, header } => (before, Some(header)),
        DelimitedEdit::DeleteColumn { column } => (column, None),
        _ => {
            return Err(PagedDocumentError::InvalidTransaction(
                "column worker received a non-column edit".into(),
            ));
        }
    };
    let mut input = tempfile::NamedTempFile::new().map_err(|source| PagedDocumentError::Io {
        path: std::env::temp_dir(),
        source,
    })?;
    document.write_to_cancellable(input.as_file_mut(), cancellation)?;
    input
        .as_file_mut()
        .sync_all()
        .map_err(|source| PagedDocumentError::Io {
            path: input.path().to_path_buf(),
            source,
        })?;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_path(input.path())
        .map_err(|source| PagedDocumentError::Io {
            path: input.path().to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, source),
        })?;
    let bytes = FileSource::open(input.path())?;
    let source_len = bytes.identity()?.len;
    let mut output = tempfile::NamedTempFile::new().map_err(|source| PagedDocumentError::Io {
        path: std::env::temp_dir(),
        source,
    })?;
    let output_path = output.path().to_path_buf();
    let mut record = csv::ByteRecord::new();
    let mut physical = 0u64;
    loop {
        if physical.is_multiple_of(1_024) && cancellation.is_cancelled() {
            return Err(PagedDocumentError::Cancelled);
        }
        let start = reader.position().byte();
        if !reader
            .read_byte_record(&mut record)
            .map_err(|source| PagedDocumentError::Io {
                path: input.path().to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, source),
            })?
        {
            break;
        }
        let end = reader.position().byte();
        let raw_end = if end < source_len {
            (end + 1).min(source_len)
        } else {
            end
        };
        let raw = bytes.read_range(start, raw_end)?;
        let terminator = if resident_source {
            "\n"
        } else {
            delimited_record_terminator(&raw)
        };
        let mut fields = record
            .iter()
            .map(|field| String::from_utf8_lossy(field).into_owned())
            .collect::<Vec<_>>();
        if let Some(header) = &header {
            fields.insert(
                column.min(fields.len()),
                if physical == 0 {
                    header.clone()
                } else {
                    String::new()
                },
            );
        } else if column < fields.len() {
            fields.remove(column);
        }
        output
            .write_all(serialize_delimited_record(&fields, delimiter, terminator).as_bytes())
            .map_err(|source| PagedDocumentError::Io {
                path: output_path.clone(),
                source,
            })?;
        physical += 1;
        progress.store(physical, Ordering::Relaxed);
    }
    if physical == 0
        && let Some(header) = &header
    {
        output
            .write_all(
                serialize_delimited_record(std::slice::from_ref(header), delimiter, "").as_bytes(),
            )
            .map_err(|source| PagedDocumentError::Io {
                path: output_path.clone(),
                source,
            })?;
    }
    output
        .as_file_mut()
        .sync_all()
        .map_err(|source| PagedDocumentError::Io {
            path: output_path.clone(),
            source,
        })?;
    let output_reader = output.reopen().map_err(|source| PagedDocumentError::Io {
        path: output_path,
        source,
    })?;
    document.replace_text_reader(0..document.len(), output_reader)?;
    Ok(document)
}
