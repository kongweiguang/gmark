// @author kongweiguang

use super::*;

impl Block {
    /// 渲染 callout 标题、图标与折叠状态。
    pub(super) fn render_callout_content(
        &mut self,
        focused_base: Stateful<Div>,
        focused: bool,
        is_placeholder: bool,
        variant: crate::components::CalloutVariant,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let d = &theme.dimensions;
        let t = &theme.typography;
        let (accent, _) = callout_accent_and_background(variant, theme);
        let title_is_empty = self.record.title.visible_text().is_empty();
        let show_static_default_label = title_is_empty && !focused;
        let header_label = SharedString::from(variant.label());
        let header_text = if show_static_default_label {
            div()
                .text_size(px(t.text_size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(accent)
                .child(header_label.clone())
                .into_any_element()
        } else {
            div()
                .min_w(px(0.0))
                .flex_grow()
                .text_size(px(t.text_size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(accent)
                .child(self.render_text_or_mixed_inline_visuals(
                    theme,
                    focused,
                    is_placeholder,
                    Some(header_label),
                    Some(accent),
                    accent,
                    t.text_size,
                    FontWeight::SEMIBOLD,
                    cx,
                ))
                .into_any_element()
        };

        focused_base
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(d.callout_header_gap))
            .child(
                div()
                    .id(ElementId::Name(
                        format!("callout-icon-{}-{}", variant.marker(), self.record.id).into(),
                    ))
                    .debug_selector(move || {
                        format!("callout-icon-{}", variant.marker().to_ascii_lowercase())
                    })
                    .size(px(18.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(accent)
                    .child(svg().path(callout_icon(variant)).size(px(15.0))),
            )
            .child(header_text)
            .into_any_element()
    }
}
