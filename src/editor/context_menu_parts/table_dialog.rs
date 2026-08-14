// @author kongweiguang

use super::*;
use crate::theme::workbench::SurfaceKind;
use crate::ui::visual_preferences::VisualPreferencesManager;

impl Editor {
    /// 表格插入保持业务步进控件，但把正文和操作区交给标准弹框几何，避免窄窗自定义高度裁掉按钮。
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
                            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                                cx.stop_propagation()
                            })
                            .child(
                                crate::editor::render::dialog_content(
                                    "table-insert-dialog-content",
                                    theme,
                                )
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
                                compact_dialog_actions(theme)
                                    .id("table-insert-dialog-actions")
                                    .debug_selector(|| "table-insert-dialog-actions".to_owned())
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
