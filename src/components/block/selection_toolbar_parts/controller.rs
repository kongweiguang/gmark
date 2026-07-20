// @author kongweiguang

use super::*;

impl Block {
    pub(crate) fn refresh_selection_toolbar(&mut self) {
        let Some(range) = self.selection_toolbar_range() else {
            self.selection_toolbar_dismissed_range = None;
            self.selection_toolbar_keyboard_active = false;
            self.selection_toolbar_overflow_open = false;
            self.selection_toolbar_type_menu_open = false;
            return;
        };
        if self.selection_toolbar_dismissed_range.as_ref() != Some(&range) {
            self.selection_toolbar_dismissed_range = None;
        }
    }

    pub(crate) fn selection_toolbar_visible(&self) -> bool {
        let Some(range) = self.selection_toolbar_range() else {
            return false;
        };
        self.selection_toolbar_dismissed_range.as_ref() != Some(&range)
    }

    pub(crate) fn handle_selection_toolbar_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.refresh_selection_toolbar();
        if !self.selection_toolbar_visible() {
            return false;
        }
        let modifiers = event.keystroke.modifiers;
        if event.keystroke.key.eq_ignore_ascii_case("f10")
            && modifiers.alt
            && !modifiers.control
            && !modifiers.platform
            && !modifiers.shift
        {
            self.selection_toolbar_keyboard_active = true;
            self.selection_toolbar_keyboard_index = 0;
            cx.notify();
            return true;
        }
        if self.selection_toolbar_keyboard_active {
            let commands = ToolbarCommand::PRIMARY
                .into_iter()
                .filter(|command| {
                    (*command != ToolbarCommand::BlockType || !self.is_table_cell())
                        && self.selection_toolbar_command_available(*command)
                })
                .collect::<Vec<_>>();
            if commands.is_empty() {
                self.selection_toolbar_keyboard_active = false;
                return false;
            }
            self.selection_toolbar_keyboard_index = self
                .selection_toolbar_keyboard_index
                .min(commands.len() - 1);
            match event.keystroke.key.as_str() {
                "left" | "up" => {
                    self.selection_toolbar_keyboard_index =
                        if self.selection_toolbar_keyboard_index == 0 {
                            commands.len() - 1
                        } else {
                            self.selection_toolbar_keyboard_index - 1
                        };
                }
                "right" | "down" => {
                    self.selection_toolbar_keyboard_index =
                        (self.selection_toolbar_keyboard_index + 1) % commands.len();
                }
                "home" => self.selection_toolbar_keyboard_index = 0,
                "end" => self.selection_toolbar_keyboard_index = commands.len() - 1,
                "enter" | "space" => self.invoke_selection_toolbar_command(
                    commands[self.selection_toolbar_keyboard_index],
                    window,
                    cx,
                ),
                "escape" => self.selection_toolbar_keyboard_active = false,
                _ => return false,
            }
            cx.notify();
            return true;
        }
        if event.keystroke.key != "escape" {
            return false;
        }
        let Some(range) = self.selection_toolbar_range() else {
            return false;
        };
        self.selection_toolbar_dismissed_range = Some(range);
        self.selection_toolbar_keyboard_active = false;
        self.selection_toolbar_overflow_open = false;
        self.selection_toolbar_type_menu_open = false;
        cx.notify();
        true
    }

    fn selection_toolbar_command_active(&self, command: ToolbarCommand) -> bool {
        let Some(range) = self.selection_toolbar_range() else {
            return false;
        };
        match command {
            ToolbarCommand::BlockType => self.selection_toolbar_type_menu_open,
            ToolbarCommand::Bold => self
                .record
                .title
                .selection_has_style(range, StyleFlag::Bold),
            ToolbarCommand::Italic => self
                .record
                .title
                .selection_has_style(range, StyleFlag::Italic),
            ToolbarCommand::Strikethrough => self
                .record
                .title
                .selection_has_style(range, StyleFlag::Strikethrough),
            ToolbarCommand::Code => self
                .record
                .title
                .selection_has_style(range, StyleFlag::Code),
            ToolbarCommand::Underline => self
                .record
                .title
                .selection_has_style(range, StyleFlag::Underline),
            ToolbarCommand::Link => self.record.title.selection_has_link(range),
            ToolbarCommand::Overflow => self.selection_toolbar_overflow_open,
            ToolbarCommand::ClearFormatting => false,
        }
    }

    pub(super) fn selection_toolbar_command_available(&self, command: ToolbarCommand) -> bool {
        if command == ToolbarCommand::BlockType {
            return self.editor_selection_range.is_none()
                && !self.is_table_cell()
                && EditingCommandId::for_block_kind(&self.kind()).is_some();
        }
        let Some(id) = command.editing_command() else {
            return true;
        };
        if !INLINE_COMMANDS.contains(&id) {
            return false;
        }
        id.is_available(self.editing_command_context())
    }

    pub(super) fn apply_selection_toolbar_command(
        &mut self,
        command: ToolbarCommand,
        cx: &mut Context<Self>,
    ) {
        match command {
            ToolbarCommand::BlockType => {
                self.selection_toolbar_type_menu_open = !self.selection_toolbar_type_menu_open;
                self.selection_toolbar_overflow_open = false;
                cx.notify();
                return;
            }
            ToolbarCommand::Bold => self.toggle_inline_format(InlineFormat::Bold, cx),
            ToolbarCommand::Italic => self.toggle_inline_format(InlineFormat::Italic, cx),
            ToolbarCommand::Strikethrough => {
                self.toggle_inline_format(InlineFormat::Strikethrough, cx)
            }
            ToolbarCommand::Code => self.toggle_inline_format(InlineFormat::Code, cx),
            ToolbarCommand::Link => self.toggle_inline_link(cx),
            ToolbarCommand::Underline => self.toggle_inline_format(InlineFormat::Underline, cx),
            ToolbarCommand::ClearFormatting => self.clear_inline_formatting(cx),
            ToolbarCommand::Overflow => {
                self.selection_toolbar_overflow_open = !self.selection_toolbar_overflow_open;
                self.selection_toolbar_type_menu_open = false;
                cx.notify();
                return;
            }
        }
        self.selection_toolbar_overflow_open = false;
        self.selection_toolbar_type_menu_open = false;
    }

    fn invoke_selection_toolbar_command(
        &mut self,
        command: ToolbarCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor_selection_range.is_some()
            && let Some(command_id) = command.editing_command()
            && command_id.is_available(EditingContext {
                selection: EditingSelectionContext::AcrossBlocks,
                ..self.editing_command_context()
            })
        {
            self.selection_toolbar_overflow_open = false;
            self.selection_toolbar_type_menu_open = false;
            cx.emit(BlockEvent::RequestEditingCommand {
                command: command_id,
            });
            cx.notify();
            return;
        }
        if command == ToolbarCommand::Link {
            self.open_selection_link_editor(window, cx);
        } else {
            self.apply_selection_toolbar_command(command, cx);
        }
    }

    pub(super) fn open_selection_link_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(range) = self.selection_toolbar_range() else {
            return;
        };
        let target = self
            .record
            .title
            .selection_link_destination(range.clone())
            .unwrap_or_default();
        let had_target = self.record.title.selection_has_link(range.clone());
        let input = cx.new(|cx| {
            let mut input = Block::with_record(cx, BlockRecord::paragraph(target));
            input.set_compact_source_host();
            input.set_input_placeholder("https://example.com");
            input.set_host_submit_enabled(true);
            input
        });
        let parent = cx.entity().downgrade();
        input.update(cx, move |input, _cx| {
            input.set_host_action_handler(move |action, window, cx| match action {
                BlockHostAction::Submit(destination) => {
                    let destination = {
                        let destination = destination.trim();
                        (!destination.is_empty()).then(|| destination.to_owned())
                    };
                    let _ = parent.update(cx, |block, cx| {
                        block.commit_selection_link_destination(destination, window, cx)
                    });
                }
                BlockHostAction::DismissTransientUi => {
                    let _ = parent.update(cx, |block, cx| {
                        block.cancel_selection_link_editor(window, cx)
                    });
                }
                _ => {}
            });
            input.focus_handle.focus(window);
        });
        self.selection_toolbar_link_input = Some(input);
        self.selection_toolbar_link_range = Some(range);
        self.selection_toolbar_link_had_target = had_target;
        self.selection_toolbar_overflow_open = false;
        self.selection_toolbar_type_menu_open = false;
        cx.notify();
    }

    pub(super) fn commit_selection_link_editor(
        &mut self,
        remove: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let destination = if remove {
            None
        } else {
            self.selection_toolbar_link_input
                .as_ref()
                .map(|input| input.read(cx).display_text().trim().to_owned())
                .filter(|target| !target.is_empty())
        };
        self.commit_selection_link_destination(destination, window, cx);
    }

    fn commit_selection_link_destination(
        &mut self,
        destination: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(range) = self.selection_toolbar_link_range.clone() else {
            return;
        };
        let mut next_title = self.record.title.clone();
        if next_title.set_inline_link_destination(range.clone(), destination) {
            self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
            self.apply_title_edit(
                next_title,
                range.end,
                None,
                Some(range),
                Some(self.selection_reversed),
                false,
                cx,
            );
        }
        self.close_selection_link_editor(window, cx);
    }

    fn cancel_selection_link_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_selection_link_editor(window, cx);
    }

    fn close_selection_link_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selection_toolbar_link_input = None;
        self.selection_toolbar_link_range = None;
        self.selection_toolbar_link_had_target = false;
        self.focus_handle.focus(window);
        cx.notify();
    }

    fn render_selection_toolbar_button(
        &self,
        command: ToolbarCommand,
        show_block_type_label: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let strings = cx.global::<I18nManager>().strings();
        let tooltip_label: SharedString = if command == ToolbarCommand::BlockType {
            EditingCommandId::for_block_kind(&self.kind())
                .and_then(|command| {
                    strings
                        .slash_commands
                        .get(command.descriptor().localization_key)
                        .cloned()
                })
                .unwrap_or_else(|| command.label(strings))
                .into()
        } else {
            command.label(strings).into()
        };
        let active = self.selection_toolbar_command_active(command);
        let available = self.selection_toolbar_command_available(command);
        let keyboard_focused = self.selection_toolbar_keyboard_active
            && ToolbarCommand::PRIMARY
                .into_iter()
                .filter(|candidate| {
                    (*candidate != ToolbarCommand::BlockType || !self.is_table_cell())
                        && self.selection_toolbar_command_available(*candidate)
                })
                .nth(self.selection_toolbar_keyboard_index)
                == Some(command);
        let symbol: AnyElement = match command {
            ToolbarCommand::BlockType => {
                let descriptor = EditingCommandId::for_block_kind(&self.kind())
                    .unwrap_or(EditingCommandId::Paragraph)
                    .descriptor();
                div()
                    .flex()
                    .items_center()
                    .gap(px(if show_block_type_label { 6.0 } else { 2.0 }))
                    .child(
                        svg()
                            .path(descriptor.icon_path)
                            .size(px(15.0))
                            .text_color(c.dialog_body),
                    )
                    .when(show_block_type_label, |content| {
                        content.child(
                            div()
                                .min_w(px(0.0))
                                .max_w(px(62.0))
                                .overflow_hidden()
                                .truncate()
                                .text_size(px(12.0))
                                .text_color(c.dialog_body)
                                .child(tooltip_label.clone()),
                        )
                    })
                    .child(
                        svg()
                            .path("icon/ui/chevron-down.svg")
                            .size(px(11.0))
                            .text_color(c.dialog_muted),
                    )
                    .into_any_element()
            }
            ToolbarCommand::Code | ToolbarCommand::Link | ToolbarCommand::Overflow => svg()
                .path(match command {
                    ToolbarCommand::Code => CODE_ICON,
                    ToolbarCommand::Link => LINK_ICON,
                    ToolbarCommand::Overflow => MORE_ICON,
                    _ => unreachable!(),
                })
                .size(px(15.0))
                .text_color(if active { c.text_link } else { c.dialog_body })
                .into_any_element(),
            _ => {
                let symbol = div()
                    .text_size(px(13.0))
                    .text_color(if active { c.text_link } else { c.dialog_body })
                    .child(command.symbol());
                match command {
                    ToolbarCommand::Bold => symbol.font_weight(FontWeight::BOLD),
                    ToolbarCommand::Italic => symbol.italic(),
                    ToolbarCommand::Strikethrough => symbol.line_through(),
                    ToolbarCommand::Underline => symbol.underline(),
                    ToolbarCommand::ClearFormatting => symbol,
                    ToolbarCommand::BlockType => symbol,
                    _ => symbol,
                }
                .into_any_element()
            }
        };
        div()
            .id(SharedString::from(format!(
                "selection-toolbar-{}",
                command.id()
            )))
            .debug_selector(move || format!("selection-toolbar-{}", command.id()))
            .w(px(if command == ToolbarCommand::BlockType {
                if show_block_type_label { 106.0 } else { 42.0 }
            } else {
                28.0
            }))
            .h(px(28.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .border(px(1.0))
            .border_color(if keyboard_focused {
                c.text_link
            } else {
                c.dialog_surface
            })
            .bg(if active {
                c.dialog_secondary_button_hover
            } else {
                c.dialog_surface
            })
            .hover(|this| this.bg(c.dialog_secondary_button_hover))
            .active(|this| this.opacity(0.86))
            .opacity(if available { 1.0 } else { 0.45 })
            .when(available, |button| button.cursor_pointer())
            .tooltip(move |_window, cx| crate::ui::ui_tooltip(tooltip_label.clone(), cx))
            .child(symbol)
            .when(available, |button| {
                button.on_click(cx.listener(move |block, _event, window, cx| {
                    block.invoke_selection_toolbar_command(command, window, cx);
                }))
            })
            .into_any_element()
    }

    pub(crate) fn render_selection_toolbar(
        &self,
        theme: &Theme,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.selection_toolbar_visible() {
            return None;
        }
        let selection = self.active_range_or_cursor_bounds()?;
        let text_bounds = self.last_bounds?;
        let attached_surface_height = if self.selection_toolbar_type_menu_open {
            312.0
        } else if self.selection_toolbar_link_input.is_some() {
            42.0
        } else {
            0.0
        };
        let position =
            toolbar_window_position(selection, text_bounds, viewport, attached_surface_height);
        let d = &theme.dimensions;
        let c = &theme.colors;
        let toolbar_width = selection_toolbar_width(text_bounds, viewport);
        let show_block_type_label = toolbar_width >= TOOLBAR_EXPANDED_WIDTH;
        let viewport_height = f32::from(viewport.height);
        let (type_menu_above, type_menu_available_height) = attached_surface_placement(
            position,
            312.0,
            viewport_height,
            d.menu_bar_height,
            d.status_bar_height,
        );
        let (link_editor_above, _) = attached_surface_placement(
            position,
            42.0,
            viewport_height,
            d.menu_bar_height,
            d.status_bar_height,
        );
        let origin_left = f32::from(text_bounds.left()) - d.block_padding_x;
        let origin_top = f32::from(text_bounds.top()) - d.block_padding_y;
        let buttons = ToolbarCommand::PRIMARY
            .into_iter()
            .filter(|command| {
                *command != ToolbarCommand::BlockType
                    || (!self.is_table_cell() && self.editor_selection_range.is_none())
            })
            .map(|command| {
                self.render_selection_toolbar_button(command, show_block_type_label, theme, cx)
            })
            .collect::<Vec<_>>();
        let overflow_width = 32.0;
        let overflow_on_right =
            position.left + toolbar_width + TOOLBAR_GAP + overflow_width + VIEWPORT_INSET
                <= f32::from(viewport.width);
        let overflow_on_left = position.left >= VIEWPORT_INSET + overflow_width + TOOLBAR_GAP;
        let overflow = self.selection_toolbar_overflow_open.then(|| {
            let menu = div()
                .id("selection-toolbar-overflow-menu")
                .debug_selector(|| "selection-toolbar-overflow-menu".to_owned())
                .absolute()
                .top_0()
                .p(px(2.0))
                .bg(c.dialog_surface)
                .border(px(d.dialog_border_width))
                .border_color(c.dialog_border)
                .rounded(px(6.0))
                .shadow_md()
                .child(self.render_selection_toolbar_button(
                    ToolbarCommand::ClearFormatting,
                    false,
                    theme,
                    cx,
                ));
            if overflow_on_right {
                menu.left(px(toolbar_width + 4.0))
            } else if overflow_on_left {
                menu.right(px(toolbar_width + 4.0))
            } else if position.above {
                menu.bottom(px(TOOLBAR_HEIGHT + 4.0))
            } else {
                menu.top(px(TOOLBAR_HEIGHT + 4.0))
            }
        });
        let current_kind = self.kind();
        let type_menu = self.selection_toolbar_type_menu_open.then(|| {
            let type_menu_max_height = type_menu_available_height.clamp(1.0, 312.0);
            let items = TRANSFORM_COMMANDS
                .into_iter()
                .map(|command| {
                    let descriptor = command.descriptor();
                    let selected = EditingCommandId::for_block_kind(&current_kind) == Some(command);
                    let label = cx
                        .global::<I18nManager>()
                        .strings()
                        .slash_commands
                        .get(descriptor.localization_key)
                        .cloned()
                        .unwrap_or_else(|| descriptor.localization_key.to_owned());
                    div()
                        .id(SharedString::from(format!(
                            "selection-toolbar-block-type-{command:?}"
                        )))
                        .h(px(d.menu_item_height.max(30.0)))
                        .px(px(d.menu_item_padding_x))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .rounded(px(d.menu_item_radius))
                        .bg(if selected {
                            c.dialog_secondary_button_hover
                        } else {
                            c.dialog_surface
                        })
                        .hover(|item| item.bg(c.dialog_secondary_button_hover))
                        .cursor_pointer()
                        .child(svg().path(descriptor.icon_path).size(px(15.0)))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .text_size(px(d.menu_text_size))
                                .text_color(c.dialog_body)
                                .child(label),
                        )
                        .on_click(cx.listener(move |block, _event, _window, cx| {
                            block.selection_toolbar_type_menu_open = false;
                            cx.emit(BlockEvent::RequestEditingCommand { command });
                            cx.notify();
                        }))
                        .into_any_element()
                })
                .collect::<Vec<_>>();
            let menu = div()
                .id("selection-toolbar-block-type-menu")
                .debug_selector(|| "selection-toolbar-block-type-menu".to_owned())
                .absolute()
                .left_0()
                .w(px(188.0))
                .max_h(px(type_menu_max_height))
                .overflow_y_scroll()
                .p(px(d.menu_panel_padding))
                .flex()
                .flex_col()
                .gap(px(d.menu_panel_gap))
                .bg(c.dialog_surface)
                .border(px(d.dialog_border_width))
                .border_color(c.dialog_border)
                .rounded(px(d.menu_panel_radius.min(8.0)))
                .shadow_lg()
                .children(items);
            if type_menu_above {
                menu.bottom(px(TOOLBAR_HEIGHT + 4.0))
            } else {
                menu.top(px(TOOLBAR_HEIGHT + 4.0))
            }
        });
        let link_editor = self.selection_toolbar_link_input.as_ref().map(|input| {
            let strings = cx.global::<I18nManager>().strings();
            let apply_label = strings
                .slash_commands
                .get("apply_link")
                .cloned()
                .unwrap_or_else(|| "Apply".to_owned());
            let remove_label = strings
                .slash_commands
                .get("remove_link")
                .cloned()
                .unwrap_or_else(|| "Remove".to_owned());
            let input = input.clone();
            let popover_width =
                292.0_f32.min((f32::from(viewport.width) - 2.0 * VIEWPORT_INSET).max(1.0));
            let popover_window_left = (position.left + toolbar_width - popover_width).clamp(
                VIEWPORT_INSET,
                (f32::from(viewport.width) - popover_width - VIEWPORT_INSET).max(VIEWPORT_INSET),
            );
            let popover = div()
                .id("selection-toolbar-link-editor")
                .debug_selector(|| "selection-toolbar-link-editor".to_owned())
                .absolute()
                .left(px(popover_window_left - position.left))
                .w(px(popover_width))
                .p(px(6.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .bg(c.dialog_surface)
                .border(px(d.dialog_border_width))
                .border_color(c.dialog_border)
                .rounded(px(8.0))
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .h(px(28.0))
                        .px(px(6.0))
                        .flex()
                        .items_center()
                        .rounded(px(5.0))
                        .border(px(d.dialog_border_width))
                        .border_color(c.dialog_border)
                        .bg(c.code_language_input_bg)
                        .child(input),
                )
                .when(self.selection_toolbar_link_had_target, |popover| {
                    popover.child(
                        div()
                            .id("selection-toolbar-link-remove")
                            .h(px(28.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(5.0))
                            .text_size(px(d.menu_text_size))
                            .text_color(c.dialog_muted)
                            .hover(|button| button.bg(c.dialog_secondary_button_hover))
                            .cursor_pointer()
                            .child(remove_label)
                            .on_click(cx.listener(|block, _event, window, cx| {
                                block.commit_selection_link_editor(true, window, cx);
                            })),
                    )
                })
                .child(
                    div()
                        .id("selection-toolbar-link-apply")
                        .h(px(28.0))
                        .px(px(9.0))
                        .flex()
                        .items_center()
                        .rounded(px(5.0))
                        .text_size(px(d.menu_text_size))
                        .text_color(c.dialog_primary_button_text)
                        .bg(c.dialog_primary_button_bg)
                        .hover(|button| button.bg(c.dialog_primary_button_hover))
                        .cursor_pointer()
                        .child(apply_label)
                        .on_click(cx.listener(|block, _event, window, cx| {
                            block.commit_selection_link_editor(false, window, cx);
                        })),
                );
            if link_editor_above {
                popover.bottom(px(TOOLBAR_HEIGHT + 4.0))
            } else {
                popover.top(px(TOOLBAR_HEIGHT + 4.0))
            }
        });
        let panel = div()
            .id("selection-toolbar")
            .debug_selector(|| "selection-toolbar".to_owned())
            .absolute()
            .left(px(position.left - origin_left))
            .top(px(position.top - origin_top))
            .w(px(toolbar_width))
            .h(px(TOOLBAR_HEIGHT))
            .p(px(2.0))
            .flex()
            .items_center()
            .gap(px(2.0))
            .rounded(px(6.0))
            .occlude()
            .bg(c.dialog_surface)
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .on_mouse_up(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .children(buttons)
            .children(overflow)
            .children(type_menu)
            .children(link_editor);
        Some(deferred(panel).with_priority(20).into_any_element())
    }
}
