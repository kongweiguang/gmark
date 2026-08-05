// @author kongweiguang

//! Editable primitive-value rows inside JSON graph cards.

use super::panel_state::JsonGraphRenderContext;
use super::support::field_edit_target_for_identity;
use super::*;
use gpui::AnyElement;

pub(super) fn render_json_graph_field_row(
    context: &JsonGraphRenderContext,
    field: &JsonGraphField,
    row_height: f32,
    cx: &mut Context<DocumentHost>,
) -> AnyElement {
    let colors = &cx.global::<ThemeManager>().current_arc().colors;
    let edit_target = field_edit_target_for_identity(
        context.projection_epoch,
        context.projection_revision,
        field,
    );
    let field_id = field.id.clone();
    let field_source = field.source.range.clone();
    let row_selected = context
        .selected_id
        .as_ref()
        .is_some_and(|selected_id| selected_id == &field.id);
    let field_value_color = context.palette.value(field.kind);
    let field_label: SharedString = field.label.to_string().into();
    let field_value: SharedString = field.display_value.to_string().into();
    div()
        .id(SharedString::from(format!(
            "json-graph-field-element-{}",
            field.id.as_str()
        )))
        .debug_selector({
            let id = field.id.as_str().to_owned();
            move || format!("json-graph-field-{id}")
        })
        .relative()
        .h(px(row_height))
        .px(px(10.0 * context.zoom))
        .flex()
        .items_center()
        .gap(px(6.0 * context.zoom))
        .border_t(px(1.0))
        .border_color(colors.workbench.border_subtle.opacity(0.58))
        .bg(if row_selected {
            context.palette.accent.opacity(0.11)
        } else {
            context.palette.surface
        })
        .text_size(px((11.0 * context.zoom).clamp(8.5, 16.0)))
        .cursor_pointer()
        .child(
            div()
                .id(SharedString::from(format!(
                    "json-graph-field-label-{}",
                    field.id.as_str()
                )))
                .max_w(relative(0.46))
                .overflow_hidden()
                .truncate()
                .text_color(context.palette.text)
                .tooltip({
                    let text = field_label.clone();
                    move |_window, cx| crate::ui::ui_tooltip(text.clone(), cx)
                })
                .child(field_label),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "json-graph-field-value-{}",
                    field.id.as_str()
                )))
                .min_w(px(0.0))
                .flex_1()
                .overflow_hidden()
                .truncate()
                .text_color(field_value_color)
                .tooltip({
                    let text = field_value.clone();
                    move |_window, cx| crate::ui::ui_tooltip(text.clone(), cx)
                })
                .child(field_value),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "json-graph-field-hit-{}",
                    field.id.as_str()
                )))
                .debug_selector({
                    let id = field.id.as_str().to_owned();
                    move || format!("json-graph-field-hit-{id}")
                })
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.select_json_graph_item(
                            field_id.clone(),
                            field_source.clone(),
                            window,
                            cx,
                        );
                        if event.click_count >= 2 {
                            this.begin_json_graph_edit(edit_target.clone(), window, cx);
                        }
                    }),
                ),
        )
        .into_any_element()
}
