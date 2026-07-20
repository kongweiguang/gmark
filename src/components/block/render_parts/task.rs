// @author kongweiguang

use super::*;

impl Block {
    /// 渲染任务项复选框与正文。
    pub(super) fn render_task_list_content(
        &mut self,
        focused_base: Stateful<Div>,
        focused: bool,
        is_placeholder: bool,
        checked: bool,
        showing_rendered_image: bool,
        theme: &Theme,
        strings: &I18nStrings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let marker_width = d.list_marker_width.max(d.task_checkbox_size);
        let first_line_height = t.text_size * t.text_line_height;
        let checkbox = div()
            .id(ElementId::Name(
                format!("task-checkbox-{}", self.record.id).into(),
            ))
            .debug_selector(|| "task-checkbox".to_owned())
            .size(px(d.task_checkbox_size))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(d.task_checkbox_radius))
            .border(px(d.task_checkbox_border_width))
            .border_color(c.task_checkbox_border)
            .bg(if checked {
                c.task_checkbox_checked_bg
            } else {
                c.task_checkbox_bg
            })
            .text_color(c.task_checkbox_check);
        let checkbox = if self.is_read_only() {
            checkbox
        } else {
            checkbox
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(Self::on_task_checkbox_mouse_down),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(Self::on_task_checkbox_mouse_up),
                )
        };
        focused_base
            .text_size(px(t.text_size))
            .text_color(c.text_default)
            .line_height(rems(t.text_line_height))
            .w_full()
            .flex()
            .flex_row()
            .items_start()
            .gap(px(d.list_marker_gap))
            .children([
                div()
                    .w(px(marker_width))
                    .h(px(first_line_height))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(checkbox.children(checked.then(|| {
                        svg()
                            .path(CHECK_ICON)
                            .size(px(d.task_checkbox_check_size))
                            .debug_selector(|| "task-checkbox-check".to_owned())
                    }))),
                if showing_rendered_image {
                    let viewport_width = f32::from(window.viewport_size().width.max(px(1.0)));
                    let resize_basis_width =
                        effective_list_item_image_width(self, viewport_width, d);
                    if let Some(mut runtime) = self.image_runtime().cloned() {
                        let width_percent = self.current_image_width_percent();
                        runtime.width_percent = width_percent;
                        let max_width =
                            Length::Definite(relative(f32::from(width_percent) / 100.0));
                        div()
                            .min_w(px(0.0))
                            .flex_grow()
                            .child(self.render_image_content(
                                runtime,
                                max_width,
                                px(d.image_root_max_height),
                                px(d.image_root_placeholder_height),
                                resize_basis_width,
                                theme,
                                strings,
                                cx,
                            ))
                    } else {
                        div().min_w(px(0.0)).flex_grow().child(
                            self.render_text_or_mixed_inline_visuals(
                                theme,
                                focused,
                                is_placeholder,
                                None,
                                None,
                                c.text_default,
                                t.text_size,
                                FontWeight::NORMAL,
                                cx,
                            ),
                        )
                    }
                } else {
                    div().min_w(px(0.0)).flex_grow().child(
                        self.render_text_or_mixed_inline_visuals(
                            theme,
                            focused,
                            is_placeholder,
                            None,
                            None,
                            c.text_default,
                            t.text_size,
                            FontWeight::NORMAL,
                            cx,
                        ),
                    )
                },
            ])
            .into_any_element()
    }
}
