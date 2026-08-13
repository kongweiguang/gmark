// @author kongweiguang

//! Shared visual treatment for icon-only control tooltips.

use gpui::*;

use crate::ui::theme::ThemeManager;

struct UiTooltip {
    label: SharedString,
}

impl Render for UiTooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<ThemeManager>().current();
        div()
            .id("ui-tooltip")
            .debug_selector(|| "ui-tooltip".to_owned())
            .max_w(px(280.0))
            .px(px(8.0))
            .py(px(5.0))
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .rounded(px(5.0))
            .border(px(theme.dimensions.dialog_border_width))
            .border_color(theme.colors.dialog_border)
            .bg(theme.colors.dialog_surface)
            .text_size(px((theme.dimensions.menu_text_size - 1.0).max(10.0)))
            .text_color(theme.colors.dialog_secondary_button_text)
            .shadow_md()
            .child(self.label.clone())
    }
}

/// GPUI owns hover delay and viewport placement; the view only owns Gmark's
/// restrained visual treatment so every icon-only control is consistent.
pub(crate) fn ui_tooltip(label: impl Into<SharedString>, cx: &mut App) -> AnyView {
    let label = label.into();
    cx.new(|_| UiTooltip { label }).into()
}
