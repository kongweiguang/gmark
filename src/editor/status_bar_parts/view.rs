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

/// 统计用户感知字符；CRLF、组合音标和 ZWJ emoji 都只占一个字符。
/// 空格与换行仍属于文档内容，行为与纯文本编辑器的字符统计一致。
pub(crate) fn count_characters(text: &str) -> usize {
    text.graphemes(true).count()
}

#[path = "view/line_endings.rs"]
mod line_endings;
#[path = "view/metrics.rs"]
mod metrics;
#[path = "view/mode.rs"]
mod mode;
#[path = "view/sidebars.rs"]
mod sidebars;

pub(super) use line_endings::render_line_ending_picker;
#[cfg(test)]
pub(super) use metrics::normalized_action_id;
pub(super) use metrics::{render_character_count, render_cursor, render_custom_button};
pub(super) use mode::render_mode_switch;
pub(super) use sidebars::{
    render_document_sidebar_toggle, render_recovery_status, render_sidebar_toggle,
    should_render_file_status,
};
