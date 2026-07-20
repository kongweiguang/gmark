// @author kongweiguang

use super::*;

pub(super) fn selected_available_index(
    commands: &[SlashCommand],
    current: usize,
    direction: i32,
    context: EditingContext,
) -> Option<usize> {
    if commands.is_empty() {
        return None;
    }
    let len = commands.len();
    (1..=len).find_map(|distance| {
        let index = if direction < 0 {
            (current + len - distance % len) % len
        } else {
            (current + distance) % len
        };
        commands[index].is_available(context).then_some(index)
    })
}

pub(super) fn boundary_available_index(
    commands: &[SlashCommand],
    from_end: bool,
    context: EditingContext,
) -> Option<usize> {
    if from_end {
        commands
            .iter()
            .rposition(|command| command.is_available(context))
    } else {
        commands
            .iter()
            .position(|command| command.is_available(context))
    }
}

/// Maps a command index to the rendered scroll child, accounting for group headings.
pub(super) fn slash_menu_child_index(state: &SlashMenuState, target: usize) -> usize {
    let mut child_index = 0;
    let mut previous_category = None;
    for (index, command) in state.filtered.iter().enumerate() {
        if index == 0 && state.recent_count > 0 {
            child_index += 1;
        }
        if index < state.recent_count {
            if index == target {
                return child_index;
            }
            child_index += 1;
            continue;
        }
        let category = command.descriptor().category;
        if previous_category != Some(category) {
            previous_category = Some(category);
            child_index += 1;
        }
        if index == target {
            return child_index;
        }
        child_index += 1;
    }
    child_index
}

impl Block {
    pub(crate) fn dismiss_slash_menu(&mut self) -> bool {
        let Some(state) = self.slash_menu.take() else {
            return false;
        };
        self.slash_menu_dismissed_query = Some(state.query);
        true
    }

    pub(crate) fn editing_command_context(&self) -> EditingContext {
        let block = if self.is_table_cell() {
            EditingBlockContext::TableCell
        } else {
            match self.kind() {
                BlockKind::Paragraph
                | BlockKind::Heading { .. }
                | BlockKind::BulletedListItem
                | BlockKind::NumberedListItem
                | BlockKind::TaskListItem { .. }
                | BlockKind::Quote
                | BlockKind::Callout(_)
                | BlockKind::FootnoteDefinition => EditingBlockContext::RichText,
                BlockKind::RawMarkdown | BlockKind::Comment | BlockKind::HtmlBlock => {
                    EditingBlockContext::Raw
                }
                BlockKind::CodeBlock { .. } => EditingBlockContext::Code,
                BlockKind::MathBlock | BlockKind::MermaidBlock => EditingBlockContext::Math,
                BlockKind::Table | BlockKind::Separator => EditingBlockContext::Structural,
            }
        };
        let selection = if self.editor_selection_range.is_some() {
            EditingSelectionContext::AcrossBlocks
        } else if self.selected_range.is_empty() {
            EditingSelectionContext::None
        } else {
            EditingSelectionContext::WithinBlock
        };
        EditingContext {
            view_mode: if self.show_source_line_numbers() || self.compact_source_host() {
                EditingViewMode::Source
            } else {
                EditingViewMode::Rendered
            },
            block,
            selection,
            read_only: self.is_read_only(),
            sibling_index: self.structural_sibling_index,
            sibling_count: self.structural_sibling_count,
        }
    }

    pub(crate) fn supports_slash_commands(&self) -> bool {
        !self.is_read_only()
            && !self.uses_raw_text_editing()
            && !self.is_table_cell()
            && matches!(
                self.kind(),
                BlockKind::Paragraph
                    | BlockKind::Heading { .. }
                    | BlockKind::BulletedListItem
                    | BlockKind::NumberedListItem
                    | BlockKind::TaskListItem { .. }
                    | BlockKind::Quote
                    | BlockKind::Callout(_)
            )
    }

    fn current_slash_query(&self) -> Option<(std::ops::Range<usize>, &str)> {
        if !self.supports_slash_commands()
            || self.editor_selection_range.is_some()
            || !self.selected_range.is_empty()
            || self.marked_range.is_some()
        {
            return None;
        }
        let text = self.display_text();
        let cursor = self.selected_range.end;
        let before_cursor = text.get(..cursor)?;
        let slash = before_cursor.rfind('/')?;
        if slash > 0
            && !before_cursor[..slash]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace)
        {
            return None;
        }
        let query = &before_cursor[slash + 1..];
        if query.len() > MAX_SLASH_QUERY_BYTES || query.contains(['\n', '\r']) {
            return None;
        }
        Some((slash..cursor, query))
    }

    pub(crate) fn refresh_slash_menu(&mut self, cx: &App) {
        if self
            .slash_menu
            .as_ref()
            .is_some_and(|state| state.structural_revision != self.structural_context_revision)
        {
            self.slash_menu = None;
            self.slash_menu_dismissed_query = None;
        }
        if let Some(state) = self.slash_menu.as_ref()
            && let Some(expected) = state.programmatic_text.as_deref()
        {
            let still_valid = !self.is_read_only()
                && (state.programmatic_allow_raw || !self.uses_raw_text_editing())
                && !self.is_table_cell()
                && self.marked_range.is_none()
                && self.selected_range.is_empty()
                && self.cursor_offset() == state.trigger_range.end
                && self.display_text() == expected;
            if still_valid {
                return;
            }
            self.slash_menu = None;
        }
        let Some((trigger_range, query)) = self
            .current_slash_query()
            .map(|(range, query)| (range, query.to_owned()))
        else {
            self.slash_menu = None;
            self.slash_menu_dismissed_query = None;
            return;
        };
        if self.slash_menu_dismissed_query.as_deref() == Some(&query) {
            self.slash_menu = None;
            return;
        }
        self.slash_menu_dismissed_query = None;
        if self
            .slash_menu
            .as_ref()
            .is_some_and(|state| state.query == query && state.trigger_range == trigger_range)
        {
            return;
        }
        let (filtered, recent_count) = if query.trim().is_empty() {
            let recent = EditingCommandHistory::recent(cx);
            let recent_count = recent.len();
            let commands = recent
                .iter()
                .copied()
                .chain(
                    SLASH_COMMANDS
                        .into_iter()
                        .filter(|command| !recent.contains(command)),
                )
                .collect::<Vec<_>>();
            (commands, recent_count)
        } else {
            (filter_slash_commands(&query), 0)
        };
        let selected =
            boundary_available_index(&filtered, false, self.editing_command_context()).unwrap_or(0);
        self.slash_menu = Some(SlashMenuState {
            filtered,
            query,
            selected,
            trigger_range,
            recent_count,
            programmatic_text: None,
            programmatic_allow_raw: false,
            structural_revision: self.structural_context_revision,
        });
        self.slash_menu_scroll_handle
            .set_offset(point(px(0.0), px(0.0)));
    }

    fn open_programmatic_command_menu(
        &mut self,
        commands: &[EditingCommandId],
        include_recent: bool,
        allow_raw_block: bool,
        cx: &App,
    ) {
        if self.is_read_only()
            || (!allow_raw_block && self.uses_raw_text_editing())
            || self.is_table_cell()
        {
            return;
        }
        let recent = include_recent
            .then(|| EditingCommandHistory::recent(cx))
            .unwrap_or_default();
        let recent_count = recent.len();
        let filtered = recent
            .iter()
            .copied()
            .chain(
                commands
                    .iter()
                    .copied()
                    .filter(|command| !recent.contains(command)),
            )
            .collect::<Vec<_>>();
        let cursor = self.cursor_offset();
        let selected =
            boundary_available_index(&filtered, false, self.editing_command_context()).unwrap_or(0);
        self.slash_menu = Some(SlashMenuState {
            query: String::new(),
            filtered,
            selected,
            trigger_range: cursor..cursor,
            recent_count,
            programmatic_text: Some(self.display_text().to_string()),
            programmatic_allow_raw: allow_raw_block,
            structural_revision: self.structural_context_revision,
        });
        self.slash_menu_scroll_handle
            .set_offset(point(px(0.0), px(0.0)));
        self.slash_menu_dismissed_query = None;
    }

    pub(crate) fn open_block_action_menu(&mut self, cx: &App) {
        self.open_programmatic_command_menu(&BLOCK_MENU_COMMANDS, false, true, cx);
    }

    #[cfg(test)]
    pub(crate) fn open_insert_command_menu(&mut self, cx: &App) {
        self.open_programmatic_command_menu(&SLASH_COMMANDS, true, false, cx);
    }

    pub(crate) fn render_block_gutter(
        &self,
        focused: bool,
        group: SharedString,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.is_read_only()
            || self.is_table_cell()
            || self.show_source_line_numbers()
            || self.compact_source_host()
        {
            return None;
        }
        let c = &theme.colors;
        let actions_label: SharedString = strings
            .slash_commands
            .get("group_block")
            .cloned()
            .unwrap_or_else(|| "Block Actions".to_owned())
            .into();
        let button = |id: &'static str,
                      icon: &'static str,
                      tooltip: SharedString,
                      on_click: Box<BlockGutterAction>| {
            div()
                .id(id)
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.0))
                .text_color(c.dialog_muted)
                .hover(|button| button.bg(c.dialog_secondary_button_hover))
                .active(|button| button.opacity(0.82))
                .cursor_pointer()
                .tooltip(move |_window, cx| crate::ui::ui_tooltip(tooltip.clone(), cx))
                .child(
                    svg()
                        .path(icon)
                        .size(px(14.0))
                        .text_color(c.dialog_muted)
                        .debug_selector(move || format!("{id}-icon")),
                )
                .on_click(cx.listener(move |block, _event, window, cx| {
                    on_click(block, window, cx);
                }))
        };
        let drag_payload = BlockDragPayload {
            source: cx.entity().entity_id(),
        };
        let actions_button = button(
            "block-context-actions",
            "icon/ui/more-horizontal.svg",
            actions_label,
            Box::new(|block, window, cx| {
                block.focus_handle.focus(window);
                block.open_block_action_menu(cx);
                cx.notify();
            }),
        )
        .debug_selector(move || {
            if focused {
                "focused-block-context-actions".to_owned()
            } else {
                "block-context-actions".to_owned()
            }
        })
        .cursor_move()
        .on_drag(drag_payload, |_payload, _position, _window, cx| {
            cx.new(|_| BlockDragPreview)
        });
        Some(
            div()
                .id("block-context-gutter")
                .debug_selector(|| "block-context-gutter".to_owned())
                .absolute()
                .left_0()
                .top(px(2.0))
                .h(px(28.0))
                .px(px(2.0))
                .flex()
                .items_center()
                // 单一“…”入口留在内容列内部；小窗口不会裁剪，所有块也保持左对齐。
                .opacity(if focused { 1.0 } else { 0.0 })
                .group_hover(group, |gutter| gutter.opacity(1.0))
                .child(actions_button)
                .into_any_element(),
        )
    }

    pub(crate) fn handle_slash_menu_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        self.refresh_slash_menu(cx);
        let context = self.editing_command_context();
        let Some(state) = self.slash_menu.as_mut() else {
            return false;
        };
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.platform || modifiers.alt || modifiers.function {
            return false;
        }
        let navigated = match event.keystroke.key.as_str() {
            "up" if !state.filtered.is_empty() => {
                state.selected =
                    selected_available_index(&state.filtered, state.selected, -1, context)
                        .unwrap_or(state.selected);
                true
            }
            "down" if !state.filtered.is_empty() => {
                state.selected =
                    selected_available_index(&state.filtered, state.selected, 1, context)
                        .unwrap_or(state.selected);
                true
            }
            "home" if !state.filtered.is_empty() => {
                state.selected = boundary_available_index(&state.filtered, false, context)
                    .unwrap_or(state.selected);
                true
            }
            "end" if !state.filtered.is_empty() => {
                state.selected = boundary_available_index(&state.filtered, true, context)
                    .unwrap_or(state.selected);
                true
            }
            "enter" | "tab" => {
                let command = state
                    .filtered
                    .get(state.selected)
                    .copied()
                    .filter(|command| command.is_available(context));
                if let Some(command) = command {
                    let trigger_range = state.trigger_range.clone();
                    self.choose_slash_command(command, trigger_range, cx);
                }
                false
            }
            "escape" => {
                self.slash_menu_dismissed_query = Some(state.query.clone());
                self.slash_menu = None;
                false
            }
            _ => return false,
        };
        if navigated && let Some(state) = self.slash_menu.as_ref() {
            self.slash_menu_scroll_handle
                .scroll_to_item(slash_menu_child_index(state, state.selected));
        }
        cx.notify();
        true
    }

    /// GPUI may dispatch Enter as the `Newline` action before the raw key-down
    /// listener. Keep Slash selection on the same command path in both cases.
    pub(crate) fn commit_slash_menu(&mut self, cx: &mut Context<Self>) -> bool {
        self.refresh_slash_menu(cx);
        let context = self.editing_command_context();
        let Some((command, trigger_range)) = self.slash_menu.as_ref().and_then(|state| {
            state
                .filtered
                .get(state.selected)
                .copied()
                .filter(|command| command.is_available(context))
                .map(|command| (command, state.trigger_range.clone()))
        }) else {
            return false;
        };
        self.choose_slash_command(command, trigger_range, cx);
        true
    }

    fn choose_slash_command(
        &mut self,
        command: SlashCommand,
        trigger_range: std::ops::Range<usize>,
        cx: &mut Context<Self>,
    ) {
        self.slash_menu = None;
        self.slash_menu_dismissed_query = None;
        cx.emit(BlockEvent::RequestSlashCommand {
            command,
            trigger_range,
        });
        cx.notify();
    }

    pub(crate) fn render_slash_menu(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.slash_menu.as_ref()?;
        let text_bounds = self.last_bounds?;
        let anchor = self.active_range_or_cursor_bounds().unwrap_or(text_bounds);
        let placement =
            slash_menu_placement(anchor, viewport, slash_menu_estimated_height(state, theme));
        let origin_left = f32::from(text_bounds.left()) - theme.dimensions.block_padding_x;
        let origin_top = f32::from(text_bounds.top()) - theme.dimensions.block_padding_y;
        let context = self.editing_command_context();
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let panel = div()
            .id("slash-command-menu")
            .debug_selector(|| "slash-command-menu".to_owned())
            .absolute()
            .left(px(placement.left - origin_left))
            .top(px(placement.top - origin_top))
            .w(px(placement.width))
            .max_h(px(placement.max_height.max(1.0)))
            .overflow_y_scroll()
            .track_scroll(&self.slash_menu_scroll_handle)
            .scrollbar_width(px(0.0))
            .p(px(d.menu_panel_padding))
            .flex()
            .flex_col()
            .gap(px(d.menu_panel_gap))
            .occlude()
            .bg(c.dialog_surface)
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .rounded(px(d.menu_panel_radius.min(8.0)))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            });

        if state.filtered.is_empty() {
            return Some(
                deferred(
                    panel.child(
                        div()
                            .h(px(d.menu_item_height))
                            .px(px(d.menu_item_padding_x))
                            .flex()
                            .items_center()
                            .text_size(px(d.menu_text_size))
                            .text_color(c.dialog_muted)
                            .child(
                                strings
                                    .slash_commands
                                    .get("no_results")
                                    .cloned()
                                    .unwrap_or_else(|| "No matching block type".to_owned()),
                            ),
                    ),
                )
                .with_priority(10)
                .into_any_element(),
            );
        }

        let mut previous_category = None;
        let mut items = Vec::new();
        for (index, command) in state.filtered.iter().enumerate() {
            let command = *command;
            let descriptor = command.descriptor();
            let available = command.is_available(context);
            if index == 0 && state.recent_count > 0 {
                items.push(
                    div()
                        .h(px(24.0))
                        .px(px(d.menu_item_padding_x))
                        .flex()
                        .items_center()
                        .text_size(px((d.menu_text_size - 2.0).max(10.0)))
                        .text_color(c.dialog_muted)
                        .child(
                            strings
                                .slash_commands
                                .get("group_recent")
                                .cloned()
                                .unwrap_or_else(|| "Recent".to_owned()),
                        )
                        .into_any_element(),
                );
            }
            if index >= state.recent_count && previous_category != Some(descriptor.category) {
                previous_category = Some(descriptor.category);
                let key = match descriptor.category {
                    EditingCommandCategory::Transform => "group_transform",
                    EditingCommandCategory::Insert => "group_insert",
                    EditingCommandCategory::Block => "group_block",
                    EditingCommandCategory::Inline => continue,
                };
                items.push(
                    div()
                        .h(px(24.0))
                        .px(px(d.menu_item_padding_x))
                        .flex()
                        .items_center()
                        .text_size(px((d.menu_text_size - 2.0).max(10.0)))
                        .text_color(c.dialog_muted)
                        .child(
                            strings
                                .slash_commands
                                .get(key)
                                .cloned()
                                .unwrap_or_else(|| key.to_owned()),
                        )
                        .into_any_element(),
                );
            }
            let selected = available && index == state.selected;
            let trigger_range = state.trigger_range.clone();
            let row = div()
                .id(SharedString::from(format!("slash-command-{command:?}")))
                .h(px(d.menu_item_height.max(32.0)))
                .px(px(d.menu_item_padding_x))
                .flex()
                .items_center()
                .gap(px(10.0))
                .rounded(px(d.menu_item_radius))
                .opacity(if available { 1.0 } else { 0.45 })
                .bg(if selected {
                    c.dialog_secondary_button_hover
                } else {
                    c.dialog_surface
                })
                .when(available, |row| {
                    row.hover(|this| this.bg(c.dialog_secondary_button_hover))
                        .on_hover(cx.listener(move |block, hovered: &bool, _window, cx| {
                            if *hovered
                                && let Some(state) = block.slash_menu.as_mut()
                                && state.selected != index
                            {
                                state.selected = index;
                                cx.notify();
                            }
                        }))
                        .cursor_pointer()
                        .on_click(cx.listener(move |block, _event, _window, cx| {
                            block.choose_slash_command(command, trigger_range.clone(), cx);
                        }))
                });
            items.push(
                row.child(
                    div()
                        .debug_selector(move || {
                            format!(
                                "slash-command-icon-{}",
                                command.descriptor().localization_key
                            )
                        })
                        .w(px(24.0))
                        .h(px(24.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(c.dialog_muted)
                        .child(svg().path(descriptor.icon_path).size(px(16.0))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .truncate()
                        .text_size(px(d.menu_text_size))
                        .font_weight(t.dialog_body_weight.to_font_weight())
                        .text_color(c.dialog_secondary_button_text)
                        .child(
                            strings
                                .slash_commands
                                .get(descriptor.localization_key)
                                .cloned()
                                .unwrap_or_else(|| descriptor.localization_key.to_owned()),
                        ),
                )
                .into_any_element(),
            );
        }
        Some(
            deferred(panel.children(items))
                .with_priority(10)
                .into_any_element(),
        )
    }
}
