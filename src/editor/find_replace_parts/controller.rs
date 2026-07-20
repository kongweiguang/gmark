// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn refresh_find_if_stale(&mut self, cx: &mut Context<Self>) {
        let stale = self.find_panel.as_ref().is_some_and(|state| {
            state.revision != self.source_document.revision() && state.task.is_none()
        });
        if stale {
            self.schedule_find(cx);
        }
    }

    fn close_find_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let restore = self.find_panel.take().and_then(|state| state.restore_focus);
        if let Some(block) = restore.and_then(|id| self.focusable_entity_by_id(id)) {
            block.read(cx).focus_handle.focus(window);
        }
        cx.notify();
    }

    pub(in crate::editor) fn handle_find_panel_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.find_panel.as_ref() else {
            return false;
        };
        let keyboard_target = state.keyboard_target;
        match event.keystroke.key.as_str() {
            "escape" => self.close_find_panel(window, cx),
            "enter" => {
                if keyboard_target.is_control() {
                    self.activate_find_keyboard_target(keyboard_target, window, cx);
                } else {
                    let delta = if event.keystroke.modifiers.shift {
                        -1
                    } else {
                        1
                    };
                    self.navigate_find_match(delta, window, cx);
                }
            }
            "space" if keyboard_target.is_control() => {
                self.activate_find_keyboard_target(keyboard_target, window, cx);
            }
            "tab" => self.move_find_keyboard_target(event.keystroke.modifiers.shift, window, cx),
            _ => return false,
        }
        true
    }

    fn move_find_keyboard_target(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.find_panel.as_mut() else {
            return;
        };
        const FIND_ONLY_ORDER: [FindKeyboardTarget; 7] = [
            FindKeyboardTarget::Query,
            FindKeyboardTarget::CaseSensitive,
            FindKeyboardTarget::WholeWord,
            FindKeyboardTarget::Regex,
            FindKeyboardTarget::Previous,
            FindKeyboardTarget::Next,
            FindKeyboardTarget::Close,
        ];
        const REPLACE_ORDER: [FindKeyboardTarget; 8] = [
            FindKeyboardTarget::Query,
            FindKeyboardTarget::Replacement,
            FindKeyboardTarget::CaseSensitive,
            FindKeyboardTarget::WholeWord,
            FindKeyboardTarget::Regex,
            FindKeyboardTarget::Previous,
            FindKeyboardTarget::Next,
            FindKeyboardTarget::Close,
        ];
        let order = if state.show_replace {
            REPLACE_ORDER.as_slice()
        } else {
            FIND_ONLY_ORDER.as_slice()
        };
        let current = order
            .iter()
            .position(|target| *target == state.keyboard_target)
            .unwrap_or(0);
        let next = if reverse {
            current.checked_sub(1).unwrap_or(order.len() - 1)
        } else {
            (current + 1) % order.len()
        };
        state.keyboard_target = order[next];
        match state.keyboard_target {
            FindKeyboardTarget::Query => state.query.read(cx).focus_handle.focus(window),
            FindKeyboardTarget::Replacement => {
                state.replacement.read(cx).focus_handle.focus(window)
            }
            _ => state.focus_handle.focus(window),
        }
        cx.notify();
    }

    fn activate_find_keyboard_target(
        &mut self,
        target: FindKeyboardTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            FindKeyboardTarget::CaseSensitive => {
                self.toggle_find_option(|options| &mut options.case_sensitive, cx)
            }
            FindKeyboardTarget::WholeWord => {
                self.toggle_find_option(|options| &mut options.whole_word, cx)
            }
            FindKeyboardTarget::Regex => self.toggle_find_option(|options| &mut options.regex, cx),
            FindKeyboardTarget::Previous => {
                self.navigate_find_match(-1, window, cx);
                self.focus_find_keyboard_target(FindKeyboardTarget::Previous, window, cx);
            }
            FindKeyboardTarget::Next => {
                self.navigate_find_match(1, window, cx);
                self.focus_find_keyboard_target(FindKeyboardTarget::Next, window, cx);
            }
            FindKeyboardTarget::Close => self.close_find_panel(window, cx),
            FindKeyboardTarget::Query | FindKeyboardTarget::Replacement => {}
        }
    }

    fn focus_find_keyboard_target(
        &mut self,
        target: FindKeyboardTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.find_panel.as_mut() else {
            return;
        };
        state.keyboard_target = target;
        if target.is_control() {
            state.focus_handle.focus(window);
        } else {
            match target {
                FindKeyboardTarget::Query => state.query.read(cx).focus_handle.focus(window),
                FindKeyboardTarget::Replacement => {
                    state.replacement.read(cx).focus_handle.focus(window)
                }
                _ => {}
            }
        }
        cx.notify();
    }

    pub(in crate::editor) fn navigate_find_match(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_selection = self.capture_source_selection_snapshot(cx).range();
        let Some(state) = self.find_panel.as_mut() else {
            return;
        };
        if state.matches.is_empty() {
            return;
        }
        let len = state.matches.len() as isize;
        if current_selection == state.matches[state.selected] {
            state.selected = (state.selected as isize + delta).rem_euclid(len) as usize;
        } else if delta < 0 {
            state.selected = (state.selected as isize - 1).rem_euclid(len) as usize;
        }
        let range = state.matches[state.selected].clone();
        let query = state.query.clone();
        if let Some(y) = self
            .virtual_surface
            .as_ref()
            .and_then(|surface| surface.y_for_source_offset(range.start))
        {
            self.scroll_handle.set_offset(point(px(0.0), px(-y)));
            let viewport_height = f32::from(self.scroll_handle.bounds().size.height.max(px(1.0)));
            self.sync_virtual_surface_mounts(y, viewport_height, 800.0, cx);
        }
        self.apply_selection_snapshot_in_current_mode(
            &UndoSelectionSnapshot::from_range(range, false),
            cx,
        );
        self.pending_focus = None;
        self.pending_scroll_active_block_into_view = true;
        self.pending_scroll_recheck_after_layout = true;
        query.read(cx).focus_handle.focus(window);
        cx.notify();
    }

    fn toggle_find_option(
        &mut self,
        option: fn(&mut FindOptions) -> &mut bool,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.find_panel.as_mut() else {
            return;
        };
        let target = option(&mut state.options);
        *target = !*target;
        self.schedule_find(cx);
    }

    pub(in crate::editor) fn replace_current_find_match(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view_mode == super::ViewMode::Preview || !self.source_encoding.is_utf8() {
            return;
        }
        let Some(state) = self.find_panel.as_ref() else {
            return;
        };
        let Some(range) = state.matches.get(state.selected).cloned() else {
            return;
        };
        if state.revision != self.source_document.revision() {
            self.schedule_find(cx);
            return;
        }
        let query = state.query.read(cx).display_text().to_owned();
        let replacement = state.replacement.read(cx).display_text().to_owned();
        let options = state.options;
        let source = self.source_document.text();
        let Ok(regex) = compile_find_regex(&query, options) else {
            return;
        };
        let Some(replacement) =
            replacement_for_range(&regex, &source, range.clone(), &replacement, options.regex)
        else {
            self.schedule_find(cx);
            return;
        };
        let selected = range.start..range.start + replacement.len();
        if self.apply_find_edits(vec![TextEdit::new(range, replacement)], selected, cx) {
            self.schedule_find(cx);
            if let Some(state) = self.find_panel.as_ref() {
                state.query.read(cx).focus_handle.focus(window);
            }
        }
    }

    pub(in crate::editor) fn replace_all_find_matches(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view_mode == super::ViewMode::Preview || !self.source_encoding.is_utf8() {
            return;
        }
        let Some(state) = self.find_panel.as_ref() else {
            return;
        };
        if state.matches.is_empty() || state.revision != self.source_document.revision() {
            return;
        }
        let query = state.query.read(cx).display_text().to_owned();
        let replacement_template = state.replacement.read(cx).display_text().to_owned();
        let options = state.options;
        let source = self.source_document.text();
        let Ok(regex) = compile_find_regex(&query, options) else {
            return;
        };
        let mut edits = Vec::with_capacity(state.matches.len());
        let mut first_selection = None;
        for range in &state.matches {
            let Some(replacement) = replacement_for_range(
                &regex,
                &source,
                range.clone(),
                &replacement_template,
                options.regex,
            ) else {
                continue;
            };
            if first_selection.is_none() {
                first_selection = Some(range.start..range.start + replacement.len());
            }
            edits.push(TextEdit::new(range.clone(), replacement));
        }
        let selection = first_selection.unwrap_or(0..0);
        if self.apply_find_edits(edits, selection, cx) {
            self.schedule_find(cx);
            if let Some(state) = self.find_panel.as_ref() {
                state.query.read(cx).focus_handle.focus(window);
            }
        }
    }

    /// 所有替换共享一个 Rope transaction；失败时 projection、dirty 与 undo 均保持不变。
    fn apply_find_edits(
        &mut self,
        edits: Vec<TextEdit>,
        selection: Range<usize>,
        cx: &mut Context<Self>,
    ) -> bool {
        if edits.is_empty() || self.view_mode == super::ViewMode::Preview {
            return false;
        }
        self.finalize_pending_undo_capture(cx);
        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        let revision = self.source_document.revision();
        let updated = match self
            .source_document
            .apply_transaction(Transaction::new(revision, edits))
        {
            Ok(updated) => updated,
            Err(error) => {
                self.pending_undo_capture = None;
                self.pending_virtual_undo_selection = None;
                eprintln!("文档替换事务提交失败: {error}");
                return false;
            }
        };
        let source = updated.text();
        self.projection_cache_task = None;
        self.projection_cache_scheduled_revision = None;
        if self.virtual_surface.is_some() && self.view_mode == super::ViewMode::Rendered {
            let prepared = Arc::new(if let Some(previous) = self.projection_cache.as_deref() {
                PreparedSplitProjection::from_snapshot_incremental_regions_only(updated, previous)
            } else {
                PreparedSplitProjection::from_snapshot_adaptive(
                    updated,
                    Self::VIRTUAL_SURFACE_REGION_THRESHOLD,
                )
            });
            self.active_entity_id = None;
            self.pending_focus = None;
            self.install_virtual_surface_projection(Arc::clone(&prepared), cx);
            self.rebuild_runtime_context_from_markdown(&prepared.source, cx);
            self.projection_cache = Some(prepared);
        } else {
            match self.view_mode {
                super::ViewMode::Rendered => {
                    self.rebuild_primary_projection_from_source_reusing(cx)
                }
                super::ViewMode::Source | super::ViewMode::Split => {
                    let block = Self::new_block(cx, BlockRecord::paragraph(source.clone()));
                    block.update(cx, |block, _cx| block.set_source_document_mode());
                    self.document.replace_roots(vec![block], cx);
                    self.table_cells.clear();
                    if self.view_mode == super::ViewMode::Split {
                        self.schedule_split_preview_projection(cx);
                    }
                }
                super::ViewMode::Preview => return false,
            }
        }
        self.pending_dirty_source = Some(source);
        self.render_row_cache = None;
        self.status_bar.invalidate_word_count();
        self.document_dirty = true;
        self.pending_window_edited = true;
        self.pending_window_title_refresh = true;
        self.apply_selection_snapshot_in_current_mode(
            &UndoSelectionSnapshot::from_range(selection, false),
            cx,
        );
        self.pending_focus = None;
        self.finalize_pending_undo_capture(cx);
        self.schedule_recovery_journal(cx);
        self.schedule_auto_save(cx);
        self.schedule_active_block_spellcheck(cx);
        self.pending_scroll_active_block_into_view = true;
        self.pending_scroll_recheck_after_layout = true;
        cx.notify();
        true
    }

    pub(in crate::editor) fn render_find_panel(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        top: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.find_panel.as_ref()?;
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let count = if let Some(error) = state.error.as_ref() {
            error.clone()
        } else if state.matches.is_empty() {
            strings.find_no_results.clone()
        } else {
            let total = if state.truncated {
                format!("{}+", state.matches.len())
            } else {
                state.matches.len().to_string()
            };
            strings
                .find_match_count_template
                .replace("{current}", &(state.selected + 1).to_string())
                .replace("{total}", &total)
        };
        let editor = cx.entity().downgrade();
        let option_button =
            |id: &'static str,
             label: String,
             icon: &'static str,
             target: FindKeyboardTarget,
             active: bool,
             option: fn(&mut FindOptions) -> &mut bool| {
                let click_editor = editor.clone();
                let hover_editor = editor.clone();
                div()
                    .id(id)
                    .debug_selector(move || id.to_owned())
                    .relative()
                    .h(px(26.0))
                    .min_w(px(26.0))
                    .px(px(5.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .border(px(1.0))
                    .border_color(if state.keyboard_target == target {
                        c.text_link
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .bg(if active {
                        c.dialog_secondary_button_hover
                    } else {
                        c.dialog_secondary_button_bg
                    })
                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .text_color(c.dialog_body)
                    .child(
                        svg()
                            .path(icon)
                            .size(px(15.0))
                            .text_color(if active { c.text_link } else { c.dialog_body })
                            .debug_selector(move || format!("{id}-icon")),
                    )
                    .children(
                        (state.tooltip_visible == Some(id))
                            .then(|| render_find_tooltip(label, theme)),
                    )
                    .on_hover(move |hovered, _window, cx| {
                        let _ = hover_editor.update(cx, |editor, cx| {
                            editor.set_find_tooltip_hover(id, *hovered, cx);
                        });
                    })
                    .on_click(move |_event, window, cx| {
                        let _ = click_editor.update(cx, |editor, cx| {
                            editor.focus_find_keyboard_target(target, window, cx);
                            editor.toggle_find_option(option, cx);
                        });
                    })
                    .into_any_element()
            };
        let compact_button =
            |id: &'static str,
             label: String,
             handler: fn(&mut Editor, &mut Window, &mut Context<Editor>)| {
                div()
                    .id(id)
                    .h(px(26.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .bg(c.dialog_secondary_button_bg)
                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .text_size(px(t.dialog_button_size))
                    .text_color(c.dialog_secondary_button_text)
                    .on_click(cx.listener(move |editor, _event, window, cx| {
                        handler(editor, window, cx);
                    }))
                    .child(label)
                    .into_any_element()
            };
        let find_row = div()
            .h(px(34.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .id("document-find-input")
                    .debug_selector(|| "document-find-input".to_owned())
                    .w(px(210.0))
                    .h(px(30.0))
                    .px(px(7.0))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .rounded(px(5.0))
                    .border(px(d.dialog_border_width))
                    .border_color(if state.keyboard_target == FindKeyboardTarget::Query {
                        c.text_link
                    } else {
                        c.dialog_border
                    })
                    .bg(c.code_language_input_bg)
                    .child(state.query.clone()),
            )
            .child(
                div()
                    .id("document-find-count")
                    .debug_selector(|| "document-find-count".to_owned())
                    .w(px(74.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(px(12.0))
                    .text_color(if state.error.is_some() {
                        c.dialog_danger_button_bg
                    } else {
                        c.dialog_muted
                    })
                    .child(count),
            )
            .child(option_button(
                "document-find-case",
                strings.find_case_sensitive.clone(),
                FIND_CASE_ICON,
                FindKeyboardTarget::CaseSensitive,
                state.options.case_sensitive,
                |options| &mut options.case_sensitive,
            ))
            .child(option_button(
                "document-find-word",
                strings.find_whole_word.clone(),
                FIND_WORD_ICON,
                FindKeyboardTarget::WholeWord,
                state.options.whole_word,
                |options| &mut options.whole_word,
            ))
            .child(option_button(
                "document-find-regex",
                strings.find_regex.clone(),
                FIND_REGEX_ICON,
                FindKeyboardTarget::Regex,
                state.options.regex,
                |options| &mut options.regex,
            ))
            .child(
                div()
                    .id("document-find-previous")
                    .size(px(26.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .bg(c.dialog_secondary_button_bg)
                    .text_color(c.dialog_body)
                    .border(px(1.0))
                    .border_color(if state.keyboard_target == FindKeyboardTarget::Previous {
                        c.text_link
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .on_click(cx.listener(|editor, _event, window, cx| {
                        editor.navigate_find_match(-1, window, cx);
                        editor.focus_find_keyboard_target(FindKeyboardTarget::Previous, window, cx);
                    }))
                    .child(
                        svg()
                            .path(CHEVRON_UP_ICON)
                            .size(px(15.0))
                            .text_color(c.dialog_body)
                            .debug_selector(|| "document-find-previous-icon".to_owned()),
                    ),
            )
            .child(
                div()
                    .id("document-find-next")
                    .size(px(26.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .bg(c.dialog_secondary_button_bg)
                    .text_color(c.dialog_body)
                    .border(px(1.0))
                    .border_color(if state.keyboard_target == FindKeyboardTarget::Next {
                        c.text_link
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .on_click(cx.listener(|editor, _event, window, cx| {
                        editor.navigate_find_match(1, window, cx);
                        editor.focus_find_keyboard_target(FindKeyboardTarget::Next, window, cx);
                    }))
                    .child(
                        svg()
                            .path(CHEVRON_DOWN_ICON)
                            .size(px(15.0))
                            .text_color(c.dialog_body)
                            .debug_selector(|| "document-find-next-icon".to_owned()),
                    ),
            )
            .child(
                div()
                    .id("document-find-close")
                    .size(px(26.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .bg(c.dialog_secondary_button_bg)
                    .text_color(c.dialog_body)
                    .border(px(1.0))
                    .border_color(if state.keyboard_target == FindKeyboardTarget::Close {
                        c.text_link
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .on_click(cx.listener(|editor, _event, window, cx| {
                        editor.focus_find_keyboard_target(FindKeyboardTarget::Close, window, cx);
                        editor.close_find_panel(window, cx);
                    }))
                    .child(
                        svg()
                            .path(CLOSE_ICON)
                            .size(px(15.0))
                            .text_color(c.dialog_body)
                            .debug_selector(|| "document-find-close-icon".to_owned()),
                    ),
            )
            .into_any_element();
        let replace_row = state.show_replace.then(|| {
            div()
                .h(px(34.0))
                .flex()
                .items_center()
                .gap(px(5.0))
                .child(
                    div()
                        .id("document-replace-input")
                        .debug_selector(|| "document-replace-input".to_owned())
                        .w(px(288.0))
                        .h(px(30.0))
                        .px(px(7.0))
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .rounded(px(5.0))
                        .border(px(d.dialog_border_width))
                        .border_color(
                            if state.keyboard_target == FindKeyboardTarget::Replacement {
                                c.text_link
                            } else {
                                c.dialog_border
                            },
                        )
                        .bg(c.code_language_input_bg)
                        .child(state.replacement.clone()),
                )
                .child(compact_button(
                    "document-replace-current",
                    strings.find_replace.clone(),
                    Editor::replace_current_find_match,
                ))
                .child(compact_button(
                    "document-replace-all",
                    strings.find_replace_all.clone(),
                    Editor::replace_all_find_matches,
                ))
                .into_any_element()
        });
        Some(
            div()
                .id("document-find-panel")
                .debug_selector(|| "document-find-panel".to_owned())
                .absolute()
                .top(px(top + 8.0))
                .right(px(12.0))
                .w(px(540.0))
                .max_w(relative(0.94))
                .track_focus(&state.focus_handle)
                .p(px(6.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .occlude()
                .bg(c.dialog_surface)
                .border(px(d.dialog_border_width))
                .border_color(c.dialog_border)
                .rounded(px(10.0))
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(find_row)
                .children(replace_row)
                .into_any_element(),
        )
    }
}
