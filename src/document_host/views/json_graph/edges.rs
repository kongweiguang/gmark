// @author kongweiguang

//! Canvas painting for JSON graph edges and the reference grid.

use super::model::edge_intersects_viewport;
use super::panel_state::JsonGraphRenderContext;
use super::*;
use gpui::{AnyElement, PathBuilder, canvas, point};

pub(super) fn render_json_graph_edges(
    context: &JsonGraphRenderContext,
    graph_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
) -> AnyElement {
    let selected_edge_color = context.palette.accent.opacity(0.96);
    let grid_color = context.palette.grid;
    let camera_x = context.camera_x;
    let camera_y = context.camera_y;
    let zoom = context.zoom;
    let selected_node = context.selected_node_index.is_some();
    let edge_paths = context
        .layout
        .edges
        .iter()
        .filter(|edge| {
            edge_intersects_viewport(
                edge,
                camera_x,
                camera_y,
                zoom,
                context.viewport_width,
                context.viewport_height,
            )
        })
        .map(|edge| {
            let from = point(
                px(camera_x + edge.from_x * zoom),
                px(camera_y + edge.from_y * zoom),
            );
            let to = point(
                px(camera_x + edge.to_x * zoom),
                px(camera_y + edge.to_y * zoom),
            );
            let branch = context
                .branch_by_index
                .get(edge.to_index)
                .copied()
                .flatten();
            (
                from,
                to,
                context.selected_edges.contains(&edge.edge_index),
                context.palette.branch(branch, context.palette.edge),
            )
        })
        .collect::<Vec<_>>();
    let graph_bounds_for_prepaint = graph_bounds.clone();
    canvas(
        move |bounds, _, _| {
            if let Ok(mut current) = graph_bounds_for_prepaint.lock() {
                *current = Some(bounds);
            }
        },
        move |bounds, _, window, _| {
            let spacing = (32.0 * zoom).clamp(18.0, 56.0);
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            let mut grid = PathBuilder::stroke(px(1.0));
            let mut x = camera_x.rem_euclid(spacing);
            while x <= width {
                let mut y = camera_y.rem_euclid(spacing);
                while y <= height {
                    let center = point(bounds.origin.x + px(x), bounds.origin.y + px(y));
                    grid.move_to(point(center.x - px(1.5), center.y));
                    grid.line_to(point(center.x + px(1.5), center.y));
                    grid.move_to(point(center.x, center.y - px(1.5)));
                    grid.line_to(point(center.x, center.y + px(1.5)));
                    y += spacing;
                }
                x += spacing;
            }
            if let Ok(path) = grid.build() {
                window.paint_path(path, grid_color);
            }
            for (from, to, selected, branch_color) in &edge_paths {
                let from = point(bounds.origin.x + from.x, bounds.origin.y + from.y);
                let to = point(bounds.origin.x + to.x, bounds.origin.y + to.y);
                let control = (f32::from(to.x - from.x) * 0.5).max(24.0);
                let mut builder = PathBuilder::stroke(px(if *selected { 1.8 } else { 1.1 }));
                builder.move_to(from);
                builder.cubic_bezier_to(
                    to,
                    point(from.x + px(control), from.y),
                    point(to.x - px(control), to.y),
                );
                if let Ok(path) = builder.build() {
                    window.paint_path(
                        path,
                        if *selected {
                            selected_edge_color
                        } else if selected_node {
                            branch_color.opacity(0.28)
                        } else {
                            branch_color.opacity(0.62)
                        },
                    );
                }
            }
        },
    )
    .absolute()
    .size_full()
    .into_any_element()
}
