// @author kongweiguang

use super::*;

impl Block {
    /// 渲染脚注定义序号、正文与返回引用入口。
    pub(super) fn render_footnote_definition_content(
        &mut self,
        focused_base: Stateful<Div>,
        focused: bool,
        is_placeholder: bool,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let ordinal = self.footnote_definition_ordinal();
        let badge = ordinal
            .map(|ordinal| ordinal.to_string())
            .unwrap_or_else(|| "?".to_string());
        let badge_text_size = px((t.code_size - 1.0).max(10.0));
        let header = focused_base
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(d.list_marker_gap))
            .text_size(px(t.code_size))
            .text_color(c.text_quote)
            .child(
                div()
                    .px(px(d.footnote_badge_padding_x))
                    .py(px(d.footnote_badge_padding_y))
                    .rounded(px(999.0))
                    .bg(c.footnote_badge_bg)
                    .text_size(badge_text_size)
                    .text_color(c.footnote_badge_text)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(badge)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_grow()
                    .text_color(c.text_quote)
                    .child(self.render_text_or_mixed_inline_visuals(
                        theme,
                        focused,
                        is_placeholder,
                        None,
                        None,
                        c.text_quote,
                        t.code_size,
                        FontWeight::NORMAL,
                        cx,
                    )),
            );

        if self.footnote_definition_has_backref() {
            let backref_tooltip: SharedString = strings.footnote_back_to_reference.clone().into();
            header
                .child(
                    div()
                        .id(ElementId::Name(
                            format!("footnote-backref-{}", self.record.id).into(),
                        ))
                        .debug_selector(|| "footnote-backref".to_owned())
                        .size(px(20.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.0))
                        .text_color(c.footnote_backref)
                        .hover(|this| this.text_color(c.text_link).bg(c.chrome_hover))
                        .cursor_pointer()
                        .tooltip(move |_window, cx| {
                            crate::ui::ui_tooltip(backref_tooltip.clone(), cx)
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(Self::on_footnote_backref_mouse_down),
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(Self::on_footnote_backref_mouse_up),
                        )
                        .child(
                            svg()
                                .path(FOOTNOTE_BACKREF_ICON)
                                .size(px(14.0))
                                .debug_selector(|| "footnote-backref-icon".to_owned()),
                        ),
                )
                .into_any_element()
        } else {
            header.into_any_element()
        }
    }
}
