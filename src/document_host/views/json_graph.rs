// @author kongweiguang

//! JSON graph projection view composed from focused interaction modules.

use super::*;

#[path = "json_graph/canvas.rs"]
mod canvas;
#[path = "json_graph/card_rows.rs"]
mod card_rows;
#[path = "json_graph/cards.rs"]
mod cards;
#[path = "json_graph/controls.rs"]
mod controls;
#[path = "json_graph/edges.rs"]
mod edges;
#[path = "json_graph/editing.rs"]
mod editing;
#[path = "json_graph/layout_selection.rs"]
mod layout_selection;
#[path = "json_graph/model.rs"]
pub(super) mod model;
#[path = "json_graph/overlays.rs"]
mod overlays;
#[path = "json_graph/panel.rs"]
mod panel;
#[path = "json_graph/panel_state.rs"]
mod panel_state;
#[path = "json_graph/selection.rs"]
mod selection;
#[path = "json_graph/style.rs"]
mod style;
#[path = "json_graph/support.rs"]
mod support;

pub(crate) use model::GraphLayoutCache;
pub(super) use support::json_graph_node_matches_query;

#[cfg(test)]
use model::{
    CARD_HEADER_HEIGHT as GRAPH_CARD_HEADER_HEIGHT, CARD_ROW_HEIGHT as GRAPH_CARD_ROW_HEIGHT,
    MIN_ZOOM as GRAPH_MIN_ZOOM, fit_camera, graph_layout,
};
#[cfg(test)]
use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use support::{
    expand_ancestors, jsonpath_for_display, search_reveal_row_limit, zoom_camera_around,
};

#[cfg(test)]
#[path = "../../../tests/unit/document_views/json_graph.rs"]
mod tests;
