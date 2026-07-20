// @author kongweiguang

use super::*;

pub(super) fn render_workspace_tooltip(label: String, top: f32, theme: &Theme) -> AnyElement {
    div()
        .id("workspace-tooltip")
        .debug_selector(|| "workspace-tooltip".to_owned())
        .absolute()
        .top(px(top))
        .left(px(0.0))
        .min_w(px(76.0))
        .h(px(26.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .whitespace_nowrap()
        .rounded(px(5.0))
        .bg(theme.colors.dialog_surface)
        .border(px(theme.dimensions.dialog_border_width))
        .border_color(theme.colors.dialog_border)
        .shadow_md()
        .text_size(px(theme.typography.text_size * 0.78))
        .text_color(theme.colors.text_default)
        .child(label)
        .into_any_element()
}
