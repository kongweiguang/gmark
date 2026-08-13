// @author kongweiguang

use super::*;

#[test]
fn semantic_tree_is_bounded_and_exposes_source_contract() {
    let snapshot = EditorAccessibilitySnapshot {
        title: "large.md".to_owned(),
        mode: AccessibilityMode::Source,
        dirty: true,
        status: "64 MiB · 20,000 lines".to_owned(),
        error: Some("invalid JSON near byte 42".to_owned()),
        busy: false,
        update_actions: Vec::new(),
        close_actions: Vec::new(),
        search_visible: true,
        navigation_visible: true,
        caret: Some((0, 2)),
        lines: (0..2_000)
            .map(|line| (line, format!("row {line}")))
            .collect(),
        folds: Vec::new(),
        math: None,
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
    assert_eq!(document.label(), Some("Source editor"));
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
fn semantic_tree_announces_rendered_mode() {
    let tree = build_tree(EditorAccessibilitySnapshot {
        mode: AccessibilityMode::Live,
        ..EditorAccessibilitySnapshot::default()
    });
    let document = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == DOCUMENT_ID)
        .map(|(_, node)| node)
        .expect("document node");
    assert_eq!(document.label(), Some("Live rendered view"));
    let mode = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == MODE_ID)
        .map(|(_, node)| node)
        .expect("mode node");
    assert_eq!(mode.value(), Some("Live"));
}

#[test]
fn fold_buttons_expose_expanded_state() {
    let tree = build_tree(EditorAccessibilitySnapshot {
        lines: vec![(0, "# title".to_owned()), (1, "body".to_owned())],
        folds: vec![AccessibilityFold {
            start_line: 0,
            end_line: 1,
            collapsed: false,
            target: Some(AccessibilityFoldTarget::SourceLine),
        }],
        ..EditorAccessibilitySnapshot::default()
    });
    let button = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == NodeId(FIRST_FOLD_ID))
        .map(|(_, node)| node)
        .expect("fold button");
    assert_eq!(button.label(), Some("Collapse lines 1 through 2"));
    assert_eq!(button.is_expanded(), Some(true));

    let tree = build_tree(EditorAccessibilitySnapshot {
        lines: vec![(0, "# title".to_owned()), (1, "body".to_owned())],
        folds: vec![AccessibilityFold {
            start_line: 0,
            end_line: 1,
            collapsed: true,
            target: Some(AccessibilityFoldTarget::SourceLine),
        }],
        ..EditorAccessibilitySnapshot::default()
    });
    let button = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == NodeId(FIRST_FOLD_ID))
        .map(|(_, node)| node)
        .expect("fold button");
    assert_eq!(button.label(), Some("Expand lines 1 through 2"));
    assert_eq!(button.is_expanded(), Some(false));
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

/// 更新动作必须进入系统可访问性树，真实平台 driver 才能操作可见流程而无需像素猜测。
#[test]
fn updater_actions_expose_stable_secondary_and_primary_buttons() {
    let tree = build_tree(EditorAccessibilitySnapshot {
        update_actions: vec!["Later".to_owned(), "Restart and Install".to_owned()],
        ..EditorAccessibilitySnapshot::default()
    });
    let secondary = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == UPDATE_SECONDARY_ID)
        .map(|(_, node)| node)
        .expect("secondary updater action");
    let primary = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == UPDATE_PRIMARY_ID)
        .map(|(_, node)| node)
        .expect("primary updater action");
    assert_eq!(secondary.label(), Some("Later"));
    assert_eq!(primary.label(), Some("Restart and Install"));
    assert!(secondary.supports_action(Action::Click));
    assert!(primary.supports_action(Action::Click));
}

/// 普通关闭确认也必须通过系统语义树可达，三平台 E2E 才能验证真实退出语义而非坐标点击。
#[test]
fn unsaved_close_actions_expose_all_three_decisions() {
    let tree = build_tree(EditorAccessibilitySnapshot {
        close_actions: vec![
            "Continue Editing".to_owned(),
            "Discard and Close".to_owned(),
            "Save and Close".to_owned(),
        ],
        ..EditorAccessibilitySnapshot::default()
    });
    for (id, label) in [
        (CLOSE_CANCEL_ID, "Continue Editing"),
        (CLOSE_DISCARD_ID, "Discard and Close"),
        (CLOSE_SAVE_ID, "Save and Close"),
    ] {
        let node = tree
            .nodes
            .iter()
            .find(|(node_id, _)| *node_id == id)
            .map(|(_, node)| node)
            .expect("close decision");
        assert_eq!(node.label(), Some(label));
        assert!(node.supports_action(Action::Click));
    }
}

#[test]
fn semantic_text_budget_is_utf8_safe() {
    let snapshot = EditorAccessibilitySnapshot {
        lines: (0..512)
            .map(|line| (line, "测".repeat(MAX_EXPOSED_LINE_BYTES)))
            .collect(),
        math: None,
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

#[test]
fn active_math_exposes_slot_tabs_page_and_grid_roving_focus() {
    let tree = build_tree(EditorAccessibilitySnapshot {
        math: Some(AccessibilityMath {
            source: "\\begin{matrix}a&b\\\\c&d\\end{matrix}".to_owned(),
            slot_value: "b".to_owned(),
            slot_cursor: 1,
            slot_label: "Formula slot".to_owned(),
            symbols_label: "Symbols".to_owned(),
            structures_label: "Structures".to_owned(),
            page: AccessibilityMathPage::Symbols,
            controls: vec![AccessibilityMathControl {
                key: "alpha".to_owned(),
                label: "Insert Alpha".to_owned(),
                page: AccessibilityMathPage::Symbols,
            }],
            grid: Some(AccessibilityMathGrid {
                rows: 2,
                columns: 2,
                active_row: 0,
                active_column: 1,
                cells: vec![
                    AccessibilityMathGridCell {
                        row: 0,
                        column: 0,
                        value: "a".to_owned(),
                    },
                    AccessibilityMathGridCell {
                        row: 0,
                        column: 1,
                        value: "b".to_owned(),
                    },
                    AccessibilityMathGridCell {
                        row: 1,
                        column: 0,
                        value: "c".to_owned(),
                    },
                    AccessibilityMathGridCell {
                        row: 1,
                        column: 1,
                        value: "d".to_owned(),
                    },
                ],
            }),
        }),
        ..EditorAccessibilitySnapshot::default()
    });
    let math = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == MATH_ID)
        .map(|(_, node)| node)
        .expect("math node");
    assert_eq!(math.role(), Role::Math);
    let input = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == MATH_INPUT_ID)
        .map(|(_, node)| node)
        .expect("math input");
    assert_eq!(input.role(), Role::TextInput);
    assert_eq!(input.value(), Some("b"));
    assert!(input.supports_action(Action::ReplaceSelectedText));
    let symbols = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == MATH_SYMBOLS_TAB_ID)
        .map(|(_, node)| node)
        .expect("symbols tab");
    assert_eq!(symbols.is_selected(), Some(true));
    assert!(symbols.supports_action(Action::Click));
    let action = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == NodeId(FIRST_MATH_ACTION_ID))
        .map(|(_, node)| node)
        .expect("math action");
    assert_eq!(action.label(), Some("Insert Alpha"));
    assert!(action.supports_action(Action::Click));
    let grid = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == MATH_GRID_ID)
        .map(|(_, node)| node)
        .expect("math grid");
    assert_eq!(
        grid.active_descendant(),
        Some(NodeId(FIRST_MATH_GRID_CELL_ID + 1))
    );
    let cell = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == NodeId(FIRST_MATH_GRID_CELL_ID + 1))
        .map(|(_, node)| node)
        .expect("active grid cell");
    assert!(cell.supports_action(Action::Focus));
    assert_eq!(tree.focus, MATH_INPUT_ID);
}
