// @author kongweiguang

use super::*;

impl Block {
    /// 统一渲染六级标题，保证字号、字重与留白 token 的映射集中维护。
    pub(super) fn render_heading_content(
        &mut self,
        focused_base: Stateful<Div>,
        focused: bool,
        is_placeholder: bool,
        level: u8,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        match level {
            1 => focused_base
                .text_size(px(t.h1_size))
                .font_weight(t.h1_weight.to_font_weight())
                .text_color(c.text_h1)
                // H1 作为文档标题只靠字号与留白建立层级，不自动制造贯穿内容区的横线。
                .mb(px(d.h1_margin_bottom + d.h1_padding_bottom))
                .child(self.render_text_or_mixed_inline_visuals(
                    theme,
                    focused,
                    is_placeholder,
                    None,
                    None,
                    c.text_h1,
                    t.h1_size,
                    t.h1_weight.to_font_weight(),
                    cx,
                ))
                .into_any_element(),
            2 => focused_base
                .text_size(px(t.h2_size))
                .font_weight(t.h2_weight.to_font_weight())
                .text_color(c.text_h2)
                .mb(px(d.h1_margin_bottom + d.h1_padding_bottom))
                .child(self.render_text_or_mixed_inline_visuals(
                    theme,
                    focused,
                    is_placeholder,
                    None,
                    None,
                    c.text_h2,
                    t.h2_size,
                    t.h2_weight.to_font_weight(),
                    cx,
                ))
                .into_any_element(),
            3 => focused_base
                .text_size(px(t.h3_size))
                .font_weight(t.h3_weight.to_font_weight())
                .text_color(c.text_h3)
                .child(self.render_text_or_mixed_inline_visuals(
                    theme,
                    focused,
                    is_placeholder,
                    None,
                    None,
                    c.text_h3,
                    t.h3_size,
                    t.h3_weight.to_font_weight(),
                    cx,
                ))
                .into_any_element(),
            4 => focused_base
                .text_size(px(t.h4_size))
                .font_weight(t.h4_weight.to_font_weight())
                .text_color(c.text_h4)
                .child(self.render_text_or_mixed_inline_visuals(
                    theme,
                    focused,
                    is_placeholder,
                    None,
                    None,
                    c.text_h4,
                    t.h4_size,
                    t.h4_weight.to_font_weight(),
                    cx,
                ))
                .into_any_element(),
            5 => focused_base
                .text_size(px(t.h5_size))
                .font_weight(t.h5_weight.to_font_weight())
                .text_color(c.text_h5)
                .child(self.render_text_or_mixed_inline_visuals(
                    theme,
                    focused,
                    is_placeholder,
                    None,
                    None,
                    c.text_h5,
                    t.h5_size,
                    t.h5_weight.to_font_weight(),
                    cx,
                ))
                .into_any_element(),
            6 => focused_base
                .text_size(px(t.h6_size))
                .font_weight(t.h6_weight.to_font_weight())
                .text_color(c.text_h6)
                .child(self.render_text_or_mixed_inline_visuals(
                    theme,
                    focused,
                    is_placeholder,
                    None,
                    None,
                    c.text_h6,
                    t.h6_size,
                    t.h6_weight.to_font_weight(),
                    cx,
                ))
                .into_any_element(),
            _ => unreachable!("heading level is normalized to 1..=6"),
        }
    }
}
