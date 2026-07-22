// @author kongweiguang

use super::*;

impl Block {
    pub(crate) fn on_code_language_move_left(
        &mut self,
        _: &MoveLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        if self.code_language_selected_range.is_empty() {
            self.move_code_language_to(
                self.previous_code_language_boundary(self.code_language_cursor_offset()),
                cx,
            );
        } else {
            self.move_code_language_to(self.code_language_selected_range.start, cx);
        }
    }

    pub(crate) fn on_code_language_move_right(
        &mut self,
        _: &MoveRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        if self.code_language_selected_range.is_empty() {
            self.move_code_language_to(
                self.next_code_language_boundary(self.code_language_cursor_offset()),
                cx,
            );
        } else {
            self.move_code_language_to(self.code_language_selected_range.end, cx);
        }
    }

    pub(crate) fn on_code_language_home(
        &mut self,
        _: &Home,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.move_code_language_to(0, cx);
    }

    pub(crate) fn on_code_language_end(
        &mut self,
        _: &End,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.move_code_language_to(self.code_language_text().len(), cx);
    }

    pub(crate) fn on_code_language_select_left(
        &mut self,
        _: &SelectLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.select_code_language_to(
            self.previous_code_language_boundary(self.code_language_cursor_offset()),
            cx,
        );
    }

    pub(crate) fn on_code_language_select_right(
        &mut self,
        _: &SelectRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.select_code_language_to(
            self.next_code_language_boundary(self.code_language_cursor_offset()),
            cx,
        );
    }

    pub(crate) fn on_code_language_select_all(
        &mut self,
        _: &SelectAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.move_code_language_to(0, cx);
        self.select_code_language_to(self.code_language_text().len(), cx);
    }

    pub(crate) fn on_code_language_copy(
        &mut self,
        _: &Copy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        if !self.code_language_selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.code_language_text()[self.code_language_selected_range.clone()].to_string(),
            ));
        }
    }

    pub(crate) fn on_code_language_cut(
        &mut self,
        _: &Cut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        if !self.code_language_selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.code_language_text()[self.code_language_selected_range.clone()].to_string(),
            ));
            self.replace_code_language_text_in_range(
                self.code_language_selected_range.clone(),
                "",
                None,
                false,
                cx,
            );
        }
    }

    pub(crate) fn on_code_language_paste(
        &mut self,
        _: &Paste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_code_language_text_in_range(
                self.code_language_selected_range.clone(),
                &text,
                None,
                false,
                cx,
            );
        }
    }

    pub(crate) fn on_code_language_focus_content(
        &mut self,
        _: &FocusPrev,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub(crate) fn on_code_language_focus_next(
        &mut self,
        _: &FocusNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.code_language_focus_handle.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        // Down from the language field leaves the code block: the editor focuses
        // the block below, creating a trailing paragraph first when the code
        // block is the last block. Enter does not exit (see on_code_language_newline).
        cx.emit(BlockEvent::RequestFocusNext { preferred_x: None });
    }

    pub(crate) fn on_code_language_indent(
        &mut self,
        _: &IndentBlock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.code_language_focus_handle.is_focused(window) {
            cx.stop_propagation();
        }
    }

    pub(crate) fn on_code_language_outdent(
        &mut self,
        _: &OutdentBlock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.code_language_focus_handle.is_focused(window) {
            cx.stop_propagation();
        }
    }

    pub(crate) fn on_code_language_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.code_language_is_selecting = true;
        self.code_language_focus_handle.focus(window);
        let offset = self.code_language_index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_code_language_to(offset, cx);
        } else {
            self.move_code_language_to(offset, cx);
        }
    }

    pub(crate) fn on_code_language_mouse_up(
        &mut self,
        _: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.code_language_is_selecting = false;
    }

    pub(crate) fn on_code_language_mouse_up_out(
        &mut self,
        _: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // GPUI dispatches mouse_up_out during capture; do not stop propagation
        // here, or controls under the pointer cannot synthesize on_click.
        if self.code_language_is_selecting {
            self.code_language_is_selecting = false;
            cx.notify();
        }
    }

    pub(crate) fn on_code_language_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.code_language_is_selecting {
            // A stale selecting flag can survive a missed mouse-up. Only extend
            // the selection while the platform still reports an active drag.
            if !event.dragging() {
                self.code_language_is_selecting = false;
                cx.notify();
                return;
            }
            cx.stop_propagation();
            self.select_code_language_to(
                self.code_language_index_for_mouse_position(event.position),
                cx,
            );
        }
    }

    pub(crate) fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // 原生表格容器包裹着独立的单元格编辑器；容器不得重复处理从单元格冒泡的点击，
        // 否则会在单元格请求聚焦后立即把焦点抢回表格块。
        if self.kind() == BlockKind::Table && self.table_runtime.is_some() {
            self.is_selecting = false;
            return;
        }

        if self.showing_rendered_image() {
            self.is_selecting = false;
            if event.click_count >= 2 {
                self.request_image_edit_expansion();
            } else {
                self.select_rendered_image(cx);
            }
            if self.focus_handle.is_focused(window) {
                if self.sync_image_focus_state(true) {
                    cx.notify();
                }
            } else {
                cx.emit(BlockEvent::RequestFocus);
            }
            cx.stop_propagation();
            return;
        }

        let offset = self.index_for_mouse_position(event.position);
        let was_focused = self.focus_handle.is_focused(window);

        // Cmd/Ctrl+click follows a rendered link instead of editing it, so the
        // block is neither focused nor selected; the link opens on mouse-up.
        if event.modifiers.secondary() && self.pointer_link_hit(event.position).is_some() {
            self.is_selecting = false;
            cx.stop_propagation();
            return;
        }

        if was_focused {
            self.is_selecting = true;
            if event.modifiers.shift {
                self.select_to(offset, cx);
            } else {
                self.move_to(offset, cx);
            }
        } else {
            self.is_selecting = false;
            self.move_to(offset, cx);
            cx.emit(BlockEvent::RequestFocus);
        }
    }

    pub(crate) fn on_read_only_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pointer_link_hit(event.position).is_some() {
            self.is_selecting = false;
            cx.stop_propagation();
            return;
        }

        self.focus_handle.focus(window);
        self.is_selecting = true;
        let offset = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    pub(crate) fn on_read_only_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_selecting = self.is_selecting;
        self.is_selecting = false;
        if !was_selecting && let Some(link) = self.pointer_link_hit(event.position) {
            self.open_rendered_link(&link, cx);
        }
    }

    /// Resolve the inline link under a pointer position against the most recent
    /// rendered text layout, if any. Returns `None` while the block shows raw
    /// source or when the pointer is not over a link.
    pub(crate) fn pointer_link_hit(&self, position: Point<Pixels>) -> Option<super::InlineLinkHit> {
        self.last_layout
            .as_ref()
            .zip(self.last_bounds)
            .and_then(|(lines, bounds)| {
                super::element::link_at_position(
                    self,
                    lines,
                    bounds,
                    self.last_line_height,
                    position,
                )
            })
            .cloned()
    }

    /// Handle mouse-down on a rendered inline link (in a mixed inline-visual
    /// block). A Cmd/Ctrl+click is claimed here so it follows the link instead
    /// of focusing the block; the destination opens on the matching mouse-up.
    pub(crate) fn on_rendered_link_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only Cmd/Ctrl+click follows the link; a plain click falls through so
        // the block focuses for editing like any other inline text.
        if event.modifiers.secondary() {
            cx.stop_propagation();
        }
    }

    /// Open a rendered inline link's destination through the editor prompt.
    pub(crate) fn open_rendered_link(
        &mut self,
        link: &super::InlineLinkHit,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        cx.emit(BlockEvent::RequestOpenLink {
            prompt_target: link.prompt_target.clone(),
            open_target: link.open_target.clone(),
        });
    }

    pub(crate) fn on_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.finish_image_resize(cx) {
            cx.stop_propagation();
            return;
        }
        self.is_selecting = false;

        // Cmd/Ctrl+click follows a rendered link, using the same open-link
        // prompt as the double-click gesture below.
        if event.modifiers.secondary()
            && let Some(link) = self.pointer_link_hit(event.position)
        {
            self.open_rendered_link(&link, cx);
            return;
        }

        if event.click_count >= 2 {
            let footnote = self
                .last_layout
                .as_ref()
                .zip(self.last_bounds)
                .and_then(|(lines, bounds)| {
                    super::element::footnote_at_position(
                        self,
                        lines,
                        bounds,
                        self.last_line_height,
                        event.position,
                    )
                })
                .cloned();
            if let Some(footnote) = footnote {
                cx.stop_propagation();
                cx.emit(BlockEvent::RequestJumpToFootnoteDefinition { id: footnote.id });
                return;
            }

            if let Some(link) = self.pointer_link_hit(event.position) {
                self.open_rendered_link(&link, cx);
            }
        }
    }

    pub(crate) fn on_footnote_backref_mouse_down(
        &mut self,
        _: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if !self.focus_handle.is_focused(window) {
            cx.emit(BlockEvent::RequestFocus);
        }
    }

    pub(crate) fn on_footnote_backref_mouse_up(
        &mut self,
        _: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.footnote_definition_id() else {
            return;
        };
        cx.stop_propagation();
        cx.emit(BlockEvent::RequestJumpToFootnoteBackref { id });
    }

    pub(crate) fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.image_resize_session.is_some() {
            if event.dragging() {
                self.update_image_resize(event.position.x, cx);
            } else {
                self.finish_image_resize(cx);
            }
            cx.stop_propagation();
            return;
        }
        if self.is_selecting {
            // A stale selecting flag can survive a missed mouse-up. Only extend
            // the selection while the platform still reports an active drag.
            if !event.dragging() {
                self.is_selecting = false;
                cx.notify();
                return;
            }
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    pub(crate) fn on_task_checkbox_mouse_down(
        &mut self,
        _: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if !self.focus_handle.is_focused(window) {
            cx.emit(BlockEvent::RequestFocus);
        }
    }

    pub(crate) fn on_task_checkbox_mouse_up(
        &mut self,
        _: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.kind().is_task_list_item() || self.is_source_raw_mode() {
            return;
        }

        cx.stop_propagation();
        cx.emit(BlockEvent::ToggleTaskChecked);
    }

    pub(crate) fn on_table_append_column_zone_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_table_append_column_hover_part(None, Some(*hovered), None, cx);
    }

    pub(crate) fn on_table_append_column_button_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_table_append_column_hover_part(None, None, Some(*hovered), cx);
    }

    pub(crate) fn on_table_append_row_zone_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_table_append_row_hover_part(None, Some(*hovered), None, cx);
    }

    pub(crate) fn on_table_append_row_button_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_table_append_row_hover_part(None, None, Some(*hovered), cx);
    }

    pub(crate) fn on_table_append_column_edge_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_table_append_column_hover_part(Some(*hovered), None, None, cx);
    }

    pub(crate) fn on_table_append_row_edge_hover(
        &mut self,
        hovered: &bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_table_append_row_hover_part(Some(*hovered), None, None, cx);
    }

    pub(crate) fn on_append_table_column(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind() == BlockKind::Table {
            cx.emit(BlockEvent::RequestAppendTableColumn);
        }
    }

    pub(crate) fn on_append_table_row(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind() == BlockKind::Table {
            cx.emit(BlockEvent::RequestAppendTableRow);
        }
    }

    pub(crate) fn on_bold_selection(
        &mut self,
        _: &BoldSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_format(InlineFormat::Bold, cx);
    }

    pub(crate) fn on_italic_selection(
        &mut self,
        _: &ItalicSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_format(InlineFormat::Italic, cx);
    }

    pub(crate) fn on_strikethrough_selection(
        &mut self,
        _: &StrikethroughSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_format(InlineFormat::Strikethrough, cx);
    }

    pub(crate) fn on_underline_selection(
        &mut self,
        _: &UnderlineSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_format(InlineFormat::Underline, cx);
    }

    pub(crate) fn on_highlight_selection(
        &mut self,
        _: &HighlightSelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_format(InlineFormat::Highlight, cx);
    }

    pub(crate) fn on_superscript_selection(
        &mut self,
        _: &SuperscriptSelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_format(InlineFormat::Superscript, cx);
    }

    pub(crate) fn on_subscript_selection(
        &mut self,
        _: &SubscriptSelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_format(InlineFormat::Subscript, cx);
    }

    pub(crate) fn on_inline_math_selection(
        &mut self,
        _: &InlineMathSelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.insert_inline_math(cx);
    }

    pub(crate) fn on_code_selection(
        &mut self,
        _: &CodeSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_format(InlineFormat::Code, cx);
    }

    pub(crate) fn on_link_selection(
        &mut self,
        _: &LinkSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_inline_link(cx);
    }

    pub(crate) fn on_exit_code_block(
        &mut self,
        _: &ExitCodeBlock,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let exits_multiline_block = self.is_table_cell() || self.kind().is_multiline_text_block();

        if exits_multiline_block {
            cx.emit(BlockEvent::RequestNewline {
                trailing: InlineTextTree::plain(String::new()),
                source_already_mutated: false,
            });
        } else if self.callout_depth > 0 {
            cx.emit(BlockEvent::RequestCalloutBreak);
        } else if self.quote_depth > 0 {
            cx.emit(BlockEvent::RequestQuoteBreak);
        }
    }
}
