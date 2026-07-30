// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn apply_slash_command(
        &mut self,
        block: Entity<super::Block>,
        command: SlashCommand,
        trigger_range: std::ops::Range<usize>,
        cx: &mut Context<Self>,
    ) {
        let programmatic = trigger_range.is_empty();
        let (valid, original_kind) = {
            let block = block.read(cx);
            let kind = block.kind();
            let valid = (programmatic || block.supports_slash_commands())
                && block.selected_range.is_empty()
                && block.selected_range.end == trigger_range.end
                && (programmatic
                    || (trigger_range.start < trigger_range.end
                        && block
                            .display_text()
                            .get(trigger_range.start..trigger_range.start + 1)
                            == Some("/")));
            (valid, kind)
        };
        if !valid {
            return;
        }
        let Some(location) = self.document.find_block_location(block.entity_id()) else {
            return;
        };
        let sibling_count = location
            .parent
            .as_ref()
            .map(|parent| parent.read(cx).children.len())
            .unwrap_or_else(|| self.document.root_count());
        let command_view_mode = match self.view_mode {
            super::ViewMode::Rendered => EditingViewMode::Rendered,
            super::ViewMode::Source => EditingViewMode::Source,
            super::ViewMode::Split => EditingViewMode::Split,
            super::ViewMode::Preview => EditingViewMode::Preview,
        };
        let command_context = {
            let block = block.read(cx);
            let mut context = block.editing_command_context();
            context.view_mode = command_view_mode;
            context.sibling_index = location.index;
            context.sibling_count = sibling_count;
            context
        };
        if !command.is_available(command_context) {
            return;
        }

        if programmatic && command == SlashCommand::Table {
            // 左侧块操作是显式鼠标入口，表格需要先给出可见反馈并让用户确认尺寸；
            // 键入 `/表格` 的高效路径仍直接插入默认表格。
            self.open_table_insert_dialog_for_target(
                crate::editor::context_menu::TableInsertTarget::After(block.entity_id()),
                cx,
            );
            return;
        }

        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        let (cleaned_title, cursor) = {
            let block_ref = block.read(cx);
            let clean_range = block_ref.current_to_clean_range(trigger_range);
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
            (result.tree, cursor)
        };
        let query_only = cleaned_title.visible_text().trim().is_empty();

        match command.plan() {
            EditingCommandPlan::InsertResource => {
                self.finalize_pending_undo_capture(cx);
                self.prompt_and_insert_resource(
                    block,
                    location.parent.clone(),
                    location.index,
                    original_kind,
                    cleaned_title,
                    cursor,
                    query_only,
                    cx,
                );
                return;
            }
            EditingCommandPlan::ChangeBlockKind(kind) => {
                Self::set_block_title_and_kind(&block, kind, cleaned_title, cursor, cx);
                self.document.rebuild_metadata_and_snapshot(cx);
                self.focus_block(block.entity_id());
            }
            EditingCommandPlan::InsertFootnoteReference => {
                let id = Self::next_footnote_id(&self.document.markdown_text(cx));
                let reference = format!("[^{id}]");
                let result = cleaned_title.replace_visible_range(
                    cursor..cursor,
                    &reference,
                    InlineInsertionAttributes::default(),
                );
                let next_cursor = result.map_offset(cursor + reference.len());
                Self::set_block_title_and_kind(&block, original_kind, result.tree, next_cursor, cx);
                let definition = Self::new_footnote_definition_block(cx, id);
                self.document.insert_blocks_at(
                    None,
                    self.document.root_count(),
                    vec![definition],
                    cx,
                );
                self.document.rebuild_metadata_and_snapshot(cx);
                self.focus_block(block.entity_id());
            }
            EditingCommandPlan::InsertTable
            | EditingCommandPlan::InsertImage
            | EditingCommandPlan::InsertMath
            | EditingCommandPlan::InsertMermaid
            | EditingCommandPlan::InsertFootnoteDefinition
            | EditingCommandPlan::InsertHorizontalRule => {
                let inserted = match command.plan() {
                    EditingCommandPlan::InsertTable => {
                        Self::new_table_block(cx, TableData::new_empty(2, 2))
                    }
                    EditingCommandPlan::InsertImage => {
                        Self::new_block(cx, BlockRecord::paragraph("![]()"))
                    }
                    EditingCommandPlan::InsertMath => {
                        Self::new_block(cx, BlockRecord::math("$$\n\n$$"))
                    }
                    EditingCommandPlan::InsertMermaid => Self::new_block(
                        cx,
                        BlockRecord::mermaid("```mermaid\nflowchart TD\n    A --> B\n```"),
                    ),
                    EditingCommandPlan::InsertFootnoteDefinition => {
                        let id = Self::next_footnote_id(&self.document.markdown_text(cx));
                        Self::new_footnote_definition_block(cx, id)
                    }
                    EditingCommandPlan::InsertHorizontalRule => Self::new_block(
                        cx,
                        BlockRecord::new(
                            BlockKind::Separator,
                            InlineTextTree::plain(String::new()),
                        ),
                    ),
                    _ => unreachable!("insert command matched above"),
                };
                if query_only {
                    self.document.with_structure_mutation(cx, |document, cx| {
                        let _ = document.remove_block_by_id_raw(block.entity_id(), cx);
                        document.insert_blocks_at_raw(
                            location.parent.clone(),
                            location.index,
                            vec![inserted.clone()],
                            cx,
                        );
                    });
                } else {
                    Self::set_block_title_and_kind(
                        &block,
                        original_kind.clone(),
                        cleaned_title,
                        cursor,
                        cx,
                    );
                    self.document.insert_blocks_at(
                        location.parent.clone(),
                        location.index + 1,
                        vec![inserted.clone()],
                        cx,
                    );
                }
                self.ensure_trailing_paragraph_after_structural(&inserted, cx);
                self.rebuild_table_runtimes(cx);
                if command == SlashCommand::Table {
                    if let Some(first_cell) = inserted
                        .read(cx)
                        .table_runtime
                        .as_ref()
                        .and_then(|runtime| runtime.header.first())
                    {
                        self.focus_block(first_cell.entity_id());
                    }
                } else {
                    inserted.update(cx, |inserted, cx| {
                        let target = if command == SlashCommand::Image {
                            2
                        } else if command == SlashCommand::Math {
                            3
                        } else if command == SlashCommand::Mermaid {
                            "```mermaid\n".len()
                        } else {
                            0
                        };
                        inserted.assign_collapsed_selection_offset(
                            target.min(inserted.visible_len()),
                            CollapsedCaretAffinity::Default,
                            None,
                        );
                        cx.notify();
                    });
                    let focus = if command == SlashCommand::FootnoteDefinition {
                        inserted
                            .read(cx)
                            .children
                            .first()
                            .map(Entity::entity_id)
                            .unwrap_or_else(|| inserted.entity_id())
                    } else {
                        inserted.entity_id()
                    };
                    self.focus_block(focus);
                }
            }
            EditingCommandPlan::DuplicateBlock => {
                Self::set_block_title_and_kind(
                    &block,
                    original_kind.clone(),
                    cleaned_title,
                    cursor,
                    cx,
                );
                let duplicate = Self::clone_block_subtree(&block, cx);
                self.document.insert_blocks_at(
                    location.parent.clone(),
                    location.index + 1,
                    vec![duplicate.clone()],
                    cx,
                );
                self.focus_block(duplicate.entity_id());
            }
            EditingCommandPlan::MoveBlock(delta) => {
                Self::set_block_title_and_kind(&block, original_kind, cleaned_title, cursor, cx);
                let target = if delta < 0 {
                    location.index.checked_sub(1)
                } else {
                    (location.index + 1 < sibling_count).then_some(location.index + 1)
                };
                if let Some(target) = target {
                    self.document.with_structure_mutation(cx, |document, cx| {
                        let Some((removed, _)) =
                            document.remove_block_by_id_raw(block.entity_id(), cx)
                        else {
                            return;
                        };
                        document.insert_blocks_at_raw(
                            location.parent.clone(),
                            target,
                            vec![removed],
                            cx,
                        );
                    });
                }
                self.focus_block(block.entity_id());
            }
            EditingCommandPlan::DeleteBlock => {
                let visible = self.document.visible_blocks();
                let index = visible
                    .iter()
                    .position(|visible| visible.entity.entity_id() == block.entity_id())
                    .unwrap_or(0);
                let fallback = visible
                    .get(index + 1)
                    .or_else(|| index.checked_sub(1).and_then(|index| visible.get(index)))
                    .map(|visible| visible.entity.entity_id());
                self.document.with_structure_mutation(cx, |document, cx| {
                    let _ = document.remove_block_by_id_raw(block.entity_id(), cx);
                });
                let focus = if self.document.root_count() == 0 {
                    let paragraph = Self::new_block(cx, BlockRecord::paragraph(String::new()));
                    let id = paragraph.entity_id();
                    self.document.insert_blocks_at(None, 0, vec![paragraph], cx);
                    Some(id)
                } else {
                    fallback
                };
                if let Some(focus) = focus {
                    self.focus_block(focus);
                }
            }
            EditingCommandPlan::ApplyInline(_) => return,
        }
        self.rebuild_image_runtimes(cx);
        EditingCommandHistory::record(command, cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        self.request_active_block_scroll_into_view(cx);
        cx.notify();
    }
}
