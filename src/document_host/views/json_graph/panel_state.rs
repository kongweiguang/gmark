// @author kongweiguang

//! Per-frame JSON graph projection, layout, and camera state.

use super::layout_selection::selected_path_edges;
use super::model::{
    GraphLayout, GraphLayoutKey, MAX_ZOOM as GRAPH_MAX_ZOOM, MIN_ZOOM as GRAPH_MIN_ZOOM,
    READABLE_MIN_ZOOM, SEARCH_REVEAL_ZOOM, fit_camera, graph_layout, initial_collapsed_items,
};
use super::style::JsonGraphPalette;
use super::support::{
    bounded_graph_content, bounded_node_content, field_edit_target_for_identity,
    node_edit_target_for_identity,
};
use super::*;
use crate::theme::workbench::SurfaceKind;
use crate::ui::visual_preferences::VisualPreferencesManager;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct JsonGraphKeyboardNode {
    pub(super) id: JsonGraphItemId,
    pub(super) source: Range<u64>,
    pub(super) parent: Option<JsonGraphItemId>,
    pub(super) first_child: Option<JsonGraphItemId>,
}

#[derive(Clone)]
pub(super) struct JsonGraphSelectedDetail {
    pub(super) json_path: Arc<str>,
    pub(super) content: SharedString,
    pub(super) edit_target: JsonGraphEditTarget,
}

pub(super) struct JsonGraphRenderContext {
    pub(super) graph: JsonGraphProjection,
    pub(super) layout: Arc<GraphLayout>,
    pub(super) projection_epoch: u64,
    pub(super) projection_revision: u64,
    pub(super) viewport_width: f32,
    pub(super) viewport_height: f32,
    pub(super) camera_x: f32,
    pub(super) camera_y: f32,
    pub(super) zoom: f32,
    pub(super) query: String,
    pub(super) index_by_id: HashMap<JsonGraphItemId, usize>,
    pub(super) selected_id: Option<JsonGraphItemId>,
    pub(super) selected_node_index: Option<usize>,
    pub(super) keyboard_nodes: Vec<JsonGraphKeyboardNode>,
    pub(super) keyboard_selected_position: Option<usize>,
    pub(super) selected_detail: Option<JsonGraphSelectedDetail>,
    pub(super) palette: JsonGraphPalette,
    pub(super) collapsed: HashSet<Arc<str>>,
    pub(super) branch_by_index: Vec<Option<usize>>,
    pub(super) selected_edges: HashSet<usize>,
    pub(super) row_limits: HashMap<JsonGraphItemId, usize>,
}

impl DocumentHost {
    pub(super) fn render_json_graph_empty_state(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = cx.global::<ThemeManager>().current_arc();
        let strings = cx.global::<I18nManager>().strings().clone();
        let colors = &theme.colors;
        let visual_preferences = cx
            .try_global::<VisualPreferencesManager>()
            .map(VisualPreferencesManager::current)
            .unwrap_or_default();
        let content_material = colors
            .workbench
            .material(SurfaceKind::Editor, visual_preferences);
        let control_material = colors
            .workbench
            .material(SurfaceKind::Glass, visual_preferences);
        let (title, detail): (SharedString, SharedString) =
            if let Some(error) = &self.derived_projection_error {
                (
                    strings.json_graph_preview_unavailable.clone().into(),
                    error.clone(),
                )
            } else {
                (
                    strings.json_graph_generating.clone().into(),
                    strings.json_graph_generating_detail.clone().into(),
                )
            };
        div()
            .id("json-graph-empty-state")
            .debug_selector(|| "json-graph-empty-state".to_owned())
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .bg(content_material.background)
            .text_color(colors.workbench.text_primary)
            .child(div().text_size(px(14.0)).child(title))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(colors.workbench.text_tertiary)
                    .child(detail),
            )
            .children(self.derived_projection_error_offset.map(|offset| {
                div()
                    .id("json-graph-error-jump")
                    .debug_selector(|| "json-graph-error-jump".to_owned())
                    .mt(px(4.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .bg(control_material.background)
                    .hover(|button| button.bg(colors.workbench.control_hover))
                    .child(
                        strings
                            .json_graph_locate_byte_template
                            .replace("{offset}", &offset.to_string()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.jump_byte_offset_to_source(offset, cx);
                        cx.emit(DocumentHostEvent::ViewModeChanged(DocumentHostMode::Source));
                    }))
            }))
    }

    pub(super) fn prepare_json_graph_render_context(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) -> Option<JsonGraphRenderContext> {
        let installed_snapshot = self
            .derived_projection_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.as_any().downcast_ref::<JsonGraphSnapshot>())?;
        let graph = installed_snapshot.projection().clone();
        let projection_epoch = installed_snapshot.document_epoch();
        let projection_revision = installed_snapshot.revision();
        let projection_identity = (
            projection_epoch,
            projection_revision,
            installed_snapshot.generation(),
        );
        if self.graph_projection_identity != Some(projection_identity) {
            self.graph_projection_identity = Some(projection_identity);
            self.graph_row_limits.clear();
            self.graph_layout_cache = None;
            self.graph_state_initialized = false;
            self.graph_needs_fit = true;
            self.graph_fit_all_requested = false;
        }

        let view_id = DocumentViewId::json_graph();
        let view_state = document_view_state_mut(&mut self.document, &mut self.tab_view_state)
            .derived
            .entry(view_id)
            .or_default();
        let viewport = (viewport_width.max(1.0), viewport_height.max(1.0));
        if self.graph_last_viewport.is_none_or(|last| {
            (last.0 - viewport.0).abs() > 1.0 || (last.1 - viewport.1).abs() > 1.0
        }) {
            if let Some(last) = self.graph_last_viewport
                && !self.graph_needs_fit
            {
                let zoom = view_state.zoom.max(f32::EPSILON);
                let world_x = (last.0 * 0.5 - view_state.camera_x) / zoom;
                let world_y = (last.1 * 0.5 - view_state.camera_y) / zoom;
                view_state.camera_x = viewport.0 * 0.5 - world_x * zoom;
                view_state.camera_y = viewport.1 * 0.5 - world_y * zoom;
            }
            self.graph_last_viewport = Some(viewport);
        }
        if !self.graph_state_initialized {
            if view_state.collapsed_items.is_empty() {
                view_state.collapsed_items =
                    initial_collapsed_items(&graph, &self.graph_row_limits);
            }
            self.graph_state_initialized = true;
        }
        let collapsed = view_state
            .collapsed_items
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        let layout_key = GraphLayoutKey::new(
            projection_identity.0,
            projection_identity.1,
            projection_identity.2,
            &collapsed,
            &self.graph_row_limits,
        );
        let layout = if let Some(cache) = self
            .graph_layout_cache
            .as_ref()
            .filter(|cache| cache.key == layout_key)
        {
            cache.layout.clone()
        } else {
            let layout = Arc::new(graph_layout(&graph, &collapsed, &self.graph_row_limits));
            self.graph_layout_cache = Some(GraphLayoutCache {
                key: layout_key,
                layout: layout.clone(),
            });
            layout
        };
        if self.graph_needs_fit
            || (view_state.camera_x == 0.0 && view_state.camera_y == 0.0 && view_state.zoom == 1.0)
        {
            let minimum_zoom = if self.graph_fit_all_requested {
                GRAPH_MIN_ZOOM
            } else {
                READABLE_MIN_ZOOM
            };
            let (x, y, zoom) = fit_camera(&layout, viewport_width, viewport_height, minimum_zoom);
            view_state.camera_x = x;
            view_state.camera_y = y;
            view_state.zoom = zoom;
            self.graph_needs_fit = false;
            self.graph_fit_all_requested = false;
        }
        if let Some((anchor_id, anchor_position)) = self.graph_recenter_anchor.take()
            && let Some(position) = layout
                .nodes
                .iter()
                .find(|position| graph.nodes[position.index].id == anchor_id)
        {
            view_state.camera_x = f32::from(anchor_position.x)
                - (position.x + position.width * 0.5) * view_state.zoom;
            view_state.camera_y = f32::from(anchor_position.y)
                - (position.y + position.height * 0.5) * view_state.zoom;
        }
        if let Some(target) = self.graph_pending_center.take()
            && let Some(position) = layout
                .nodes
                .iter()
                .find(|position| graph.nodes[position.index].id == target)
        {
            view_state.zoom = view_state.zoom.max(SEARCH_REVEAL_ZOOM);
            view_state.camera_x =
                viewport_width * 0.5 - (position.x + position.width * 0.5) * view_state.zoom;
            view_state.camera_y =
                viewport_height * 0.5 - (position.y + position.height * 0.5) * view_state.zoom;
        }
        let camera_x = view_state.camera_x;
        let camera_y = view_state.camera_y;
        let zoom = view_state.zoom.clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM);
        let query = self
            .structured_filter_input
            .read(cx)
            .display_text()
            .to_lowercase();
        let index_by_id = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.clone(), index))
            .collect::<HashMap<_, _>>();
        let selected_id = self.graph_selected_item.clone();
        let selected_node_index = selected_id.as_ref().and_then(|id| {
            index_by_id.get(id).copied().or_else(|| {
                graph
                    .nodes
                    .iter()
                    .position(|node| node.fields.iter().any(|field| field.id == *id))
            })
        });
        let keyboard_nodes = layout
            .visible_order
            .iter()
            .filter_map(|index| {
                let node = graph.nodes.get(*index)?;
                let parent = layout
                    .parent_by_node
                    .get(*index)
                    .and_then(|parent| *parent)
                    .and_then(|parent| graph.nodes.get(parent))
                    .map(|parent| parent.id.clone());
                let first_child = layout
                    .children_by_node
                    .get(*index)
                    .and_then(|children| children.first())
                    .and_then(|child| graph.nodes.get(*child))
                    .map(|child| child.id.clone());
                Some(JsonGraphKeyboardNode {
                    id: node.id.clone(),
                    source: node.source.range.clone(),
                    parent,
                    first_child,
                })
            })
            .collect::<Vec<_>>();
        let keyboard_selected_position = selected_node_index.and_then(|selected| {
            keyboard_nodes
                .iter()
                .position(|node| graph.nodes[selected].id == node.id)
        });
        let selected_detail = selected_id.as_ref().and_then(|selected| {
            graph.nodes.iter().find_map(|node| {
                if node.id == *selected {
                    return Some(JsonGraphSelectedDetail {
                        json_path: node.json_path.clone(),
                        content: bounded_node_content(self.document.as_ref(), node),
                        edit_target: node_edit_target_for_identity(
                            projection_epoch,
                            projection_revision,
                            node,
                        ),
                    });
                }
                let field = node.fields.iter().find(|field| field.id == *selected)?;
                Some(JsonGraphSelectedDetail {
                    json_path: field.json_path.clone(),
                    content: bounded_graph_content(
                        self.document.as_ref(),
                        field.source.range.clone(),
                        &field.label,
                    ),
                    edit_target: field_edit_target_for_identity(
                        projection_epoch,
                        projection_revision,
                        field,
                    ),
                })
            })
        });
        let palette =
            JsonGraphPalette::from_theme(&cx.global::<ThemeManager>().current_arc().colors);
        let selected_edges = selected_path_edges(&layout, selected_node_index);
        let mut branch_by_index = vec![None; graph.nodes.len()];
        for node in &layout.nodes {
            branch_by_index[node.index] = node.branch;
        }

        Some(JsonGraphRenderContext {
            graph,
            layout,
            projection_epoch,
            projection_revision,
            viewport_width,
            viewport_height,
            camera_x,
            camera_y,
            zoom,
            query,
            index_by_id,
            selected_id,
            selected_node_index,
            keyboard_nodes,
            keyboard_selected_position,
            selected_detail,
            palette,
            collapsed,
            branch_by_index,
            selected_edges,
            row_limits: self.graph_row_limits.clone(),
        })
    }
}
