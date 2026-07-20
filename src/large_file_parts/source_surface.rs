// @author kongweiguang

use super::*;

impl DiskSourceAdapter {
    /// 为可见源码行创建普通 Block 输入面。实体数量受 Source row LRU 同一上限约束，
    /// 因而字符命中测试、IME 与布局缓存不会随文件行数增长。
    pub(super) fn ensure_source_row_block(
        &mut self,
        line: usize,
        cx: &mut Context<Self>,
    ) -> Option<Entity<Block>> {
        let layout_identity = self.source_layout_identity_for_row(line)?;
        // provisional 行只来自稳定文件句柄的可见窗口，尚无可提交 transaction 的
        // PieceTree 真值。此时保留选择与复制，但必须拒绝键盘、粘贴和 IME 写入；
        // 精确文档安装后复用同一 Block 并恢复编辑，避免用户看到最终会丢失的假修改。
        let read_only = self.document.is_none();
        if let Some(block) = self.source_row_blocks.get(&line) {
            block.update(cx, |block, _cx| {
                block.set_source_layout_identity(layout_identity);
                block.set_read_only(read_only);
            });
            return Some(block.clone());
        }
        let row = self.displayed_screen_lines.row(line)?;
        let row_text = row.text.to_string();
        let syntax_language = crate::components::code_language_for_path(&self.path);
        let host = cx.entity().downgrade();
        let block = cx.new(move |cx| {
            let mut block = Block::with_record(
                cx,
                BlockRecord::with_plain_text(BlockKind::Paragraph, row_text),
            );
            block.set_compact_source_host();
            block.set_read_only(read_only);
            block.set_source_syntax_language(syntax_language);
            block.set_source_layout_identity(layout_identity);
            block.set_host_action_handler(move |action, window, cx| {
                let _ = host.update(cx, |view, cx| {
                    view.on_line_edit_host_action(action, window, cx)
                });
            });
            block
        });
        cx.subscribe(&block, Self::on_line_edit_event).detach();
        self.source_row_blocks.insert(line, block.clone());
        self.apply_source_selection_visual(line, &block, cx);
        Some(block)
    }

    fn source_layout_identity_for_row(&self, line: usize) -> Option<SourceLayoutIdentity> {
        let row = self.displayed_screen_lines.row(line)?;
        Some(SourceLayoutIdentity {
            document_epoch: self.document_epoch,
            document_revision: self
                .document
                .as_ref()
                .map(LargeDocumentAdapter::revision)
                .unwrap_or_default(),
            source_range: row.content_range.clone(),
            column_window_start: self.displayed_screen_lines.column_window_start,
            show_line_endings: self.show_line_endings,
        })
    }

    #[cfg(test)]
    pub(crate) fn source_layout_cache_metrics_for_test(&self, cx: &App) -> (u64, u64, usize) {
        self.source_row_blocks.values().fold(
            (0u64, 0u64, 0usize),
            |(hits, misses, entries), block| {
                let block = block.read(cx);
                (
                    hits.saturating_add(block.source_layout_cache_hits),
                    misses.saturating_add(block.source_layout_cache_misses),
                    entries + usize::from(block.source_layout_cache_key.is_some()),
                )
            },
        )
    }

    pub(super) fn activate_source_row_from_pointer(
        &mut self,
        line: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.saving || self.reloading {
            return;
        }
        let Some(block) = self.source_row_blocks.get(&line).cloned() else {
            return;
        };
        let Some(row) = self.displayed_screen_lines.row(line).cloned() else {
            return;
        };
        let previous = self
            .document
            .as_ref()
            .map(LargeDocumentAdapter::source_selection)
            .unwrap_or_default();

        if event.click_count >= 3 {
            block.update(cx, |block, cx| {
                block.selected_range = 0..block.display_text().len();
                block.selection_reversed = false;
                cx.notify();
            });
        } else if event.click_count == 2 {
            block.update(cx, |block, cx| {
                let caret = block.selected_range.end.min(block.display_text().len());
                let word = source_word_range(block.display_text(), caret);
                block.selected_range = word;
                block.selection_reversed = false;
                cx.notify();
            });
        }

        let local_selection = source_selection_from_block(block.read(cx), row.content_range.start);
        let selection = if event.modifiers.shift {
            SourceSelection {
                anchor: previous.anchor,
                head: local_selection.head,
            }
        } else {
            local_selection
        };
        self.set_source_selection(selection, cx);
        self.source_drag_anchor = Some(selection.anchor);

        if event.modifiers.shift && self.selection_spans_multiple_lines(selection) {
            self.active_edit = None;
            self.focus_handle.focus(window);
        } else {
            self.active_edit = Some(LargeLineEdit {
                line,
                range: row.replace_range,
                ending: row.ending,
                leading_truncated: row.leading_truncated,
                trailing_truncated: row.trailing_truncated,
                block: block.clone(),
            });
            block.read(cx).focus_handle.focus(window);
        }
        self.sync_source_selection_visuals(cx);
        cx.emit(DiskSourceEvent::StateChanged);
        cx.notify();
    }

    pub(super) fn sync_selection_from_active_source_block(
        &mut self,
        block: &Entity<Block>,
        cx: &mut Context<Self>,
    ) {
        let Some(active) = self
            .active_edit
            .as_ref()
            .filter(|active| active.block == *block)
        else {
            return;
        };
        let Some(row) = self.displayed_screen_lines.row(active.line) else {
            return;
        };
        let selection = source_selection_from_block(block.read(cx), row.content_range.start);
        self.set_source_selection(selection, cx);
        self.sync_source_selection_visuals(cx);
        cx.emit(DiskSourceEvent::StateChanged);
    }

    pub(super) fn on_source_surface_mouse_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            self.source_drag_anchor = None;
            self.stop_source_drag_autoscroll();
            return;
        }
        let Some(anchor) = self.source_drag_anchor else {
            return;
        };
        let Some((line, block)) = self.source_block_at_point(event.position, cx) else {
            return;
        };
        let Some(row) = self.displayed_screen_lines.row(line) else {
            return;
        };
        let local = block.read(cx).index_for_mouse_position(event.position);
        let head = SourceAnchor::new(
            row.content_range
                .start
                .saturating_add(local.min(row.text.len()) as u64),
            SourceAffinity::After,
        );
        self.active_edit = None;
        self.focus_handle.focus(window);
        self.set_source_selection(SourceSelection { anchor, head }, cx);
        self.sync_source_selection_visuals(cx);

        let viewport = self.scroll_handle.0.borrow().base_handle.bounds();
        if event.position.y <= viewport.top() + px(self.source_row_height * 1.5) {
            self.start_source_drag_autoscroll(-1, cx);
        } else if event.position.y >= viewport.bottom() - px(self.source_row_height) {
            self.start_source_drag_autoscroll(1, cx);
        } else {
            self.stop_source_drag_autoscroll();
        }
        cx.emit(DiskSourceEvent::StateChanged);
        cx.notify();
    }

    pub(super) fn on_source_surface_mouse_up(
        &mut self,
        _: &gpui::MouseUpEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.source_drag_anchor = None;
        self.stop_source_drag_autoscroll();
    }

    pub(super) fn start_source_drag_autoscroll(&mut self, direction: i8, cx: &mut Context<Self>) {
        let direction = direction.signum();
        if direction == 0 || self.source_drag_autoscroll_direction == direction {
            return;
        }
        self.source_drag_autoscroll_direction = direction;
        self.source_drag_autoscroll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let keep_running = this
                    .update(cx, |view, cx| view.source_drag_autoscroll_tick(cx))
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        });
    }

    fn stop_source_drag_autoscroll(&mut self) {
        self.source_drag_autoscroll_direction = 0;
        self.source_drag_autoscroll_task = Task::ready(());
    }

    pub(super) fn source_drag_autoscroll_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(anchor) = self.source_drag_anchor else {
            self.source_drag_autoscroll_direction = 0;
            return false;
        };
        let direction = self.source_drag_autoscroll_direction;
        if direction == 0 {
            return false;
        }
        let visible = self.displayed_screen_lines.visible.clone();
        let target_line = if direction < 0 {
            visible.start
        } else {
            visible.end.saturating_sub(1)
        };
        let Some(row) = self.displayed_screen_lines.row(target_line) else {
            return true;
        };
        let head = if direction < 0 {
            SourceAnchor::new(row.content_range.start, SourceAffinity::Before)
        } else {
            SourceAnchor::new(row.content_range.end, SourceAffinity::After)
        };
        self.active_edit = None;
        self.set_source_selection(SourceSelection { anchor, head }, cx);
        self.sync_source_selection_visuals(cx);

        let next = if direction < 0 {
            visible.start.saturating_sub(1)
        } else {
            visible.end.min(self.line_count().saturating_sub(1))
        };
        self.scroll_source_line_strict(next, ScrollStrategy::Top);
        cx.emit(DiskSourceEvent::StateChanged);
        cx.notify();
        true
    }

    pub(super) fn open_source_context_menu(
        &mut self,
        line: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(block) = self.source_row_blocks.get(&line).cloned() else {
            return;
        };
        let Some(row) = self.displayed_screen_lines.row(line) else {
            return;
        };
        let local = block.read(cx).index_for_mouse_position(event.position);
        let offset = row
            .content_range
            .start
            .saturating_add(local.min(row.text.len()) as u64);
        let existing = self
            .document
            .as_ref()
            .map(LargeDocumentAdapter::source_selection)
            .unwrap_or_default();
        if existing.range().is_empty() || !existing.range().contains(&offset) {
            let caret = SourceAnchor::new(offset, SourceAffinity::After);
            self.active_edit = None;
            self.set_source_selection(
                SourceSelection {
                    anchor: caret,
                    head: caret,
                },
                cx,
            );
            self.sync_source_selection_visuals(cx);
        }
        self.source_context_menu = Some(event.position);
        let menu_focus = self.source_context_menu_focus_handle.clone();
        // 菜单的 focus node 要到下一帧才进入 dispatch tree；延后聚焦可确保
        // Escape action 命中菜单自身，而不是沿用右键前的行内 Block 路径。
        window.defer(cx, move |window, _cx| menu_focus.focus(window));
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn capture_source_surface_mouse_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Right {
            if let Some((line, _)) = self.source_block_at_point(event.position, cx) {
                self.open_source_context_menu(line, event, window, cx);
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    pub(super) fn run_source_context_command(
        &mut self,
        command: SourceContextCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.source_context_menu = None;
        match command {
            SourceContextCommand::Copy => self.on_copy(&Copy, window, cx),
            SourceContextCommand::Cut => self.on_cut(&Cut, window, cx),
            SourceContextCommand::Paste => self.on_paste(&Paste, window, cx),
            SourceContextCommand::SelectAll => self.on_select_all(&SelectAll, window, cx),
            SourceContextCommand::ExportSelection => {
                self.on_export_selection(&ExportSelection, window, cx)
            }
            SourceContextCommand::ExportSelectionUtf8 => self.export_selection_as_utf8(window, cx),
        }
        cx.notify();
    }

    pub(super) fn on_source_surface_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape" && self.source_context_menu.take().is_some() {
            self.focus_handle.focus(window);
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn source_block_at_point(
        &self,
        position: gpui::Point<gpui::Pixels>,
        cx: &App,
    ) -> Option<(usize, Entity<Block>)> {
        let mut nearest = None;
        for (line, block) in &self.source_row_blocks {
            let Some(bounds) = block.read(cx).last_bounds else {
                continue;
            };
            if position.y >= bounds.top() && position.y <= bounds.bottom() {
                return Some((*line, block.clone()));
            }
            let distance = if position.y < bounds.top() {
                f32::from(bounds.top() - position.y)
            } else {
                f32::from(position.y - bounds.bottom())
            };
            if nearest
                .as_ref()
                .is_none_or(|(_, _, best_distance)| distance < *best_distance)
            {
                nearest = Some((*line, block.clone(), distance));
            }
        }
        nearest.map(|(line, block, _)| (line, block))
    }

    fn selection_spans_multiple_lines(&self, selection: SourceSelection) -> bool {
        let Some(document) = self.document.as_ref() else {
            return false;
        };
        let range = selection.range();
        let start = document.line_for_offset(range.start);
        let end = document.line_for_offset(range.end.saturating_sub(1));
        start.zip(end).is_some_and(|(start, end)| start != end)
    }

    pub(super) fn set_source_selection(
        &mut self,
        selection: SourceSelection,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        document.set_source_selection(selection);
        self.view_state.source.selection = document.source_selection();
        let normalized = document.source_selection().range();
        let start_line = document
            .line_for_offset(normalized.start)
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or_default();
        let end_offset = if normalized.is_empty() {
            normalized.end
        } else {
            normalized.end.saturating_sub(1)
        };
        let end_line = document
            .line_for_offset(end_offset)
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or(start_line);
        self.selection_anchor = document
            .line_for_offset(selection.anchor.byte_offset)
            .and_then(|line| usize::try_from(line).ok());
        self.selected_lines = Some(start_line..end_line.saturating_add(1));
        self.error = None;
        cx.notify();
    }

    pub(super) fn sync_source_selection_visuals(&mut self, cx: &mut Context<Self>) {
        let rows = self
            .source_row_blocks
            .iter()
            .map(|(line, block)| (*line, block.clone()))
            .collect::<Vec<_>>();
        for (line, block) in rows {
            self.apply_source_selection_visual(line, &block, cx);
        }
    }

    fn apply_source_selection_visual(
        &self,
        line: usize,
        block: &Entity<Block>,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let Some(row) = self.displayed_screen_lines.row(line) else {
            return;
        };
        let selection = document.source_selection();
        let normalized = selection.range();
        let intersection_start = normalized.start.max(row.content_range.start);
        let intersection_end = normalized.end.min(row.content_range.end);
        let is_active_local = self
            .active_edit
            .as_ref()
            .is_some_and(|active| active.line == line)
            && !self.selection_spans_multiple_lines(selection);
        let search_range = normalized
            .is_empty()
            .then(|| self.selected_search_range(line))
            .flatten()
            .filter(|_| {
                self.active_edit
                    .as_ref()
                    .is_none_or(|active| active.line != line)
            });
        block.update(cx, |block, cx| {
            if is_active_local {
                block.editor_selection_range = None;
            } else if intersection_start < intersection_end {
                block.editor_selection_range = Some(
                    usize::try_from(intersection_start - row.content_range.start)
                        .unwrap_or_default()
                        ..usize::try_from(intersection_end - row.content_range.start)
                            .unwrap_or(block.display_text().len()),
                );
            } else {
                block.editor_selection_range = search_range;
            }
            cx.notify();
        });
    }
}

fn source_selection_from_block(block: &Block, source_start: u64) -> SourceSelection {
    let start = SourceAnchor::new(
        source_start.saturating_add(block.selected_range.start as u64),
        SourceAffinity::Before,
    );
    let end = SourceAnchor::new(
        source_start.saturating_add(block.selected_range.end as u64),
        SourceAffinity::After,
    );
    if block.selection_reversed {
        SourceSelection {
            anchor: end,
            head: start,
        }
    } else {
        SourceSelection {
            anchor: start,
            head: end,
        }
    }
}

fn source_word_range(text: &str, offset: usize) -> Range<usize> {
    let offset = offset.min(text.len());
    let characters = text
        .char_indices()
        .map(|(start, ch)| {
            (
                start,
                start + ch.len_utf8(),
                ch.is_alphanumeric() || ch == '_',
            )
        })
        .collect::<Vec<_>>();
    if let Some(mut index) = characters
        .iter()
        .position(|(start, end, _)| offset >= *start && offset < *end)
        .or_else(|| {
            offset.checked_sub(1).and_then(|offset| {
                characters
                    .iter()
                    .position(|(start, end, _)| offset >= *start && offset < *end)
            })
        })
        && characters[index].2
    {
        let mut start = characters[index].0;
        let mut end = characters[index].1;
        while index > 0 && characters[index - 1].2 {
            index -= 1;
            start = characters[index].0;
        }
        let mut next = index + 1;
        while next < characters.len() && characters[next].2 {
            end = characters[next].1;
            next += 1;
        }
        return start..end;
    }
    let (start, end) = if offset < text.len() {
        let end = text[offset..]
            .graphemes(true)
            .next()
            .map_or(offset, |grapheme| offset + grapheme.len());
        (offset, end)
    } else {
        text[..offset]
            .grapheme_indices(true)
            .next_back()
            .map_or((offset, offset), |(start, grapheme)| {
                (start, start + grapheme.len())
            })
    };
    start..end
}

#[cfg(test)]
mod tests {
    use super::source_word_range;

    #[test]
    fn source_word_selection_keeps_unicode_and_emoji_boundaries() {
        assert_eq!(source_word_range("alpha 世界 🙂", 8), 6..12);
        assert_eq!(source_word_range("alpha 世界 🙂", 13), 13..17);
    }
}
