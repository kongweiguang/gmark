// @author kongweiguang

use super::*;

#[test]
fn semantic_tree_is_bounded_and_exposes_source_contract() {
    let snapshot = EditorAccessibilitySnapshot {
        title: "large.md".to_owned(),
        dirty: true,
        status: "64 MiB · 20,000 lines".to_owned(),
        error: Some("invalid JSON near byte 42".to_owned()),
        busy: false,
        search_visible: true,
        navigation_visible: true,
        caret: Some((0, 2)),
        lines: (0..2_000)
            .map(|line| (line, format!("row {line}")))
            .collect(),
    }
    .bounded();
    let tree = build_tree(snapshot);
    assert_eq!(tree.tree.as_ref().map(|tree| tree.root), Some(ROOT_ID));
    assert_eq!(tree.focus, DOCUMENT_ID);
    assert!(tree.nodes.len() <= MAX_EXPOSED_LINES * 2 + 12);
    assert!(tree.nodes.iter().any(|(id, _)| *id == ERROR_ID));
    assert!(tree.nodes.iter().any(|(id, _)| *id == SEARCH_INPUT_ID));
    assert!(tree.nodes.iter().any(|(id, _)| *id == NAVIGATION_INPUT_ID));
    let document = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == DOCUMENT_ID)
        .map(|(_, node)| node)
        .expect("document node");
    assert_eq!(
        document.text_selection().map(|selection| selection.focus),
        Some(TextPosition {
            node: NodeId(FIRST_TEXT_RUN_ID),
            character_index: 2,
        })
    );
    let first_run = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == NodeId(FIRST_TEXT_RUN_ID))
        .map(|(_, node)| node)
        .expect("first text run");
    assert_eq!(
        first_run
            .character_lengths()
            .iter()
            .map(|length| *length as usize)
            .sum::<usize>(),
        first_run.value().expect("text run value").len()
    );
}

#[test]
fn busy_document_exposes_progress_role() {
    let tree = build_tree(EditorAccessibilitySnapshot {
        title: "large.md".to_owned(),
        status: "Indexing…".to_owned(),
        busy: true,
        ..EditorAccessibilitySnapshot::default()
    });
    let status = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == STATUS_ID)
        .map(|(_, node)| node)
        .expect("status node");
    assert_eq!(status.role(), Role::ProgressIndicator);
}

#[test]
fn semantic_text_budget_is_utf8_safe() {
    let snapshot = EditorAccessibilitySnapshot {
        lines: (0..512)
            .map(|line| (line, "测".repeat(MAX_EXPOSED_LINE_BYTES)))
            .collect(),
        ..EditorAccessibilitySnapshot::default()
    }
    .bounded();
    assert!(!snapshot.lines.is_empty());
    assert!(snapshot.lines.len() <= 512);
    assert!(
        snapshot
            .lines
            .iter()
            .map(|(_, text)| text.len())
            .sum::<usize>()
            <= MAX_EXPOSED_TEXT_BYTES
    );
    assert!(
        snapshot
            .lines
            .iter()
            .all(|(_, text)| text.is_char_boundary(text.len()))
    );
}
