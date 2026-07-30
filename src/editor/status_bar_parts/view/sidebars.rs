// @author kongweiguang

use super::*;

pub(in crate::editor) fn should_render_file_status(
    _recovered_session: bool,
    external_file_conflict: bool,
) -> bool {
    // 启动恢复是自动完成的正常流程，保持静默；这里只保留需要用户处理的外部冲突入口。
    external_file_conflict
}

pub(in crate::editor) fn render_recovery_status(
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

pub(in crate::editor) fn render_sidebar_toggle(
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

pub(in crate::editor) fn render_document_sidebar_toggle(
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
