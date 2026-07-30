// @author kongweiguang

use super::*;
use gmark_json_graph::{
    JsonGraphEdge, JsonGraphEdgeKind, JsonGraphField, JsonGraphItemId, JsonGraphNode,
    JsonValueKind, SourceLocator,
};

fn projection(node_count: usize) -> JsonGraphProjection {
    let nodes = (0..node_count)
        .map(|index| JsonGraphNode {
            id: JsonGraphItemId::new(format!("node-{index}")),
            json_path: Arc::from(format!("$/n#{index}")),
            source: SourceLocator::new(index as u64..index as u64 + 1),
            kind: JsonValueKind::Object,
            label: Arc::from(format!("n{index}")),
            fields: Arc::<[JsonGraphField]>::from([]),
            child_count: usize::from(index + 1 < node_count),
        })
        .collect::<Vec<_>>();
    let edges = (1..node_count)
        .map(|index| JsonGraphEdge {
            id: JsonGraphItemId::new(format!("edge-{index}")),
            from: nodes[index - 1].id.clone(),
            to: nodes[index].id.clone(),
            parent_port: JsonGraphItemId::new(format!("port-{index}")),
            source: SourceLocator::new(index as u64..index as u64 + 1),
            kind: JsonGraphEdgeKind::ObjectMember,
            label: Arc::from(format!("n{index}")),
        })
        .collect::<Vec<_>>();
    JsonGraphProjection {
        nodes: nodes.into(),
        edges: edges.into(),
        truncated: false,
    }
}

#[test]
fn small_graphs_start_fully_expanded_and_large_graphs_respect_depth() {
    assert!(initial_collapsed_items(&projection(20), &HashMap::new()).is_empty());
    let collapsed = initial_collapsed_items(&projection(140), &HashMap::new());
    assert!(!collapsed.is_empty());
    assert!(collapsed.iter().any(|id| id.as_ref() == "node-6"));
}

#[test]
fn dense_cards_reserve_one_overflow_row() {
    assert_eq!(visible_row_count(5, DEFAULT_ROW_LIMIT), 5);
    assert_eq!(visible_row_count(40, DEFAULT_ROW_LIMIT), 13);
}

#[test]
fn layout_is_deterministic_non_recursive_and_readable_fit_does_not_over_shrink() {
    let graph = projection(600);
    let collapsed = HashSet::new();
    let first = graph_layout(&graph, &collapsed, &HashMap::new());
    let second = graph_layout(&graph, &collapsed, &HashMap::new());
    assert_eq!(first, second);
    assert_eq!(first.nodes.len(), 600);
    let (_, _, zoom) = fit_camera(&first, 900.0, 600.0, READABLE_MIN_ZOOM);
    assert!(zoom >= READABLE_MIN_ZOOM);
    let (camera_x, camera_y, _) = fit_camera(&first, 900.0, 600.0, READABLE_MIN_ZOOM);
    let root = &first.nodes[0];
    assert!(camera_x + root.x * zoom >= 24.0);
    assert!(camera_y + (root.y + root.height * 0.5) * zoom >= 299.0);
}
