// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn on_tab_strip_key_down(
        &mut self,
        index: usize,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.tabs.records.len();
        if index >= count || count == 0 {
            return;
        }
        let target = match event.keystroke.key.as_str() {
            "left" => Some((index + count - 1) % count),
            "right" => Some((index + 1) % count),
            "home" => Some(0),
            "end" => Some(count - 1),
            "enter" | "space" => Some(index),
            "delete" => {
                self.request_close_tab_index(index, cx);
                if !self.tabs.show_close_dialog {
                    self.focus_tab_index(self.tabs.active, window, cx);
                }
                cx.stop_propagation();
                return;
            }
            _ => None,
        };
        if let Some(target) = target {
            self.switch_to_tab_index(target, cx);
            self.focus_tab_index(target, window, cx);
            cx.stop_propagation();
        }
    }

    pub(super) fn on_new_tab_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            self.new_untitled_tab(cx);
            cx.stop_propagation();
        }
    }

    pub(in crate::editor) fn render_tab_strip(
        &mut self,
        theme: &crate::theme::Theme,
        top: f32,
        left: f32,
        right: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let height = self.tab_strip_height();
        if height == 0.0 {
            return None;
        }
        let c = &theme.colors;
        let show_tab_bar_actions = EditorSettings::show_tab_bar_actions(cx);
        let strings = cx.global::<crate::i18n::I18nManager>().strings().clone();
        let close_tab_tooltip = strings.menu_close_tab.clone();
        let new_tab_tooltip = strings.menu_new_tab.clone();
        let tab_drop_background = c.chrome_hover;
        let editor = cx.entity().downgrade();
        let strip_release_editor = editor.clone();
        let strip_detach_editor = editor.clone();
        let new_tab_editor = editor.clone();
        let new_tab_key_editor = editor.clone();
        let (tab_focus_handles, new_tab_focus_handle) = self.ensure_tab_strip_focus_handles(cx);
        let toolbar_button = |action: DocumentToolbarAction,
                              icon: &'static str,
                              tooltip: SharedString,
                              active: bool| {
            let focus_handle = self.document_toolbar_focus_handles[action.index()].clone();
            let pointer_focus_handle = focus_handle.clone();
            let click_editor = editor.clone();
            let key_editor = editor.clone();
            let icon_color = if active {
                c.text_default
            } else {
                c.dialog_muted
            };
            div()
                .id(("document-toolbar-action", action.index()))
                .debug_selector(move || format!("document-toolbar-action-{}", action.index()))
                .size(px(TAB_TOOL_BUTTON_SIZE))
                .tab_index(0)
                .track_focus(&focus_handle)
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .border(px(1.0))
                .border_color(if active {
                    c.dialog_border
                } else {
                    hsla(0.0, 0.0, 0.0, 0.0)
                })
                .bg(if active {
                    c.chrome_hover
                } else {
                    hsla(0.0, 0.0, 0.0, 0.0)
                })
                .hover(|this| this.bg(c.chrome_hover).text_color(c.text_default))
                .focus(|this| this.border_color(c.text_link))
                .cursor_pointer()
                .tooltip(move |_window, cx| crate::ui::ui_tooltip(tooltip.clone(), cx))
                .child(svg().path(icon).size(px(15.0)).text_color(icon_color))
                .on_click(move |_event, window, cx| {
                    pointer_focus_handle.focus(window);
                    let _ = click_editor.update(cx, |editor, cx| {
                        editor.activate_document_toolbar_action(action, window, cx);
                    });
                    cx.stop_propagation();
                })
                .on_key_down(move |event, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        let _ = key_editor.update(cx, |editor, cx| {
                            editor.activate_document_toolbar_action(action, window, cx);
                        });
                        cx.stop_propagation();
                    }
                })
                .into_any_element()
        };
        let new_tab_button = div()
            .id("document-new-tab")
            .debug_selector(|| "document-new-tab".to_owned())
            .size(px(TAB_TOOL_BUTTON_SIZE))
            .tab_index(0)
            .track_focus(&new_tab_focus_handle)
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .text_color(c.dialog_muted)
            .hover(|this| this.bg(c.chrome_hover).text_color(c.text_default))
            .focus(|this| this.bg(c.chrome_hover).text_color(c.text_default))
            .cursor_pointer()
            .tooltip(move |_window, cx| crate::ui::ui_tooltip(new_tab_tooltip.clone(), cx))
            .child(
                svg()
                    .path(NEW_TAB_ICON)
                    .size(px(14.0))
                    .text_color(c.dialog_muted),
            )
            .on_click(move |_event, _window, cx| {
                let _ = new_tab_editor.update(cx, |editor, cx| {
                    editor.new_untitled_tab(cx);
                });
                cx.stop_propagation();
            })
            .on_key_down(move |event, _window, cx| {
                let _ = new_tab_key_editor.update(cx, |editor, cx| {
                    editor.on_new_tab_key_down(event, cx);
                });
            });
        Some(
            div()
                .id("document-tab-strip")
                .debug_selector(|| "document-tab-strip".to_owned())
                .absolute()
                .left(px(left))
                .right(px(right))
                .top(px(top))
                .h(px(height))
                .flex()
                .items_center()
                .overflow_hidden()
                .bg(c.tab_strip_background)
                .border_b(px(theme.dimensions.dialog_border_width))
                .border_color(c.dialog_border)
                .on_mouse_up(MouseButton::Left, move |_event, _window, cx| {
                    let _ = strip_release_editor.update(cx, |editor, _cx| {
                        editor.tabs.dragging_tab = None;
                    });
                })
                .on_mouse_up_out(MouseButton::Left, move |event, window, cx| {
                    let viewport = window.viewport_size();
                    let outside_window = event.position.x < px(0.0)
                        || event.position.y < px(0.0)
                        || event.position.x >= viewport.width
                        || event.position.y >= viewport.height;
                    let detached = strip_detach_editor
                        .update(cx, |editor, cx| {
                            let id = editor.tabs.dragging_tab.take();
                            if outside_window {
                                id.and_then(|id| editor.detach_tab_by_id(id, cx))
                            } else {
                                None
                            }
                        })
                        .ok()
                        .flatten();
                    if let Some(detached) = detached {
                        crate::app_menu::open_detached_tab_window(cx, detached);
                    }
                })
                .child(
                    div()
                        .id("document-tab-scroll")
                        .debug_selector(|| "document-tab-scroll".to_owned())
                        .h_full()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .overflow_x_scroll()
                        .children(self.tabs.records.iter().enumerate().map(|(index, record)| {
                            let active = index == self.tabs.active;
                            let (path, dirty) = if active {
                                (self.file_path.as_deref(), self.document_dirty)
                            } else {
                                record
                                    .snapshot
                                    .as_ref()
                                    .map(|snapshot| {
                                        (snapshot.file_path.as_deref(), snapshot.document_dirty)
                                    })
                                    .unwrap_or((None, false))
                            };
                            let title = path
                                .and_then(Path::file_name)
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "Untitled".to_owned());
                            let display_title = middle_ellipsis(&title, 28);
                            let leading_icon = if record.pinned {
                                TAB_PIN_ICON
                            } else {
                                TAB_DOCUMENT_ICON
                            };
                            let title_tooltip: SharedString = title.clone().into();
                            let close_tooltip: SharedString = close_tab_tooltip.clone().into();
                            let switch_editor = editor.clone();
                            let close_editor = editor.clone();
                            let context_editor = editor.clone();
                            let drop_editor = editor.clone();
                            let drag_editor = editor.clone();
                            let key_editor = editor.clone();
                            let focus_handle = tab_focus_handles[index].clone();
                            let tab_id = record.id.as_u64_pair().0;
                            let group = SharedString::from(format!("document-tab-group-{tab_id}"));
                            let drag_payload = TabDragPayload {
                                id: record.id,
                                title: title.clone(),
                                background: c.tab_strip_background,
                                text: c.text_default,
                            };
                            div()
                                .id(("document-tab", tab_id))
                                .group(group.clone())
                                .debug_selector(move || format!("document-tab-{index}"))
                                .min_w(px(TAB_MIN_WIDTH))
                                .max_w(px(TAB_MAX_WIDTH))
                                .h(px(30.0))
                                .mx(px(2.0))
                                .tab_index(0)
                                .track_focus(&focus_handle)
                                .px(px(10.0))
                                .flex()
                                .items_center()
                                .gap(px(7.0))
                                .rounded(px(8.0))
                                .border(px(theme.dimensions.dialog_border_width))
                                .border_color(if active {
                                    c.dialog_border
                                } else {
                                    hsla(0.0, 0.0, 0.0, 0.0)
                                })
                                .bg(if active {
                                    c.chrome_hover
                                } else {
                                    c.tab_strip_background
                                })
                                .hover(|this| this.bg(c.chrome_hover))
                                .focus(|this| this.bg(c.chrome_hover))
                                .cursor_pointer()
                                .tooltip(move |_window, cx| {
                                    crate::ui::ui_tooltip(title_tooltip.clone(), cx)
                                })
                                .child(
                                    div()
                                        .size(px(16.0))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .debug_selector(move || {
                                            format!("document-tab-leading-{index}")
                                        })
                                        .child(svg().path(leading_icon).size(px(13.0)).text_color(
                                            if record.pinned {
                                                c.text_link
                                            } else {
                                                c.dialog_muted
                                            },
                                        )),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .truncate()
                                        .debug_selector(move || {
                                            format!("document-tab-title-{index}")
                                        })
                                        .text_size(px(theme.typography.text_size * 0.88))
                                        .text_color(if active {
                                            c.text_default
                                        } else {
                                            c.dialog_muted
                                        })
                                        .child(display_title),
                                )
                                .child(
                                    div()
                                        .id(("document-tab-close", tab_id))
                                        .debug_selector(move || {
                                            format!("document-tab-close-{index}")
                                        })
                                        .relative()
                                        .size(px(18.0))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(4.0))
                                        .hover(|this| this.bg(c.chrome_hover))
                                        .cursor_pointer()
                                        .tooltip(move |_window, cx| {
                                            crate::ui::ui_tooltip(close_tooltip.clone(), cx)
                                        })
                                        .child(
                                            div()
                                                .absolute()
                                                .size(px(6.0))
                                                .rounded_full()
                                                .bg(c.text_link)
                                                .debug_selector(move || {
                                                    format!("document-tab-dirty-{index}")
                                                })
                                                .opacity(if dirty { 1.0 } else { 0.0 })
                                                .group_hover(group.clone(), |this| {
                                                    this.opacity(0.0)
                                                }),
                                        )
                                        .child(
                                            svg()
                                                .absolute()
                                                .path(TAB_CLOSE_ICON)
                                                .size(px(13.0))
                                                .debug_selector(move || {
                                                    format!("document-tab-close-icon-{index}")
                                                })
                                                .text_color(c.dialog_muted)
                                                .opacity(if active && !dirty { 1.0 } else { 0.0 })
                                                .group_hover(group, |this| this.opacity(1.0)),
                                        )
                                        .on_click(move |_event, _window, cx| {
                                            let _ = close_editor.update(cx, |editor, cx| {
                                                editor.request_close_tab_index(index, cx);
                                            });
                                            cx.stop_propagation();
                                        }),
                                )
                                .on_click(move |_event, _window, cx| {
                                    let _ = switch_editor.update(cx, |editor, cx| {
                                        editor.switch_to_tab_index(index, cx);
                                    });
                                })
                                .on_key_down(move |event, window, cx| {
                                    let _ = key_editor.update(cx, |editor, cx| {
                                        editor.on_tab_strip_key_down(index, event, window, cx);
                                    });
                                })
                                .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                                    let _ = context_editor.update(cx, |editor, cx| {
                                        editor.tabs.context_menu = Some(TabContextMenu {
                                            index,
                                            position: event.position,
                                        });
                                        editor.context_menu_keyboard_item = None;
                                        editor.context_menu_keyboard_submenu_item = None;
                                        editor
                                            .context_menu_scroll_handle
                                            .set_offset(point(px(0.0), px(0.0)));
                                        cx.notify();
                                    });
                                    cx.stop_propagation();
                                })
                                .on_drag(drag_payload, move |payload, position, _, cx| {
                                    let id = payload.id;
                                    let _ = drag_editor.update(cx, |editor, _cx| {
                                        editor.tabs.dragging_tab = Some(id);
                                    });
                                    cx.new(|_| TabDragPreview {
                                        payload: payload.clone(),
                                        position,
                                    })
                                })
                                .drag_over::<TabDragPayload>(move |style, _, _, _| {
                                    style.bg(tab_drop_background)
                                })
                                .on_drop(move |payload: &TabDragPayload, _window, cx| {
                                    let _ = drop_editor.update(cx, |editor, cx| {
                                        editor.tabs.dragging_tab = None;
                                        if let Some(source) = editor
                                            .tabs
                                            .records
                                            .iter()
                                            .position(|record| record.id == payload.id)
                                        {
                                            editor.reorder_tab(source, index, cx);
                                        }
                                    });
                                })
                        }))
                        .child(new_tab_button),
                )
                .children(show_tab_bar_actions.then(|| {
                    div()
                        .id("document-tab-trailing-tools")
                        .debug_selector(|| "document-tab-trailing-tools".to_owned())
                        .h_full()
                        .px(px(TAB_TOOL_GROUP_PADDING))
                        .gap(px(2.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .bg(c.chrome_background)
                        .border_l(px(theme.dimensions.dialog_border_width))
                        .border_color(c.dialog_border)
                        .children(show_tab_bar_actions.then(|| {
                            toolbar_button(
                                DocumentToolbarAction::QuickOpen,
                                QUICK_OPEN_ICON,
                                strings.quick_open_title.clone().into(),
                                false,
                            )
                        }))
                        .children(show_tab_bar_actions.then(|| {
                            toolbar_button(
                                DocumentToolbarAction::Find,
                                FIND_ICON,
                                strings.preferences_shortcut_find_in_document.clone().into(),
                                self.find_panel.is_some(),
                            )
                        }))
                        .children(show_tab_bar_actions.then(|| {
                            toolbar_button(
                                DocumentToolbarAction::CommandPalette,
                                COMMAND_PALETTE_ICON,
                                strings.command_palette_title.clone().into(),
                                self.command_palette.is_some(),
                            )
                        }))
                }))
                .into_any_element(),
        )
    }

    pub(in crate::editor) fn render_tab_close_dialog_overlay(
        &self,
        theme: &crate::theme::Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.tabs.show_close_dialog {
            return None;
        }
        let d = &theme.dimensions;
        Some(
            modal_overlay("tab-close-dialog-overlay", theme)
                .child(
                    dialog_panel("tab-close-dialog", d.dialog_width.min(520.0), theme)
                        .child(
                            dialog_content("tab-close-dialog-content", theme)
                                .child(dialog_title_with_icon(
                                    "tab-close-title",
                                    strings.unsaved_changes_title.clone(),
                                    DialogTitleIcon::Warning,
                                    theme,
                                ))
                                .child(dialog_body(strings.unsaved_changes_message.clone(), theme)),
                        )
                        .child(
                            dialog_actions(theme)
                                .child(
                                    dialog_button(
                                        "cancel-tab-close",
                                        strings.unsaved_changes_cancel.clone(),
                                        DialogButtonKind::Secondary,
                                        theme,
                                    )
                                    .on_click(cx.listener(Self::on_cancel_tab_close)),
                                )
                                .child(
                                    dialog_button(
                                        "discard-tab-close",
                                        strings.unsaved_changes_discard_and_close.clone(),
                                        DialogButtonKind::Danger,
                                        theme,
                                    )
                                    .on_click(cx.listener(Self::on_discard_tab_close)),
                                )
                                .child(
                                    dialog_button(
                                        "save-tab-close",
                                        strings.unsaved_changes_save_and_close.clone(),
                                        DialogButtonKind::Primary,
                                        theme,
                                    )
                                    .on_click(cx.listener(Self::on_save_tab_close)),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(in crate::editor) fn render_tab_context_menu_overlay(
        &self,
        theme: &crate::theme::Theme,
        strings: &crate::i18n::I18nStrings,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.focus_mode {
            return None;
        }
        let menu = self.tabs.context_menu.as_ref()?;
        let index = menu.index;
        let position = menu.position;
        let pinned = self
            .tabs
            .records
            .get(index)
            .is_some_and(|record| record.pinned);
        let can_close_others = self.tabs.records.len() > 1;
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let panel_width = d.context_menu_submenu_width.max(200.0);
        let panel_origin = clamped_floating_panel_origin(
            position,
            panel_width,
            compact_menu_panel_height(3, 0, d),
            window.viewport_size(),
        );
        let editor = cx.entity().downgrade();
        let dismiss_editor = editor.clone();
        let pin_editor = editor.clone();
        let close_editor = editor.clone();
        let close_others_editor = editor;
        let item = |id: &'static str,
                    keyboard_index: usize,
                    label: String,
                    icon: &'static str,
                    enabled: bool| {
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .h(px(d.menu_item_height))
                .px(px(d.menu_item_padding_x))
                .flex()
                .items_center()
                .gap(px(6.0))
                .rounded(px(d.menu_item_radius))
                .bg(if self.context_menu_keyboard_item == Some(keyboard_index) {
                    c.dialog_secondary_button_hover
                } else {
                    c.dialog_surface
                })
                .text_size(px(d.menu_text_size))
                .font_weight(t.dialog_body_weight.to_font_weight())
                .text_color(if enabled {
                    c.dialog_secondary_button_text
                } else {
                    c.dialog_muted
                })
                .opacity(if enabled { 1.0 } else { 0.5 })
                .child(
                    menu_icon_slot(Some(icon), c.dialog_muted)
                        .debug_selector(move || format!("{id}-icon")),
                )
                .on_hover(cx.listener(Self::on_context_menu_pointer_hover))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .truncate()
                        .child(label),
                )
        };

        Some(
            div()
                .id("tab-context-menu-overlay")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .occlude()
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    let _ = dismiss_editor.update(cx, |editor, cx| {
                        editor.tabs.context_menu = None;
                        cx.notify();
                    });
                })
                .child(
                    div()
                        .id("tab-context-menu")
                        .debug_selector(|| "tab-context-menu".to_owned())
                        .absolute()
                        .left(panel_origin.x)
                        .top(panel_origin.y)
                        .w(px(panel_width))
                        .p(px(d.menu_panel_padding))
                        .flex()
                        .flex_col()
                        .gap(px(d.menu_panel_gap))
                        .bg(c.dialog_surface)
                        .border(px(d.dialog_border_width))
                        .border_color(c.dialog_border)
                        .rounded(px(d.menu_panel_radius))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation()
                        })
                        .child(
                            item(
                                "tab-context-pin",
                                0,
                                if pinned {
                                    strings.menu_unpin_tab.clone()
                                } else {
                                    strings.menu_pin_tab.clone()
                                },
                                "icon/editor/tab-pin.svg",
                                true,
                            )
                            .hover(|this| this.bg(c.dialog_secondary_button_hover))
                            .cursor_pointer()
                            .on_click(move |_event, _window, cx| {
                                let _ = pin_editor.update(cx, |editor, cx| {
                                    editor.tabs.context_menu = None;
                                    editor.toggle_pin_tab(index, cx);
                                });
                            }),
                        )
                        .child(
                            item(
                                "tab-context-close",
                                1,
                                strings.menu_close_tab.clone(),
                                TAB_CLOSE_ICON,
                                true,
                            )
                            .hover(|this| this.bg(c.dialog_secondary_button_hover))
                            .cursor_pointer()
                            .on_click(move |_event, _window, cx| {
                                let _ = close_editor.update(cx, |editor, cx| {
                                    editor.tabs.context_menu = None;
                                    editor.request_close_tab_index(index, cx);
                                });
                            }),
                        )
                        .child(
                            item(
                                "tab-context-close-others",
                                2,
                                strings.menu_close_other_tabs.clone(),
                                TAB_CLOSE_ICON,
                                can_close_others,
                            )
                            .when(can_close_others, |this| {
                                this.hover(|this| this.bg(c.dialog_secondary_button_hover))
                                    .cursor_pointer()
                                    .on_click(move |_event, _window, cx| {
                                        let _ = close_others_editor.update(cx, |editor, cx| {
                                            editor.tabs.context_menu = None;
                                            editor.request_close_other_tabs(index, cx);
                                        });
                                    })
                            }),
                        ),
                )
                .into_any_element(),
        )
    }

    #[cfg(test)]
    pub(super) fn inactive_tab_count(&self) -> usize {
        self.tabs
            .records
            .iter()
            .filter(|record| record.snapshot.is_some())
            .count()
    }
}
