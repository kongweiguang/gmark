// @author kongweiguang

use super::*;
use gmark_json_graph::SourceLocator as JsonSourceLocator;

fn node(id: &str, fields: usize) -> JsonGraphNode {
    JsonGraphNode {
        id: JsonGraphItemId::new(id),
        json_path: Arc::from(id),
        source: JsonSourceLocator::new(0..1),
        kind: JsonValueKind::Object,
        label: Arc::from(id),
        fields: (0..fields)
            .map(|index| JsonGraphField {
                id: JsonGraphItemId::new(format!("{id}:{index}")),
                json_path: Arc::from(format!("{id}/{index}")),
                label: Arc::from(format!("k{index}")),
                display_value: Arc::from("value"),
                source: JsonSourceLocator::new(0..1),
                kind: JsonValueKind::String,
            })
            .collect::<Vec<_>>()
            .into(),
        child_count: 0,
    }
}

#[test]
fn tree_layout_is_deterministic_and_never_overlaps_siblings() {
    let mut root = node("root", 2);
    root.child_count = 2;
    let mut root_fields = root.fields.to_vec();
    root_fields[0].source = JsonSourceLocator::new(10..11);
    root_fields[1].source = JsonSourceLocator::new(40..41);
    root.fields = root_fields.into();
    let graph = JsonGraphProjection {
        nodes: vec![root, node("a", 1), node("b", 5)].into(),
        edges: vec![
            JsonGraphEdge {
                id: JsonGraphItemId::new("e1"),
                from: JsonGraphItemId::new("root"),
                to: JsonGraphItemId::new("a"),
                parent_port: JsonGraphItemId::new("port:a"),
                source: JsonSourceLocator::new(20..21),
                kind: JsonGraphEdgeKind::ObjectMember,
                label: Arc::from("a"),
            },
            JsonGraphEdge {
                id: JsonGraphItemId::new("e2"),
                from: JsonGraphItemId::new("root"),
                to: JsonGraphItemId::new("b"),
                parent_port: JsonGraphItemId::new("port:b"),
                source: JsonSourceLocator::new(50..51),
                kind: JsonGraphEdgeKind::ObjectMember,
                label: Arc::from("b"),
            },
        ]
        .into(),
        truncated: false,
    };
    let first = graph_layout(&graph, &HashSet::<Arc<str>>::new(), &HashMap::new());
    let second = graph_layout(&graph, &HashSet::<Arc<str>>::new(), &HashMap::new());
    assert_eq!(first, second);
    let a = first.nodes.iter().find(|node| node.index == 1).unwrap();
    let b = first.nodes.iter().find(|node| node.index == 2).unwrap();
    assert!(a.y + a.height + model::ROW_GAP <= b.y || b.y + b.height + model::ROW_GAP <= a.y);
    assert_eq!(first.edges.len(), 2);
    let root = first.nodes.iter().find(|node| node.index == 0).unwrap();
    let edge = &first.edges[0];
    assert_eq!(edge.from_x, root.x + root.width);
    assert_eq!(
        edge.from_y,
        root.y + GRAPH_CARD_HEADER_HEIGHT + 1.5 * GRAPH_CARD_ROW_HEIGHT
    );
    assert_eq!(edge.to_x, a.x);
    assert_eq!(edge.to_y, a.y + GRAPH_CARD_HEADER_HEIGHT * 0.5);
}

#[test]
fn collapsed_node_removes_descendants_and_fit_clamps_zoom() {
    let mut root = node("root", 0);
    root.child_count = 1;
    let graph = JsonGraphProjection {
        nodes: vec![root, node("child", 0)].into(),
        edges: vec![JsonGraphEdge {
            id: JsonGraphItemId::new("e"),
            from: JsonGraphItemId::new("root"),
            to: JsonGraphItemId::new("child"),
            parent_port: JsonGraphItemId::new("port:child"),
            source: JsonSourceLocator::new(0..1),
            kind: JsonGraphEdgeKind::ObjectMember,
            label: Arc::from("child"),
        }]
        .into(),
        truncated: false,
    };
    let collapsed = HashSet::from([Arc::<str>::from("root")]);
    let layout = graph_layout(&graph, &collapsed, &HashMap::new());
    assert_eq!(layout.nodes.len(), 1);
    assert!(layout.edges.is_empty());
    let (_, _, zoom) = fit_camera(&layout, 320.0, 200.0, GRAPH_MIN_ZOOM);
    assert!((GRAPH_MIN_ZOOM..=1.0).contains(&zoom));
}

#[test]
fn pointer_zoom_keeps_the_world_point_under_the_cursor() {
    let camera = (37.0, -12.0);
    let pointer = (420.0, 180.0);
    let old_zoom = 0.75;
    let new_zoom = 1.4;
    let world_before = (
        (pointer.0 - camera.0) / old_zoom,
        (pointer.1 - camera.1) / old_zoom,
    );
    let (camera_x, camera_y) =
        zoom_camera_around(camera.0, camera.1, old_zoom, new_zoom, pointer.0, pointer.1);
    let world_after = (
        (pointer.0 - camera_x) / new_zoom,
        (pointer.1 - camera_y) / new_zoom,
    );
    assert!((world_before.0 - world_after.0).abs() < 0.001);
    assert!((world_before.1 - world_after.1).abs() < 0.001);
}

#[test]
fn internal_graph_paths_are_presented_as_standard_jsonpath() {
    assert_eq!(jsonpath_for_display("$"), "$");
    assert_eq!(
        jsonpath_for_display("$/paths#3/~1v1~1planning~1route#2/post#0"),
        "$.paths['/v1/planning/route'].post"
    );
    assert_eq!(
        jsonpath_for_display("$/items#0/2/name#1"),
        "$.items[2].name"
    );
    assert_eq!(
        jsonpath_for_display("$/owner~0name#0/it\u{27}s\\fine#1"),
        "$['owner~name']['it\\\u{27}s\\\\fine']"
    );
}

#[test]
fn search_selection_expands_every_collapsed_ancestor() {
    let graph = JsonGraphProjection {
        nodes: vec![node("root", 0), node("child", 0), node("leaf", 0)].into(),
        edges: vec![
            JsonGraphEdge {
                id: JsonGraphItemId::new("root-child"),
                from: JsonGraphItemId::new("root"),
                to: JsonGraphItemId::new("child"),
                parent_port: JsonGraphItemId::new("port:child"),
                source: JsonSourceLocator::new(0..1),
                kind: JsonGraphEdgeKind::ObjectMember,
                label: Arc::from("child"),
            },
            JsonGraphEdge {
                id: JsonGraphItemId::new("child-leaf"),
                from: JsonGraphItemId::new("child"),
                to: JsonGraphItemId::new("leaf"),
                parent_port: JsonGraphItemId::new("port:leaf"),
                source: JsonSourceLocator::new(0..1),
                kind: JsonGraphEdgeKind::ObjectMember,
                label: Arc::from("leaf"),
            },
        ]
        .into(),
        truncated: false,
    };
    let mut collapsed = vec![Arc::from("root"), Arc::from("child")];
    expand_ancestors(&graph, &JsonGraphItemId::new("leaf"), &mut collapsed);
    assert!(collapsed.is_empty());
}

#[test]
fn search_reveals_a_hidden_dense_card_row() {
    let graph = JsonGraphProjection {
        nodes: vec![node("root", 37)].into(),
        edges: Arc::from([]),
        truncated: false,
    };
    let selected = JsonGraphItemId::new("root:36");
    let (parent, limit) = search_reveal_row_limit(&graph, &selected).unwrap();
    assert_eq!(parent.as_str(), "root");
    assert_eq!(limit, 60);
}
