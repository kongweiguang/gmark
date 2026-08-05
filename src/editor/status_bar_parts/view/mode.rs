// @author kongweiguang

use super::*;

pub(in crate::editor) fn render_mode_switch(
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
                    .text_color(theme.colors.workbench.text_secondary)
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
                                    .text_color(theme.colors.workbench.icon),
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
                    theme.colors.workbench.control_hover
                } else {
                    hsla(0., 0., 0., 0.)
                })
                .cursor_pointer()
                .focus(|this| this.border_color(theme.colors.workbench.focus_ring))
                .text_color(theme.colors.workbench.text_secondary)
                .child(
                    svg()
                        .path(current_icon)
                        .size(px(15.0))
                        .text_color(theme.colors.workbench.icon),
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
                .bg(theme.colors.workbench.elevated_surface)
                .border(px(d.dialog_border_width))
                .border_color(theme.colors.workbench.border_subtle)
                .rounded(px(10.0))
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
            theme.colors.workbench.control_hover
        } else {
            hsla(0., 0., 0., 0.)
        })
        .cursor_pointer()
        .focus(|this| this.border_color(theme.colors.workbench.focus_ring))
        .text_color(theme.colors.workbench.text_secondary)
        .child(
            svg()
                .path(icon)
                .size(px(15.0))
                .text_color(theme.colors.workbench.icon),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(theme.dimensions.status_bar_text_size))
                .text_color(theme.colors.workbench.text_primary)
                .child(label.to_owned()),
        )
        .children(active.then(|| {
            svg()
                .path("icon/ui/check.svg")
                .size(px(14.0))
                .text_color(theme.colors.workbench.accent)
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
