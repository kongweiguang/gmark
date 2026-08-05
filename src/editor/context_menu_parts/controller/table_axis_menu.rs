// @author kongweiguang

use super::*;
use crate::theme::workbench::SurfaceKind;
use crate::ui::visual_preferences::VisualPreferencesManager;

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
        let visual_preferences = cx
            .try_global::<VisualPreferencesManager>()
            .map(VisualPreferencesManager::current)
            .unwrap_or_default();
        let palette = &c.workbench;
        let material = palette.material(SurfaceKind::Glass, visual_preferences);
        let d = &theme.dimensions;
        let t = &theme.typography;
        let text_color = if danger {
            palette.danger
        } else if enabled {
            palette.text_primary
        } else {
            palette.text_secondary
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
                palette.control_hover
            } else {
                material.background
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
            row.hover(|this| this.bg(palette.control_hover))
                .cursor_pointer()
                .on_click(cx.listener(on_click))
                .into_any_element()
        } else {
            row.into_any_element()
        }
    }
}
