// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn handle_paste_image_request(
        &mut self,
        block: Entity<super::Block>,
        leading: &InlineTextTree,
        source: &PastedImageSource,
        trailing: &InlineTextTree,
        cx: &mut Context<Self>,
    ) {
        let markdown = match self.pasted_image_markdown(source) {
            Ok(markdown) => markdown,
            Err(err) => {
                self.show_image_paste_error(err, cx);
                return;
            }
        };

        if self.replace_cross_block_selection_with_text(
            &markdown,
            None,
            false,
            crate::components::UndoCaptureKind::NonCoalescible,
            cx,
        ) {
            return;
        }

        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        let can_insert_image_block = self.view_mode == super::ViewMode::Rendered
            && block.read(cx).kind() == BlockKind::Paragraph
            && self.table_cell_binding(block.entity_id()).is_none()
            && !block.read(cx).uses_raw_text_editing();

        if can_insert_image_block {
            self.insert_image_block_after_paragraph(&block, leading, &markdown, trailing, cx);
        } else {
            self.replace_current_block_selection_with_image_text(
                &block, leading, &markdown, trailing, cx,
            );
        }

        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
    }

    pub(super) fn jump_to_footnote_definition(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if let Some(block) = self
            .footnote_registry
            .binding(id)
            .and_then(|binding| self.focusable_entity_by_id(binding.definition_entity_id))
        {
            self.focus_block_range(&block, 0..0, cx);
            return true;
        }
        let Some(y) = self
            .virtual_surface
            .as_ref()
            .and_then(|surface| surface.footnote_definition_y(id))
        else {
            return false;
        };
        self.pending_virtual_footnote_focus = Some(id.to_string());
        self.scroll_handle
            .set_offset(point(px(0.0), px(-y.max(0.0))));
        cx.notify();
        true
    }

    pub(super) fn jump_to_footnote_backref(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if let Some((block, range)) = self.footnote_registry.binding(id).and_then(|binding| {
            let first_reference = binding.first_reference.as_ref()?;
            let block = self.focusable_entity_by_id(first_reference.entity_id)?;
            let range = block
                .read(cx)
                .current_range_for_footnote_occurrence(first_reference.occurrence_index)
                .unwrap_or(0..0);
            Some((block, range))
        }) {
            self.focus_block_range(&block, range, cx);
            return true;
        }
        let Some(y) = self
            .virtual_surface
            .as_ref()
            .and_then(|surface| surface.footnote_first_reference_y(id))
        else {
            return false;
        };
        self.pending_virtual_footnote_backref_focus = Some(id.to_string());
        self.scroll_handle
            .set_offset(point(px(0.0), px(-y.max(0.0))));
        cx.notify();
        true
    }

    pub(super) fn apply_slash_command(
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
            EditingCommandPlan::ChangeBlockKind(kind) => {
                Self::set_block_title_and_kind(&block, kind, cleaned_title, cursor, cx);
                self.document.rebuild_metadata_and_snapshot(cx);
                self.focus_block(block.entity_id());
            }
            EditingCommandPlan::InsertTable
            | EditingCommandPlan::InsertImage
            | EditingCommandPlan::InsertMath
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
                    self.focus_block(inserted.entity_id());
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

    /// 复制块时递归生成新的稳定身份；运行时 parent/content 会由 DocumentTree 重建。
    pub(super) fn clone_block_subtree(
        block: &Entity<super::Block>,
        cx: &mut Context<Self>,
    ) -> Entity<super::Block> {
        let (mut record, children) = {
            let block = block.read(cx);
            (block.record.clone(), block.children.clone())
        };
        record.id = uuid::Uuid::new_v4();
        record.parent = None;
        record.content.clear();
        let duplicate = Self::new_block(cx, record);
        let cloned_children = children
            .iter()
            .map(|child| Self::clone_block_subtree(child, cx))
            .collect::<Vec<_>>();
        duplicate.update(cx, |duplicate, _cx| duplicate.children = cloned_children);
        duplicate
    }

    /// Applies a menu-selected block kind while preserving the current source title and one undo
    /// boundary. Structural commands deliberately share the editor transaction path rather than
    /// mutating a `BlockRecord` from the menu layer.
    pub(crate) fn set_active_block_kind(&mut self, kind: BlockKind, cx: &mut Context<Self>) {
        let Some(block) = self.current_edit_target_from_state(cx) else {
            return;
        };
        self.set_block_kind_for(block, kind, cx);
    }

    pub(super) fn set_block_kind_for(
        &mut self,
        block: Entity<super::Block>,
        kind: BlockKind,
        cx: &mut Context<Self>,
    ) {
        if block.read(cx).is_read_only() || block.read(cx).kind() == kind {
            return;
        }
        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        block.update(cx, |block, cx| {
            block.record.kind = kind;
            block.sync_render_cache();
            cx.notify();
        });
        self.document.rebuild_metadata_and_snapshot(cx);
        self.sync_runtime_context_after_block_edit(&block, cx);
        self.mark_block_dirty(block.entity_id(), cx);
        self.request_active_block_scroll_into_view(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
    }

    pub(super) fn insert_list_group_separator_before(
        &mut self,
        entity_id: EntityId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(location) = self.document.find_block_location(entity_id) else {
            return false;
        };

        let separator = Self::new_block(cx, BlockRecord::paragraph(String::new()));
        self.document
            .insert_blocks_at(location.parent, location.index, vec![separator], cx);
        true
    }

    pub(super) fn set_block_title_and_kind(
        block: &Entity<super::Block>,
        kind: BlockKind,
        title: InlineTextTree,
        cursor: usize,
        cx: &mut Context<Self>,
    ) {
        let (kind, title, cursor) = Self::apply_paragraph_shortcuts(kind, title, cursor);
        block.update(cx, move |block, cx| {
            block.record.kind = kind;
            block.record.set_title(title.clone());
            block.sync_edit_mode_from_kind();
            block.sync_render_cache();
            let clean_cursor = cursor.min(block.record.title.visible_len());
            block.selected_range = block.clean_to_current_range(clean_cursor..clean_cursor);
            block.selection_reversed = false;
            block.marked_range = None;
            block.vertical_motion_x = None;
            block.cursor_blink_epoch = Instant::now();
            cx.notify();
        });
    }

    /// A block that a setext underline below it can promote into a heading: a
    /// non-empty, single-line, plain paragraph with no children.
    pub(super) fn is_setext_heading_target(block: &Entity<super::Block>, cx: &App) -> bool {
        let block = block.read(cx);
        if block.kind() != BlockKind::Paragraph || !block.children.is_empty() {
            return false;
        }
        let text = block.record.title.visible_text();
        !text.trim().is_empty() && !text.contains('\n')
    }

    /// Handles Enter pressed on a paragraph that is a pure setext underline.
    /// When a matching paragraph precedes it at the root, the two collapse into
    /// a heading; a lone dash run still falls back to a thematic break. Returns
    /// true when it consumed the newline.
    pub(super) fn try_form_setext_heading_on_newline(
        &mut self,
        block: &Entity<super::Block>,
        cx: &mut Context<Self>,
    ) -> bool {
        let text = block.read(cx).display_text().to_string();
        let Some(level) = BlockKind::parse_setext_underline(&text) else {
            return false;
        };
        if block.read(cx).kind() != BlockKind::Paragraph {
            return false;
        }
        let Some(location) = self.document.find_block_location(block.entity_id()) else {
            return false;
        };

        // Only root paragraphs auto-form headings; nested contexts (quotes,
        // lists) keep their existing newline behavior.
        let target = if location.parent.is_none() {
            self.document
                .previous_sibling(block.entity_id(), cx)
                .filter(|prev| Self::is_setext_heading_target(prev, cx))
        } else {
            None
        };

        // A `=` underline with no heading target is ordinary text: defer to the
        // normal newline split. A dash run still has to become a separator.
        if target.is_none() && !BlockKind::parse_separator_line(&text) {
            return false;
        }

        // The newline's own capture was already finalized by the block's Changed
        // event (nothing had changed yet), so start a fresh one here that spans
        // the heading/separator conversion. prepare is a no-op if one is pending.
        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);

        if let Some(prev) = target {
            let heading_title = prev.read(cx).record.title.clone();
            let cursor = heading_title.visible_len();
            let removed_id = block.entity_id();
            let new_paragraph = Self::new_block(cx, BlockRecord::paragraph(String::new()));

            Self::set_block_title_and_kind(
                &prev,
                BlockKind::Heading { level },
                heading_title,
                cursor,
                cx,
            );
            self.document.with_structure_mutation(cx, |document, cx| {
                let _ = document.remove_block_by_id_raw(removed_id, cx);
            });
            if let Some(heading_location) = self.document.find_block_location(prev.entity_id()) {
                self.document.insert_blocks_at(
                    heading_location.parent,
                    heading_location.index + 1,
                    vec![new_paragraph.clone()],
                    cx,
                );
            }
            self.focus_block(new_paragraph.entity_id());
        } else {
            block.update(cx, |block, _cx| block.make_separator());
            let new_paragraph = Self::new_block(cx, BlockRecord::paragraph(String::new()));
            self.document.insert_blocks_at(
                location.parent,
                location.index + 1,
                vec![new_paragraph.clone()],
                cx,
            );
            self.focus_block(new_paragraph.entity_id());
        }

        self.rebuild_image_runtimes(cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
        true
    }

    /// Handles Enter pressed on a paragraph that is a pipe-table row. A
    /// delimiter row under a header paragraph forms a native table; a body row
    /// directly under an existing table is absorbed into it. After either, the
    /// caret lands in a fresh paragraph below the table so consecutive rows can
    /// be typed. Returns true when it consumed the newline.
    pub(super) fn try_form_or_extend_table_on_newline(
        &mut self,
        block: &Entity<super::Block>,
        cx: &mut Context<Self>,
    ) -> bool {
        let text = block.read(cx).display_text().to_string();
        if block.read(cx).kind() != BlockKind::Paragraph || !is_table_row_candidate(&text) {
            return false;
        }
        let Some(location) = self.document.find_block_location(block.entity_id()) else {
            return false;
        };
        if location.parent.is_some() {
            return false;
        }
        let Some(prev) = self.document.previous_sibling(block.entity_id(), cx) else {
            return false;
        };

        if prev.read(cx).kind() == BlockKind::Table {
            // A multi-column row typed directly under a table is meant as a row,
            // so absorb it and let the table normalize ragged cell counts the
            // same way pasted rows are padded or truncated to the header width.
            return self.extend_table_with_typed_row(&prev, block, &text, cx);
        }

        if prev.read(cx).kind() != BlockKind::Paragraph {
            return false;
        }
        let header_text = prev.read(cx).display_text().to_string();
        if !is_table_row_candidate(&header_text) {
            return false;
        }
        let Some(table) = parse_root_table_region(&[header_text, text]) else {
            return false;
        };

        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        // Remove the lower (delimiter) block first so the header index is stable.
        let header_index = location.index - 1;
        let removed_delimiter = block.entity_id();
        let removed_header = prev.entity_id();
        let table_block = Self::new_table_block(cx, table);
        let new_paragraph = Self::new_block(cx, BlockRecord::paragraph(String::new()));
        self.document.with_structure_mutation(cx, |document, cx| {
            let _ = document.remove_block_by_id_raw(removed_delimiter, cx);
            let _ = document.remove_block_by_id_raw(removed_header, cx);
        });
        self.document.insert_blocks_at(
            None,
            header_index,
            vec![table_block.clone(), new_paragraph.clone()],
            cx,
        );
        self.rebuild_table_runtimes(cx);
        self.focus_block(new_paragraph.entity_id());
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
        true
    }

    pub(super) fn extend_table_with_typed_row(
        &mut self,
        table_block: &Entity<super::Block>,
        row_block: &Entity<super::Block>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        // Capture any in-progress cell edits before mutating the record.
        self.sync_table_record_from_runtime(table_block, cx);
        let Some(mut table) = table_block.read(cx).record.table.clone() else {
            return false;
        };
        let Some(row) = parse_table_body_row(text, table.column_count()) else {
            return false;
        };

        self.prepare_undo_capture(crate::components::UndoCaptureKind::NonCoalescible, cx);
        table.rows.push(row);
        table_block.update(cx, |block, cx| {
            block.record.table = Some(table);
            cx.notify();
        });

        let removed_id = row_block.entity_id();
        self.document.with_structure_mutation(cx, |document, cx| {
            let _ = document.remove_block_by_id_raw(removed_id, cx);
        });
        let new_paragraph = Self::new_block(cx, BlockRecord::paragraph(String::new()));
        if let Some(table_location) = self.document.find_block_location(table_block.entity_id()) {
            self.document.insert_blocks_at(
                table_location.parent,
                table_location.index + 1,
                vec![new_paragraph.clone()],
                cx,
            );
        }
        self.rebuild_table_runtimes(cx);
        self.focus_block(new_paragraph.entity_id());
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
        true
    }

    /// Inserts an empty paragraph after `block` when it renders as a
    /// self-contained structure the caret cannot move past (table, code, math,
    /// separator, quote, callout, footnote definition, standalone image, ...)
    /// and nothing currently follows it in its container. This keeps a rendered
    /// document from ending on such a block, so a rendered-first user can keep
    /// typing past it rather than being stranded. No-op when something already
    /// follows the block or it is not a stranding structure.
    pub(in crate::editor) fn ensure_trailing_paragraph_after_structural(
        &mut self,
        block: &Entity<super::Block>,
        cx: &mut Context<Self>,
    ) {
        let strands = {
            let block = block.read(cx);
            let kind = block.kind();
            kind.is_atomic_structural()
                || kind.is_quote_container()
                || kind.is_footnote_definition()
                || block.renders_as_standalone_image()
        };
        if !strands {
            return;
        }
        let Some(location) = self.document.find_block_location(block.entity_id()) else {
            return;
        };
        let sibling_count = match location.parent.as_ref() {
            Some(parent) => parent.read(cx).children.len(),
            None => self.document.root_count(),
        };
        if location.index + 1 < sibling_count {
            return;
        }
        let trailing = Self::new_block(cx, BlockRecord::paragraph(String::new()));
        self.document
            .insert_blocks_at(location.parent, location.index + 1, vec![trailing], cx);
    }

    pub(super) fn apply_paragraph_shortcuts(
        kind: BlockKind,
        mut title: InlineTextTree,
        cursor: usize,
    ) -> (BlockKind, InlineTextTree, usize) {
        if kind == BlockKind::Paragraph {
            let visible_text = title.visible_text();
            if let Some((detected_kind, prefix_len)) =
                BlockKind::detect_markdown_shortcut(&visible_text)
            {
                title.remove_visible_prefix(prefix_len);
                return (detected_kind, title, cursor.saturating_sub(prefix_len));
            }
        }

        (kind, title, cursor)
    }

    pub(crate) fn bump_scrollbar_visibility(&mut self, cx: &mut Context<Self>) {
        let duration = Duration::from_millis(900);
        self.scrollbar_visible_until = Instant::now() + duration;

        let weak_editor = cx.entity().downgrade();
        self.scrollbar_fade_task = Some(cx.spawn(
            async move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
                cx.background_executor()
                    .timer(duration + Duration::from_millis(50))
                    .await;
                let _ = weak_editor.update(cx, |this, cx| {
                    this.scrollbar_fade_task = None;
                    cx.notify();
                });
            },
        ));

        cx.notify();
    }

    pub(crate) fn on_editor_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scrollbar_hovered = *hovered;
        if *hovered {
            self.bump_scrollbar_visibility(cx);
        } else {
            cx.notify();
        }
    }
}
