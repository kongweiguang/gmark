// @author kongweiguang

use super::super::*;

pub(super) fn footnote_group_shell(
    children: Vec<AnyElement>,
    theme: &Theme,
    dimensions: &ThemeDimensions,
) -> AnyElement {
    div()
        .w_full()
        .flex_shrink_0()
        .px(px(crate::components::rendered_content_inset(dimensions)))
        .child(
            div()
                .debug_selector(|| "footnote-surface".to_owned())
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(0.0))
                .px(px(dimensions.footnote_padding_x))
                .py(px(dimensions.footnote_padding_y))
                .rounded(px(dimensions.footnote_radius))
                // 脚注是正文的补充层，不使用完整卡片描边；细左轨即可表达归属。
                .border_l(px(2.0))
                .border_color(theme.colors.footnote_border)
                .bg(theme.colors.footnote_bg)
                .children(children),
        )
        .into_any_element()
}
