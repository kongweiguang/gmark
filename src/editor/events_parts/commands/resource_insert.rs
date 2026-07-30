// @author kongweiguang

use super::*;
use crate::editor::PendingResourceInsertion;

impl Editor {
    pub(crate) fn prompt_and_insert_resource(
        &mut self,
        block: Entity<super::Block>,
        parent: Option<Entity<super::Block>>,
        index: usize,
        original_kind: BlockKind,
        cleaned_title: InlineTextTree,
        cursor: usize,
        query_only: bool,
        cx: &mut Context<Self>,
    ) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Insert resource".into()),
        });
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            if let Ok(Ok(Some(paths))) = prompt.await {
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
                            Some(PendingResourceInsertion::Prompted {
                                block,
                                parent,
                                index,
                                original_kind,
                                cleaned_title,
                                cursor,
                                query_only,
                                source,
                            });
                        editor.request_save_document_as(cx);
                        return;
                    }
                    editor.complete_prompted_resource_insert(
                        block,
                        parent,
                        index,
                        original_kind,
                        cleaned_title,
                        cursor,
                        query_only,
                        source,
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    pub(crate) fn on_insert_resource_action(
        &mut self,
        _: &crate::components::InsertResource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self
            .focused_edit_target(window, cx)
            .or_else(|| self.current_edit_target_from_state(cx))
        else {
            return;
        };
        let Some(location) = self.document.find_block_location(block.entity_id()) else {
            return;
        };
        let (original_kind, cleaned_title, cursor, query_only) = {
            let block_ref = block.read(cx);
            if !crate::components::EditingCommandId::Resource
                .is_available(block_ref.editing_command_context())
            {
                return;
            }
            let clean_range = block_ref.current_to_clean_range(block_ref.selected_range.clone());
            let result = block_ref
                .record
                .title
                .replace_visible_range_with_link_references(
                    clean_range.clone(),
                    "",
                    InlineInsertionAttributes::default(),
                    &self.link_reference_definitions,
                );
            let cursor = result.map_offset(clean_range.start);
            let query_only = result.tree.visible_text().trim().is_empty();
            (block_ref.kind(), result.tree, cursor, query_only)
        };
        self.prompt_and_insert_resource(
            block,
            location.parent,
            location.index,
            original_kind,
            cleaned_title,
            cursor,
            query_only,
            cx,
        );
    }

    /// Materializes the picked file and commits its Markdown insertion as one
    /// undo operation. The adapter copy is cleaned whenever parsing or the
    /// structural insertion cannot complete, so a failed UI action is atomic.
    fn complete_prompted_resource_insert(
        &mut self,
        block: Entity<super::Block>,
        parent: Option<Entity<super::Block>>,
        index: usize,
        original_kind: BlockKind,
        cleaned_title: InlineTextTree,
        cursor: usize,
        query_only: bool,
        source: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let behavior = crate::preferences::read_app_preferences()
            .map(|preferences| preferences.resource_insert_behavior())
            .unwrap_or(crate::preferences::ImagePasteBehavior::None);
        let document_path = self.file_path.clone();
        let (markdown, materialized) = match crate::resource_io::resource_markdown_for_path(
            "",
            &source,
            document_path.as_deref(),
            behavior,
            None,
        ) {
            Ok(result) => result,
            Err(error) => {
                self.show_image_paste_error(error, cx);
                return;
            }
        };

        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        let focus_id = if query_only {
            let inserted = if markdown.starts_with("![") {
                Self::new_block(cx, BlockRecord::paragraph(markdown.clone()))
            } else {
                let base_dir = document_path.as_deref().and_then(std::path::Path::parent);
                let Some(resource) = crate::components::ResourceRecord::parse(&markdown, base_dir)
                else {
                    materialized.cleanup_if_created();
                    self.finalize_pending_undo_capture(cx);
                    return;
                };
                Self::new_block(cx, BlockRecord::resource(resource))
            };
            let focus_id = inserted.entity_id();
            self.document.with_structure_mutation(cx, |document, cx| {
                let _ = document.remove_block_by_id_raw(block.entity_id(), cx);
                document.insert_blocks_at_raw(parent, index, vec![inserted], cx);
            });
            focus_id
        } else {
            let result = cleaned_title.replace_visible_range(
                cursor..cursor,
                &markdown,
                InlineInsertionAttributes::default(),
            );
            let next_cursor = result.map_offset(cursor + markdown.len());
            Self::set_block_title_and_kind(&block, original_kind, result.tree, next_cursor, cx);
            block.entity_id()
        };
        self.rebuild_image_runtimes(cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        self.focus_block(focus_id);
        cx.notify();
    }

    pub(crate) fn resume_pending_resource_insertion(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_resource_insertion.take() else {
            return;
        };
        match pending {
            PendingResourceInsertion::Prompted {
                block,
                parent,
                index,
                original_kind,
                cleaned_title,
                cursor,
                query_only,
                source,
            } => self.complete_prompted_resource_insert(
                block,
                parent,
                index,
                original_kind,
                cleaned_title,
                cursor,
                query_only,
                source,
                cx,
            ),
            PendingResourceInsertion::Pasted {
                block,
                leading,
                trailing,
                source,
            } => self.complete_paste_image_request(
                block,
                &leading,
                &PastedImageSource::LocalResource(source),
                &trailing,
                cx,
            ),
            PendingResourceInsertion::Replace {
                entity_id,
                previous,
                source,
            } => self.complete_resource_replacement(entity_id, previous, source, cx),
        }
    }

    pub(crate) fn abort_pending_resource_insertion(&mut self) {
        self.pending_resource_insertion = None;
    }
}
