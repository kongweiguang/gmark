// @author kongweiguang

use super::*;
use crate::theme::workbench::SurfaceKind;
use crate::ui::visual_preferences::VisualPreferencesManager;

impl Editor {
    pub(in crate::editor) fn render_table_insert_dialog_overlay(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.table_insert_dialog.as_ref()?;
        let c = &theme.colors;
        let visual_preferences = cx
            .try_global::<VisualPreferencesManager>()
            .map(VisualPreferencesManager::current)
            .unwrap_or_default();
        let palette = &c.workbench;
        let material = palette.material(SurfaceKind::GlassStrong, visual_preferences);
        let d = &theme.dimensions;
        let t = &theme.typography;
        let s = cx.global::<I18nManager>().strings().clone();
        let panel_padding_y = d.table_insert_stepper_gap + 4.0;
        let dialog_min_height = (
            // GPUI 的自动高度不会稳定计入横向操作行中的定高按钮，因此由组成 token
            // 明确给出单行文案下的固有高度；较长本地化文案仍可继续撑高面板。
            panel_padding_y * 2.0
                + 22.0
                + t.dialog_body_size * t.text_line_height
                + d.table_insert_stepper_button_size * 2.0
                + d.table_insert_stepper_gap * 3.0
                + d.table_insert_stepper_gap * 0.5
                + d.dialog_button_height
                + panel_padding_y
                // GPUI 的横向定高按钮不完整参与父级固有高度；除面板底部 padding 外，
                // 还要补足紧凑操作行和设备像素取整预算，避免 2× Runner 裁掉按钮。
                + d.table_insert_stepper_gap * 4.0
                + d.dialog_border_width * 6.0
        )
        .ceil();

        let stepper =
            |id_prefix: &'static str,
             label: String,
             value: usize,
             on_dec: fn(&mut Editor, &ClickEvent, &mut Window, &mut Context<Editor>),
             on_inc: fn(&mut Editor, &ClickEvent, &mut Window, &mut Context<Editor>)| {
                div()
                    .w_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_grow()
                            .pr(px(d.table_insert_stepper_gap))
                            .text_size(px(t.dialog_body_size))
                            .font_weight(t.dialog_button_weight.to_font_weight())
                            .text_color(palette.text_primary)
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(d.table_insert_stepper_gap))
                            .child(
                                div()
                                    .id((id_prefix, 0usize))
                                    .size(px(d.table_insert_stepper_button_size))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(d.table_insert_stepper_radius))
                                    .border(px(d.dialog_border_width))
                                    .border_color(material.border)
                                    .bg(palette.control_surface)
                                    .hover(|this| this.bg(palette.control_hover))
                                    .cursor_pointer()
                                    .text_color(palette.text_primary)
                                    .on_click(cx.listener(on_dec))
                                    // GPUI 的 SVG 不继承父容器文本色，必须直接着色以保证深浅主题可见。
                                    .child(
                                        svg()
                                            .path(MINUS_ICON)
                                            .size(px(14.0))
                                            .text_color(palette.text_primary),
                                    ),
                            )
                            .child(
                                div()
                                    .min_w(px(d.table_insert_stepper_value_min_width))
                                    .h(px(d.table_insert_stepper_button_size))
                                    .px(px(d.table_insert_stepper_value_padding_x))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(d.table_insert_stepper_radius))
                                    .border(px(d.dialog_border_width))
                                    .border_color(material.border)
                                    .bg(material.background)
                                    .text_size(px(t.dialog_body_size))
                                    .text_color(palette.text_primary)
                                    .child(value.to_string()),
                            )
                            .child(
                                div()
                                    .id((id_prefix, 1usize))
                                    .size(px(d.table_insert_stepper_button_size))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(d.table_insert_stepper_radius))
                                    .border(px(d.dialog_border_width))
                                    .border_color(material.border)
                                    .bg(palette.control_surface)
                                    .hover(|this| this.bg(palette.control_hover))
                                    .cursor_pointer()
                                    .text_color(palette.text_primary)
                                    .on_click(cx.listener(on_inc))
                                    .child(
                                        svg()
                                            .path(PLUS_ICON)
                                            .size(px(14.0))
                                            .text_color(palette.text_primary),
                                    ),
                            ),
                    )
            };

        Some(
            modal_overlay("table-insert-dialog-overlay", theme)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(Self::on_dismiss_context_menu_overlay),
                )
                .child(
                    div()
                        .w_full()
                        .px(px(d.editor_padding))
                        .flex()
                        .justify_center()
                        .child(
                            dialog_panel(
                                "table-insert-dialog",
                                d.dialog_width.min(d.table_insert_dialog_width),
                                theme,
                            )
                            .min_h(px(dialog_min_height))
                            // 紧凑弹窗在低高度窗口中仍需为操作按钮保留完整底部内边距。
                            .py(px(panel_padding_y))
                            .gap(px(d.table_insert_stepper_gap * 0.5))
                            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                                cx.stop_propagation()
                            })
                            .child(
                                // 短表单必须参与弹窗固有高度计算；共享滚动容器的 flex 布局会裁掉末行。
                                div()
                                    .id("table-insert-dialog-content")
                                    .debug_selector(|| "table-insert-dialog-content".to_owned())
                                    .w_full()
                                    .flex_none()
                                    .flex()
                                    .flex_col()
                                    .gap(px(d.table_insert_stepper_gap))
                                    .child(dialog_title_with_icon(
                                        "table-insert-title",
                                        s.table_insert_title.clone(),
                                        DialogTitleIcon::Table,
                                        theme,
                                    ))
                                    .child(dialog_body(s.table_insert_description.clone(), theme))
                                    .child(stepper(
                                        "table-body-rows",
                                        s.table_insert_body_rows.clone(),
                                        dialog.body_rows,
                                        Self::on_table_rows_decrement,
                                        Self::on_table_rows_increment,
                                    ))
                                    .child(stepper(
                                        "table-columns",
                                        s.table_insert_columns.clone(),
                                        dialog.columns,
                                        Self::on_table_columns_decrement,
                                        Self::on_table_columns_increment,
                                    )),
                            )
                            .child(
                                dialog_actions(theme)
                                    .id("table-insert-dialog-actions")
                                    .debug_selector(|| "table-insert-dialog-actions".to_owned())
                                    .min_h(px(d.dialog_button_height + panel_padding_y))
                                    .pt(px(panel_padding_y))
                                    .child(
                                        dialog_button(
                                            "cancel-table-insert-dialog",
                                            s.table_insert_cancel.clone(),
                                            DialogButtonKind::Secondary,
                                            theme,
                                        )
                                        .on_click(cx.listener(Self::on_cancel_table_insert_dialog)),
                                    )
                                    .child(
                                        dialog_button(
                                            "confirm-table-insert-dialog",
                                            s.table_insert_confirm.clone(),
                                            DialogButtonKind::Primary,
                                            theme,
                                        )
                                        .on_click(
                                            cx.listener(Self::on_confirm_table_insert_dialog),
                                        ),
                                    ),
                            ),
                        ),
                )
                .into_any_element(),
        )
    }
}
