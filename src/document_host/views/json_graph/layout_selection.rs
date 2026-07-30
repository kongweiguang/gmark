// @author kongweiguang

//! Selection-path helpers over a computed JSON graph layout.

use super::model::GraphLayout;
use std::collections::HashSet;

pub(super) fn selected_path_edges(
    layout: &GraphLayout,
    selected_index: Option<usize>,
) -> HashSet<usize> {
    let mut selected = HashSet::new();
    let Some(mut cursor) = selected_index else {
        return selected;
    };
    while let Some(parent) = layout.parent_by_node.get(cursor).and_then(|parent| *parent) {
        if let Some(edge) = layout
            .edges
            .iter()
            .find(|edge| edge.from_index == parent && edge.to_index == cursor)
        {
            selected.insert(edge.edge_index);
        }
        cursor = parent;
    }
    selected
}
