// @author kongweiguang

use std::collections::{BTreeMap, BTreeSet};

use uuid::Uuid;

use super::*;

fn pane(raw: u128) -> PaneId {
    PaneId::from_uuid(Uuid::from_u128(raw))
}

fn tab(raw: u128, document: &'static str) -> TabView<&'static str> {
    TabView::new(TabId::from_uuid(Uuid::from_u128(raw)), document, ())
}

fn add(
    workspace: &mut PaneWorkspace<&'static str>,
    pane: PaneId,
    raw: u128,
    document: &'static str,
) -> TabId {
    match workspace.insert_tab(pane, tab(raw, document)) {
        Ok(id) => id,
        Err(error) => panic!("insert failed: {error}"),
    }
}

fn ratios(node: &PaneNode, out: &mut Vec<f32>) {
    if let PaneNode::Split {
        ratio,
        first,
        second,
        ..
    } = node
    {
        out.push(*ratio);
        ratios(first, out);
        ratios(second, out);
    }
}

fn assert_invariants<D, V>(workspace: &PaneWorkspace<D, V>) {
    assert!((1..=MAX_PANES).contains(&workspace.pane_count()));
    let ids = collect_ids(workspace.root());
    assert_eq!(ids.len(), workspace.panes().len());
    assert!(ids.iter().all(|id| workspace.panes().contains_key(id)));
    assert!(ids.contains(&workspace.focused()));
    let mut split_ratios = Vec::new();
    ratios(workspace.root(), &mut split_ratios);
    assert!(split_ratios.iter().all(|ratio| {
        ratio.is_finite() && *ratio >= MIN_SPLIT_RATIO && *ratio <= MAX_SPLIT_RATIO
    }));
    let mut tab_ids = BTreeSet::new();
    for pane in workspace.panes().values() {
        if let Some(active) = pane.active_tab_id() {
            assert!(pane.tab(active).is_some());
        }
        for tab in pane.tabs() {
            assert!(tab_ids.insert(tab.id()));
        }
    }
}

#[test]
fn split_reaches_one_two_four_and_eight_panes_with_expected_orientation() {
    let mut workspace = PaneWorkspace::<&'static str>::with_root_id(pane(1));
    let root = workspace.focused();
    let right = workspace.split_right(root).unwrap_or(pane(0));
    assert_ne!(right, pane(0));
    let down = workspace.split_down(root).unwrap_or(pane(0));
    assert_ne!(down, pane(0));
    let _ = workspace.split_down(right);
    let _ = workspace.split_right(down);
    assert_eq!(workspace.pane_count(), 5);
    let ids = workspace.pane_ids();
    for id in ids.into_iter().take(3) {
        let _ = workspace.split_right(id);
    }
    assert!(workspace.pane_count() <= MAX_PANES);
    assert_invariants(&workspace);
}

#[test]
fn directional_split_preserves_compass_order_axis_geometry_and_focus() {
    for (direction, expected_axis, new_is_first) in [
        (PaneSplitDirection::Left, SplitAxis::Horizontal, true),
        (PaneSplitDirection::Right, SplitAxis::Horizontal, false),
        (PaneSplitDirection::Up, SplitAxis::Vertical, true),
        (PaneSplitDirection::Down, SplitAxis::Vertical, false),
    ] {
        let root = pane(1);
        let mut workspace = PaneWorkspace::<&'static str>::with_root_id(root);
        let created = workspace
            .split_toward(root, direction)
            .expect("directional split should create a pane");

        assert_eq!(workspace.focused(), created);
        assert_eq!(
            workspace.pane_ids(),
            if new_is_first {
                vec![created, root]
            } else {
                vec![root, created]
            }
        );

        let PaneNode::Split { axis, ratio, .. } = workspace.root() else {
            panic!("directional split should replace the target leaf");
        };
        assert_eq!(*axis, expected_axis);
        assert_eq!(*ratio, DEFAULT_SPLIT_RATIO);

        let mut rects = Vec::new();
        collect_rects(workspace.root(), Rect::ROOT, &mut rects);
        let root_rect = rects
            .iter()
            .find(|(id, _)| *id == root)
            .map(|(_, rect)| *rect)
            .expect("target pane rectangle");
        let created_rect = rects
            .iter()
            .find(|(id, _)| *id == created)
            .map(|(_, rect)| *rect)
            .expect("created pane rectangle");
        if expected_axis == SplitAxis::Horizontal {
            let (created_x, root_x) = if new_is_first { (0.0, 0.5) } else { (0.5, 0.0) };
            assert_eq!(
                created_rect,
                Rect {
                    x: created_x,
                    y: 0.0,
                    w: 0.5,
                    h: 1.0
                }
            );
            assert_eq!(
                root_rect,
                Rect {
                    x: root_x,
                    y: 0.0,
                    w: 0.5,
                    h: 1.0
                }
            );
        } else {
            let (created_y, root_y) = if new_is_first { (0.0, 0.5) } else { (0.5, 0.0) };
            assert_eq!(
                created_rect,
                Rect {
                    x: 0.0,
                    y: created_y,
                    w: 1.0,
                    h: 0.5
                }
            );
            assert_eq!(
                root_rect,
                Rect {
                    x: 0.0,
                    y: root_y,
                    w: 1.0,
                    h: 0.5
                }
            );
        }
    }

    let root = pane(20);
    let mut legacy_right = PaneWorkspace::<&'static str>::with_root_id(root);
    let created_right = legacy_right
        .split_right(root)
        .expect("legacy right split should remain available");
    assert_eq!(legacy_right.root().axis(), Some(SplitAxis::Horizontal));
    assert_eq!(legacy_right.pane_ids(), vec![root, created_right]);
    assert_eq!(legacy_right.focused(), created_right);

    let mut legacy_down = PaneWorkspace::<&'static str>::with_root_id(root);
    let created_down = legacy_down
        .split_down(root)
        .expect("legacy down split should remain available");
    assert_eq!(legacy_down.root().axis(), Some(SplitAxis::Vertical));
    assert_eq!(legacy_down.pane_ids(), vec![root, created_down]);
    assert_eq!(legacy_down.focused(), created_down);
}

#[test]
fn directional_split_errors_are_atomic() {
    let root = pane(30);
    let missing = pane(31);
    let mut workspace = PaneWorkspace::<&'static str>::with_root_id(root);
    let snapshot = workspace.clone();
    assert_eq!(
        workspace.split_toward(missing, PaneSplitDirection::Left),
        Err(PaneError::PaneNotFound(missing))
    );
    assert_eq!(workspace, snapshot);

    while workspace.pane_count() < MAX_PANES {
        let target = workspace.focused();
        workspace
            .split_toward(target, PaneSplitDirection::Right)
            .expect("pane limit should not be reached early");
    }
    let full_snapshot = workspace.clone();
    assert_eq!(
        workspace.split_toward(workspace.focused(), PaneSplitDirection::Down),
        Err(PaneError::TooManyPanes)
    );
    assert_eq!(workspace, full_snapshot);
}

#[test]
fn ratio_is_clamped_and_keyboard_steps_are_stable() {
    let mut workspace = PaneWorkspace::<&'static str>::with_root_id(pane(1));
    let root = workspace.focused();
    let other = workspace.split_right(root).unwrap_or(pane(0));
    assert_eq!(workspace.set_split_ratio(other, -5.0), Ok(0.5));
    assert_eq!(workspace.root().ratio(), Some(MIN_SPLIT_RATIO));
    let _ = workspace.adjust_split_ratio(other, true, false);
    assert!((workspace.root().ratio().unwrap_or(0.0) - 0.12).abs() < f32::EPSILON);
    let _ = workspace.adjust_split_ratio(other, false, true);
    assert!((workspace.root().ratio().unwrap_or(0.0) - MIN_SPLIT_RATIO).abs() < f32::EPSILON);
    assert!(workspace.set_split_ratio(other, f32::NAN).is_err());
    assert_invariants(&workspace);
}

#[test]
fn close_merges_into_boundary_and_preserves_closed_active_tab() {
    let mut workspace = PaneWorkspace::<&'static str>::with_root_id(pane(1));
    let left = workspace.focused();
    let right = workspace.split_right(left).unwrap_or(pane(0));
    let left_tab = add(&mut workspace, left, 10, "left");
    let right_tab = add(&mut workspace, right, 11, "right");
    assert_eq!(workspace.set_active_tab(left, left_tab), Ok(()));
    assert_eq!(workspace.close_pane(left), Ok(right));
    assert_eq!(workspace.pane_count(), 1);
    let state = workspace.pane(right).expect("remaining pane");
    assert_eq!(
        state
            .tabs()
            .iter()
            .map(TabView::document)
            .collect::<Vec<_>>(),
        [&"right", &"left"]
    );
    assert_eq!(state.active_tab_id(), Some(left_tab));
    assert_eq!(workspace.focused(), right);
    let _ = right_tab;
    assert_invariants(&workspace);
}

#[test]
fn close_tab_preserves_order_and_selects_right_then_left() {
    let root = pane(2_000);
    let mut workspace = PaneWorkspace::<&'static str>::with_root_id(root);
    let first = add(&mut workspace, root, 2_001, "first");
    let active = add(&mut workspace, root, 2_002, "active");
    let right = add(&mut workspace, root, 2_003, "right");
    let tail = add(&mut workspace, root, 2_004, "tail");
    workspace.set_active_tab(root, active).expect("active tab");

    let removed = workspace
        .close_tab(root, first)
        .expect("close non-active tab");
    assert_eq!(removed.id(), first);
    assert_eq!(
        workspace
            .pane(root)
            .expect("root pane")
            .tabs()
            .iter()
            .map(TabView::document)
            .copied()
            .collect::<Vec<_>>(),
        vec!["active", "right", "tail"]
    );
    assert_eq!(
        workspace.pane(root).and_then(PaneState::active_tab_id),
        Some(active)
    );
    assert_eq!(workspace.focused(), root);

    let removed = workspace.close_tab(root, active).expect("close active tab");
    assert_eq!(removed.id(), active);
    assert_eq!(
        workspace.pane(root).and_then(PaneState::active_tab_id),
        Some(right)
    );

    workspace.set_active_tab(root, tail).expect("tail tab");
    let removed = workspace.close_tab(root, tail).expect("close active tail");
    assert_eq!(removed.id(), tail);
    assert_eq!(
        workspace
            .pane(root)
            .expect("root pane")
            .tabs()
            .iter()
            .map(TabView::document)
            .copied()
            .collect::<Vec<_>>(),
        vec!["right"]
    );
    assert_eq!(
        workspace.pane(root).and_then(PaneState::active_tab_id),
        Some(right)
    );
    assert_invariants(&workspace);
}

#[test]
fn close_tab_collapses_empty_pane_and_focuses_geometric_merge_target() {
    let root = pane(2_010);
    let mut workspace = PaneWorkspace::<&'static str>::with_root_id(root);
    let right = workspace.split_right(root).expect("split pane");
    let left_tab = add(&mut workspace, root, 2_011, "left");
    let right_tab = add(&mut workspace, right, 2_012, "right");

    let removed = workspace
        .close_tab(right, right_tab)
        .expect("close only tab");
    assert_eq!(removed.id(), right_tab);
    assert_eq!(workspace.pane_count(), 1);
    assert_eq!(workspace.pane_ids(), vec![root]);
    assert_eq!(workspace.focused(), root);
    assert_eq!(
        workspace.pane(root).and_then(PaneState::active_tab_id),
        Some(left_tab)
    );
    assert_eq!(
        workspace
            .pane(root)
            .expect("remaining pane")
            .tabs()
            .iter()
            .map(TabView::document)
            .copied()
            .collect::<Vec<_>>(),
        vec!["left"]
    );
    assert_invariants(&workspace);
}

#[test]
fn close_tab_allows_last_pane_to_become_empty() {
    let root = pane(2_020);
    let mut workspace = PaneWorkspace::<&'static str>::with_root_id(root);
    let only = add(&mut workspace, root, 2_021, "only");

    let removed = workspace.close_tab(root, only).expect("close last tab");
    assert_eq!(removed.id(), only);
    assert_eq!(workspace.pane_count(), 1);
    assert_eq!(workspace.pane_ids(), vec![root]);
    assert_eq!(workspace.focused(), root);
    assert!(workspace.pane(root).is_some_and(PaneState::is_empty));
    assert_eq!(
        workspace.pane(root).and_then(PaneState::active_tab_id),
        None
    );
    assert_invariants(&workspace);
}

#[test]
fn close_tab_errors_are_atomic() {
    let root = pane(2_030);
    let missing_pane = pane(2_031);
    let mut workspace = PaneWorkspace::<&'static str>::with_root_id(root);
    let existing = add(&mut workspace, root, 2_032, "existing");
    let snapshot = workspace.clone();

    assert_eq!(
        workspace.close_tab(missing_pane, existing),
        Err(PaneError::PaneNotFound(missing_pane))
    );
    assert_eq!(workspace, snapshot);

    let missing_tab = TabId::from_uuid(Uuid::from_u128(2_033));
    assert_eq!(
        workspace.close_tab(root, missing_tab),
        Err(PaneError::TabNotFound(missing_tab))
    );
    assert_eq!(workspace, snapshot);
    assert_invariants(&workspace);
}

#[test]
fn move_copy_duplicate_and_empty_leaf_rules() {
    let mut workspace = PaneWorkspace::<&'static str>::with_root_id(pane(1));
    let source = workspace.focused();
    let target = workspace.split_right(source).unwrap_or(pane(0));
    let first = add(&mut workspace, source, 20, "a");
    let second = add(&mut workspace, source, 21, "b");
    let copied = match workspace.copy_tab(source, target, first) {
        Ok(id) => id,
        Err(error) => panic!("copy failed: {error}"),
    };
    assert!(workspace.copy_tab(source, target, first).is_err());
    assert_eq!(workspace.move_tab(source, target, second), Ok(()));
    assert_eq!(workspace.pane(source).map(PaneState::len), Some(1));
    assert_eq!(
        workspace.move_tab(source, target, first),
        Err(PaneError::DuplicateDocument)
    );
    assert_invariants(&workspace);
    let _ = workspace.move_tab(source, target, copied);
}

#[test]
fn moving_last_tab_collapses_non_unique_empty_leaf_but_root_stays() {
    let mut workspace = PaneWorkspace::<&'static str>::with_root_id(pane(1));
    let source = workspace.focused();
    let target = workspace.split_down(source).unwrap_or(pane(0));
    let id = add(&mut workspace, source, 30, "only");
    assert_eq!(workspace.move_tab(source, target, id), Ok(()));
    assert_eq!(workspace.pane_count(), 1);
    assert_eq!(workspace.focused(), target);
    assert_eq!(workspace.pane(target).map(PaneState::len), Some(1));
    assert_invariants(&workspace);
}

#[test]
fn balance_uses_leaf_counts_and_focus_is_geometric() {
    let mut workspace = PaneWorkspace::<&'static str>::with_root_id(pane(1));
    let root = workspace.focused();
    let right = workspace.split_right(root).unwrap_or(pane(0));
    let _ = workspace.split_down(right);
    let _ = workspace.split_down(right);
    workspace.balance();
    assert!((workspace.root().ratio().unwrap_or(0.0) - 0.25).abs() < f32::EPSILON);
    assert_eq!(workspace.focus(root), Ok(()));
    let adjacent = workspace
        .focus_adjacent(FocusDirection::Right)
        .unwrap_or(pane(0));
    assert_ne!(adjacent, root);
    assert_eq!(workspace.focus_adjacent(FocusDirection::Left), Ok(root));
    assert_invariants(&workspace);
}

#[test]
fn deterministic_property_style_operation_sequence_preserves_invariants() {
    let mut workspace = PaneWorkspace::<u32>::with_root_id(pane(1));
    for step in 0..256_u32 {
        let ids = workspace.pane_ids();
        let pane = ids[(step as usize) % ids.len()];
        match step % 7 {
            0 if workspace.pane_count() < MAX_PANES => {
                let _ = workspace.split_right(pane);
            }
            1 => {
                let _ = workspace.set_split_ratio(pane, (step as f32) / 17.0);
            }
            2 => workspace.balance(),
            3 => {
                let _ = workspace.focus_adjacent(FocusDirection::Right);
            }
            4 => {
                let _ = workspace.open_document(pane, step);
            }
            5 => {
                if workspace.pane_count() > 1 {
                    let _ = workspace.close_pane(pane);
                }
            }
            _ => {
                let _ = workspace.focus(pane);
            }
        }
        assert_invariants(&workspace);
    }
}

/// Small checked-in operation corpus used as a deterministic fuzz gate.
/// The bytes are deliberately plain data so CI can replay failures without
/// a random seed, external corpus directory, or heavyweight dependency.
#[test]
fn deterministic_fuzz_corpus_preserves_pane_tree_invariants() {
    const CORPUS: &[u8] = &[
        0, 4, 8, 2, 1, 7, 3, 9, 5, 6, 10, 14, 13, 12, 11, 15, 18, 17, 16, 21, 20, 19, 24, 25, 26,
        27, 31, 30, 29, 28, 33, 37, 35, 34, 36, 42, 41, 40, 39, 38, 47, 46, 45, 44, 43, 52, 51, 50,
        49, 48, 57, 56, 55, 54, 53, 63, 62, 61, 60, 59, 58,
    ];
    let mut workspace = PaneWorkspace::<u32>::with_root_id(pane(9_001));
    for (step, opcode) in CORPUS.iter().copied().enumerate() {
        let ids = workspace.pane_ids();
        let pane = ids[step % ids.len()];
        match opcode % 12 {
            0 if workspace.pane_count() < MAX_PANES => {
                let _ = workspace.split_right(pane);
            }
            1 if workspace.pane_count() < MAX_PANES => {
                let _ = workspace.split_down(pane);
            }
            2 => {
                let _ = workspace.adjust_split_ratio(pane, opcode & 1 == 0, opcode & 2 != 0);
            }
            3 => workspace.balance(),
            4 => {
                let _ = workspace.focus_adjacent(FocusDirection::Left);
            }
            5 => {
                let _ = workspace.focus_adjacent(FocusDirection::Right);
            }
            6 => {
                let _ = workspace.open_document(pane, step as u32);
            }
            7 if workspace.pane_count() > 1 => {
                let _ = workspace.close_pane(pane);
            }
            8 => {
                let _ = workspace.focus(pane);
            }
            9 => {
                let _ = workspace.set_split_ratio(pane, (opcode as f32) / 10.0);
            }
            10 => {
                let active = workspace.pane(pane).and_then(PaneState::active_tab_id);
                if let Some(active) = active {
                    let _ = workspace.set_active_tab(pane, active);
                }
            }
            _ => {
                let _ = workspace.focus_adjacent(FocusDirection::Down);
            }
        }
        assert_invariants(&workspace);
    }
}

#[test]
fn illegal_operations_do_not_mutate_snapshot() {
    let mut workspace = PaneWorkspace::<&'static str>::with_root_id(pane(1));
    let root = workspace.focused();
    let snapshot = workspace.clone();
    assert_eq!(
        workspace.close_pane(root),
        Err(PaneError::CannotCloseLastPane)
    );
    assert_eq!(workspace, snapshot);
    assert_eq!(
        workspace.focus_adjacent(FocusDirection::Left),
        Err(PaneError::NoAdjacentPane {
            from: root,
            direction: FocusDirection::Left,
        })
    );
    assert_eq!(workspace, snapshot);
    assert_eq!(
        workspace.set_split_ratio(root, f32::INFINITY),
        Err(PaneError::InvalidRatio)
    );
    assert_eq!(workspace, snapshot);
}

#[test]
fn pinned_tab_state_survives_cross_pane_move_and_copy() {
    let root = pane(9_100);
    let target = pane(9_101);
    let id = TabId::from_uuid(Uuid::from_u128(9_102));
    let pinned = TabView::with_pinned(id, "doc", (), true);
    assert!(pinned.is_pinned());
    let mut panes = BTreeMap::new();
    panes.insert(root, PaneState::with_tabs(vec![pinned]));
    panes.insert(target, PaneState::new());
    let mut workspace = PaneWorkspace::from_parts(
        PaneNode::Split {
            axis: SplitAxis::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneNode::Leaf(root)),
            second: Box::new(PaneNode::Leaf(target)),
        },
        panes,
        root,
    )
    .expect("valid pinned workspace");
    workspace
        .copy_tab(root, target, id)
        .expect("copy pinned tab");
    assert!(
        workspace
            .pane(target)
            .and_then(|pane| pane.tabs().first())
            .is_some_and(TabView::is_pinned)
    );

    let move_root = pane(9_110);
    let move_target = pane(9_111);
    let mut move_panes = BTreeMap::new();
    move_panes.insert(
        move_root,
        PaneState::with_tabs(vec![TabView::with_pinned(id, "move-doc", (), true)]),
    );
    move_panes.insert(move_target, PaneState::new());
    let mut move_workspace = PaneWorkspace::from_parts(
        PaneNode::Split {
            axis: SplitAxis::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneNode::Leaf(move_root)),
            second: Box::new(PaneNode::Leaf(move_target)),
        },
        move_panes,
        move_root,
    )
    .expect("valid move workspace");
    move_workspace
        .move_tab(move_root, move_target, id)
        .expect("move pinned tab");
    assert!(
        move_workspace
            .pane(move_target)
            .and_then(|pane| pane.tabs().iter().find(|tab| tab.id() == id))
            .is_some_and(TabView::is_pinned)
    );
}
