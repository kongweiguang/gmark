// @author kongweiguang

//! Code-block runtime cache management.

use super::*;

fn normalize_code_language_input(text: &str) -> String {
    text.replace("\r\n", " ")
        .replace(['\r', '\n'], " ")
        .trim()
        .to_string()
}

impl Block {
    pub(crate) fn open_code_language_menu(&mut self, cx: &mut Context<Self>) {
        let current = self.code_language_text();
        self.code_language_menu_selected = crate::components::CODE_LANGUAGE_MENU_ITEMS
            .iter()
            .position(|language| *language == current)
            .unwrap_or(0);
        self.code_language_menu_open = true;
        cx.notify();
    }

    pub(crate) fn toggle_code_language_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.kind().is_code_block() {
            return;
        }
        self.code_language_focus_handle.focus(window);
        if self.code_language_menu_open {
            self.code_language_menu_open = false;
            cx.notify();
        } else {
            self.open_code_language_menu(cx);
        }
    }

    pub(crate) fn select_code_language_menu_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(language) = crate::components::CODE_LANGUAGE_MENU_ITEMS.get(index) else {
            return;
        };
        let next = SharedString::from(*language);
        let current = match &self.record.kind {
            BlockKind::CodeBlock { language } => language.clone(),
            _ => return,
        };
        self.code_language_menu_open = false;
        if current.as_ref() == Some(&next) {
            cx.notify();
            return;
        }
        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        self.record.kind = BlockKind::CodeBlock {
            language: Some(next.clone()),
        };
        self.code_language_selected_range = next.len()..next.len();
        self.code_language_selection_reversed = false;
        self.code_language_marked_range = None;
        self.sync_code_highlight();
        cx.emit(BlockEvent::Changed);
        cx.notify();
    }

    pub(crate) fn handle_code_language_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.code_language_focus_handle.is_focused(window) {
            return false;
        }
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.platform || modifiers.alt || modifiers.function {
            return false;
        }
        if !self.code_language_menu_open {
            return false;
        }
        let count = crate::components::CODE_LANGUAGE_MENU_ITEMS.len();
        match event.keystroke.key.as_str() {
            "up" => {
                self.code_language_menu_selected = if self.code_language_menu_selected == 0 {
                    count - 1
                } else {
                    self.code_language_menu_selected - 1
                };
            }
            "down" => {
                self.code_language_menu_selected = (self.code_language_menu_selected + 1) % count;
            }
            "enter" => {
                self.select_code_language_menu_item(self.code_language_menu_selected, cx);
            }
            "escape" => self.code_language_menu_open = false,
            _ => return false,
        }
        cx.notify();
        true
    }

    pub(crate) fn copy_code_block(&mut self, cx: &mut Context<Self>) {
        if !self.kind().is_code_block() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.record.title.visible_text().to_owned(),
        ));
        self.code_copy_feedback = true;
        self.code_copy_feedback_task = Some(cx.spawn(
            async |this: WeakEntity<Block>, cx: &mut AsyncApp| {
                cx.background_executor()
                    .timer(Duration::from_millis(1_200))
                    .await;
                let _ = this.update(cx, |block, cx| {
                    block.code_copy_feedback = false;
                    block.code_copy_feedback_task = None;
                    cx.notify();
                });
            },
        ));
        cx.notify();
    }

    pub(crate) fn code_highlight_result(&self) -> Option<&CodeHighlightResult> {
        self.code_highlight.as_ref()
    }

    pub(crate) fn set_source_syntax_language(&mut self, language: Option<&'static str>) {
        let next = language.map(SharedString::from);
        if self.source_syntax_language == next {
            return;
        }
        self.source_syntax_language = next;
        self.sync_code_highlight();
    }

    pub(super) fn sync_code_highlight(&mut self) {
        let language = match &self.record.kind {
            BlockKind::CodeBlock { language } => language.as_deref().map(|value| &**value),
            _ => self.source_syntax_language.as_deref().map(|value| &**value),
        };
        self.code_highlight = highlight_code_block(language, self.render_cache.visible_text());
    }

    pub(crate) fn code_language_text(&self) -> &str {
        match &self.record.kind {
            BlockKind::CodeBlock {
                language: Some(language),
            } => language.as_ref(),
            _ => "",
        }
    }

    pub(crate) fn code_language_cursor_offset(&self) -> usize {
        if self.code_language_selection_reversed {
            self.code_language_selected_range.start
        } else {
            self.code_language_selected_range.end
        }
    }

    pub(crate) fn code_language_range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        Self::utf8_range_to_utf16_in(self.code_language_text(), range)
    }

    pub(crate) fn code_language_range_from_utf16(
        &self,
        range_utf16: &Range<usize>,
    ) -> Range<usize> {
        Self::utf16_range_to_utf8_in(self.code_language_text(), range_utf16)
    }

    pub(crate) fn previous_code_language_boundary(&self, offset: usize) -> usize {
        self.code_language_text()
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    pub(crate) fn next_code_language_boundary(&self, offset: usize) -> usize {
        self.code_language_text()
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.code_language_text().len())
    }

    pub(crate) fn move_code_language_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let clamped = offset.min(self.code_language_text().len());
        self.code_language_selected_range = clamped..clamped;
        self.code_language_selection_reversed = false;
        self.code_language_marked_range = None;
        self.cursor_blink_epoch = Instant::now();
        cx.notify();
    }

    pub(crate) fn select_code_language_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let clamped = offset.min(self.code_language_text().len());
        if self.code_language_selection_reversed {
            self.code_language_selected_range.start = clamped;
        } else {
            self.code_language_selected_range.end = clamped;
        }
        if self.code_language_selected_range.end < self.code_language_selected_range.start {
            self.code_language_selection_reversed = !self.code_language_selection_reversed;
            self.code_language_selected_range =
                self.code_language_selected_range.end..self.code_language_selected_range.start;
        }
        self.cursor_blink_epoch = Instant::now();
        cx.notify();
    }

    pub(crate) fn replace_code_language_text_in_range(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        selected_range_relative: Option<Range<usize>>,
        mark_inserted_text: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.kind().is_code_block() {
            return;
        }

        self.prepare_undo_capture(UndoCaptureKind::CoalescibleText, cx);

        let current = self.code_language_text().to_string();
        let range = range.start.min(current.len())..range.end.min(current.len());
        let inserted = new_text.replace("\r\n", " ").replace(['\r', '\n'], " ");
        let mut raw_next = String::new();
        raw_next.push_str(&current[..range.start]);
        raw_next.push_str(&inserted);
        raw_next.push_str(&current[range.end..]);

        let trimmed_start = raw_next.len() - raw_next.trim_start().len();
        let normalized = normalize_code_language_input(&raw_next);
        let normalized_len = normalized.len();
        let raw_inserted_end = range.start + inserted.len();
        let next_cursor = selected_range_relative
            .as_ref()
            .map(|relative| range.start + relative.end)
            .unwrap_or(raw_inserted_end)
            .saturating_sub(trimmed_start)
            .min(normalized_len);
        let next_selection = selected_range_relative
            .as_ref()
            .map(|relative| {
                let start = (range.start + relative.start)
                    .saturating_sub(trimmed_start)
                    .min(normalized_len);
                let end = (range.start + relative.end)
                    .saturating_sub(trimmed_start)
                    .min(normalized_len);
                start.min(end)..start.max(end)
            })
            .unwrap_or_else(|| next_cursor..next_cursor);
        let next_marked = if mark_inserted_text && !inserted.is_empty() {
            let start = range
                .start
                .saturating_sub(trimmed_start)
                .min(normalized_len);
            let end = raw_inserted_end
                .saturating_sub(trimmed_start)
                .min(normalized_len);
            (start < end).then_some(start..end)
        } else {
            None
        };

        let old_language = match &self.record.kind {
            BlockKind::CodeBlock { language } => language.clone(),
            _ => None,
        };
        self.record.kind = BlockKind::CodeBlock {
            language: (!normalized.is_empty()).then(|| SharedString::from(normalized)),
        };
        self.code_language_selected_range = next_selection;
        self.code_language_selection_reversed = selected_range_relative
            .as_ref()
            .is_some_and(|relative| relative.end < relative.start);
        self.code_language_marked_range = next_marked;
        self.cursor_blink_epoch = Instant::now();
        self.sync_code_highlight();

        let next_language = match &self.record.kind {
            BlockKind::CodeBlock { language } => language.clone(),
            _ => None,
        };
        if old_language != next_language {
            cx.emit(BlockEvent::Changed);
        }
        cx.notify();
    }

    pub(crate) fn code_language_index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let text = self.code_language_text();
        if text.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(line)) = (
            self.code_language_last_bounds.as_ref(),
            self.code_language_last_layout.as_ref(),
        ) else {
            return 0;
        };
        if position.x <= bounds.left() {
            return 0;
        }
        if position.x >= bounds.right() {
            return text.len();
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    pub(crate) fn reset_code_language_input_layout(&mut self) {
        self.code_language_last_layout = None;
        self.code_language_last_bounds = None;
        self.code_language_is_selecting = false;
    }
}
