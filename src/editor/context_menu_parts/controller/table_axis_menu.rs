// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn render_axis_menu_item(
        theme: &Theme,
        id: &'static str,
        label: String,
        icon: &'static str,
        enabled: bool,
        keyboard_selected: bool,
        danger: bool,
        on_click: fn(&mut Editor, &ClickEvent, &mut Window, &mut Context<Editor>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let text_color = if danger {
            c.dialog_danger_button_bg
        } else if enabled {
            c.dialog_secondary_button_text
        } else {
            c.dialog_muted
        };
        let row = div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .h(px(d.menu_item_height))
            .px(px(d.menu_item_padding_x))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(d.menu_item_radius))
            .bg(if keyboard_selected {
                c.dialog_secondary_button_hover
            } else {
                c.dialog_surface
            })
            .text_size(px(d.menu_text_size))
            .font_weight(t.dialog_body_weight.to_font_weight())
            .text_color(text_color)
            .child(
                menu_icon_slot(Some(icon), text_color).debug_selector(move || format!("{id}-icon")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .on_hover(cx.listener(Self::on_context_menu_pointer_hover));
        if enabled {
            row.hover(|this| this.bg(c.dialog_secondary_button_hover))
                .cursor_pointer()
                .on_click(cx.listener(on_click))
                .into_any_element()
        } else {
            row.into_any_element()
        }
    }
}
