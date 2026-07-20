// @author kongweiguang

//! Shared compact chrome primitives.

use gpui::*;

use crate::theme::ThemeManager;

/// 根据视口宽度在全宽与聚焦阅读宽度之间线性插值。
pub(crate) fn centered_column_ratio(
    viewport_width: f32,
    dimensions: &crate::theme::ThemeDimensions,
) -> f32 {
    if viewport_width <= dimensions.centered_shrink_start {
        return 1.0;
    }
    let progress = ((viewport_width - dimensions.centered_shrink_start)
        / (dimensions.centered_shrink_end - dimensions.centered_shrink_start))
        .clamp(0.0, 1.0);
    1.0 - progress * (1.0 - dimensions.centered_min_ratio)
}

/// 计算编辑器、块内容与大文件源码面共享的内容列宽度。
pub(crate) fn centered_column_width(
    viewport_width: f32,
    dimensions: &crate::theme::ThemeDimensions,
) -> f32 {
    let available_content_width = (viewport_width - dimensions.editor_padding * 2.0).max(1.0);
    let centered_ratio = centered_column_ratio(viewport_width, dimensions);
    (available_content_width * centered_ratio)
        .max(320.0)
        .min(dimensions.centered_max_width.max(320.0))
        .min(available_content_width)
}

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

/// GPUI owns hover delay and viewport placement; the view only owns gmark's
/// restrained visual treatment so every icon-only control is consistent.
pub(crate) fn ui_tooltip(label: impl Into<SharedString>, cx: &mut App) -> AnyView {
    let label = label.into();
    cx.new(|_| UiTooltip { label }).into()
}
