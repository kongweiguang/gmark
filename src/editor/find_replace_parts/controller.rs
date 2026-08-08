// @author kongweiguang

use super::*;
use crate::theme::workbench::SurfaceKind;
use crate::ui::visual_preferences::VisualPreferencesManager;

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
        let Some(state) = self.find_panel.take() else {
            return;
        };
        let restore = state.restore_focus;
        if let Some(snapshot) = state.view_state_before_expand {
            self.ensure_markdown_view_state();
            self.view_state
                .replace_tab_state(self.tabs.active_id(), snapshot);
            self.render_row_cache = None;
            self.prev_render_window = None;
            self.row_stride_cache.clear();
        }
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
        let visible_offset = state
            .match_metadata
            .get(state.selected)
            .map(|metadata| metadata.visible.start);
        let query = state.query.clone();
        let fold_sources = visible_offset
            .map(|offset| {
                gmark_markdown::parse_markdown(&self.source_document.text())
                    .visible_text_projection()
                    .folds_containing(offset)
                    .map(|fold| fold.source)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let view_state_snapshot = self.view_state.state_for_tab(self.tabs.active_id());
        let mut heading_keys_to_expand = Vec::new();
        let mut callout_keys_to_expand = Vec::new();
        if !fold_sources.is_empty() {
            let source_ranges = self.build_source_target_mappings_with_block_ranges(cx).1;
            for visible in self.document.flatten_visible_blocks() {
                let Some(source_range) = source_ranges.get(&visible.entity.entity_id()).cloned()
                else {
                    continue;
                };
                let (key, heading) = visible.entity.read_with(cx, |block, _cx| {
                    (
                        block
                            .presentation_fold_key
                            .as_ref()
                            .map(ToString::to_string),
                        block.presentation_fold_heading,
                    )
                });
                let Some(key) = key else { continue };
                let matches_fold = fold_sources.iter().any(|source| {
                    source.start == source_range.start && source.end == source_range.end
                });
                if !matches_fold {
                    continue;
                }
                let collapsed = view_state_snapshot.as_ref().is_some_and(|state| {
                    if heading {
                        state.collapsed_headings.get(&key).copied().unwrap_or(false)
                    } else {
                        state.collapsed_callouts.get(&key).copied().unwrap_or(false)
                    }
                });
                if collapsed {
                    if heading {
                        heading_keys_to_expand.push(key);
                    } else {
                        callout_keys_to_expand.push(key);
                    }
                }
            }
        }
        if !heading_keys_to_expand.is_empty() || !callout_keys_to_expand.is_empty() {
            if let Some(state) = self.find_panel.as_mut()
                && state.view_state_before_expand.is_none()
            {
                state.view_state_before_expand = view_state_snapshot;
            }
            self.ensure_markdown_view_state();
            self.view_state
                .update_tab(self.tabs.active_id(), |view_state| {
                    for key in heading_keys_to_expand {
                        view_state.collapsed_headings.remove(&key);
                    }
                    for key in callout_keys_to_expand {
                        view_state.collapsed_callouts.remove(&key);
                    }
                });
            self.render_row_cache = None;
            self.prev_render_window = None;
            self.row_stride_cache.clear();
        }
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
        let replaceable = state
            .match_metadata
            .get(state.selected)
            .is_some_and(|metadata| {
                metadata.replaceability == gmark_markdown::Replaceability::Direct
            });
        if !replaceable {
            return;
        }
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
        for (index, range) in state.matches.iter().enumerate() {
            if !state.match_metadata.get(index).is_some_and(|metadata| {
                metadata.replaceability == gmark_markdown::Replaceability::Direct
            }) {
                continue;
            }
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

    pub(in crate::editor) fn render_find_panel(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        top: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.find_panel.as_ref()?;
        let c = &theme.colors;
        let visual_preferences = cx
            .try_global::<VisualPreferencesManager>()
            .map(VisualPreferencesManager::current)
            .unwrap_or_default();
        let palette = &c.workbench;
        let material = palette.material(SurfaceKind::Glass, visual_preferences);
        let solid_material = palette.material(SurfaceKind::Solid, visual_preferences);
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
                        palette.focus_ring
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .bg(if active {
                        palette.control_hover
                    } else {
                        palette.control_surface
                    })
                    .hover(|this| this.bg(palette.control_hover))
                    .cursor_pointer()
                    .text_color(palette.text_primary)
                    .child(
                        svg()
                            .path(icon)
                            .size(px(15.0))
                            .text_color(if active {
                                palette.accent
                            } else {
                                palette.text_primary
                            })
                            .debug_selector(move || format!("{id}-icon")),
                    )
                    .children(
                        (state.tooltip_visible == Some(id))
                            .then(|| render_find_tooltip(label, theme, visual_preferences)),
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
                    .bg(palette.control_surface)
                    .hover(|this| this.bg(palette.control_hover))
                    .cursor_pointer()
                    .text_size(px(t.dialog_button_size))
                    .text_color(palette.text_primary)
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
                        palette.focus_ring
                    } else {
                        material.border
                    })
                    .bg(solid_material.background)
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
                        palette.danger
                    } else {
                        palette.text_secondary
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
                    .bg(palette.control_surface)
                    .text_color(palette.text_primary)
                    .border(px(1.0))
                    .border_color(if state.keyboard_target == FindKeyboardTarget::Previous {
                        palette.focus_ring
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .hover(|this| this.bg(palette.control_hover))
                    .cursor_pointer()
                    .on_click(cx.listener(|editor, _event, window, cx| {
                        editor.navigate_find_match(-1, window, cx);
                        editor.focus_find_keyboard_target(FindKeyboardTarget::Previous, window, cx);
                    }))
                    .child(
                        svg()
                            .path(CHEVRON_UP_ICON)
                            .size(px(15.0))
                            .text_color(palette.text_primary)
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
                    .bg(palette.control_surface)
                    .text_color(palette.text_primary)
                    .border(px(1.0))
                    .border_color(if state.keyboard_target == FindKeyboardTarget::Next {
                        palette.focus_ring
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .hover(|this| this.bg(palette.control_hover))
                    .cursor_pointer()
                    .on_click(cx.listener(|editor, _event, window, cx| {
                        editor.navigate_find_match(1, window, cx);
                        editor.focus_find_keyboard_target(FindKeyboardTarget::Next, window, cx);
                    }))
                    .child(
                        svg()
                            .path(CHEVRON_DOWN_ICON)
                            .size(px(15.0))
                            .text_color(palette.text_primary)
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
                    .bg(palette.control_surface)
                    .text_color(palette.text_primary)
                    .border(px(1.0))
                    .border_color(if state.keyboard_target == FindKeyboardTarget::Close {
                        palette.focus_ring
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .hover(|this| this.bg(palette.control_hover))
                    .cursor_pointer()
                    .on_click(cx.listener(|editor, _event, window, cx| {
                        editor.focus_find_keyboard_target(FindKeyboardTarget::Close, window, cx);
                        editor.close_find_panel(window, cx);
                    }))
                    .child(
                        svg()
                            .path(CLOSE_ICON)
                            .size(px(15.0))
                            .text_color(palette.text_primary)
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
                                palette.focus_ring
                            } else {
                                material.border
                            },
                        )
                        .bg(solid_material.background)
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
                .bg(material.background)
                .border(px(d.dialog_border_width))
                .border_color(material.border)
                .rounded(px(14.0))
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
