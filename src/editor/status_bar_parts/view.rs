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
    label: &'static str,
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
    recovered_session: bool,
    external_file_conflict: bool,
) -> bool {
    recovered_session || external_file_conflict
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
    let glyph_offset_y = if has_conflict { 0.0 } else { -1.0 };
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
                .child(
                    svg()
                        .path(icon)
                        .size(px(14.0))
                        .relative()
                        .top(px(glyph_offset_y))
                        .text_color(color),
                ),
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
        .text_color(c.status_bar_text)
        .child(
            svg()
                .path(SIDEBAR_ICON)
                .size(px(15.0))
                .text_color(c.status_bar_text),
        )
        .children(is_open.then(|| {
            div()
                .absolute()
                .left(px(4.0))
                .right(px(4.0))
                .bottom(px(-1.0))
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

pub(super) fn render_mode_switch(
    state: &mut StatusBarState,
    view_mode: super::ViewMode,
    source_only: bool,
    theme: &Theme,
    strings: &I18nStrings,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let d = &theme.dimensions;
    if state.mode_focus_handles.is_none() {
        state.mode_focus_handles = Some(std::array::from_fn(|_| cx.focus_handle()));
    }
    let focus_handles = state
        .mode_focus_handles
        .as_ref()
        .expect("status mode focus handles must be initialized")
        .clone();

    let segments = if source_only {
        vec![render_mode_segment(
            state,
            super::ViewMode::Source,
            super::ViewMode::Source,
            &strings.status_bar_mode_source,
            focus_handles[1].clone(),
            theme,
            cx,
        )]
    } else {
        vec![
            render_mode_segment(
                state,
                view_mode,
                super::ViewMode::Rendered,
                &strings.status_bar_mode_rendered,
                focus_handles[0].clone(),
                theme,
                cx,
            ),
            render_mode_segment(
                state,
                view_mode,
                super::ViewMode::Source,
                &strings.status_bar_mode_source,
                focus_handles[1].clone(),
                theme,
                cx,
            ),
            render_mode_segment(
                state,
                view_mode,
                super::ViewMode::Split,
                &strings.status_bar_mode_split,
                focus_handles[2].clone(),
                theme,
                cx,
            ),
            render_mode_segment(
                state,
                view_mode,
                super::ViewMode::Preview,
                &strings.status_bar_mode_preview,
                focus_handles[3].clone(),
                theme,
                cx,
            ),
        ]
    };

    div()
        .id("status-bar-mode-switch")
        .debug_selector(|| "status-bar-mode-switch".to_owned())
        .h(px(d.status_bar_height))
        .flex()
        .items_center()
        .gap(px(1.0))
        .rounded(px(4.0))
        .bg(theme.colors.status_bar_button_hover.opacity(0.45))
        .children(segments)
        .into_any_element()
}

fn render_mode_segment(
    state: &StatusBarState,
    current: super::ViewMode,
    mode: super::ViewMode,
    label: &str,
    focus_handle: FocusHandle,
    theme: &Theme,
    cx: &mut Context<Editor>,
) -> AnyElement {
    let active = current == mode;
    let hovered = state.mode_hovered == Some(mode);
    let icon = match mode {
        super::ViewMode::Rendered => LIVE_MODE_ICON,
        super::ViewMode::Source => SOURCE_MODE_ICON,
        super::ViewMode::Split => SPLIT_MODE_ICON,
        super::ViewMode::Preview => PREVIEW_MODE_ICON,
    };
    let pointer_focus_handle = focus_handle.clone();
    div()
        .id(SharedString::from(format!("status-bar-mode-{mode:?}")))
        .debug_selector(move || format!("status-bar-mode-{mode:?}"))
        .relative()
        .size(px(theme.dimensions.status_bar_height))
        .tab_index(0)
        .track_focus(&focus_handle)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.0))
        .border(px(1.0))
        .border_color(hsla(0.0, 0.0, 0.0, 0.0))
        .bg(if active || hovered {
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
        .children(active.then(|| {
            div()
                .absolute()
                .left(px(4.0))
                .right(px(4.0))
                .bottom(px(-1.0))
                .h(px(2.0))
                .rounded(px(1.0))
                .bg(theme.colors.text_link)
                .debug_selector(move || format!("status-bar-mode-{mode:?}-indicator"))
        }))
        .children(
            (state.tooltip_visible == Some(StatusTooltip::Mode(mode))).then(|| {
                let alignment = if mode == super::ViewMode::Preview {
                    StatusTooltipAlignment::End
                } else {
                    StatusTooltipAlignment::Center
                };
                status_bar_tooltip(
                    label.to_owned(),
                    theme,
                    alignment,
                    format!("status-bar-mode-tooltip-{mode:?}"),
                )
            }),
        )
        .on_hover(cx.listener(move |editor, hovered: &bool, _window, cx| {
            editor.set_status_mode_tooltip_hover(mode, *hovered, cx);
        }))
        .on_click(cx.listener(move |editor, _: &ClickEvent, window, cx| {
            pointer_focus_handle.focus(window);
            editor.set_view_mode(mode, cx);
        }))
        .on_key_down(
            cx.listener(move |editor, event: &KeyDownEvent, _window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    editor.set_view_mode(mode, cx);
                    cx.stop_propagation();
                }
            }),
        )
        .into_any_element()
}

#[derive(Clone, Copy)]
enum StatusTooltipAlignment {
    Start,
    Center,
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
        StatusTooltipAlignment::Center => tooltip.left(px(-24.0)),
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
