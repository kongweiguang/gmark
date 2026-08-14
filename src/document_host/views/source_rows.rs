// @author kongweiguang

//! Mounted source-row blocks and pointer editing.

use super::*;

impl DocumentHost {
    /// 空文件只有一个稳定的 `0..0` 行；提前发布这份快照可让首帧直接挂载
    /// 可编辑 Block，避免 uniform_list 等待后台 viewport 任务时吞掉第一次点击。
    pub(super) fn install_empty_source_row(&mut self) {
        if self
            .document
            .as_ref()
            .is_none_or(|document| !document.is_empty())
        {
            return;
        }
        let row = Arc::new(BoundedLineWindow::new(
            0..0,
            0..0,
            String::new(),
            String::new(),
            false,
            false,
        ));
        self.source_rows.insert(0, row.clone());
        self.source_row_epochs.insert(0, self.source_cache_epoch);
        let document_revision = self.document.as_ref().map_or(0, SharedDocument::revision);
        self.displayed_screen_lines = Arc::new(ScreenLines {
            document_revision,
            generation: self.coordinator.source_generation,
            cache_epoch: self.source_cache_epoch,
            column_window_start: self.source_window_start,
            visible: 0..1,
            rows: Arc::new(BTreeMap::from([(0, row)])),
        });
    }

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
        let syntax_language = crate::components::code_language_for_path(&self.path);
        let syntax_context = self.source_syntax_contexts.get(&line).cloned();
        if let Some(block) = self.source_row_blocks.get(&line) {
            block.update(cx, |block, _cx| {
                block.set_source_layout_identity(layout_identity);
                block.set_read_only(read_only);
                block.set_source_syntax_context(syntax_language, syntax_context);
            });
            return Some(block.clone());
        }
        let row = self.displayed_screen_lines.row(line)?;
        let row_text = row.text.to_string();
        let host = cx.entity().downgrade();
        let block = cx.new(move |cx| {
            let mut block = Block::with_record(
                cx,
                BlockRecord::with_plain_text(BlockKind::Paragraph, row_text),
            );
            block.set_compact_source_host();
            block.set_read_only(read_only);
            block.set_source_syntax_context(syntax_language, syntax_context);
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
                .map(SharedDocument::revision)
                .unwrap_or_default(),
            source_range: row.content_range.clone(),
            column_window_start: self.displayed_screen_lines.column_window_start,
            show_line_endings: self.show_line_endings,
        })
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
            .map(SharedDocument::source_selection)
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

        if block.read(cx).is_read_only() {
            // provisional 行只承担浏览与选择；不设置 active_edit，避免 Changed 事件
            // 在 document 尚未安装时被 export.rs 静默丢弃并留下“卡死”焦点。
            self.active_edit = None;
            self.focus_handle.focus(window);
        } else if event.modifiers.shift && self.selection_spans_multiple_lines(selection) {
            self.active_edit = None;
            self.focus_handle.focus(window);
        } else {
            self.active_edit = Some(SourceLineEdit {
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
        cx.emit(DocumentHostEvent::StateChanged);
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
        cx.emit(DocumentHostEvent::StateChanged);
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
        cx.emit(DocumentHostEvent::StateChanged);
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
        cx.emit(DocumentHostEvent::StateChanged);
        cx.notify();
        true
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
#[path = "../../../tests/unit/document_views/source_surface.rs"]
mod tests;
