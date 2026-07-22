// @author kongweiguang

use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};

use gmark_json_graph::{
    CancellationSignal, DEFAULT_JSON_GRAPH_ITEM_LIMIT, DocumentSnapshot, JsonGraphError,
    JsonGraphProvider, JsonGraphRequest, JsonGraphRoot, JsonValueKind, SnapshotError,
    SourceLocator,
};

struct MemoryDocument {
    revision: u64,
    bytes: Vec<u8>,
}

impl MemoryDocument {
    fn new(revision: u64, source: impl Into<Vec<u8>>) -> Self {
        Self {
            revision,
            bytes: source.into(),
        }
    }
}

impl DocumentSnapshot for MemoryDocument {
    fn revision(&self) -> gmark_document_core::DocumentRevision {
        gmark_document_core::DocumentRevision(self.revision)
    }

    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>, SnapshotError> {
        let start =
            usize::try_from(range.start).map_err(|error| SnapshotError::new(error.to_string()))?;
        let end =
            usize::try_from(range.end).map_err(|error| SnapshotError::new(error.to_string()))?;
        self.bytes
            .get(start..end)
            .map(ToOwned::to_owned)
            .ok_or_else(|| SnapshotError::new("test range is outside the snapshot"))
    }
}

#[derive(Default)]
struct TestCancellation(bool);

impl CancellationSignal for TestCancellation {
    fn is_cancelled(&self) -> bool {
        self.0
    }
}

fn request(revision: u64, item_limit: usize) -> JsonGraphRequest {
    JsonGraphRequest {
        document_epoch: 3,
        revision,
        generation: 5,
        root: None,
        item_limit,
    }
}

#[test]
fn builds_container_cards_scalar_rows_edges_and_precise_ranges() {
    let source = br#"{"name":"Ada","items":[{"ok":true},null]}"#;
    let snapshot = JsonGraphProvider
        .build(
            &MemoryDocument::new(9, source.to_vec()),
            &request(9, DEFAULT_JSON_GRAPH_ITEM_LIMIT),
            &TestCancellation::default(),
        )
        .unwrap();
    let graph = snapshot.projection();
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.edges.len(), 2);
    assert_eq!(graph.nodes[0].label.as_ref(), "$");
    assert_eq!(graph.nodes[0].source.range, 0..source.len() as u64);
    assert_eq!(graph.nodes[0].fields[0].label.as_ref(), "name");
    assert_eq!(graph.nodes[0].fields[0].json_path.as_ref(), "$/name#0");
    assert_eq!(graph.nodes[0].fields[0].display_value.as_ref(), "Ada");
    assert_eq!(graph.nodes[2].fields[0].kind, JsonValueKind::Boolean);
    assert_eq!(graph.edges[0].parent_port.as_str(), "port:node:$/items#1");
    assert!(!graph.truncated);
    assert!(request(9, DEFAULT_JSON_GRAPH_ITEM_LIMIT).accepts(&snapshot));
}

#[test]
fn breadth_first_budget_keeps_top_level_structure_complete() {
    let source = br#"{"deep":{"a":{"b":{"c":1}}},"top1":{},"top2":{}}"#;
    let snapshot = JsonGraphProvider
        .build(
            &MemoryDocument::new(1, source.to_vec()),
            &request(1, 4),
            &TestCancellation::default(),
        )
        .unwrap();
    let labels = snapshot
        .projection()
        .nodes
        .iter()
        .map(|node| node.label.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(labels, ["$", "deep", "top1", "top2"]);
    assert!(snapshot.projection().truncated);
}

#[test]
fn same_level_container_entries_survive_scalar_rows_for_subtree_navigation() {
    let source = br#"{"a":1,"b":2,"nested":{"value":3}}"#;
    let snapshot = JsonGraphProvider
        .build(
            &MemoryDocument::new(2, source.to_vec()),
            &request(2, 3),
            &TestCancellation::default(),
        )
        .unwrap();
    let graph = snapshot.projection();
    assert!(
        graph
            .nodes
            .iter()
            .any(|node| node.label.as_ref() == "nested")
    );
    assert_eq!(graph.nodes[0].fields.len(), 1);
    assert!(graph.truncated);
}

#[test]
fn handles_unicode_escaped_pointers_duplicate_keys_scalar_roots_and_empty_containers() {
    let document = MemoryDocument::new(
        4,
        r#"{"a/b":"中文","til~de":0,"dup":{"x":1},"dup":{"x":2},"empty":[]}"#
            .as_bytes()
            .to_vec(),
    );
    let snapshot = JsonGraphProvider
        .build(&document, &request(4, 32), &TestCancellation::default())
        .unwrap();
    let graph = snapshot.projection();
    assert_eq!(graph.nodes[0].fields[0].display_value.as_ref(), "中文");
    assert!(graph.nodes[0].fields[0].id.as_str().contains("a~1b"));
    assert!(graph.nodes[0].fields[1].id.as_str().contains("til~0de"));
    let duplicate_paths = graph
        .nodes
        .iter()
        .filter(|node| node.label.as_ref() == "dup")
        .map(|node| node.json_path.clone())
        .collect::<Vec<_>>();
    assert_eq!(duplicate_paths.len(), 2);
    assert_ne!(duplicate_paths[0], duplicate_paths[1]);
    assert!(
        graph
            .nodes
            .iter()
            .any(|node| { node.label.as_ref() == "empty" && node.kind == JsonValueKind::Array })
    );

    let scalar = JsonGraphProvider
        .build(
            &MemoryDocument::new(4, b"1234567890123456789012345678901234567890".to_vec()),
            &request(4, 8),
            &TestCancellation::default(),
        )
        .unwrap();
    assert_eq!(scalar.projection().nodes[0].kind, JsonValueKind::Number);
}

#[test]
fn reports_exact_invalid_offset_rejects_stale_revision_and_honors_cancellation() {
    let invalid = MemoryDocument::new(1, br#"{"a":1,}"#.to_vec());
    let error = JsonGraphProvider
        .build(&invalid, &request(1, 32), &TestCancellation::default())
        .unwrap_err();
    assert!(matches!(
        error,
        JsonGraphError::InvalidJson { offset: 7, .. }
    ));

    assert!(matches!(
        JsonGraphProvider.build(&invalid, &request(2, 32), &TestCancellation::default()),
        Err(JsonGraphError::SourceChanged)
    ));
    assert!(matches!(
        JsonGraphProvider.build(&invalid, &request(1, 32), &TestCancellation(true)),
        Err(JsonGraphError::Cancelled)
    ));
}

#[test]
fn supports_deep_nesting_and_focused_subtree_ranges_without_recursion() {
    let mut source = "[".repeat(600);
    source.push('0');
    source.push_str(&"]".repeat(600));
    let deep = MemoryDocument::new(6, source.into_bytes());
    assert!(
        JsonGraphProvider
            .build(
                &deep,
                &request(6, DEFAULT_JSON_GRAPH_ITEM_LIMIT),
                &TestCancellation::default()
            )
            .is_ok()
    );

    let source = br#"{"outside":0,"focus":{"value":1}}"#;
    let focus_start = source
        .windows(7)
        .position(|window| window == b"{\"value")
        .unwrap() as u64;
    let focus_end = source.len() as u64 - 1;
    let mut focused = request(7, 32);
    focused.root = Some(JsonGraphRoot::new(
        SourceLocator::new(focus_start..focus_end),
        "$/focus#0",
        "focus",
    ));
    let snapshot = JsonGraphProvider
        .build(
            &MemoryDocument::new(7, source.to_vec()),
            &focused,
            &TestCancellation::default(),
        )
        .unwrap();
    assert_eq!(snapshot.projection().nodes.len(), 1);
    assert_eq!(
        snapshot.projection().nodes[0].source.range,
        focus_start..focus_end
    );
    assert_eq!(
        snapshot.projection().nodes[0].json_path.as_ref(),
        "$/focus#0"
    );
    assert_eq!(snapshot.projection().nodes[0].label.as_ref(), "focus");
}

#[test]
fn streams_large_values_in_bounded_chunks_and_keeps_a_valid_summary() {
    struct RecordingDocument {
        inner: MemoryDocument,
        largest_read: AtomicUsize,
    }
    impl DocumentSnapshot for RecordingDocument {
        fn revision(&self) -> gmark_document_core::DocumentRevision {
            self.inner.revision()
        }
        fn len(&self) -> u64 {
            self.inner.len()
        }
        fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>, SnapshotError> {
            self.largest_read
                .fetch_max((range.end - range.start) as usize, Ordering::Relaxed);
            self.inner.read_range(range)
        }
    }

    let long_value = format!("前缀{}结尾", "x".repeat(256 * 1024));
    let source = format!(r#"{{"value":"{long_value}"}}"#).into_bytes();
    let document = RecordingDocument {
        inner: MemoryDocument::new(8, source),
        largest_read: AtomicUsize::new(0),
    };
    let snapshot = JsonGraphProvider
        .build(&document, &request(8, 16), &TestCancellation::default())
        .unwrap();
    let display = snapshot.projection().nodes[0].fields[0]
        .display_value
        .as_ref();
    assert!(display.starts_with("前缀"));
    assert!(display.ends_with('…'));
    assert!(document.largest_read.load(Ordering::Relaxed) <= 64 * 1024);
}

#[test]
fn cancellation_can_interrupt_an_in_progress_projection() {
    struct CountingCancellation(AtomicUsize);
    impl CancellationSignal for CountingCancellation {
        fn is_cancelled(&self) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed) > 20
        }
    }
    let source = format!(
        "[{}]",
        (0..20_000)
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    let result = JsonGraphProvider.build(
        &MemoryDocument::new(10, source.into_bytes()),
        &request(10, 1_500),
        &CountingCancellation(AtomicUsize::new(0)),
    );
    assert!(matches!(result, Err(JsonGraphError::Cancelled)));
}

#[test]
fn rejects_unpaired_unicode_surrogates() {
    let result = JsonGraphProvider.build(
        &MemoryDocument::new(11, br#"{"bad":"\uD800"}"#.to_vec()),
        &request(11, 16),
        &TestCancellation::default(),
    );
    assert!(matches!(result, Err(JsonGraphError::InvalidJson { .. })));
}
