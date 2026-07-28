// @author kongweiguang

use gmark_json_graph::{JsonGraphItemId, JsonGraphProjection};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

pub(super) const CARD_MIN_WIDTH: f32 = 224.0;
pub(super) const CARD_MAX_WIDTH: f32 = 336.0;
pub(super) const CARD_HEADER_HEIGHT: f32 = 32.0;
pub(super) const CARD_ROW_HEIGHT: f32 = 26.0;
pub(super) const COLUMN_GAP: f32 = 112.0;
pub(super) const ROW_GAP: f32 = 20.0;
pub(super) const CANVAS_PADDING: f32 = 64.0;
pub(super) const MIN_ZOOM: f32 = 0.35;
pub(super) const READABLE_MIN_ZOOM: f32 = 0.78;
pub(super) const SEARCH_REVEAL_ZOOM: f32 = 0.9;
pub(super) const MAX_ZOOM: f32 = 2.0;
pub(super) const DEFAULT_ROW_LIMIT: usize = 12;
pub(super) const ROW_LIMIT_STEP: usize = 24;
pub(super) const SMALL_GRAPH_NODE_LIMIT: usize = 80;
pub(super) const INITIAL_VISIBLE_NODE_BUDGET: usize = 120;
pub(super) const INITIAL_DEPTH_LIMIT: usize = 6;
pub(super) const VIEWPORT_OVERSCAN: f32 = 200.0;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PositionedGraphNode {
    pub(super) index: usize,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) branch: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PositionedGraphEdge {
    pub(super) edge_index: usize,
    pub(super) from_index: usize,
    pub(super) to_index: usize,
    pub(super) from_x: f32,
    pub(super) from_y: f32,
    pub(super) to_x: f32,
    pub(super) to_y: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct GraphLayout {
    pub(super) nodes: Vec<PositionedGraphNode>,
    pub(super) edges: Vec<PositionedGraphEdge>,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) parent_by_node: Vec<Option<usize>>,
    pub(super) children_by_node: Vec<Vec<usize>>,
    pub(super) visible_order: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GraphLayoutKey {
    document_epoch: u64,
    revision: u64,
    generation: u64,
    collapsed: Vec<String>,
    row_limits: Vec<(String, usize)>,
}

impl GraphLayoutKey {
    pub(super) fn new(
        document_epoch: u64,
        revision: u64,
        generation: u64,
        collapsed: &HashSet<Arc<str>>,
        row_limits: &HashMap<JsonGraphItemId, usize>,
    ) -> Self {
        let mut collapsed = collapsed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        collapsed.sort_unstable();
        let mut row_limits = row_limits
            .iter()
            .map(|(id, limit)| (id.as_str().to_owned(), *limit))
            .collect::<Vec<_>>();
        row_limits.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        Self {
            document_epoch,
            revision,
            generation,
            collapsed,
            row_limits,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GraphLayoutCache {
    pub(super) key: GraphLayoutKey,
    pub(super) layout: Arc<GraphLayout>,
}

fn outgoing_edge_indices(graph: &JsonGraphProjection) -> Vec<Vec<usize>> {
    let index_by_id = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut outgoing = vec![Vec::new(); graph.nodes.len()];
    for (edge_index, edge) in graph.edges.iter().enumerate() {
        if let Some(parent) = index_by_id.get(edge.from.as_str()) {
            outgoing[*parent].push(edge_index);
        }
    }
    outgoing
}

fn ordered_row_edges(
    graph: &JsonGraphProjection,
    node_index: usize,
    outgoing: &[Vec<usize>],
) -> Vec<Option<usize>> {
    let node = &graph.nodes[node_index];
    let mut rows = node
        .fields
        .iter()
        .map(|field| (field.source.range.start, None))
        .chain(outgoing[node_index].iter().map(|edge_index| {
            (
                graph.edges[*edge_index].source.range.start,
                Some(*edge_index),
            )
        }))
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.0);
    rows.into_iter().map(|row| row.1).collect()
}

pub(super) fn row_limit(
    node_id: &JsonGraphItemId,
    row_limits: &HashMap<JsonGraphItemId, usize>,
) -> usize {
    row_limits
        .get(node_id)
        .copied()
        .unwrap_or(DEFAULT_ROW_LIMIT)
        .max(1)
}

pub(super) fn visible_row_count(total: usize, limit: usize) -> usize {
    total
        .min(limit)
        .saturating_add(usize::from(total > limit))
        .max(1)
}

pub(super) fn initial_collapsed_items(
    graph: &JsonGraphProjection,
    row_limits: &HashMap<JsonGraphItemId, usize>,
) -> Vec<Arc<str>> {
    if graph.nodes.len() <= SMALL_GRAPH_NODE_LIMIT {
        return Vec::new();
    }
    let index_by_id = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let outgoing = outgoing_edge_indices(graph);
    let mut parent = vec![None; graph.nodes.len()];
    for edge in graph.edges.iter() {
        if let (Some(from), Some(to)) = (
            index_by_id.get(edge.from.as_str()),
            index_by_id.get(edge.to.as_str()),
        ) {
            parent[*to] = Some(*from);
        }
    }
    let roots = parent
        .iter()
        .enumerate()
        .filter_map(|(index, parent)| parent.is_none().then_some(index))
        .collect::<Vec<_>>();
    let mut visible_count = roots.len();
    let mut collapsed = Vec::new();
    let mut queue = roots
        .into_iter()
        .map(|root| (root, 0usize))
        .collect::<VecDeque<_>>();
    while let Some((index, depth)) = queue.pop_front() {
        let rows = ordered_row_edges(graph, index, &outgoing);
        let limit = row_limit(&graph.nodes[index].id, row_limits);
        let children = rows
            .into_iter()
            .take(limit)
            .flatten()
            .filter_map(|edge_index| {
                index_by_id
                    .get(graph.edges[edge_index].to.as_str())
                    .copied()
            })
            .collect::<Vec<_>>();
        if children.is_empty() {
            continue;
        }
        if depth >= INITIAL_DEPTH_LIMIT
            || visible_count.saturating_add(children.len()) > INITIAL_VISIBLE_NODE_BUDGET
        {
            collapsed.push(Arc::from(graph.nodes[index].id.as_str()));
            continue;
        }
        visible_count += children.len();
        queue.extend(children.into_iter().map(|child| (child, depth + 1)));
    }
    collapsed
}

fn card_size(
    graph: &JsonGraphProjection,
    node_index: usize,
    outgoing: &[Vec<usize>],
    row_limits: &HashMap<JsonGraphItemId, usize>,
) -> (f32, f32) {
    let node = &graph.nodes[node_index];
    let limit = row_limit(&node.id, row_limits);
    let rows = ordered_row_edges(graph, node_index, outgoing);
    let widest = std::iter::once(node.label.chars().count())
        .chain(
            node.fields
                .iter()
                .take(limit)
                .map(|field| field.label.chars().count() + field.display_value.chars().count() + 3),
        )
        .chain(
            rows.iter()
                .take(limit)
                .flatten()
                .map(|edge_index| graph.edges[*edge_index].label.chars().count() + 10),
        )
        .max()
        .unwrap_or(8);
    let width = (60.0 + widest.min(40) as f32 * 7.0).clamp(CARD_MIN_WIDTH, CARD_MAX_WIDTH);
    let height = CARD_HEADER_HEIGHT
        + visible_row_count(node.fields.len() + outgoing[node_index].len(), limit) as f32
            * CARD_ROW_HEIGHT;
    (width, height)
}

pub(super) fn graph_layout(
    graph: &JsonGraphProjection,
    collapsed: &HashSet<Arc<str>>,
    row_limits: &HashMap<JsonGraphItemId, usize>,
) -> GraphLayout {
    if graph.nodes.is_empty() {
        return GraphLayout::default();
    }
    let index_by_id = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let outgoing = outgoing_edge_indices(graph);
    let mut children = vec![Vec::new(); graph.nodes.len()];
    let mut parent = vec![None; graph.nodes.len()];
    let mut row_by_edge = HashMap::new();
    for node_index in 0..graph.nodes.len() {
        let limit = row_limit(&graph.nodes[node_index].id, row_limits);
        for (row_index, edge_index) in ordered_row_edges(graph, node_index, &outgoing)
            .into_iter()
            .take(limit)
            .enumerate()
        {
            let Some(edge_index) = edge_index else {
                continue;
            };
            let Some(child) = index_by_id
                .get(graph.edges[edge_index].to.as_str())
                .copied()
            else {
                continue;
            };
            children[node_index].push(child);
            parent[child] = Some(node_index);
            row_by_edge.insert(edge_index, row_index);
        }
    }
    let roots = parent
        .iter()
        .enumerate()
        .filter_map(|(index, parent)| parent.is_none().then_some(index))
        .collect::<Vec<_>>();
    let mut visible = vec![false; graph.nodes.len()];
    let mut depth = vec![0usize; graph.nodes.len()];
    let mut branch = vec![None; graph.nodes.len()];
    let mut stack = roots
        .iter()
        .rev()
        .map(|root| (*root, false))
        .collect::<Vec<_>>();
    let mut postorder = Vec::new();
    while let Some((index, visited)) = stack.pop() {
        if visited {
            postorder.push(index);
            continue;
        }
        visible[index] = true;
        stack.push((index, true));
        if collapsed.contains(graph.nodes[index].id.as_str()) {
            continue;
        }
        for (ordinal, child) in children[index].iter().copied().enumerate().rev() {
            depth[child] = depth[index].saturating_add(1);
            branch[child] = branch[index].or(Some(ordinal));
            stack.push((child, false));
        }
    }
    let sizes = (0..graph.nodes.len())
        .map(|index| card_size(graph, index, &outgoing, row_limits))
        .collect::<Vec<_>>();
    let mut subtree_height = vec![0.0f32; graph.nodes.len()];
    for index in postorder.iter().copied() {
        let visible_children = children[index]
            .iter()
            .copied()
            .filter(|child| visible[*child])
            .collect::<Vec<_>>();
        let children_height = visible_children
            .iter()
            .map(|child| subtree_height[*child])
            .sum::<f32>()
            + ROW_GAP * visible_children.len().saturating_sub(1) as f32;
        subtree_height[index] = sizes[index].1.max(children_height);
    }
    let mut max_width_by_depth = Vec::<f32>::new();
    for (index, node_depth) in depth.iter().copied().enumerate() {
        if !visible[index] {
            continue;
        }
        if max_width_by_depth.len() <= node_depth {
            max_width_by_depth.resize(node_depth + 1, 0.0);
        }
        max_width_by_depth[node_depth] = max_width_by_depth[node_depth].max(sizes[index].0);
    }
    let mut x_by_depth = Vec::with_capacity(max_width_by_depth.len());
    let mut x = CANVAS_PADDING;
    for width in max_width_by_depth {
        x_by_depth.push(x);
        x += width + COLUMN_GAP;
    }
    let mut positions = vec![None; graph.nodes.len()];
    let mut root_top = CANVAS_PADDING;
    let mut queue = roots
        .iter()
        .map(|root| {
            let top = root_top;
            root_top += subtree_height[*root] + ROW_GAP;
            (*root, top)
        })
        .collect::<VecDeque<_>>();
    while let Some((index, subtree_top)) = queue.pop_front() {
        if !visible[index] {
            continue;
        }
        let (width, height) = sizes[index];
        let y = subtree_top + (subtree_height[index] - height) * 0.5;
        positions[index] = Some(PositionedGraphNode {
            index,
            x: x_by_depth[depth[index]],
            y,
            width,
            height,
            branch: branch[index],
        });
        let mut child_top = subtree_top;
        for child in children[index]
            .iter()
            .copied()
            .filter(|child| visible[*child])
        {
            queue.push_back((child, child_top));
            child_top += subtree_height[child] + ROW_GAP;
        }
    }
    let nodes = positions.iter().flatten().cloned().collect::<Vec<_>>();
    let mut edges = Vec::new();
    for (edge_index, edge) in graph.edges.iter().enumerate() {
        let (Some(from_index), Some(to_index), Some(row_index)) = (
            index_by_id.get(edge.from.as_str()).copied(),
            index_by_id.get(edge.to.as_str()).copied(),
            row_by_edge.get(&edge_index).copied(),
        ) else {
            continue;
        };
        let (Some(from), Some(to)) = (&positions[from_index], &positions[to_index]) else {
            continue;
        };
        edges.push(PositionedGraphEdge {
            edge_index,
            from_index,
            to_index,
            from_x: from.x + from.width,
            from_y: from.y + CARD_HEADER_HEIGHT + (row_index as f32 + 0.5) * CARD_ROW_HEIGHT,
            to_x: to.x,
            to_y: to.y + CARD_HEADER_HEIGHT * 0.5,
        });
    }
    let width = nodes
        .iter()
        .map(|node| node.x + node.width)
        .fold(0.0, f32::max)
        + CANVAS_PADDING;
    let height = nodes
        .iter()
        .map(|node| node.y + node.height)
        .fold(0.0, f32::max)
        + CANVAS_PADDING;
    let visible_order = nodes.iter().map(|node| node.index).collect::<Vec<_>>();
    GraphLayout {
        nodes,
        edges,
        width,
        height,
        parent_by_node: parent,
        children_by_node: children,
        visible_order,
    }
}

pub(super) fn fit_camera(
    layout: &GraphLayout,
    viewport_width: f32,
    viewport_height: f32,
    minimum_zoom: f32,
) -> (f32, f32, f32) {
    if layout.width <= 0.0 || layout.height <= 0.0 {
        return (0.0, 0.0, 1.0);
    }
    let natural_zoom = ((viewport_width - 48.0).max(1.0) / layout.width)
        .min((viewport_height - 48.0).max(1.0) / layout.height);
    let zoom = natural_zoom.clamp(minimum_zoom, 1.0);
    if natural_zoom < minimum_zoom
        && minimum_zoom > MIN_ZOOM
        && let Some(root) = layout.nodes.iter().find(|node| {
            layout
                .parent_by_node
                .get(node.index)
                .is_some_and(Option::is_none)
        })
    {
        // 可读适配不把超大画布中心塞进首屏：根节点留在左侧安全边距，纵向居中，
        // 用户从 JSON 起点自然向右浏览；只有显式“适配全部”才居中整个画布。
        let camera_x = 32.0 - root.x * zoom;
        let camera_y = viewport_height * 0.5 - (root.y + root.height * 0.5) * zoom;
        return (camera_x, camera_y, zoom);
    }
    let camera_x = (viewport_width - layout.width * zoom) * 0.5;
    let camera_y = (viewport_height - layout.height * zoom) * 0.5;
    (camera_x, camera_y, zoom)
}

pub(super) fn node_intersects_viewport(
    node: &PositionedGraphNode,
    camera_x: f32,
    camera_y: f32,
    zoom: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> bool {
    let left = camera_x + node.x * zoom;
    let top = camera_y + node.y * zoom;
    let right = left + node.width * zoom;
    let bottom = top + node.height * zoom;
    right >= -VIEWPORT_OVERSCAN
        && bottom >= -VIEWPORT_OVERSCAN
        && left <= viewport_width + VIEWPORT_OVERSCAN
        && top <= viewport_height + VIEWPORT_OVERSCAN
}

pub(super) fn edge_intersects_viewport(
    edge: &PositionedGraphEdge,
    camera_x: f32,
    camera_y: f32,
    zoom: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> bool {
    let left = camera_x + edge.from_x.min(edge.to_x) * zoom;
    let right = camera_x + edge.from_x.max(edge.to_x) * zoom;
    let top = camera_y + edge.from_y.min(edge.to_y) * zoom;
    let bottom = camera_y + edge.from_y.max(edge.to_y) * zoom;
    right >= -VIEWPORT_OVERSCAN
        && bottom >= -VIEWPORT_OVERSCAN
        && left <= viewport_width + VIEWPORT_OVERSCAN
        && top <= viewport_height + VIEWPORT_OVERSCAN
}

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

#[cfg(test)]
mod tests {
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
}
