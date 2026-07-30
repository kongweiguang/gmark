// @author kongweiguang

//! Interactive JSON graph canvas composition and input handling.

use super::controls::JsonGraphControls;
use super::model::{MAX_ZOOM as GRAPH_MAX_ZOOM, MIN_ZOOM as GRAPH_MIN_ZOOM};
use super::overlays::JsonGraphOverlays;
use super::panel_state::JsonGraphRenderContext;
use super::support::zoom_camera_around;
use super::*;
use gpui::AnyElement;

pub(super) fn render_json_graph_canvas(
    host: &DocumentHost,
    context: &JsonGraphRenderContext,
    graph_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    edges: AnyElement,
    node_elements: Vec<AnyElement>,
    controls: JsonGraphControls,
    overlays: JsonGraphOverlays,
    cx: &mut Context<DocumentHost>,
) -> Stateful<Div> {
    let graph_background = div()
        .id("json-graph-background-hit-target")
        .debug_selector(|| "json-graph-background-hit-target".to_owned())
        .absolute()
        .size_full()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                if this.graph_selected_item.is_some() {
                    cx.stop_propagation();
                    this.dismiss_json_graph_details();
                    cx.notify();
                }
            }),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            if this.graph_selected_item.is_some() {
                cx.stop_propagation();
                this.dismiss_json_graph_details();
                cx.notify();
            }
        }));
    let graph_bounds_for_scroll = graph_bounds.clone();
    let graph_bounds_for_capture = graph_bounds.clone();
    let split_canvas = host.view_mode == DocumentHostViewMode::Split;
    let viewport_width = context.viewport_width;
    let viewport_height = context.viewport_height;
    let keyboard_nodes = context.keyboard_nodes.clone();
    let keyboard_selected_position = context.keyboard_selected_position;
    let palette = context.palette;
    let focus_border = cx.global::<ThemeManager>().current_arc().colors.text_link;
    div()
        .id("json-graph-canvas")
        .debug_selector(|| "json-graph-canvas".to_owned())
        .size_full()
        .relative()
        .overflow_hidden()
        .border(px(if split_canvas { 0.0 } else { 1.0 }))
        .border_color(hsla(0.0, 0.0, 0.0, 0.0))
        .bg(palette.canvas)
        .tab_index(0)
        .track_focus(&host.graph_focus_handle)
        .focus(move |canvas| {
            canvas.border_color(if split_canvas {
                hsla(0.0, 0.0, 0.0, 0.0)
            } else {
                focus_border
            })
        })
        .capture_any_mouse_down(
            cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                if this.graph_selected_item.is_none() {
                    return;
                }
                let origin = graph_bounds_for_capture
                    .lock()
                    .ok()
                    .and_then(|bounds| *bounds)
                    .map(|bounds| bounds.origin)
                    .unwrap_or_default();
                let x = f32::from(event.position.x - origin.x);
                let y = f32::from(event.position.y - origin.y);
                let wide = viewport_width >= 820.0;
                let left = if wide {
                    (viewport_width - 372.0).max(12.0)
                } else {
                    12.0
                };
                let right = viewport_width - 12.0;
                let top = if wide {
                    54.0
                } else {
                    (viewport_height - viewport_height.min(320.0) - 12.0).max(54.0)
                };
                let bottom = viewport_height - 12.0;
                if x < left || x > right || y < top || y > bottom {
                    this.dismiss_json_graph_details();
                    cx.notify();
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                this.graph_focus_handle.focus(window);
                this.graph_context_menu = None;
                this.dismiss_json_graph_details();
                let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
                    .derived
                    .entry(DocumentViewId::json_graph())
                    .or_default();
                this.graph_pan_session = Some((event.position, state.camera_x, state.camera_y));
                cx.notify();
            }),
        )
        .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
            if !event.dragging() {
                return;
            }
            let Some((origin, camera_x, camera_y)) = this.graph_pan_session else {
                return;
            };
            let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
                .derived
                .entry(DocumentViewId::json_graph())
                .or_default();
            state.camera_x = camera_x + f32::from(event.position.x - origin.x);
            state.camera_y = camera_y + f32::from(event.position.y - origin.y);
            cx.notify();
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                if this.graph_pan_session.take().is_some() {
                    cx.notify();
                }
            }),
        )
        .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
            let delta = event.delta.pixel_delta(px(28.0));
            let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
                .derived
                .entry(DocumentViewId::json_graph())
                .or_default();
            if event.modifiers.control || event.modifiers.platform {
                let old_zoom = state.zoom.clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM);
                let new_zoom = (old_zoom + (-f32::from(delta.y) / 700.0))
                    .clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM);
                let origin = graph_bounds_for_scroll
                    .lock()
                    .ok()
                    .and_then(|bounds| *bounds)
                    .map(|bounds| bounds.origin)
                    .unwrap_or_default();
                let pointer_x = f32::from(event.position.x - origin.x);
                let pointer_y = f32::from(event.position.y - origin.y);
                (state.camera_x, state.camera_y) = zoom_camera_around(
                    state.camera_x,
                    state.camera_y,
                    old_zoom,
                    new_zoom,
                    pointer_x,
                    pointer_y,
                );
                state.zoom = new_zoom;
            } else {
                state.camera_x += f32::from(delta.x);
                state.camera_y += f32::from(delta.y);
            }
            cx.notify();
        }))
        .on_key_down(
            cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                if key == "escape" {
                    if this.graph_selected_item.is_some() {
                        this.dismiss_json_graph_details();
                    } else if this.graph_context_menu.take().is_none() {
                        return;
                    }
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }

                let current = keyboard_selected_position.unwrap_or(0);
                let mut target = None;
                match key {
                    "up" if !keyboard_nodes.is_empty() => {
                        target = keyboard_nodes.get(current.saturating_sub(1)).cloned();
                    }
                    "down" if !keyboard_nodes.is_empty() => {
                        target = keyboard_nodes
                            .get((current + 1).min(keyboard_nodes.len() - 1))
                            .cloned();
                    }
                    "left" | "right" | "space" if !keyboard_nodes.is_empty() => {
                        let node = &keyboard_nodes[current];
                        let state =
                            document_view_state_mut(&mut this.document, &mut this.tab_view_state)
                                .derived
                                .entry(DocumentViewId::json_graph())
                                .or_default();
                        let collapsed = state
                            .collapsed_items
                            .iter()
                            .any(|item| item.as_ref() == node.id.as_str());
                        if key == "left" && node.first_child.is_some() && !collapsed {
                            state.collapsed_items.push(Arc::from(node.id.as_str()));
                        } else if key == "left" {
                            target = node.parent.as_ref().and_then(|parent| {
                                keyboard_nodes
                                    .iter()
                                    .find(|candidate| candidate.id == *parent)
                                    .cloned()
                            });
                        } else if key == "right" && collapsed {
                            state
                                .collapsed_items
                                .retain(|item| item.as_ref() != node.id.as_str());
                        } else if key == "right" {
                            target = node.first_child.as_ref().and_then(|child| {
                                keyboard_nodes
                                    .iter()
                                    .find(|candidate| candidate.id == *child)
                                    .cloned()
                            });
                        } else if node.first_child.is_some() {
                            if collapsed {
                                state
                                    .collapsed_items
                                    .retain(|item| item.as_ref() != node.id.as_str());
                            } else {
                                state.collapsed_items.push(Arc::from(node.id.as_str()));
                            }
                        }
                    }
                    "enter" if !keyboard_nodes.is_empty() => {
                        // 选中即展示检查器；无内部游标时 Enter 从首节点开始。
                        if keyboard_selected_position.is_none() {
                            target = keyboard_nodes.first().cloned();
                        }
                    }
                    _ => return,
                }
                if let Some(target) = target {
                    this.graph_pending_center = Some(target.id.clone());
                    this.select_json_graph_item(target.id, target.source, window, cx);
                }
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .child(edges)
        .child(graph_background)
        .children(node_elements)
        .child(controls.toolbar)
        .child(controls.zoom_toolbar)
        .children(controls.stale_banner)
        .children(controls.truncated_banner)
        .children(overlays.detail_panel)
        .children(overlays.context_menu)
}
