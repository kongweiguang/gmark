// @author kongweiguang

use super::*;

const FOLD_BUTTON_SIZE: f32 = 18.0;
const HEADING_FOLD_GUTTER_OFFSET: f32 = 22.0;

impl Block {
    pub(super) fn render_fold_button(
        &self,
        theme: &Theme,
        heading: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let key = self.presentation_fold_key.as_ref()?.to_string();
        let collapsed = self.presentation_collapsed;
        let focus_handle = self.fold_focus_handle.clone();
        let label = if collapsed { "展开" } else { "折叠" };
        let icon = if collapsed {
            "icon/ui/chevron-right.svg"
        } else {
            "icon/ui/chevron-down.svg"
        };
        let accent = theme.colors.workbench.text_secondary;
        let focus_ring = theme.colors.workbench.focus_ring;
        Some(
            div()
                .id(ElementId::Name(
                    format!("markdown-fold-{}-{}", self.record.id, heading).into(),
                ))
                .debug_selector(move || {
                    if heading {
                        "heading-fold-button".to_owned()
                    } else {
                        "callout-fold-button".to_owned()
                    }
                })
                .tab_index(0)
                .track_focus(&focus_handle)
                .cursor_pointer()
                .size(px(FOLD_BUTTON_SIZE))
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .text_color(accent)
                .hover(|this| this.bg(theme.colors.workbench.control_hover))
                .focus(|this| this.bg(focus_ring.opacity(0.22)))
                .tooltip(move |_window, cx| crate::ui::ui_tooltip(label, cx))
                .on_click(cx.listener({
                    let key = key.clone();
                    let focus_handle = focus_handle.clone();
                    move |_block, _event, window, cx| {
                        focus_handle.focus(window);
                        cx.emit(BlockEvent::RequestToggleCollapse {
                            key: key.clone(),
                            heading,
                        });
                        cx.stop_propagation();
                    }
                }))
                .on_key_down(cx.listener({
                    let key = key.clone();
                    let focus_handle = focus_handle.clone();
                    move |_block, event: &KeyDownEvent, window, cx| match event
                        .keystroke
                        .key
                        .as_str()
                    {
                        "enter" | "space" => {
                            focus_handle.focus(window);
                            cx.emit(BlockEvent::RequestToggleCollapse {
                                key: key.clone(),
                                heading,
                            });
                            cx.stop_propagation();
                        }
                        "escape" => {
                            _block.focus_handle.focus(window);
                            cx.stop_propagation();
                        }
                        _ => {}
                    }
                }))
                .child(svg().path(icon).size(px(12.0)).text_color(accent))
                .into_any_element(),
        )
    }

    fn render_heading_title_with_gutter(
        &mut self,
        theme: &Theme,
        focused: bool,
        is_placeholder: bool,
        text_color: Hsla,
        text_size: f32,
        font_weight: FontWeight,
        fold_button: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.render_text_or_mixed_inline_visuals(
            theme,
            focused,
            is_placeholder,
            None,
            None,
            text_color,
            text_size,
            font_weight,
            cx,
        );
        let fold_top =
            ((text_size * theme.typography.text_line_height - FOLD_BUTTON_SIZE) / 2.0).max(0.0);
        let fold_slot = fold_button.map(|button| {
            div()
                .debug_selector(|| "heading-fold-slot".to_owned())
                .absolute()
                .left(px(-HEADING_FOLD_GUTTER_OFFSET))
                .top(px(fold_top))
                .size(px(FOLD_BUTTON_SIZE))
                .child(button)
        });

        div()
            .debug_selector(|| "heading-fold-title".to_owned())
            .relative()
            .w_full()
            .min_w(px(0.0))
            .children(fold_slot)
            .child(title)
            .into_any_element()
    }

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
        let fold_button = self.render_fold_button(theme, true, cx);
        let (text_color, text_size, font_weight) = match level {
            1 => (c.text_h1, t.h1_size, t.h1_weight.to_font_weight()),
            2 => (c.text_h2, t.h2_size, t.h2_weight.to_font_weight()),
            3 => (c.text_h3, t.h3_size, t.h3_weight.to_font_weight()),
            4 => (c.text_h4, t.h4_size, t.h4_weight.to_font_weight()),
            5 => (c.text_h5, t.h5_size, t.h5_weight.to_font_weight()),
            6 => (c.text_h6, t.h6_size, t.h6_weight.to_font_weight()),
            _ => unreachable!("heading level is normalized to 1..=6"),
        };
        let title = self.render_heading_title_with_gutter(
            theme,
            focused,
            is_placeholder,
            text_color,
            text_size,
            font_weight,
            fold_button,
            cx,
        );

        focused_base
            .text_size(px(text_size))
            .font_weight(font_weight)
            .text_color(text_color)
            // H1/H2 只靠字号与留白建立层级，不自动制造贯穿内容区的横线。
            .when(level <= 2, |base| {
                base.mb(px(d.h1_margin_bottom + d.h1_padding_bottom))
            })
            .child(title)
            .into_any_element()
    }
}
