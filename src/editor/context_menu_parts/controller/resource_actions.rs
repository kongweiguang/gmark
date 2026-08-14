// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn replace_resource_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.resource_context_block(cx) else {
            return;
        };
        let entity_id = block.entity_id();
        let Some(previous) = self.resource_context_record(cx) else {
            return;
        };
        self.close_context_menu(cx);
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Replace resource".into()),
        });
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let Ok(Ok(Some(paths))) = prompt.await else {
                return;
            };
            let Some(source) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, move |editor, cx| {
                let behavior = crate::preferences::read_app_preferences()
                    .map(|preferences| preferences.resource_insert_behavior())
                    .unwrap_or(crate::preferences::ImagePasteBehavior::None);
                if editor.file_path.is_none()
                    && behavior != crate::preferences::ImagePasteBehavior::None
                {
                    editor.pending_resource_insertion =
                        Some(crate::editor::PendingResourceInsertion::Replace {
                            entity_id,
                            previous,
                            source,
                        });
                    editor.request_save_document_as(cx);
                    return;
                }
                editor.complete_resource_replacement(entity_id, previous, source, cx);
            });
        })
        .detach();
    }

    pub(crate) fn complete_resource_replacement(
        &mut self,
        entity_id: EntityId,
        previous: ResourceRecord,
        source: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        let behavior = crate::preferences::read_app_preferences()
            .map(|preferences| preferences.resource_insert_behavior())
            .unwrap_or(crate::preferences::ImagePasteBehavior::None);
        self.schedule_resource_replacement(entity_id, previous, source, behavior, cx);
    }

    /// 在资源替换提交前把文件系统复制和路径解析移到后台，并以文档 epoch/revision
    /// 丢弃迟到结果；清理 guard 只拥有本次任务创建的副本，旧资源永远不会被删除。
    fn schedule_resource_replacement(
        &mut self,
        entity_id: EntityId,
        previous: ResourceRecord,
        source: std::path::PathBuf,
        behavior: crate::preferences::ResourceInsertBehavior,
        cx: &mut Context<Self>,
    ) {
        let document_path = self.file_path.clone();
        let expected_epoch = self.document_epoch;
        let expected_revision = self.source_document.revision();
        let expected_tab_id = self.tabs.active_id();
        let label = previous.label;
        let explicit_kind = previous.explicit_kind;
        let weak_editor = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_spawn(async move {
                    Self::materialize_resource_with_limits(
                        &label,
                        &source,
                        document_path.as_deref(),
                        behavior,
                        explicit_kind,
                    )
                })
                .await;
            match result {
                Err(error) => {
                    let _ = weak_editor.update(cx, |editor, cx| {
                        if crate::editor::file_drop::resource_materialization_is_current_for_tab(
                            expected_epoch,
                            expected_revision,
                            expected_tab_id,
                            editor.document_epoch,
                            editor.source_document.revision(),
                            editor.tabs.active_id(),
                        ) {
                            editor.show_image_paste_error(error, cx);
                        }
                    });
                }
                Ok((markdown, materialized)) => {
                    // 先把 guard 放进 closure 的捕获环境；WeakEntity 未执行回调时，closure
                    // 仍会被销毁并回收新副本，避免实体消失留下孤立文件。
                    let mut cleanup =
                        crate::editor::file_drop::ResourceCleanupGuard::new(materialized);
                    let _ = weak_editor.update(cx, move |editor, cx| {
                        if !crate::editor::file_drop::resource_materialization_is_current_for_tab(
                            expected_epoch,
                            expected_revision,
                            expected_tab_id,
                            editor.document_epoch,
                            editor.source_document.revision(),
                            editor.tabs.active_id(),
                        ) {
                            return;
                        }
                        let Some(block) = editor.focusable_entity_by_id(entity_id) else {
                            return;
                        };
                        editor.prepare_undo_capture(
                            crate::components::UndoCaptureKind::NonCoalescible,
                            cx,
                        );
                        let title = InlineTextTree::from_markdown(&markdown);
                        let cursor = title.visible_len();
                        Self::set_block_title_and_kind(
                            &block,
                            block.read(cx).kind(),
                            title,
                            cursor,
                            cx,
                        );
                        editor.rebuild_image_runtimes(cx);
                        editor.mark_dirty(cx);
                        editor.finalize_pending_undo_capture(cx);
                        editor.focus_block(entity_id);
                        cleanup.disarm();
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    pub(in crate::editor) fn copy_resource_address_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(destination) = self
            .resource_context_record(cx)
            .map(|resource| resource.destination)
        else {
            return;
        };
        self.close_context_menu(cx);
        cx.write_to_clipboard(ClipboardItem::new_string(destination));
    }

    pub(in crate::editor) fn convert_resource_to_link_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.resource_context_block(cx) else {
            return;
        };
        let Some(record) = self.resource_context_record(cx) else {
            return;
        };
        self.close_context_menu(cx);
        let markdown = plain_link_markdown(&record);
        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        let title = InlineTextTree::from_markdown(&markdown);
        let cursor = title.visible_len();
        Self::set_block_title_and_kind(&block, block.read(cx).kind(), title, cursor, cx);
        self.rebuild_image_runtimes(cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        self.focus_block(block.entity_id());
        cx.notify();
    }

    pub(in crate::editor) fn delete_resource_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.resource_context_block(cx) else {
            return;
        };
        self.close_context_menu(cx);
        block.update(cx, |_, cx| {
            cx.emit(crate::components::BlockEvent::RequestDelete)
        });
    }

    pub(in crate::editor) fn relocate_resource_from_context_menu(
        &mut self,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.resource_context_block(cx) else {
            return;
        };
        let entity_id = block.entity_id();
        let Some(previous) = self.resource_context_record(cx) else {
            return;
        };
        self.close_context_menu(cx);
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Relocate resource".into()),
        });
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let Ok(Ok(Some(paths))) = prompt.await else {
                return;
            };
            let Some(source) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, move |editor, cx| {
                editor.schedule_resource_replacement(
                    entity_id,
                    previous,
                    source,
                    crate::preferences::ImagePasteBehavior::None,
                    cx,
                );
            });
        })
        .detach();
    }
}

fn plain_link_markdown(record: &ResourceRecord) -> String {
    let marked = record.to_markdown();
    marked
        .rfind(" \"gmark:resource")
        .map(|index| format!("{}{}", &marked[..index], ')'))
        .unwrap_or(marked)
}
