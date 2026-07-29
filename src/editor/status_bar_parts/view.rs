// @author kongweiguang

use super::*;

pub(super) fn render_overflow_text(id: &'static str, label: String, theme: &Theme) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .h(px(theme.dimensions.status_bar_height))
        .flex()
        .items_center()
        .text_size(px(theme.dimensions.status_bar_text_size))
        .text_color(theme.colors.status_bar_text)
        .child(label)
        .into_any_element()
}

pub(super) fn render_large_overflow_action(
    id: &'static str,
    label: String,
    active: bool,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .h(px(28.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .bg(if active {
            theme.colors.status_bar_button_hover
        } else {
            hsla(0.0, 0.0, 0.0, 0.0)
        })
        .hover(|item| item.bg(theme.colors.status_bar_button_hover))
        .cursor_pointer()
        .text_size(px(theme.dimensions.status_bar_text_size))
        .text_color(theme.colors.status_bar_text)
        .child(label)
}

pub(super) fn render_source_format_overflow_button(
    state: &mut StatusBarState,
    theme: &Theme,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let d = &theme.dimensions;
    let open = state.format_overflow_open;
    let focus_handle = state
        .overflow_focus_handle
        .get_or_insert_with(|| cx.focus_handle())
        .clone();
    let pointer_focus_handle = focus_handle.clone();
    div()
        .id("status-bar-format-overflow-button")
        .debug_selector(|| "status-bar-format-overflow-button".to_owned())
        .h(px(d.status_bar_height))
        .min_w(px(28.0))
        .tab_index(0)
        .track_focus(&focus_handle)
        .px(px(6.0))
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .rounded(px(4.0))
        .border(px(1.0))
        .border_color(hsla(0.0, 0.0, 0.0, 0.0))
        .bg(if open {
            theme.colors.status_bar_button_hover
        } else {
            hsla(0., 0., 0., 0.)
        })
        .hover(|this| this.bg(theme.colors.status_bar_button_hover))
        .focus(|this| this.border_color(theme.colors.text_link))
        .cursor_pointer()
        .text_color(theme.colors.status_bar_text)
        .child(
            svg()
                .path(MORE_ICON)
                .size(px(15.0))
                .text_color(theme.colors.status_bar_text),
        )
        .children(open.then(|| {
            div()
                .absolute()
                .left(px(5.0))
                .right(px(5.0))
                .bottom(px(-1.0))
                .h(px(2.0))
                .rounded(px(1.0))
                .bg(theme.colors.text_link)
                .debug_selector(|| "status-bar-format-overflow-indicator".to_owned())
        }))
        .on_click(cx.listener(move |editor, _: &ClickEvent, window, cx| {
            pointer_focus_handle.focus(window);
            editor.status_bar.line_ending_menu_open = false;
            editor.status_bar.mode_menu_open = false;
            editor.status_bar.format_overflow_open = !editor.status_bar.format_overflow_open;
            cx.notify();
        }))
        .on_key_down(cx.listener(|editor, event: &KeyDownEvent, _window, cx| {
            match event.keystroke.key.as_str() {
                "enter" | "space" => {
                    editor.status_bar.format_overflow_open =
                        !editor.status_bar.format_overflow_open;
                    cx.notify();
                    cx.stop_propagation();
                }
                "escape" if editor.status_bar.format_overflow_open => {
                    editor.status_bar.format_overflow_open = false;
                    cx.notify();
                    cx.stop_propagation();
                }
                _ => {}
            }
        }))
        .into_any_element()
}

pub(super) fn should_render_file_status(
    _recovered_session: bool,
    external_file_conflict: bool,
) -> bool {
    // 启动恢复是自动完成的正常流程，保持静默；这里只保留需要用户处理的外部冲突入口。
    external_file_conflict
}

pub(super) fn render_recovery_status(
    state: &mut StatusBarState,
    has_conflict: bool,
    theme: &Theme,
    strings: &I18nStrings,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let color = if has_conflict {
        theme.colors.callout_warning_border
    } else {
        theme.colors.status_bar_text
    };
    let icon = if has_conflict {
        CONFLICT_ICON
    } else {
        RECOVERY_ICON
    };
    let icon_selector = if has_conflict {
        "status-bar-recovery-conflict-icon"
    } else {
        "status-bar-recovery-restored-icon"
    };
    let status = div()
        .id("status-bar-recovery")
        .debug_selector(|| "status-bar-recovery".to_owned())
        .h(px(theme.dimensions.status_bar_height))
        .max_w(px(160.0))
        .px(px(5.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded(px(4.0))
        .text_size(px(theme.dimensions.status_bar_text_size))
        .text_color(color)
        .child(
            div()
                .size(px(16.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .debug_selector(move || icon_selector.to_owned())
                .child(svg().path(icon).size(px(14.0)).text_color(color)),
        )
        .child(
            div()
                .min_w(px(0.0))
                .overflow_hidden()
                .truncate()
                .debug_selector(|| "status-bar-recovery-label".to_owned())
                .child(if has_conflict {
                    strings.recovery_conflict_status.clone()
                } else {
                    strings.recovery_status.clone()
                }),
        );

    if has_conflict {
        let focus_handle = state
            .conflict_focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let pointer_focus_handle = focus_handle.clone();
        status
            .tab_index(0)
            .track_focus(&focus_handle)
            .border(px(1.0))
            .border_color(hsla(0.0, 0.0, 0.0, 0.0))
            .cursor_pointer()
            .hover(|this| this.bg(theme.colors.status_bar_button_hover))
            .focus(|this| this.border_color(theme.colors.text_link))
            .on_click(cx.listener(move |editor, _: &ClickEvent, window, cx| {
                pointer_focus_handle.focus(window);
                let Some(path) = editor.file_path.clone() else {
                    return;
                };
                editor.present_external_file_conflict(&path, window, cx);
            }))
            .on_key_down(cx.listener(|editor, event: &KeyDownEvent, window, cx| {
                if !matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    return;
                }
                let Some(path) = editor.file_path.clone() else {
                    return;
                };
                editor.present_external_file_conflict(&path, window, cx);
                cx.stop_propagation();
            }))
            .into_any_element()
    } else {
        status.into_any_element()
    }
}

pub(super) fn render_sidebar_toggle(
    state: &mut StatusBarState,
    is_open: bool,
    theme: &Theme,
    strings: &I18nStrings,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;
    let focus_handle = state
        .sidebar_focus_handle
        .get_or_insert_with(|| cx.focus_handle())
        .clone();
    let pointer_focus_handle = focus_handle.clone();

    div()
        .id("status-bar-sidebar-toggle")
        .debug_selector(|| "status-bar-sidebar-toggle".to_owned())
        .relative()
        .size(px(d.status_bar_height))
        .tab_index(0)
        .track_focus(&focus_handle)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .border(px(1.0))
        .border_color(hsla(0.0, 0.0, 0.0, 0.0))
        .bg(if state.sidebar_hovered || is_open {
            c.status_bar_button_hover
        } else {
            hsla(0., 0., 0., 0.)
        })
        .cursor_pointer()
        .focus(|this| this.border_color(c.text_link))
        .text_color(c.text_default)
        .child(
            svg()
                .path(SIDEBAR_ICON)
                .size(px(15.0))
                .text_color(c.text_default),
        )
        .children(is_open.then(|| {
            div()
                .absolute()
                .left(px(4.0))
                .right(px(4.0))
                .bottom(px(0.0))
                .h(px(2.0))
                .rounded(px(1.0))
                .bg(c.text_link)
                .debug_selector(|| "status-bar-sidebar-indicator".to_owned())
        }))
        .children(
            (state.tooltip_visible == Some(StatusTooltip::Sidebar)).then(|| {
                status_bar_tooltip(
                    strings.status_bar_files.clone(),
                    theme,
                    StatusTooltipAlignment::Start,
                    "status-bar-sidebar-tooltip".to_owned(),
                )
            }),
        )
        .on_hover(cx.listener(
            |editor: &mut Editor,
             hovered: &bool,
             _window: &mut Window,
             cx: &mut Context<Editor>| {
                editor.status_bar.sidebar_hovered = *hovered;
                editor.set_status_sidebar_tooltip_hover(*hovered, cx);
            },
        ))
        .on_click(cx.listener(
            move |editor: &mut Editor,
                  _: &gpui::ClickEvent,
                  window: &mut Window,
                  cx: &mut Context<Editor>| {
                pointer_focus_handle.focus(window);
                editor.toggle_workspace_drawer(window, cx);
            },
        ))
        .on_key_down(cx.listener(
            |editor: &mut Editor,
             event: &KeyDownEvent,
             window: &mut Window,
             cx: &mut Context<Editor>| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    editor.toggle_workspace_drawer(window, cx);
                    cx.stop_propagation();
                }
            },
        ))
        .into_any_element()
}

pub(super) fn render_document_sidebar_toggle(
    state: &mut StatusBarState,
    is_open: bool,
    theme: &Theme,
    strings: &I18nStrings,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;
    let focus_handle = state
        .document_sidebar_focus_handle
        .get_or_insert_with(|| cx.focus_handle())
        .clone();
    let pointer_focus_handle = focus_handle.clone();
    let label = strings.status_bar_document_sidebar.clone();
    div()
        .id("status-bar-document-sidebar-toggle")
        .debug_selector(|| "status-bar-document-sidebar-toggle".to_owned())
        .relative()
        .size(px(d.status_bar_height))
        .tab_index(0)
        .track_focus(&focus_handle)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .border(px(1.0))
        .border_color(hsla(0.0, 0.0, 0.0, 0.0))
        .bg(if state.document_sidebar_hovered || is_open {
            c.status_bar_button_hover
        } else {
            hsla(0., 0., 0., 0.)
        })
        .cursor_pointer()
        .focus(|this| this.border_color(c.text_link))
        .text_color(c.text_default)
        .child(
            svg()
                .path(DOCUMENT_SIDEBAR_ICON)
                .size(px(15.0))
                .text_color(c.text_default),
        )
        .children(is_open.then(|| {
            div()
                .absolute()
                .left(px(4.0))
                .right(px(4.0))
                .bottom(px(0.0))
                .h(px(2.0))
                .rounded(px(1.0))
                .bg(c.text_link)
                .debug_selector(|| "status-bar-document-sidebar-indicator".to_owned())
        }))
        .children(
            (state.tooltip_visible == Some(StatusTooltip::DocumentSidebar)).then(|| {
                status_bar_tooltip(
                    label,
                    theme,
                    StatusTooltipAlignment::End,
                    "status-bar-document-sidebar-tooltip".to_owned(),
                )
            }),
        )
        .on_hover(cx.listener(
            |editor: &mut Editor,
             hovered: &bool,
             _window: &mut Window,
             cx: &mut Context<Editor>| {
                editor.set_status_document_sidebar_tooltip_hover(*hovered, cx);
            },
        ))
        .on_click(cx.listener(
            move |editor: &mut Editor,
                  _: &gpui::ClickEvent,
                  window: &mut Window,
                  cx: &mut Context<Editor>| {
                pointer_focus_handle.focus(window);
                editor.toggle_document_sidebar_drawer(window, cx);
            },
        ))
        .on_key_down(cx.listener(
            |editor: &mut Editor,
             event: &KeyDownEvent,
             window: &mut Window,
             cx: &mut Context<Editor>| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    editor.toggle_document_sidebar_drawer(window, cx);
                    cx.stop_propagation();
                }
            },
        ))
        .into_any_element()
}

pub(super) fn render_line_ending_picker(
    state: &mut StatusBarState,
    current_label: String,
    theme: &Theme,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let open = state.line_ending_menu_open;
    let button_focus_handle = state
        .line_ending_button_focus_handle
        .get_or_insert_with(|| cx.focus_handle())
        .clone();
    if state.line_ending_focus_handles.is_none() {
        state.line_ending_focus_handles = Some(std::array::from_fn(|_| cx.focus_handle()));
    }
    let focus_handles = state
        .line_ending_focus_handles
        .as_ref()
        .expect("line-ending focus handles must be initialized")
        .clone();
    let first_item_focus = focus_handles[0].clone();
    let pointer_focus_handle = button_focus_handle.clone();
    let menu_items = [
        ("lf", "LF", LineEnding::Lf),
        ("crlf", "CRLF", LineEnding::CrLf),
        ("cr", "CR", LineEnding::Cr),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, label, ending))| {
        render_line_ending_menu_item(
            id,
            label,
            ending,
            current_label == label,
            index,
            focus_handles.clone(),
            button_focus_handle.clone(),
            theme,
            cx,
        )
    })
    .collect::<Vec<_>>();

    div()
        .id("status-bar-line-ending-picker")
        .debug_selector(|| "status-bar-line-ending-picker".to_owned())
        .relative()
        .h(px(theme.dimensions.status_bar_height))
        .flex()
        .items_center()
        .child(
            div()
                .id("status-bar-line-ending-button")
                .debug_selector(|| "status-bar-line-ending-button".to_owned())
                .h(px(theme.dimensions.status_bar_height))
                .min_w(px(30.0))
                .px(px(5.0))
                .tab_index(0)
                .track_focus(&button_focus_handle)
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .border(px(1.0))
                .border_color(hsla(0.0, 0.0, 0.0, 0.0))
                .bg(if open {
                    theme.colors.status_bar_button_hover
                } else {
                    hsla(0.0, 0.0, 0.0, 0.0)
                })
                .hover(|this| this.bg(theme.colors.status_bar_button_hover))
                .focus(|this| this.border_color(theme.colors.text_link))
                .cursor_pointer()
                .text_size(px(theme.dimensions.status_bar_text_size))
                .text_color(theme.colors.status_bar_text)
                .child(current_label)
                .on_click(cx.listener(move |editor, _: &ClickEvent, window, cx| {
                    pointer_focus_handle.focus(window);
                    editor.status_bar.format_overflow_open = false;
                    editor.status_bar.mode_menu_open = false;
                    editor.status_bar.line_ending_menu_open =
                        !editor.status_bar.line_ending_menu_open;
                    cx.notify();
                }))
                .on_key_down(
                    cx.listener(move |editor, event: &KeyDownEvent, window, cx| {
                        match event.keystroke.key.as_str() {
                            "enter" | "space" => {
                                editor.status_bar.format_overflow_open = false;
                                editor.status_bar.mode_menu_open = false;
                                editor.status_bar.line_ending_menu_open =
                                    !editor.status_bar.line_ending_menu_open;
                                if editor.status_bar.line_ending_menu_open {
                                    first_item_focus.focus(window);
                                }
                                cx.notify();
                                cx.stop_propagation();
                            }
                            "escape" if editor.status_bar.line_ending_menu_open => {
                                editor.status_bar.line_ending_menu_open = false;
                                cx.notify();
                                cx.stop_propagation();
                            }
                            _ => {}
                        }
                    }),
                ),
        )
        .children(open.then(|| {
            div()
                .id("status-bar-line-ending-menu")
                .debug_selector(|| "status-bar-line-ending-menu".to_owned())
                .absolute()
                .right(px(0.0))
                .bottom(px(theme.dimensions.status_bar_height + 4.0))
                .w(px(92.0))
                .occlude()
                .p(px(4.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .bg(theme.colors.dialog_surface)
                .border(px(theme.dimensions.dialog_border_width))
                .border_color(theme.colors.dialog_border)
                .rounded(px(8.0))
                .shadow_lg()
                .children(menu_items)
        }))
        .into_any_element()
}

fn render_line_ending_menu_item(
    id: &'static str,
    label: &'static str,
    ending: LineEnding,
    active: bool,
    index: usize,
    focus_handles: [FocusHandle; 3],
    button_focus_handle: FocusHandle,
    theme: &Theme,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let focus_handle = focus_handles[index].clone();
    let pointer_focus_handle = focus_handle.clone();
    let keyboard_focus_handles = focus_handles.clone();
    div()
        .id(SharedString::from(format!("status-bar-line-ending-{id}")))
        .debug_selector(move || format!("status-bar-line-ending-{id}"))
        .h(px(30.0))
        .w_full()
        .px(px(8.0))
        .tab_index(0)
        .track_focus(&focus_handle)
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(6.0))
        .border(px(1.0))
        .border_color(hsla(0.0, 0.0, 0.0, 0.0))
        .bg(if active {
            theme.colors.status_bar_button_hover
        } else {
            hsla(0.0, 0.0, 0.0, 0.0)
        })
        .hover(|this| this.bg(theme.colors.status_bar_button_hover))
        .focus(|this| this.border_color(theme.colors.text_link))
        .cursor_pointer()
        .child(
            div()
                .flex_1()
                .text_size(px(theme.dimensions.status_bar_text_size))
                .text_color(theme.colors.text_default)
                .child(label),
        )
        .children(active.then(|| {
            svg()
                .path("icon/ui/check.svg")
                .size(px(14.0))
                .text_color(theme.colors.text_link)
        }))
        .on_click(cx.listener(move |editor, _: &ClickEvent, window, cx| {
            pointer_focus_handle.focus(window);
            editor.status_bar.line_ending_menu_open = false;
            editor.normalize_line_endings(ending, cx);
            cx.notify();
            cx.stop_propagation();
        }))
        .on_key_down(
            cx.listener(move |editor, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "enter" | "space" => {
                        editor.status_bar.line_ending_menu_open = false;
                        editor.normalize_line_endings(ending, cx);
                        cx.stop_propagation();
                    }
                    "escape" => {
                        editor.status_bar.line_ending_menu_open = false;
                        button_focus_handle.focus(window);
                        cx.notify();
                        cx.stop_propagation();
                    }
                    "up" => {
                        keyboard_focus_handles[(index + 2) % 3].focus(window);
                        cx.stop_propagation();
                    }
                    "down" => {
                        keyboard_focus_handles[(index + 1) % 3].focus(window);
                        cx.stop_propagation();
                    }
                    _ => {}
                }
            }),
        )
        .into_any_element()
}

pub(super) fn render_mode_switch(
    state: &mut StatusBarState,
    view_mode: super::ViewMode,
    available_modes: &[super::ViewMode],
    json_document: bool,
    theme: &Theme,
    strings: &I18nStrings,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let d = &theme.dimensions;
    if available_modes.len() == 1 {
        // 单一模式是当前文档能力的静态状态，不伪装成可展开的选择器。
        state.mode_menu_open = false;
        let mode = available_modes[0];
        let selector = format!("status-bar-mode-{mode:?}");
        return div()
            .id("status-bar-mode-picker")
            .debug_selector(|| "status-bar-mode-picker".to_owned())
            .h(px(d.status_bar_height))
            .flex()
            .items_center()
            .child(
                div()
                    .id("status-bar-mode-switch")
                    .debug_selector(|| "status-bar-mode-switch".to_owned())
                    .size(px(d.status_bar_height))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .text_color(theme.colors.status_bar_text)
                    .child(
                        div()
                            .id(ElementId::Name(selector.clone().into()))
                            .debug_selector(move || selector.clone())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                svg()
                                    .path(mode_icon(mode))
                                    .size(px(15.0))
                                    .text_color(theme.colors.status_bar_text),
                            ),
                    ),
            )
            .into_any_element();
    }
    let open = state.mode_menu_open;
    let button_focus_handle = state
        .mode_button_focus_handle
        .get_or_insert_with(|| cx.focus_handle())
        .clone();
    if state.mode_focus_handles.is_none() {
        state.mode_focus_handles = Some(std::array::from_fn(|_| cx.focus_handle()));
    }
    let focus_handles = state
        .mode_focus_handles
        .as_ref()
        .expect("status mode focus handles must be initialized")
        .clone();

    let menu_items = available_modes
        .iter()
        .copied()
        .enumerate()
        .map(|(index, mode)| {
            let label = match mode {
                super::ViewMode::Rendered if json_document => &strings.json_graph_live_edit,
                super::ViewMode::Rendered => &strings.status_bar_mode_rendered,
                super::ViewMode::Source => &strings.status_bar_mode_source,
                super::ViewMode::Split => &strings.status_bar_mode_split,
                super::ViewMode::Preview => &strings.status_bar_mode_preview,
            };
            render_mode_menu_item(
                view_mode,
                mode,
                label,
                index,
                available_modes.len(),
                focus_handles.clone(),
                button_focus_handle.clone(),
                theme,
                cx,
            )
        })
        .collect::<Vec<_>>();
    let current_label = match view_mode {
        super::ViewMode::Rendered if json_document => &strings.json_graph_live_edit,
        super::ViewMode::Rendered => &strings.status_bar_mode_rendered,
        super::ViewMode::Source => &strings.status_bar_mode_source,
        super::ViewMode::Split => &strings.status_bar_mode_split,
        super::ViewMode::Preview => &strings.status_bar_mode_preview,
    };
    let current_icon = mode_icon(view_mode);
    let pointer_focus_handle = button_focus_handle.clone();
    let keyboard_item_focus = focus_handles[0].clone();

    div()
        .id("status-bar-mode-picker")
        .debug_selector(|| "status-bar-mode-picker".to_owned())
        .relative()
        .h(px(d.status_bar_height))
        .flex()
        .items_center()
        .child(
            div()
                .id("status-bar-mode-switch")
                .debug_selector(|| "status-bar-mode-switch".to_owned())
                .relative()
                .size(px(d.status_bar_height))
                .tab_index(0)
                .track_focus(&button_focus_handle)
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .border(px(1.0))
                .border_color(hsla(0.0, 0.0, 0.0, 0.0))
                .bg(if open || state.mode_hovered == Some(view_mode) {
                    theme.colors.status_bar_button_hover
                } else {
                    hsla(0., 0., 0., 0.)
                })
                .cursor_pointer()
                .focus(|this| this.border_color(theme.colors.text_link))
                .text_color(theme.colors.status_bar_text)
                .child(
                    svg()
                        .path(current_icon)
                        .size(px(15.0))
                        .text_color(theme.colors.status_bar_text),
                )
                .children(
                    (!open && state.tooltip_visible == Some(StatusTooltip::Mode(view_mode))).then(
                        || {
                            status_bar_tooltip(
                                current_label.to_owned(),
                                theme,
                                StatusTooltipAlignment::End,
                                "status-bar-mode-tooltip".to_owned(),
                            )
                        },
                    ),
                )
                .on_hover(cx.listener(move |editor, hovered: &bool, _window, cx| {
                    editor.set_status_mode_tooltip_hover(view_mode, *hovered, cx);
                }))
                .on_click(cx.listener(move |editor, _: &ClickEvent, window, cx| {
                    pointer_focus_handle.focus(window);
                    editor.status_bar.format_overflow_open = false;
                    editor.status_bar.line_ending_menu_open = false;
                    editor.status_bar.mode_menu_open = !editor.status_bar.mode_menu_open;
                    cx.notify();
                }))
                .on_key_down(
                    cx.listener(move |editor, event: &KeyDownEvent, window, cx| {
                        match event.keystroke.key.as_str() {
                            "enter" | "space" => {
                                editor.status_bar.format_overflow_open = false;
                                editor.status_bar.mode_menu_open =
                                    !editor.status_bar.mode_menu_open;
                                if editor.status_bar.mode_menu_open {
                                    keyboard_item_focus.focus(window);
                                }
                                cx.notify();
                                cx.stop_propagation();
                            }
                            "escape" if editor.status_bar.mode_menu_open => {
                                editor.status_bar.mode_menu_open = false;
                                cx.notify();
                                cx.stop_propagation();
                            }
                            _ => {}
                        }
                    }),
                ),
        )
        .children(open.then(|| {
            div()
                .id("status-bar-mode-menu")
                .debug_selector(|| "status-bar-mode-menu".to_owned())
                .absolute()
                .right(px(0.0))
                .bottom(px(d.status_bar_height + 4.0))
                .min_w(px(120.0))
                .occlude()
                .p(px(4.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .bg(theme.colors.dialog_surface)
                .border(px(d.dialog_border_width))
                .border_color(theme.colors.dialog_border)
                .rounded(px(8.0))
                .shadow_lg()
                .children(menu_items)
        }))
        .into_any_element()
}

fn render_mode_menu_item(
    current: super::ViewMode,
    mode: super::ViewMode,
    label: &str,
    index: usize,
    item_count: usize,
    focus_handles: [FocusHandle; 4],
    button_focus_handle: FocusHandle,
    theme: &Theme,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let active = current == mode;
    let icon = mode_icon(mode);
    let focus_handle = focus_handles[index].clone();
    let pointer_focus_handle = focus_handle.clone();
    let keyboard_focus_handles = focus_handles.clone();
    div()
        .id(SharedString::from(format!("status-bar-mode-{mode:?}")))
        .debug_selector(move || format!("status-bar-mode-{mode:?}"))
        .h(px(30.0))
        .w_full()
        .px(px(8.0))
        .tab_index(0)
        .track_focus(&focus_handle)
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(6.0))
        .border(px(1.0))
        .border_color(hsla(0.0, 0.0, 0.0, 0.0))
        .bg(if active {
            theme.colors.status_bar_button_hover
        } else {
            hsla(0., 0., 0., 0.)
        })
        .cursor_pointer()
        .focus(|this| this.border_color(theme.colors.text_link))
        .text_color(theme.colors.status_bar_text)
        .child(
            svg()
                .path(icon)
                .size(px(15.0))
                .text_color(theme.colors.status_bar_text),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(theme.dimensions.status_bar_text_size))
                .text_color(theme.colors.text_default)
                .child(label.to_owned()),
        )
        .children(active.then(|| {
            svg()
                .path("icon/ui/check.svg")
                .size(px(14.0))
                .text_color(theme.colors.text_link)
                .debug_selector(move || format!("status-bar-mode-{mode:?}-indicator"))
        }))
        .on_click(cx.listener(move |editor, _: &ClickEvent, window, cx| {
            pointer_focus_handle.focus(window);
            editor.status_bar.mode_menu_open = false;
            editor.activate_status_view_mode(mode, window, cx);
        }))
        .on_key_down(
            cx.listener(move |editor, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "enter" | "space" => {
                        editor.status_bar.mode_menu_open = false;
                        editor.activate_status_view_mode(mode, window, cx);
                        cx.stop_propagation();
                    }
                    "escape" => {
                        editor.status_bar.mode_menu_open = false;
                        button_focus_handle.focus(window);
                        cx.notify();
                        cx.stop_propagation();
                    }
                    "up" => {
                        keyboard_focus_handles[(index + item_count - 1) % item_count].focus(window);
                        cx.stop_propagation();
                    }
                    "down" => {
                        keyboard_focus_handles[(index + 1) % item_count].focus(window);
                        cx.stop_propagation();
                    }
                    _ => {}
                }
            }),
        )
        .into_any_element()
}

fn mode_icon(mode: super::ViewMode) -> &'static str {
    match mode {
        super::ViewMode::Rendered => LIVE_MODE_ICON,
        super::ViewMode::Source => SOURCE_MODE_ICON,
        super::ViewMode::Split => SPLIT_MODE_ICON,
        super::ViewMode::Preview => PREVIEW_MODE_ICON,
    }
}

#[derive(Clone, Copy)]
enum StatusTooltipAlignment {
    Start,
    End,
}

fn status_bar_tooltip(
    label: String,
    theme: &Theme,
    alignment: StatusTooltipAlignment,
    debug_selector: String,
) -> AnyElement {
    let tooltip = div()
        .id("status-bar-tooltip")
        .debug_selector(move || debug_selector.clone())
        .absolute()
        .bottom(px(theme.dimensions.status_bar_height + 2.0))
        .min_w(px(72.0))
        .max_w(px(200.0))
        .h(px(26.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .bg(theme.colors.dialog_surface)
        .border(px(theme.dimensions.dialog_border_width))
        .border_color(theme.colors.dialog_border)
        .shadow_md()
        .text_size(px(theme.dimensions.status_bar_text_size))
        .text_color(theme.colors.text_default)
        .whitespace_nowrap()
        .child(label);
    match alignment {
        StatusTooltipAlignment::Start => tooltip.left(px(0.0)),
        StatusTooltipAlignment::End => tooltip.right(px(0.0)),
    }
    .into_any_element()
}

pub(super) fn render_cursor((line, col): (usize, usize), theme: &Theme) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;

    let label = format!("{} : {}", &line.to_string(), &col.to_string());

    div()
        .id("status-bar-cursor")
        .debug_selector(|| "status-bar-cursor".to_owned())
        .text_size(px(d.status_bar_text_size))
        .text_color(c.status_bar_text)
        .child(label)
        .into_any_element()
}

pub(super) fn render_character_count(
    selection_count: Option<usize>,
    total_count: usize,
    theme: &Theme,
    strings: &I18nStrings,
) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;

    let label = if let Some(sel) = selection_count {
        format!(
            "{} / {} {}",
            sel, total_count, strings.status_bar_word_count_suffix
        )
    } else {
        format!("{} {}", total_count, strings.status_bar_word_count_suffix)
    };

    div()
        .id("status-bar-word-count")
        .debug_selector(|| "status-bar-word-count".to_owned())
        .text_size(px(d.status_bar_text_size))
        .text_color(c.status_bar_text_dim)
        .child(label)
        .into_any_element()
}

pub(super) fn render_custom_button(
    state: &mut StatusBarState,
    button: &StatusBarButton,
    theme: &Theme,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;

    let id = button.id.clone();
    let action_id = button.action_id.clone();
    let key_action_id = action_id.clone();
    let debug_id = format!("status-bar-custom-button-{}", button.id);
    let element_id = debug_id.clone();
    let hovered = state.custom_button_hovered.as_deref() == Some(&button.id);
    let focus_handle = state
        .custom_button_focus_handles
        .entry(button.id.clone())
        .or_insert_with(|| cx.focus_handle())
        .clone();
    let pointer_focus_handle = focus_handle.clone();

    div()
        .id(ElementId::Name(element_id.into()))
        .debug_selector(move || debug_id.clone())
        .h(px(d.status_bar_height))
        .tab_index(0)
        .track_focus(&focus_handle)
        .px(px(6.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .border(px(1.0))
        .border_color(hsla(0.0, 0.0, 0.0, 0.0))
        .bg(if hovered {
            c.status_bar_button_hover
        } else {
            hsla(0., 0., 0., 0.)
        })
        .cursor_pointer()
        .focus(|this| this.border_color(c.text_link))
        .text_size(px(d.status_bar_text_size))
        .text_color(c.status_bar_text)
        .child(button.label.clone())
        .on_hover(cx.listener(
            move |editor: &mut Editor,
                  hovered: &bool,
                  _window: &mut Window,
                  cx: &mut Context<Editor>| {
                if *hovered {
                    editor.status_bar.custom_button_hovered = Some(id.clone());
                } else if editor.status_bar.custom_button_hovered.as_deref() == Some(&id) {
                    editor.status_bar.custom_button_hovered = None;
                }
                cx.notify();
            },
        ))
        .on_click(cx.listener(move |editor, _: &ClickEvent, window, cx| {
            pointer_focus_handle.focus(window);
            editor.status_bar.format_overflow_open = false;
            let action = status_bar_action(&action_id, window, cx);
            cx.notify();
            if let Some(action) = action {
                window.dispatch_action(action, cx);
            }
        }))
        .on_key_down(
            cx.listener(move |editor, event: &KeyDownEvent, window, cx| {
                if !matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    return;
                }
                editor.status_bar.format_overflow_open = false;
                let action = status_bar_action(&key_action_id, window, cx);
                cx.notify();
                if let Some(action) = action {
                    window.dispatch_action(action, cx);
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
}

fn status_bar_action(action_id: &str, window: &Window, cx: &App) -> Option<Box<dyn Action>> {
    let requested = action_id.trim();
    if requested.is_empty() {
        return None;
    }
    window.available_actions(cx).into_iter().find(|action| {
        action.name() == requested || normalized_action_id(action.name()) == requested
    })
}

pub(super) fn normalized_action_id(name: &str) -> String {
    let name = name.rsplit("::").next().unwrap_or(name);
    let mut normalized = String::with_capacity(name.len() + 8);
    let mut previous_was_lowercase_or_digit = false;
    for ch in name.chars() {
        if matches!(ch, '-' | ' ' | '.') {
            if !normalized.ends_with('_') {
                normalized.push('_');
            }
            previous_was_lowercase_or_digit = false;
        } else if ch.is_uppercase() {
            if previous_was_lowercase_or_digit && !normalized.ends_with('_') {
                normalized.push('_');
            }
            normalized.extend(ch.to_lowercase());
            previous_was_lowercase_or_digit = false;
        } else {
            normalized.push(ch);
            previous_was_lowercase_or_digit = ch.is_lowercase() || ch.is_ascii_digit();
        }
    }
    normalized.trim_matches('_').to_owned()
}

/// 统计用户感知字符；CRLF、组合音标和 ZWJ emoji 都只占一个字符。
/// 空格与换行仍属于文档内容，行为与纯文本编辑器的字符统计一致。
pub fn count_characters(text: &str) -> usize {
    text.graphemes(true).count()
}
