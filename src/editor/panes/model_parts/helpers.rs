// @author kongweiguang

//! Pure tree, transfer, and geometric helpers shared by the workspace.

use std::collections::{BTreeMap, BTreeSet};

use super::*;

pub(super) fn normalize_ratio(ratio: f32) -> Result<f32, PaneError> {
    if !ratio.is_finite() {
        return Err(PaneError::InvalidRatio);
    }
    Ok(ratio.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO))
}

pub(crate) fn collect_ids(node: &PaneNode) -> BTreeSet<PaneId> {
    let mut ids = BTreeSet::new();
    collect_ids_into(node, &mut ids);
    ids
}

pub(super) fn collect_ids_into(node: &PaneNode, ids: &mut BTreeSet<PaneId>) {
    match node {
        PaneNode::Leaf(id) => {
            ids.insert(*id);
        }
        PaneNode::Split { first, second, .. } => {
            collect_ids_into(first, ids);
            collect_ids_into(second, ids);
        }
    }
}

pub(super) fn collect_ids_in_order(node: &PaneNode, ids: &mut Vec<PaneId>) {
    match node {
        PaneNode::Leaf(id) => ids.push(*id),
        PaneNode::Split { first, second, .. } => {
            collect_ids_in_order(first, ids);
            collect_ids_in_order(second, ids);
        }
    }
}

pub(super) fn split_leaf(
    node: &mut PaneNode,
    target: PaneId,
    new_id: PaneId,
    axis: SplitAxis,
    ratio: f32,
    new_is_first: bool,
) -> bool {
    match node {
        PaneNode::Leaf(id) if *id == target => {
            let (first, second) = if new_is_first {
                (
                    Box::new(PaneNode::Leaf(new_id)),
                    Box::new(PaneNode::Leaf(target)),
                )
            } else {
                (
                    Box::new(PaneNode::Leaf(target)),
                    Box::new(PaneNode::Leaf(new_id)),
                )
            };
            *node = PaneNode::Split {
                axis,
                ratio,
                first,
                second,
            };
            true
        }
        PaneNode::Leaf(_) => false,
        PaneNode::Split { first, second, .. } => {
            if first.contains_pane(target) {
                split_leaf(first, target, new_id, axis, ratio, new_is_first)
            } else {
                split_leaf(second, target, new_id, axis, ratio, new_is_first)
            }
        }
    }
}

pub(super) fn find_path(node: &PaneNode, pane: PaneId) -> Option<Vec<bool>> {
    match node {
        PaneNode::Leaf(id) if *id == pane => Some(Vec::new()),
        PaneNode::Leaf(_) => None,
        PaneNode::Split { first, second, .. } => {
            if let Some(mut path) = find_path(first, pane) {
                path.insert(0, false);
                Some(path)
            } else if let Some(mut path) = find_path(second, pane) {
                path.insert(0, true);
                Some(path)
            } else {
                None
            }
        }
    }
}

pub(super) fn ratio_at_path(node: &PaneNode, path: &[bool]) -> Option<f32> {
    if path.is_empty() {
        return match node {
            PaneNode::Leaf(_) => None,
            PaneNode::Split { ratio, .. } => Some(*ratio),
        };
    }
    match node {
        PaneNode::Leaf(_) => None,
        PaneNode::Split { first, second, .. } => {
            if path[0] {
                ratio_at_path(second, &path[1..])
            } else {
                ratio_at_path(first, &path[1..])
            }
        }
    }
}

pub(super) fn set_ratio_at_path(node: &mut PaneNode, path: &[bool], ratio: f32) -> bool {
    if path.is_empty() {
        return match node {
            PaneNode::Leaf(_) => false,
            PaneNode::Split { ratio: current, .. } => {
                *current = ratio;
                true
            }
        };
    }
    match node {
        PaneNode::Leaf(_) => false,
        PaneNode::Split { first, second, .. } => {
            if path[0] {
                set_ratio_at_path(second, &path[1..], ratio)
            } else {
                set_ratio_at_path(first, &path[1..], ratio)
            }
        }
    }
}

pub(super) fn balanced_ratio_at_path(node: &PaneNode, path: &[bool]) -> Option<f32> {
    if path.is_empty() {
        return match node {
            PaneNode::Leaf(_) => None,
            PaneNode::Split { first, second, .. } => {
                Some(balance_ratio(first.leaf_count(), second.leaf_count()))
            }
        };
    }
    match node {
        PaneNode::Leaf(_) => None,
        PaneNode::Split { first, second, .. } => {
            if path[0] {
                balanced_ratio_at_path(second, &path[1..])
            } else {
                balanced_ratio_at_path(first, &path[1..])
            }
        }
    }
}

pub(super) fn balance_node(node: &mut PaneNode) -> usize {
    match node {
        PaneNode::Leaf(_) => 1,
        PaneNode::Split {
            first,
            second,
            ratio,
            ..
        } => {
            let left = balance_node(first);
            let right = balance_node(second);
            *ratio = balance_ratio(left, right);
            left + right
        }
    }
}

pub(super) fn balance_ratio(left: usize, right: usize) -> f32 {
    let total = left.saturating_add(right).max(1);
    (left as f32 / total as f32).clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO)
}

pub(super) fn leftmost_id(node: &PaneNode) -> PaneId {
    match node {
        PaneNode::Leaf(id) => *id,
        PaneNode::Split { first, .. } => leftmost_id(first),
    }
}

pub(super) fn rightmost_id(node: &PaneNode) -> PaneId {
    match node {
        PaneNode::Leaf(id) => *id,
        PaneNode::Split { second, .. } => rightmost_id(second),
    }
}

pub(super) fn append_to_leaf<D, V>(
    panes: &mut BTreeMap<PaneId, PaneState<D, V>>,
    pane: PaneId,
    mut tabs: Vec<TabView<D, V>>,
    active: Option<TabId>,
) {
    if let Some(state) = panes.get_mut(&pane) {
        state.tabs.append(&mut tabs);
        if let Some(active) = active {
            if state.tabs.iter().any(|tab| tab.id == active) {
                state.active = Some(active);
            }
        } else if state.active.is_none() {
            state.active = state.tabs.first().map(TabView::id);
        }
    }
}

pub(super) fn collapse_at_path<D, V>(
    node: PaneNode,
    path: &[bool],
    source_is_first: bool,
    tabs: Vec<TabView<D, V>>,
    active: Option<TabId>,
    panes: &mut BTreeMap<PaneId, PaneState<D, V>>,
) -> (PaneNode, PaneId) {
    match node {
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } if path.is_empty() => {
            let sibling = if source_is_first { *second } else { *first };
            let target = if source_is_first {
                leftmost_id(&sibling)
            } else {
                rightmost_id(&sibling)
            };
            append_to_leaf(panes, target, tabs, active);
            let _ = (axis, ratio);
            (sibling, target)
        }
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            if path[0] {
                let (updated, target) =
                    collapse_at_path(*second, &path[1..], source_is_first, tabs, active, panes);
                (
                    PaneNode::Split {
                        axis,
                        ratio,
                        first,
                        second: Box::new(updated),
                    },
                    target,
                )
            } else {
                let (updated, target) =
                    collapse_at_path(*first, &path[1..], source_is_first, tabs, active, panes);
                (
                    PaneNode::Split {
                        axis,
                        ratio,
                        first: Box::new(updated),
                        second,
                    },
                    target,
                )
            }
        }
        PaneNode::Leaf(id) => (PaneNode::Leaf(id), id),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

impl Rect {
    pub(crate) const ROOT: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    fn right(self) -> f64 {
        self.x + self.w
    }

    fn bottom(self) -> f64 {
        self.y + self.h
    }

    fn center_x(self) -> f64 {
        self.x + self.w / 2.0
    }

    fn center_y(self) -> f64 {
        self.y + self.h / 2.0
    }
}

pub(crate) fn collect_rects(node: &PaneNode, rect: Rect, out: &mut Vec<(PaneId, Rect)>) {
    match node {
        PaneNode::Leaf(id) => out.push((*id, rect)),
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => match axis {
            SplitAxis::Horizontal => {
                let first_width = rect.w * f64::from(*ratio);
                collect_rects(
                    first,
                    Rect {
                        x: rect.x,
                        y: rect.y,
                        w: first_width,
                        h: rect.h,
                    },
                    out,
                );
                collect_rects(
                    second,
                    Rect {
                        x: rect.x + first_width,
                        y: rect.y,
                        w: rect.w - first_width,
                        h: rect.h,
                    },
                    out,
                );
            }
            SplitAxis::Vertical => {
                let first_height = rect.h * f64::from(*ratio);
                collect_rects(
                    first,
                    Rect {
                        x: rect.x,
                        y: rect.y,
                        w: rect.w,
                        h: first_height,
                    },
                    out,
                );
                collect_rects(
                    second,
                    Rect {
                        x: rect.x,
                        y: rect.y + first_height,
                        w: rect.w,
                        h: rect.h - first_height,
                    },
                    out,
                );
            }
        },
    }
}

pub(super) fn choose_adjacent(
    from: PaneId,
    current: Rect,
    direction: FocusDirection,
    rects: &[(PaneId, Rect)],
) -> Option<PaneId> {
    const EPSILON: f64 = 1e-9;
    let mut candidates = Vec::new();
    for (id, rect) in rects.iter().copied() {
        if id == from {
            continue;
        }
        let overlap_x = (current.right().min(rect.right()) - current.x.max(rect.x)).max(0.0);
        let overlap_y = (current.bottom().min(rect.bottom()) - current.y.max(rect.y)).max(0.0);
        let (primary, secondary, aligned) = match direction {
            FocusDirection::Right => (
                (rect.x - current.right()).max(0.0),
                (rect.center_y() - current.center_y()).abs(),
                rect.x >= current.right() - EPSILON && overlap_y > EPSILON,
            ),
            FocusDirection::Left => (
                (current.x - rect.right()).max(0.0),
                (rect.center_y() - current.center_y()).abs(),
                rect.right() <= current.x + EPSILON && overlap_y > EPSILON,
            ),
            FocusDirection::Down => (
                (rect.y - current.bottom()).max(0.0),
                (rect.center_x() - current.center_x()).abs(),
                rect.y >= current.bottom() - EPSILON && overlap_x > EPSILON,
            ),
            FocusDirection::Up => (
                (current.y - rect.bottom()).max(0.0),
                (rect.center_x() - current.center_x()).abs(),
                rect.bottom() <= current.y + EPSILON && overlap_x > EPSILON,
            ),
        };
        if aligned {
            candidates.push((primary, secondary, id));
        }
    }
    if candidates.is_empty() {
        for (id, rect) in rects.iter().copied() {
            if id == from {
                continue;
            }
            let (ahead, primary, secondary) = match direction {
                FocusDirection::Right => (
                    rect.center_x() > current.center_x() + EPSILON,
                    (rect.center_x() - current.center_x()).max(0.0),
                    (rect.center_y() - current.center_y()).abs(),
                ),
                FocusDirection::Left => (
                    rect.center_x() < current.center_x() - EPSILON,
                    (current.center_x() - rect.center_x()).max(0.0),
                    (rect.center_y() - current.center_y()).abs(),
                ),
                FocusDirection::Down => (
                    rect.center_y() > current.center_y() + EPSILON,
                    (rect.center_y() - current.center_y()).max(0.0),
                    (rect.center_x() - current.center_x()).abs(),
                ),
                FocusDirection::Up => (
                    rect.center_y() < current.center_y() - EPSILON,
                    (current.center_y() - rect.center_y()).max(0.0),
                    (rect.center_x() - current.center_x()).abs(),
                ),
            };
            if ahead {
                candidates.push((primary, secondary, id));
            }
        }
    }
    candidates.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.total_cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    candidates.first().map(|(_, _, id)| *id)
}
