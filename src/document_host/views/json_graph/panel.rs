// @author kongweiguang

//! Composition root for the virtualized JSON graph view.

use super::canvas::render_json_graph_canvas;
use super::cards::render_json_graph_nodes;
use super::controls::render_json_graph_controls;
use super::edges::render_json_graph_edges;
use super::overlays::render_json_graph_overlays;
use super::*;

impl DocumentHost {
    pub(in crate::document_host::implementation) fn render_json_graph_panel(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let Some(context) =
            self.prepare_json_graph_render_context(viewport_width, viewport_height, cx)
        else {
            return self.render_json_graph_empty_state(cx);
        };
        let graph_bounds = Arc::new(Mutex::new(None));
        let edges = render_json_graph_edges(&context, graph_bounds.clone());
        let node_elements = render_json_graph_nodes(&context, graph_bounds.clone(), cx);
        let controls = render_json_graph_controls(self, &context, graph_bounds.clone(), cx);
        let overlays = render_json_graph_overlays(self, &context, cx);
        render_json_graph_canvas(
            self,
            &context,
            graph_bounds,
            edges,
            node_elements,
            controls,
            overlays,
            cx,
        )
    }
}
