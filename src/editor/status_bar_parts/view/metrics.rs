// @author kongweiguang

use super::*;

pub(in crate::editor) fn render_cursor((line, col): (usize, usize), theme: &Theme) -> AnyElement {
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

pub(in crate::editor) fn render_character_count(
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

pub(in crate::editor) fn render_custom_button(
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

pub(in crate::editor) fn normalized_action_id(name: &str) -> String {
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
