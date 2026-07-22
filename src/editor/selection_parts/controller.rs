// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn apply_virtual_cross_block_inline_targets(
        &mut self,
        selection: NormalizedCrossBlockSelection,
        targets: &[CrossBlockInlineTarget],
        cx: &mut Context<Self>,
    ) -> bool {
        let mut changed = targets
            .iter()
            .filter(|target| {
                self.source_document
                    .snapshot()
                    .text_for_range(target.source_content_range.clone())
                    .is_ok_and(|source| source != target.replacement)
            })
            .collect::<Vec<_>>();
        if changed.is_empty() {
            return false;
        }
        changed.sort_by_key(|target| target.source_content_range.start);
        let union_start = changed[0].source_content_range.start;
        let union_end = changed
            .last()
            .map(|target| target.source_content_range.end)
            .unwrap_or(union_start);
        let snapshot = self.source_document.snapshot();
        let Ok(mut replacement) = snapshot.text_for_range(union_start..union_end) else {
            return false;
        };
        for target in changed.iter().rev() {
            let start = target.source_content_range.start - union_start;
            let end = target.source_content_range.end - union_start;
            replacement.replace_range(start..end, &target.replacement);
        }

        let endpoint_after_edit = |endpoint: CrossBlockSelectionEndpoint| -> Option<usize> {
            let target = targets
                .iter()
                .find(|target| target.entity.entity_id() == endpoint.entity_id)?;
            let clean_offset = target
                .entity
                .read(cx)
                .current_to_clean_offset(endpoint.offset);
            let delta_before = changed
                .iter()
                .filter(|changed| {
                    changed.source_content_range.end <= target.source_content_range.start
                })
                .map(|changed| {
                    changed.replacement.len() as isize - changed.source_content_range.len() as isize
                })
                .sum::<isize>();
            let content_start = target
                .source_content_range
                .start
                .checked_add_signed(delta_before)?;
            Some(
                content_start
                    + target
                        .next_title
                        .markdown_offset_map()
                        .visible_to_markdown_offset(clean_offset),
            )
        };
        let Some(start_source) = endpoint_after_edit(selection.start) else {
            return false;
        };
        let Some(end_source) = endpoint_after_edit(selection.end) else {
            return false;
        };
        if !self.apply_virtual_cross_block_source_edit(union_start..union_end, &replacement, cx) {
            return false;
        }
        let next_mappings = self.build_source_target_mappings(cx);
        let Some(start) = self.endpoint_for_source_offset(start_source, &next_mappings, cx) else {
            return false;
        };
        let Some(end) = self.endpoint_for_source_offset(end_source, &next_mappings, cx) else {
            return false;
        };
        let (anchor, focus) = if selection.reversed {
            (end, start)
        } else {
            (start, end)
        };
        self.cross_block_selection = Some(CrossBlockSelection { anchor, focus });
        self.focus_block(focus.entity_id);
        self.sync_cross_block_selection_visuals(cx);
        true
    }

    fn source_mapping_by_entity_id(&self, cx: &App) -> HashMap<EntityId, SourceTargetMapping> {
        self.build_source_target_mappings(cx)
            .into_iter()
            .map(|mapping| (mapping.entity.entity_id(), mapping))
            .collect()
    }

    fn endpoint_source_offset(
        &self,
        endpoint: CrossBlockSelectionEndpoint,
        mappings: &HashMap<EntityId, SourceTargetMapping>,
        cx: &App,
    ) -> Option<usize> {
        let mapping = mappings.get(&endpoint.entity_id)?;
        let block = mapping.entity.read(cx);
        let visible_len = block.visible_len();
        if endpoint.offset == 0 {
            return Some(mapping.full_source_range.start);
        }
        if endpoint.offset >= visible_len {
            return Some(mapping.full_source_range.end);
        }
        let markdown_offset = block
            .current_range_to_markdown_range(endpoint.offset..endpoint.offset)
            .start;
        let max_content = mapping.content_to_source.len().saturating_sub(1);
        Some(
            mapping.full_source_range.start
                + mapping.content_to_source[markdown_offset.min(max_content)],
        )
    }

    pub(super) fn endpoint_for_source_offset(
        &self,
        offset: usize,
        mappings: &[SourceTargetMapping],
        cx: &App,
    ) -> Option<CrossBlockSelectionEndpoint> {
        let mapping = mappings.iter().min_by_key(|mapping| {
            Self::source_offset_distance(&mapping.full_source_range, offset)
        })?;
        let local = if offset <= mapping.full_source_range.start {
            0
        } else if offset >= mapping.full_source_range.end {
            mapping.full_source_range.len()
        } else {
            offset - mapping.full_source_range.start
        };
        let content_offset =
            mapping.source_to_content[local.min(mapping.source_to_content.len().saturating_sub(1))];
        let block = mapping.entity.read(cx);
        Some(CrossBlockSelectionEndpoint {
            entity_id: mapping.entity.entity_id(),
            offset: block.markdown_offset_to_current_offset(content_offset),
        })
    }

    pub(super) fn cross_block_source_range_for_normalized(
        &self,
        selection: NormalizedCrossBlockSelection,
        cx: &App,
    ) -> Option<Range<usize>> {
        let (mapping_list, block_ranges) = self.build_source_target_mappings_with_block_ranges(cx);
        let mappings: HashMap<EntityId, SourceTargetMapping> = mapping_list
            .into_iter()
            .map(|mapping| (mapping.entity.entity_id(), mapping))
            .collect();
        let visible = self.document.visible_blocks();

        // Resolve an endpoint to a source offset. Atomic blocks (tables, etc.)
        // carry no per-block text mapping, so endpoint_source_offset returns
        // None for them; fall back to the block's own source span, picking the
        // side that keeps the block inside the selection.
        let endpoint_offset =
            |endpoint: CrossBlockSelectionEndpoint, index: usize, at_end: bool| -> Option<usize> {
                if let Some(offset) = self.endpoint_source_offset(endpoint, &mappings, cx) {
                    return Some(offset);
                }
                let entity = visible.get(index)?.entity.clone();
                let range = block_ranges.get(&entity.entity_id())?;
                Some(if at_end { range.end } else { range.start })
            };

        let start = endpoint_offset(selection.start, selection.start_index, false)?;
        let end = endpoint_offset(selection.end, selection.end_index, true)?;
        let (mut lo, mut hi) = (start.min(end), start.max(end));

        // Endpoint offsets can never point *after* a zero-visible-len (atomic)
        // block, so a table at the trailing boundary of the selection would be
        // left behind. Union in the full source range of every atomic block
        // whose visible index falls inside the selection so it is removed whole.
        for index in selection.start_index..=selection.end_index {
            let entity = visible.get(index)?.entity.clone();
            if entity.read(cx).visible_len() == 0 {
                if let Some(range) = block_ranges.get(&entity.entity_id()) {
                    lo = lo.min(range.start);
                    hi = hi.max(range.end);
                }
            }
        }
        Some(lo..hi)
    }

    fn rebuild_after_cross_block_source_edit(&mut self, source: String, cx: &mut Context<Self>) {
        self.sync_source_document_from_projection(&source);
        match self.view_mode {
            ViewMode::Rendered => {
                self.rebuild_primary_projection_from_source(cx);
            }
            ViewMode::Source | ViewMode::Split => {
                let block = Self::new_block(
                    cx,
                    crate::components::BlockRecord::paragraph(source.clone()),
                );
                block.update(cx, |block, _cx| block.set_source_document_mode());
                self.document.replace_roots(vec![block], cx);
                self.table_cells.clear();
            }
            ViewMode::Preview => {}
        }
    }

    /// 跨块编辑可能同时覆盖多个虚拟区域，必须直接提交到 Rope 真值。
    ///
    /// 这里不能复用 `mark_dirty`：它只序列化活动 Entity 所属的单一区域，
    /// 在跨区域删除后会把残留的 mounted Entity 再次写回源码。
    fn apply_virtual_cross_block_source_edit(
        &mut self,
        source_range: Range<usize>,
        replacement: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.virtual_surface.is_none() || self.view_mode != ViewMode::Rendered {
            return false;
        }

        let snapshot = self.source_document.snapshot();
        let old_fragment = match snapshot.text_for_range(source_range.clone()) {
            Ok(fragment) => fragment,
            Err(error) => {
                eprintln!("virtual surface 跨块编辑范围无效: {error}");
                return false;
            }
        };
        let old_revision = snapshot.revision();
        let transaction = gmark_document::Transaction::new(
            old_revision,
            vec![gmark_document::TextEdit::new(
                source_range,
                replacement.to_owned(),
            )],
        );
        let updated = match self.source_document.apply_transaction(transaction) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                eprintln!("virtual surface 跨块源码事务提交失败: {error}");
                return false;
            }
        };

        self.status_bar.apply_virtual_text_edit(
            old_revision,
            updated.revision(),
            &old_fragment,
            replacement,
        );

        // 取消旧 revision 的后台发布；跨区域结构变化必须用新边界重新物化。
        self.projection_cache_task = None;
        self.projection_cache_scheduled_revision = None;
        let prepared = Arc::new(if let Some(previous) = self.projection_cache.as_deref() {
            super::PreparedSplitProjection::from_snapshot_incremental_regions_only(
                updated, previous,
            )
        } else {
            super::PreparedSplitProjection::from_snapshot_adaptive(
                updated,
                Self::VIRTUAL_SURFACE_REGION_THRESHOLD,
            )
        });
        self.active_entity_id = None;
        self.pending_focus = None;
        self.install_virtual_surface_projection(Arc::clone(&prepared), cx);
        self.rebuild_runtime_context_from_markdown(&prepared.source, cx);
        self.projection_cache = Some(prepared);
        self.pending_virtual_global_runtime_refresh = false;
        self.pending_dirty_source = None;
        self.render_row_cache = None;

        let source_len = self.source_document.len();
        if let Some(input_trace) = super::perf::take_input_mutation() {
            input_trace.record_dirty_sync(source_len);
            if self.pending_input_trace.is_none() {
                self.pending_input_trace = Some(input_trace);
            }
        }
        if !self.document_dirty {
            self.document_dirty = true;
            self.pending_window_edited = true;
            self.pending_window_title_refresh = true;
        }
        self.schedule_recovery_journal(cx);
        self.schedule_auto_save(cx);
        self.schedule_active_block_spellcheck(cx);
        true
    }

    fn apply_marked_source_range(&mut self, source_range: Range<usize>, cx: &mut Context<Self>) {
        if source_range.is_empty() {
            return;
        }
        let mappings = self.build_source_target_mappings(cx);
        let Some(start) = self.endpoint_for_source_offset(source_range.start, &mappings, cx) else {
            return;
        };
        let Some(end) = self.endpoint_for_source_offset(source_range.end, &mappings, cx) else {
            return;
        };
        if start.entity_id != end.entity_id {
            return;
        }
        let Some(block) = self.focusable_entity_by_id(start.entity_id) else {
            return;
        };
        block.update(cx, |block, cx| {
            block.marked_range = Some(start.offset.min(end.offset)..start.offset.max(end.offset));
            cx.notify();
        });
    }

    pub(in crate::editor) fn replace_cross_block_selection_with_text(
        &mut self,
        new_text: &str,
        selected_range_relative: Option<Range<usize>>,
        mark_inserted_text: bool,
        undo_kind: UndoCaptureKind,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(selection) = self.normalized_cross_block_selection(cx) else {
            return false;
        };
        let Some(source_range) = self.cross_block_source_range_for_normalized(selection, cx) else {
            return false;
        };

        self.prepare_undo_capture(undo_kind, cx);
        let source_len = self.source_document.len();
        let start = source_range.start.min(source_len);
        let end = source_range.end.min(source_len);
        self.cross_block_selection = None;
        self.cross_block_drag = None;

        let inserted_start = start;
        let inserted_end = inserted_start + new_text.len();
        let selected_source_range = selected_range_relative
            .map(|relative| {
                inserted_start + relative.start.min(new_text.len())
                    ..inserted_start + relative.end.min(new_text.len())
            })
            .unwrap_or(inserted_end..inserted_end);
        let marked_source_range =
            (mark_inserted_text && !new_text.is_empty()).then_some(inserted_start..inserted_end);

        let virtual_edit = self.virtual_surface.is_some() && self.view_mode == ViewMode::Rendered;
        if virtual_edit {
            if !self.apply_virtual_cross_block_source_edit(start..end, new_text, cx) {
                self.pending_virtual_undo_selection = None;
                return false;
            }
        } else {
            let mut source = self.current_document_source(cx);
            source.replace_range(start..end, new_text);
            self.rebuild_after_cross_block_source_edit(source, cx);
        }
        self.apply_selection_snapshot_in_current_mode(
            &UndoSelectionSnapshot::from_range(selected_source_range, false),
            cx,
        );
        if let Some(marked_source_range) = marked_source_range {
            self.apply_marked_source_range(marked_source_range, cx);
        }
        if !virtual_edit {
            self.mark_dirty(cx);
        }
        self.finalize_pending_undo_capture(cx);
        self.sync_table_axis_visuals(cx);
        self.dismiss_contextual_overlays(cx);
        self.sync_cross_block_selection_visuals(cx);
        self.request_active_block_scroll_into_view(cx);
        cx.notify();
        true
    }

    pub(in crate::editor) fn cross_block_selected_markdown(&self, cx: &App) -> Option<String> {
        let selection = self.normalized_cross_block_selection(cx)?;
        let source = self.current_document_source(cx);
        let mappings = self.source_mapping_by_entity_id(cx);
        let visible = self.document.visible_blocks();

        // Join blocks with the same spacing the document serializer uses
        // (collect_root_markdown_lines): a blank line between blocks, but tight
        // list items stay on consecutive lines. A flat single-newline join used
        // to silently fuse separate paragraphs on paste, and once setext pairs
        // are recognized it could even fabricate a heading from two paragraphs.
        let mut result = String::new();
        let mut wrote_chunk = false;
        let mut pending_empty = 0usize;
        let mut previous_was_list_item = false;

        for index in selection.start_index..=selection.end_index {
            let entity = visible.get(index)?.entity.clone();
            let block = entity.read(cx);
            let len = block.visible_len();
            let range = if selection.start_index == selection.end_index {
                selection.start.offset.min(len)..selection.end.offset.min(len)
            } else if index == selection.start_index {
                selection.start.offset.min(len)..len
            } else if index == selection.end_index {
                0..selection.end.offset.min(len)
            } else {
                0..len
            };
            let full_block = range.start == 0
                && range.end == len
                && (selection.start_index != selection.end_index || len > 0);
            // Cut deletes any atomic block covered by a multi-block selection
            // (see cross_block_source_range_for_normalized), so the clipboard
            // must serialize those blocks too, including boundary ones, not
            // just interior. Otherwise cut would drop a table from the clipboard
            // that it nonetheless removed from the document.
            let include_atomic = len == 0 && selection.start_index != selection.end_index;
            if range.is_empty() && !include_atomic {
                continue;
            }

            // Empty paragraphs are blank-line separators, not content: defer
            // them so the gap between real blocks is reproduced as a blank line
            // rather than collapsed. Atomic content (tables, separators, images)
            // is len 0 too but is not an empty paragraph, so it still serializes.
            if (full_block || include_atomic) && Editor::is_empty_root_paragraph(block) {
                pending_empty += 1;
                continue;
            }

            let current_is_list_item = block.kind().is_list_item();
            if wrote_chunk {
                let separator_lines = if previous_was_list_item && current_is_list_item {
                    pending_empty
                } else {
                    pending_empty + 1
                };
                result.push_str(&"\n".repeat(separator_lines + 1));
            }
            result.push_str(&self.markdown_chunk_for_block(
                &entity,
                range,
                full_block || include_atomic,
                &source,
                &mappings,
                cx,
            ));
            wrote_chunk = true;
            pending_empty = 0;
            previous_was_list_item = current_is_list_item;
        }

        Some(result)
    }

    fn markdown_chunk_for_block(
        &self,
        entity: &Entity<Block>,
        range: Range<usize>,
        full_block: bool,
        source: &str,
        mappings: &HashMap<EntityId, SourceTargetMapping>,
        cx: &App,
    ) -> String {
        if let Some(mapping) = mappings.get(&entity.entity_id()) {
            if full_block {
                return source[mapping.full_source_range.clone()].to_string();
            }

            let start = self
                .endpoint_source_offset(
                    CrossBlockSelectionEndpoint {
                        entity_id: entity.entity_id(),
                        offset: range.start,
                    },
                    mappings,
                    cx,
                )
                .unwrap_or(mapping.full_source_range.start);
            let end = self
                .endpoint_source_offset(
                    CrossBlockSelectionEndpoint {
                        entity_id: entity.entity_id(),
                        offset: range.end,
                    },
                    mappings,
                    cx,
                )
                .unwrap_or(mapping.full_source_range.end);
            return source[start.min(end)..start.max(end)].to_string();
        }

        let block = entity.read(cx);
        if full_block {
            return match block.kind() {
                BlockKind::Table => block
                    .record
                    .table
                    .as_ref()
                    .map(serialize_table_markdown_lines)
                    .map(|lines| lines.join("\n"))
                    .unwrap_or_default(),
                _ => block
                    .record
                    .markdown_line(block.render_depth, block.list_ordinal),
            };
        }

        let markdown = block.record.title.serialize_markdown();
        let markdown_range = block.current_range_to_markdown_range(range);
        markdown
            .get(markdown_range)
            .map(ToOwned::to_owned)
            .unwrap_or_default()
    }

    pub(super) fn delete_cross_block_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(selection) = self.normalized_cross_block_selection(cx) else {
            return false;
        };
        let Some(source_range) = self.cross_block_source_range_for_normalized(selection, cx) else {
            return false;
        };
        if source_range.is_empty() {
            return false;
        }

        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        let source_len = self.source_document.len();
        let start = source_range.start.min(source_len);
        let end = source_range.end.min(source_len);
        self.cross_block_selection = None;
        self.cross_block_drag = None;

        let virtual_edit = self.virtual_surface.is_some() && self.view_mode == ViewMode::Rendered;
        if virtual_edit {
            if !self.apply_virtual_cross_block_source_edit(start..end, "", cx) {
                self.pending_virtual_undo_selection = None;
                return false;
            }
        } else {
            let mut source = self.current_document_source(cx);
            source.replace_range(start..end, "");
            self.rebuild_after_cross_block_source_edit(source, cx);
        }

        self.apply_selection_snapshot_in_current_mode(
            &UndoSelectionSnapshot::collapsed(start, gmark_document_core::SourceAffinity::Before),
            cx,
        );
        if !virtual_edit {
            self.mark_dirty(cx);
        }
        self.finalize_pending_undo_capture(cx);
        self.sync_table_axis_visuals(cx);
        self.dismiss_contextual_overlays(cx);
        self.sync_cross_block_selection_visuals(cx);
        cx.notify();
        true
    }

    /// Returns the markdown text of the current selection, whether cross-block
    /// or within a single block. Returns `None` when nothing is selected.
    pub(in crate::editor) fn selected_markdown_text(&self, cx: &App) -> Option<String> {
        // Prefer cross-block selection when present.
        if let Some(text) = self.cross_block_selected_markdown(cx) {
            if !text.is_empty() {
                return Some(text);
            }
        }

        // Fall back to a single block with a non-collapsed selection range.
        for visible in self.document.visible_blocks() {
            let block = visible.entity.read(cx);
            if block.selected_range.is_empty() {
                continue;
            }
            let markdown_range =
                block.current_range_to_markdown_range(block.selected_range.clone());
            let full_markdown = block.record.title.serialize_markdown();
            let start = markdown_range.start.min(full_markdown.len());
            let end = markdown_range.end.min(full_markdown.len());
            if start < end {
                return Some(full_markdown[start..end].to_owned());
            }
        }

        None
    }
}
