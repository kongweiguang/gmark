// @author kongweiguang

//! Virtualized JSON graph cards and their editable rows.

use super::card_rows::render_json_graph_field_row;
use super::model::{
    CARD_HEADER_HEIGHT as GRAPH_CARD_HEADER_HEIGHT, CARD_ROW_HEIGHT as GRAPH_CARD_ROW_HEIGHT,
    PositionedGraphNode, ROW_LIMIT_STEP, node_intersects_viewport, row_limit,
};
use super::panel_state::JsonGraphRenderContext;
use super::support::{
    GraphCardRow, graph_card_rows, json_graph_node_matches_query, node_edit_target_for_identity,
};
use super::*;
use gpui::AnyElement;
use std::collections::HashMap;

pub(super) fn render_json_graph_nodes(
    context: &JsonGraphRenderContext,
    graph_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    cx: &mut Context<DocumentHost>,
) -> Vec<AnyElement> {
    let mut outgoing_by_parent = HashMap::<&str, Vec<&JsonGraphEdge>>::new();
    for edge in context.graph.edges.iter() {
        outgoing_by_parent
            .entry(edge.from.as_str())
            .or_default()
            .push(edge);
    }
    context
        .layout
        .nodes
        .iter()
        .filter(|position| {
            node_intersects_viewport(
                position,
                context.camera_x,
                context.camera_y,
                context.zoom,
                context.viewport_width,
                context.viewport_height,
            )
        })
        .map(|position| {
            render_json_graph_card(context, position, &outgoing_by_parent, &graph_bounds, cx)
        })
        .collect()
}

fn render_json_graph_card(
    context: &JsonGraphRenderContext,
    position: &PositionedGraphNode,
    outgoing_by_parent: &HashMap<&str, Vec<&JsonGraphEdge>>,
    graph_bounds: &Arc<Mutex<Option<Bounds<Pixels>>>>,
    cx: &mut Context<DocumentHost>,
) -> AnyElement {
    let theme = cx.global::<ThemeManager>().current_arc();
    let strings = cx.global::<I18nManager>().strings_arc();
    let colors = &theme.colors;
    let node = &context.graph.nodes[position.index];
    let id = node.id.clone();
    let source = node.source.range.clone();
    let node_kind = node.kind;
    let node_label = node.label.clone();
    let node_edit_range = node.source.range.clone();
    let projection_epoch = context.projection_epoch;
    let projection_revision = context.projection_revision;
    let collapsible = node.child_count > 0;
    let collapsed = context.collapsed.contains(node.id.as_str());
    let selected = context
        .selected_id
        .as_ref()
        .is_some_and(|selected_id| selected_id == &node.id);
    let matches_query =
        !context.query.is_empty() && json_graph_node_matches_query(node, &context.query);
    let branch_color = context
        .palette
        .branch(position.branch, colors.workbench.border_subtle);
    let left = context.camera_x + position.x * context.zoom;
    let top = context.camera_y + position.y * context.zoom;
    let width = position.width * context.zoom;
    let header_height = GRAPH_CARD_HEADER_HEIGHT * context.zoom;
    let row_height = GRAPH_CARD_ROW_HEIGHT * context.zoom;
    let toggle_id = id.clone();
    let context_id = id.clone();
    let context_bounds = graph_bounds.clone();
    let toggle_anchor = point(
        px(left + width * 0.5),
        px(top + position.height * context.zoom * 0.5),
    );
    let all_rows = graph_card_rows(
        node,
        outgoing_by_parent
            .get(node.id.as_str())
            .into_iter()
            .flatten()
            .copied(),
    );
    let visible_limit = row_limit(&node.id, &context.row_limits);
    let hidden_rows = all_rows.len().saturating_sub(visible_limit);
    let mut row_elements = all_rows
        .into_iter()
        .take(visible_limit)
        .map(|row| match row {
            GraphCardRow::Field(field) => {
                render_json_graph_field_row(context, field, row_height, cx)
            }
            GraphCardRow::Child(edge) => {
                render_json_graph_child_row(context, edge, branch_color, row_height, cx)
            }
        })
        .collect::<Vec<_>>();
    if hidden_rows > 0 {
        let reveal_id = id.clone();
        let reveal_anchor = toggle_anchor;
        let reveal_count = hidden_rows.min(ROW_LIMIT_STEP);
        row_elements.push(
            div()
                .id(SharedString::from(format!(
                    "json-graph-show-more-{}",
                    node.id.as_str()
                )))
                .debug_selector({
                    let id = node.id.as_str().to_owned();
                    move || format!("json-graph-show-more-{id}")
                })
                .h(px(row_height))
                .px(px(10.0 * context.zoom))
                .flex()
                .items_center()
                .justify_center()
                .border_t(px(1.0))
                .border_color(colors.workbench.border_subtle.opacity(0.58))
                .bg(context.palette.surface)
                .text_size(px((10.5 * context.zoom).clamp(8.5, 15.0)))
                .text_color(colors.workbench.accent)
                .cursor_pointer()
                .hover(|row| row.bg(colors.workbench.control_hover))
                .child(
                    strings
                        .json_graph_show_more_template
                        .replace("{count}", &reveal_count.to_string()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.graph_recenter_anchor = Some((reveal_id.clone(), reveal_anchor));
                    let limit = this
                        .graph_row_limits
                        .entry(reveal_id.clone())
                        .or_insert(model::DEFAULT_ROW_LIMIT);
                    *limit = limit.saturating_add(ROW_LIMIT_STEP);
                    this.graph_layout_cache = None;
                    cx.notify();
                }))
                .into_any_element(),
        );
    }
    div()
        .id(SharedString::from(format!(
            "json-graph-node-{}",
            node.id.as_str()
        )))
        .debug_selector({
            let id = node.id.as_str().to_owned();
            move || format!("json-graph-node-{id}")
        })
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(width))
        .rounded(px(10.0 * context.zoom.max(0.75)))
        .border(px(if selected || matches_query { 2.0 } else { 1.0 }))
        .border_color(if selected {
            context.palette.accent
        } else if matches_query {
            context.palette.search
        } else {
            branch_color.opacity(0.52)
        })
        .bg(context.palette.surface)
        .when(selected, |card| card.shadow_md())
        .cursor_pointer()
        .child(
            div()
                .h(px(header_height))
                .px(px(10.0 * context.zoom))
                .flex()
                .items_center()
                .justify_between()
                .rounded_t(px(9.0 * context.zoom.max(0.75)))
                .bg(if matches_query && !selected {
                    context.palette.search.opacity(0.13)
                } else {
                    branch_color.opacity(0.18)
                })
                .text_size(px((12.0 * context.zoom).clamp(9.0, 18.0)))
                .text_color(colors.workbench.text_primary)
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "json-graph-node-label-{}",
                            node.id.as_str()
                        )))
                        .min_w(px(0.0))
                        .truncate()
                        .tooltip({
                            let text: SharedString = node.label.to_string().into();
                            move |_window, cx| crate::ui::ui_tooltip(text.clone(), cx)
                        })
                        .child(node.label.to_string()),
                )
                .children(collapsible.then(|| {
                    div()
                        .id(SharedString::from(format!(
                            "json-graph-collapse-{}",
                            node.id.as_str()
                        )))
                        .size(px((20.0 * context.zoom).max(16.0)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.0))
                        .hover(|button| button.bg(colors.workbench.control_hover))
                        .child(if collapsed { "+" } else { "−" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.graph_recenter_anchor = Some((toggle_id.clone(), toggle_anchor));
                            let state = document_view_state_mut(
                                &mut this.document,
                                &mut this.tab_view_state,
                            )
                            .derived
                            .entry(DocumentViewId::json_graph())
                            .or_default();
                            if let Some(index) = state
                                .collapsed_items
                                .iter()
                                .position(|item| item.as_ref() == toggle_id.as_str())
                            {
                                state.collapsed_items.remove(index);
                            } else {
                                state.collapsed_items.push(Arc::from(toggle_id.as_str()));
                            }
                            cx.notify();
                        }))
                })),
        )
        .children(row_elements)
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                let origin = context_bounds
                    .lock()
                    .ok()
                    .and_then(|bounds| *bounds)
                    .map(|bounds| bounds.origin)
                    .unwrap_or_default();
                this.graph_context_menu = Some(JsonGraphContextMenu {
                    node: context_id.clone(),
                    position: point(event.position.x - origin.x, event.position.y - origin.y),
                });
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                cx.stop_propagation();
                this.graph_context_menu = None;
                this.select_json_graph_item(id.clone(), source.clone(), window, cx);
                if event.click_count() >= 2 {
                    this.begin_json_graph_edit(
                        JsonGraphEditTarget {
                            item_id: id.clone(),
                            range: node_edit_range.clone(),
                            document_epoch: projection_epoch,
                            base_revision: projection_revision,
                            label: node_label.clone(),
                            kind: node_kind,
                        },
                        window,
                        cx,
                    );
                }
            }),
        )
        .into_any_element()
}

fn render_json_graph_child_row(
    context: &JsonGraphRenderContext,
    edge: &JsonGraphEdge,
    branch_color: gpui::Hsla,
    row_height: f32,
    cx: &mut Context<DocumentHost>,
) -> AnyElement {
    let colors = &cx.global::<ThemeManager>().current_arc().colors;
    let child = context
        .index_by_id
        .get(&edge.to)
        .and_then(|index| context.graph.nodes.get(*index));
    let child_summary = child
        .map(|child| {
            let marker = match child.kind {
                JsonValueKind::Array => "[…]",
                JsonValueKind::Object => "{…}",
                _ => "→",
            };
            format!("{marker} · {}", child.fields.len() + child.child_count)
        })
        .unwrap_or_else(|| "→".to_owned());
    let child_id = edge.to.clone();
    let child_source = edge.source.range.clone();
    let edit_target = child.map(|child| {
        node_edit_target_for_identity(context.projection_epoch, context.projection_revision, child)
    });
    let row_selected = context
        .selected_id
        .as_ref()
        .is_some_and(|selected_id| selected_id == &edge.to);
    let child_branch = child
        .and_then(|child| context.index_by_id.get(&child.id))
        .and_then(|index| context.branch_by_index.get(*index).copied().flatten());
    let child_color = context.palette.branch(child_branch, branch_color);
    let child_label: SharedString = edge.label.to_string().into();
    let row_selector = format!("json-graph-child-row-{}", edge.parent_port.as_str());
    let port_selector = format!("json-graph-port-{}", edge.parent_port.as_str());
    div()
        .id(SharedString::from(row_selector.clone()))
        .debug_selector(move || row_selector.clone())
        .relative()
        .h(px(row_height))
        .pl(px(10.0 * context.zoom))
        .pr(px(14.0 * context.zoom))
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
                    "json-graph-child-label-{}",
                    edge.parent_port.as_str()
                )))
                .min_w(px(0.0))
                .flex_1()
                .overflow_hidden()
                .truncate()
                .text_color(context.palette.text)
                .tooltip({
                    let text = child_label.clone();
                    move |_window, cx| crate::ui::ui_tooltip(text.clone(), cx)
                })
                .child(child_label),
        )
        .child(
            div()
                .text_color(colors.workbench.text_tertiary)
                .child(child_summary),
        )
        .child(
            div()
                .id(SharedString::from(port_selector.clone()))
                .debug_selector(move || port_selector.clone())
                .absolute()
                .right(px(-5.0 * context.zoom))
                .size(px((10.0 * context.zoom).max(7.0)))
                .rounded_full()
                .border(px(1.0))
                .border_color(child_color.opacity(0.72))
                .bg(child_color.opacity(0.2)),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "json-graph-child-hit-{}",
                    edge.parent_port.as_str()
                )))
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
                            child_id.clone(),
                            child_source.clone(),
                            window,
                            cx,
                        );
                        if event.click_count >= 2
                            && let Some(target) = edit_target.clone()
                        {
                            this.begin_json_graph_edit(target, window, cx);
                        }
                    }),
                ),
        )
        .into_any_element()
}
