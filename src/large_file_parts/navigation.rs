// @author kongweiguang

use super::*;

impl DiskSourceAdapter {
    pub(super) fn jump_to_search_result(&mut self, cx: &mut Context<Self>) {
        let Some(found_start) = self
            .search_results
            .get(self.search_selected)
            .map(|found| found.range.start)
        else {
            return;
        };
        let line = if let Some(document) = self.document.as_ref() {
            let Some(line) = document
                .line_for_offset(found_start)
                .and_then(|line| usize::try_from(line).ok())
            else {
                return;
            };
            self.anchor_source_window_for_byte(line as u64, found_start);
            line
        } else {
            let estimated = self.probe.estimated_lines.max(1);
            let line = ((found_start as u128 * estimated as u128) / self.probe.len.max(1) as u128)
                .min(usize::MAX as u128) as usize;
            self.source_window_start = 0;
            self.invalidate_source_rows();
            line.min(self.line_count().saturating_sub(1))
        };
        self.view_mode = LargeViewMode::Source;
        self.select_source_lines(line..line.saturating_add(1), false);
        self.scroll_source_line(line, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn navigate_search(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.search_results.is_empty() {
            return;
        }
        let count = self.search_results.len() as i64;
        self.search_selected =
            (self.search_selected as i64 + i64::from(delta)).rem_euclid(count) as usize;
        self.jump_to_search_result(cx);
    }

    pub(super) fn toggle_search_option(
        &mut self,
        update: impl FnOnce(&mut SearchOptions),
        cx: &mut Context<Self>,
    ) {
        update(&mut self.search_options);
        self.schedule_search(cx);
    }

    pub(crate) fn on_find_in_document(
        &mut self,
        _: &FindInDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation_visible = false;
        self.search_visible = true;
        let host = cx.entity().downgrade();
        self.search_input.update(cx, move |input, _cx| {
            input.set_host_action_handler(move |action, window, cx| {
                let _ = host.update(cx, |view, cx| {
                    view.on_search_host_action(action, window, cx)
                });
            });
            input.focus_handle.focus(window);
        });
        cx.notify();
    }

    pub(crate) fn on_go_to_line(
        &mut self,
        _: &GoToLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_visible = false;
        self.navigation_visible = true;
        let host = cx.entity().downgrade();
        self.navigation_input.update(cx, move |input, _cx| {
            input.set_host_action_handler(move |action, window, cx| {
                let _ = host.update(cx, |view, cx| {
                    view.on_navigation_host_action(action, window, cx)
                });
            });
            let len = input.display_text().len();
            input.selected_range = 0..len;
            input.focus_handle.focus(window);
        });
        cx.notify();
    }

    pub(crate) fn on_find_next(&mut self, _: &FindNext, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate_search(1, cx);
    }

    pub(crate) fn on_find_previous(
        &mut self,
        _: &FindPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_search(-1, cx);
    }

    pub(crate) fn on_dismiss_transient_ui(
        &mut self,
        _: &DismissTransientUi,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.search_visible || self.navigation_visible || self.source_context_menu.is_some() {
            self.search_visible = false;
            self.navigation_visible = false;
            self.source_context_menu = None;
            self.focus_handle.focus(window);
            cx.notify();
        }
    }

    pub(super) fn scroll_page(&mut self, toward_end: bool, cx: &mut Context<Self>) {
        let handle = self.scroll_handle.0.borrow().base_handle.clone();
        let row_height = self.source_row_height.max(1.0);
        let local_top = (-f32::from(handle.offset().y) / row_height)
            .max(0.0)
            .floor() as usize;
        let top = self.source_list_origin.saturating_add(local_top);
        let page_rows = (f32::from(handle.bounds().size.height) / row_height)
            .floor()
            .max(1.0) as usize;
        let target = if toward_end {
            top.saturating_add(page_rows)
                .min(self.line_count().saturating_sub(1))
        } else {
            top.saturating_sub(page_rows)
        };
        // UniformList 的 logical_scroll_top/bottom 只描述当前挂载子树，虚拟列表中会同时
        // 返回 0；必须把稳定行高的像素 offset 映射回全局行，PageUp/Down 才能闭环。
        self.scroll_source_line_strict(target, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn on_page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_page(false, cx);
    }

    pub(super) fn on_page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_page(true, cx);
    }

    pub(super) fn on_jump_to_top(&mut self, _: &JumpToTop, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_source_line_strict(0, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn on_jump_to_bottom(
        &mut self,
        _: &JumpToBottom,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(last) = self.line_count().checked_sub(1) {
            self.scroll_source_line_strict(last, ScrollStrategy::Bottom);
            cx.notify();
        }
    }

    pub(super) fn json_root_index(&self) -> Option<&JsonIndex> {
        match self.structured_index.as_ref() {
            Some(StructuredIndex::Json { index, .. }) => Some(index),
            _ => None,
        }
    }

    pub(super) fn json_container_index(&self, path: &[u64]) -> Option<&JsonIndex> {
        if path.is_empty() {
            self.json_root_index()
        } else {
            self.json_child_indexes.get(path)
        }
    }

    pub(super) fn json_visible_count(&self, container_path: &[u64], index: &JsonIndex) -> u64 {
        let mut count = index.item_count();
        for expanded in &self.json_expanded_nodes {
            if expanded.len() != container_path.len() + 1 || !expanded.starts_with(container_path) {
                continue;
            }
            if let Some(child) = self.json_child_indexes.get(expanded) {
                count = count.saturating_add(self.json_visible_count(expanded, child));
            }
        }
        count
    }

    pub(super) fn json_node_at(&self, display_index: u64) -> Option<JsonNode> {
        let root = self.json_root_index()?;
        self.json_node_at_in(&[], root, display_index, 0)
    }

    pub(super) fn json_node_at_in(
        &self,
        container_path: &[u64],
        index: &JsonIndex,
        display_index: u64,
        depth: usize,
    ) -> Option<JsonNode> {
        let mut inserted = 0u64;
        for expanded in &self.json_expanded_nodes {
            if expanded.len() != container_path.len() + 1 || !expanded.starts_with(container_path) {
                continue;
            }
            let item = *expanded.last()?;
            let root_position = item.saturating_add(inserted);
            if display_index < root_position {
                break;
            }
            if display_index == root_position {
                return Some(JsonNode {
                    container_path: container_path.to_vec(),
                    item,
                    depth,
                });
            }
            let child = self.json_child_indexes.get(expanded)?;
            let child_count = self.json_visible_count(expanded, child);
            if display_index <= root_position.saturating_add(child_count) {
                return self.json_node_at_in(
                    expanded,
                    child,
                    display_index - root_position - 1,
                    depth + 1,
                );
            }
            inserted = inserted.saturating_add(child_count);
        }
        let item = display_index.saturating_sub(inserted);
        (item < index.item_count()).then(|| JsonNode {
            container_path: container_path.to_vec(),
            item,
            depth,
        })
    }

    pub(super) fn request_json_rows(&mut self, visible: Range<usize>, cx: &mut Context<Self>) {
        let Some(StructuredIndex::Json { source, .. }) = self.structured_index.clone() else {
            return;
        };
        let Some(root) = self.json_root_index() else {
            return;
        };
        let row_count = self.json_visible_count(&[], root);
        let start = visible.start.saturating_sub(STRUCTURED_OVERSCAN_ROWS) as u64;
        let end = (visible.end.saturating_add(STRUCTURED_OVERSCAN_ROWS) as u64).min(row_count);
        let nodes = (start..end)
            .filter_map(|row| self.json_node_at(row))
            .filter(|node| !self.json_rows.contains_key(&node.path()))
            .filter_map(|node| {
                self.json_container_index(&node.container_path)
                    .cloned()
                    .map(|index| (node, index))
            })
            .collect::<Vec<_>>();
        if nodes.is_empty() {
            return;
        }
        self.structured_generation = self.structured_generation.wrapping_add(1);
        let generation = self.structured_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        self.structured_pending = Some(start..end);
        self.structured_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let mut rows = Vec::with_capacity(nodes.len());
                    for (node, index) in nodes {
                        let Some(range) = index.item_range(node.item)? else {
                            continue;
                        };
                        rows.push((
                            node.path(),
                            StructuredRow {
                                index: node.item,
                                byte_range: range,
                                column_start: 0,
                                cells: read_json_cells(&index, &source, node.item)?,
                                depth: node.depth,
                            },
                        ));
                    }
                    Ok::<_, gmark_large_document::LargeDocumentError>(rows)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.structured_generation) {
                    return;
                }
                view.structured_pending = None;
                match result {
                    Ok(rows) => view.json_rows.extend(rows),
                    Err(error) => view.set_structure_error(error),
                }
                cx.notify();
            });
        });
    }

    pub(super) fn activate_json_node(&mut self, display_row: u64, cx: &mut Context<Self>) {
        if let Some(cancellation) = self.json_expand_cancellation.take() {
            cancellation.cancel();
        }
        let Some(node) = self.json_node_at(display_row) else {
            return;
        };
        let path = node.path();
        if self.json_child_indexes.contains_key(&path) {
            if !self.json_expanded_nodes.remove(&path) {
                self.json_expanded_nodes.insert(path);
            }
            self.structured_pending = None;
            cx.notify();
            return;
        }
        let Some(parent) = self.json_container_index(&node.container_path).cloned() else {
            return;
        };
        self.json_expand_generation = self.json_expand_generation.wrapping_add(1);
        let generation = self.json_expand_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        let cancellation = SearchCancellation::default();
        self.json_expand_cancellation = Some(cancellation.clone());
        self.json_expand_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    parent.child_index_cancellable(
                        node.item,
                        JsonIndexOptions::default(),
                        &cancellation,
                    )
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.json_expand_generation) {
                    return;
                }
                view.json_expand_cancellation = None;
                match result {
                    Ok(Some(child)) => {
                        view.json_child_indexes.insert(path.clone(), child);
                        view.json_expanded_nodes.insert(path);
                        view.structured_pending = None;
                    }
                    Ok(None) => {
                        if let Some(byte_offset) =
                            view.json_rows.get(&path).map(|row| row.byte_range.start)
                        {
                            view.jump_byte_offset_to_source(byte_offset, cx);
                        }
                    }
                    Err(gmark_large_document::LargeDocumentError::Cancelled) => {}
                    Err(error) => view.set_structure_error(error),
                }
                cx.notify();
            });
        });
    }

    pub(super) fn request_structured_rows(
        &mut self,
        visible: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.structured_index, Some(StructuredIndex::Json { .. })) {
            self.request_json_rows(visible, cx);
            return;
        }
        let Some(index) = self.structured_index.clone() else {
            return;
        };
        let filter_active = !self
            .structured_filter_input
            .read(cx)
            .display_text()
            .trim()
            .is_empty();
        let row_count = if filter_active {
            self.structured_filtered_rows.len() as u64
        } else {
            index.row_count()
        };
        let start = visible.start.saturating_sub(STRUCTURED_OVERSCAN_ROWS) as u64;
        let end = (visible.end.saturating_add(STRUCTURED_OVERSCAN_ROWS) as u64).min(row_count);
        if start >= end {
            return;
        }
        let logical_rows = if filter_active {
            let Some(start) = usize::try_from(start).ok() else {
                return;
            };
            let Some(end) = usize::try_from(end).ok() else {
                return;
            };
            let Some(rows) = self.structured_filtered_rows.get(start..end) else {
                return;
            };
            rows.to_vec()
        } else {
            (start..end).collect::<Vec<_>>()
        };
        if logical_rows
            .iter()
            .all(|row| self.structured_rows.contains_key(row))
        {
            return;
        }
        if self
            .structured_pending
            .as_ref()
            .is_some_and(|pending| pending.start <= start && pending.end >= end)
        {
            return;
        }

        self.structured_generation = self.structured_generation.wrapping_add(1);
        let generation = self.structured_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        let requested = start..end;
        let column_start = self.structured_column_window_start;
        let column_end = column_start.saturating_add(STRUCTURED_COLUMN_WINDOW);
        let columns = column_start..column_end;
        self.structured_pending = Some(requested.clone());
        self.structured_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    if filter_active {
                        let mut rows = Vec::with_capacity(logical_rows.len());
                        for row in logical_rows {
                            rows.extend(index.read_rows(row, 1, columns.clone())?);
                        }
                        Ok(rows)
                    } else {
                        index.read_rows(
                            requested.start,
                            usize::try_from(requested.end - requested.start)
                                .unwrap_or(STRUCTURED_OVERSCAN_ROWS * 3),
                            columns,
                        )
                    }
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.structured_generation) {
                    return;
                }
                view.structured_pending = None;
                match result {
                    Ok(rows) => {
                        view.structured_rows.clear();
                        view.structured_rows
                            .extend(rows.into_iter().map(|row| (row.index, row)));
                        view.clear_structure_error();
                    }
                    Err(error) => view.set_structure_error(error),
                }
                cx.notify();
            });
        });
    }

    pub(super) fn jump_structured_row_to_source(&mut self, row: u64, cx: &mut Context<Self>) {
        let Some(byte_offset) = self
            .structured_rows
            .get(&row)
            .map(|row| row.byte_range.start)
        else {
            return;
        };
        self.jump_byte_offset_to_source(byte_offset, cx);
    }

    pub(super) fn jump_byte_offset_to_source(&mut self, byte_offset: u64, cx: &mut Context<Self>) {
        let Some(line) = self
            .document
            .as_ref()
            .and_then(|document| document.line_for_offset(byte_offset.min(document.len())))
            .and_then(|line| usize::try_from(line).ok())
        else {
            return;
        };
        self.anchor_source_window_for_byte(line as u64, byte_offset);
        self.view_mode = LargeViewMode::Source;
        self.select_source_lines(line..line.saturating_add(1), false);
        self.scroll_source_line(line, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn source_list_len(&self) -> usize {
        self.line_count()
            .saturating_sub(self.source_list_origin)
            .min(SOURCE_LIST_WINDOW_ROWS)
    }

    pub(super) fn scroll_source_line(&mut self, line: usize, strategy: ScrollStrategy) {
        let local = self.prepare_source_list_target(line);
        self.scroll_handle.scroll_to_item(local, strategy);
    }

    pub(super) fn scroll_source_line_strict(&mut self, line: usize, strategy: ScrollStrategy) {
        let local = self.prepare_source_list_target(line);
        self.scroll_handle.scroll_to_item_strict(local, strategy);
    }

    fn prepare_source_list_target(&mut self, requested: usize) -> usize {
        let total = self.line_count().max(1);
        let target = requested.min(total.saturating_sub(1));
        let window_end = self
            .source_list_origin
            .saturating_add(SOURCE_LIST_WINDOW_ROWS)
            .min(total);
        if target < self.source_list_origin || target >= window_end {
            self.source_list_origin = source_list_origin_for_target(total, target);
        }
        target.saturating_sub(self.source_list_origin)
    }

    pub(super) fn line_count(&self) -> usize {
        self.document.as_ref().map_or_else(
            || {
                usize::try_from(self.probe.estimated_lines)
                    .unwrap_or(usize::MAX)
                    .max(self.preview_lines.len())
            },
            |document| usize::try_from(document.line_count()).unwrap_or(usize::MAX),
        )
    }

    pub(super) fn line_window(&self, line: usize) -> Option<&BoundedLineWindow> {
        self.displayed_screen_lines.row(line)
    }

    pub(super) fn line_text(&self, line: usize) -> SharedString {
        if let Some(window) = self.line_window(line) {
            return window.rendered(self.show_line_endings);
        }
        self.preview_lines.get(line).cloned().unwrap_or_default()
    }

    pub(super) fn selected_search_range(&self, line: usize) -> Option<Range<usize>> {
        let found = self.search_results.get(self.search_selected)?;
        let document = self.document.as_ref()?;
        if document.line_for_offset(found.range.start)? != line as u64 {
            return None;
        }
        let window = self.line_window(line)?;
        if found.range.start >= window.content_range.end
            || found.range.end <= window.content_range.start
        {
            return None;
        }
        let rendered = &window.text;
        let start = usize::try_from(
            found
                .range
                .start
                .max(window.content_range.start)
                .saturating_sub(window.content_range.start),
        )
        .ok()?;
        let end = usize::try_from(
            found
                .range
                .end
                .min(window.content_range.end)
                .saturating_sub(window.content_range.start),
        )
        .ok()?;
        if start >= end
            || end > rendered.len()
            || !rendered.is_char_boundary(start)
            || !rendered.is_char_boundary(end)
        {
            return None;
        }
        Some(start..end)
    }

    #[cfg(test)]
    pub(super) fn begin_line_edit(
        &mut self,
        line: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.saving || self.reloading {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let Ok(Some(windowed)) =
            read_bounded_line_window(document, line as u64, self.source_window_start)
        else {
            return;
        };
        let BoundedLineWindow {
            replace_range,
            text,
            ending,
            leading_truncated,
            trailing_truncated,
            ..
        } = windowed;
        let text = text.to_string();
        let host = cx.entity().downgrade();
        let block = cx.new(move |cx| {
            let mut block =
                Block::with_record(cx, BlockRecord::with_plain_text(BlockKind::Paragraph, text));
            block.set_compact_source_host();
            block.set_host_action_handler(move |action, window, cx| {
                let _ = host.update(cx, |view, cx| {
                    view.on_line_edit_host_action(action, window, cx)
                });
            });
            block
        });
        cx.subscribe(&block, Self::on_line_edit_event).detach();
        self.source_row_blocks.insert(line, block.clone());
        block.update(cx, |block, _cx| {
            block.selected_range = block.display_text().len()..block.display_text().len();
            block.focus_handle.focus(window);
        });
        self.active_edit = Some(LargeLineEdit {
            line,
            range: replace_range,
            ending,
            leading_truncated,
            trailing_truncated,
            block,
        });
        cx.emit(DiskSourceEvent::StateChanged);
        cx.notify();
    }

    pub(super) fn on_line_edit_host_action(
        &mut self,
        action: BlockHostAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            BlockHostAction::Submit(_) => {}
            BlockHostAction::Save => self.on_save_document(&SaveDocument, window, cx),
            BlockHostAction::Undo => self.on_undo(&Undo, window, cx),
            BlockHostAction::Redo => self.on_redo(&Redo, window, cx),
            BlockHostAction::Find => self.on_find_in_document(&FindInDocument, window, cx),
            BlockHostAction::FindNext => self.on_find_next(&FindNext, window, cx),
            BlockHostAction::FindPrevious => self.on_find_previous(&FindPrevious, window, cx),
            BlockHostAction::GoToLine => self.on_go_to_line(&GoToLine, window, cx),
            BlockHostAction::PageUp => {
                self.leave_line_edit_for_viewport_navigation(window);
                self.on_page_up(&PageUp, window, cx);
            }
            BlockHostAction::PageDown => {
                self.leave_line_edit_for_viewport_navigation(window);
                self.on_page_down(&PageDown, window, cx);
            }
            BlockHostAction::JumpToTop => {
                self.leave_line_edit_for_viewport_navigation(window);
                self.on_jump_to_top(&JumpToTop, window, cx);
            }
            BlockHostAction::JumpToBottom => {
                self.leave_line_edit_for_viewport_navigation(window);
                self.on_jump_to_bottom(&JumpToBottom, window, cx);
            }
            BlockHostAction::DismissTransientUi => {
                self.on_dismiss_transient_ui(&DismissTransientUi, window, cx)
            }
        }
    }

    pub(super) fn leave_line_edit_for_viewport_navigation(&mut self, window: &mut Window) {
        // 翻页会卸载当前虚拟行；焦点若继续留在该 Block，下一次快捷键没有可达的
        // element path。编辑已按 Changed 事件增量提交，可以安全回到宿主焦点。
        self.active_edit = None;
        self.focus_handle.focus(window);
    }

    pub(super) fn select_or_edit_line(
        &mut self,
        line: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_source_row_from_pointer(line, event, window, cx);
    }
}
