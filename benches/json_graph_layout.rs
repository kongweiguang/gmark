// @author kongweiguang

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use gmark_json_graph::{
    JsonGraphEdge, JsonGraphEdgeKind, JsonGraphItemId, JsonGraphNode, JsonGraphProjection,
    JsonValueKind, SourceLocator,
};
use std::collections::{HashMap, HashSet};
use std::hint::black_box;
use std::sync::Arc;

#[allow(dead_code)]
#[path = "../src/document_host/views/json_graph/model.rs"]
mod model;

fn fixture(node_count: usize, wide: bool) -> JsonGraphProjection {
    let nodes = (0..node_count)
        .map(|index| JsonGraphNode {
            id: JsonGraphItemId::new(format!("node-{index}")),
            json_path: Arc::from(format!("$/node#{index}")),
            source: SourceLocator::new(index as u64..index as u64 + 1),
            kind: JsonValueKind::Object,
            label: Arc::from(format!("node-{index}")),
            fields: Arc::from([]),
            child_count: if wide {
                usize::from(index == 0) * node_count.saturating_sub(1)
            } else {
                usize::from(index + 1 < node_count)
            },
        })
        .collect::<Vec<_>>();
    let edges = (1..node_count)
        .map(|index| {
            let parent = if wide { 0 } else { index - 1 };
            JsonGraphEdge {
                id: JsonGraphItemId::new(format!("edge-{index}")),
                from: nodes[parent].id.clone(),
                to: nodes[index].id.clone(),
                parent_port: JsonGraphItemId::new(format!("port-{index}")),
                source: SourceLocator::new(index as u64..index as u64 + 1),
                kind: JsonGraphEdgeKind::ObjectMember,
                label: Arc::from(format!("child-{index}")),
            }
        })
        .collect::<Vec<_>>();
    JsonGraphProjection {
        nodes: nodes.into(),
        edges: edges.into(),
        truncated: node_count >= 1_500,
    }
}

fn json_graph_layout(c: &mut Criterion) {
    let collapsed = HashSet::<Arc<str>>::new();
    let mut group = c.benchmark_group("json graph layout");
    for count in [100, 500, 1_500] {
        let graph = fixture(count, false);
        group.bench_with_input(BenchmarkId::new("deep", count), &count, |bench, _| {
            bench.iter(|| {
                black_box(model::graph_layout(
                    black_box(&graph),
                    black_box(&collapsed),
                    black_box(&HashMap::new()),
                ));
            });
        });
    }
    let wide = fixture(1_500, true);
    let mut row_limits = HashMap::new();
    row_limits.insert(wide.nodes[0].id.clone(), 1_500);
    group.bench_function("wide fanout 1500", |bench| {
        bench.iter(|| {
            black_box(model::graph_layout(
                black_box(&wide),
                black_box(&collapsed),
                black_box(&row_limits),
            ));
        });
    });
    group.finish();
}

criterion_group!(benches, json_graph_layout);
criterion_main!(benches);
