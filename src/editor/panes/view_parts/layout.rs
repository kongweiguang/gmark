// @author kongweiguang

//! Pure recursive geometry and mount planning for pane views.

use std::collections::BTreeMap;

use crate::editor::panes::{
    MAX_SPLIT_RATIO, MIN_SPLIT_RATIO, PaneId, PaneNode, PaneWorkspace, SplitAxis, TabId,
};

/// Minimum content size of one visible pane leaf.
pub const MIN_PANE_WIDTH: f32 = 80.0;
/// Minimum content height of one visible pane leaf.
pub const MIN_PANE_HEIGHT: f32 = 100.0;
/// Pointer hit area reserved for a divider.  The visible line is one pixel.
pub const PANE_DIVIDER_HIT_SIZE: f32 = 6.0;
/// Stable height of each pane-local tab bar, aligned with the root tab chrome.
pub const PANE_TAB_BAR_HEIGHT: f32 = 36.0;

/// Viewport dimensions passed to the pure layout function.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaneViewport {
    pub width: f32,
    pub height: f32,
}

impl PaneViewport {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// A pane's absolute rectangle in the workspace viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaneRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PaneRect {
    const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Geometry and path information for one split divider.
#[derive(Clone, Debug, PartialEq)]
pub struct PaneDivider {
    /// The split's root-to-child path.  This identifies nested splits without
    /// conflating a split with one of its descendant leaf ids.
    path: Vec<bool>,
    axis: SplitAxis,
    rect: PaneRect,
    ratio: f32,
    span: f32,
}

impl PaneDivider {
    pub fn path(&self) -> &[bool] {
        &self.path
    }

    pub const fn axis(&self) -> SplitAxis {
        self.axis
    }

    pub const fn rect(&self) -> PaneRect {
        self.rect
    }

    pub const fn ratio(&self) -> f32 {
        self.ratio
    }

    /// Length available to the first child after reserving divider hit area.
    pub const fn span(&self) -> f32 {
        self.span
    }
}

/// Result of laying out a pane tree for a viewport.
#[derive(Clone, Debug, PartialEq)]
pub struct PaneLayout {
    rects: BTreeMap<PaneId, PaneRect>,
    order: Vec<PaneId>,
    dividers: Vec<PaneDivider>,
    hidden: Vec<PaneId>,
    focused: PaneId,
    degraded: bool,
}

impl PaneLayout {
    pub fn rect(&self, pane: PaneId) -> Option<PaneRect> {
        self.rects.get(&pane).copied()
    }

    pub fn rects(&self) -> &BTreeMap<PaneId, PaneRect> {
        &self.rects
    }

    /// Leaf order is deterministic tree (first-child then second-child) order.
    pub fn pane_order(&self) -> &[PaneId] {
        &self.order
    }

    pub fn dividers(&self) -> &[PaneDivider] {
        &self.dividers
    }

    /// Leaves omitted from the content surface in compact/degraded layout.
    pub fn hidden(&self) -> &[PaneId] {
        &self.hidden
    }

    pub const fn focused(&self) -> PaneId {
        self.focused
    }

    /// True when the viewport cannot satisfy the minimum dimensions for all
    /// leaves.  The model tree and ratios are never changed in this state.
    pub const fn is_degraded(&self) -> bool {
        self.degraded
    }

    pub fn visible_count(&self) -> usize {
        self.rects.len()
    }

    pub fn hidden_count(&self) -> usize {
        self.hidden.len()
    }
}

/// Compute recursive pane geometry without mutating the model.
///
/// Every leaf remains visible even when the viewport is compact. The
/// `degraded` flag only relaxes minimum CSS sizes; it never replaces sibling
/// panes with an implementation-id switcher or mutates the durable tree.
pub fn compute_pane_layout(root: &PaneNode, viewport: PaneViewport, focused: PaneId) -> PaneLayout {
    let mut rects = BTreeMap::new();
    let mut order = Vec::new();
    let mut dividers = Vec::new();
    layout_node(
        root,
        PaneRect::new(0.0, 0.0, viewport.width.max(0.0), viewport.height.max(0.0)),
        &mut Vec::new(),
        &mut rects,
        &mut order,
        &mut dividers,
    );

    let focused = if order.contains(&focused) {
        focused
    } else {
        // A valid PaneWorkspace always has a focused leaf.  Keeping this
        // fallback makes the pure function total for callers restoring an
        // intermediate snapshot.
        order.first().copied().unwrap_or(focused)
    };
    let enough_space = viewport.width >= MIN_PANE_WIDTH
        && viewport.height >= MIN_PANE_HEIGHT
        && rects
            .values()
            .all(|rect| rect.width >= MIN_PANE_WIDTH && rect.height >= MIN_PANE_HEIGHT);
    PaneLayout {
        rects,
        order,
        dividers,
        hidden: Vec::new(),
        focused,
        degraded: !enough_space,
    }
}

fn layout_node(
    node: &PaneNode,
    rect: PaneRect,
    path: &mut Vec<bool>,
    rects: &mut BTreeMap<PaneId, PaneRect>,
    order: &mut Vec<PaneId>,
    dividers: &mut Vec<PaneDivider>,
) {
    match node {
        PaneNode::Leaf(id) => {
            rects.insert(*id, rect);
            order.push(*id);
        }
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let ratio = ratio.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO);
            let (first_rect, divider_rect, second_rect, span) = match axis {
                SplitAxis::Horizontal => {
                    let span = (rect.width - PANE_DIVIDER_HIT_SIZE).max(0.0);
                    let first_width = span * ratio;
                    let second_width = (span - first_width).max(0.0);
                    (
                        PaneRect::new(rect.x, rect.y, first_width, rect.height),
                        PaneRect::new(
                            rect.x + first_width,
                            rect.y,
                            PANE_DIVIDER_HIT_SIZE,
                            rect.height,
                        ),
                        PaneRect::new(
                            rect.x + first_width + PANE_DIVIDER_HIT_SIZE,
                            rect.y,
                            second_width,
                            rect.height,
                        ),
                        span,
                    )
                }
                SplitAxis::Vertical => {
                    let span = (rect.height - PANE_DIVIDER_HIT_SIZE).max(0.0);
                    let first_height = span * ratio;
                    let second_height = (span - first_height).max(0.0);
                    (
                        PaneRect::new(rect.x, rect.y, rect.width, first_height),
                        PaneRect::new(
                            rect.x,
                            rect.y + first_height,
                            rect.width,
                            PANE_DIVIDER_HIT_SIZE,
                        ),
                        PaneRect::new(
                            rect.x,
                            rect.y + first_height + PANE_DIVIDER_HIT_SIZE,
                            rect.width,
                            second_height,
                        ),
                        span,
                    )
                }
            };
            dividers.push(PaneDivider {
                path: path.clone(),
                axis: *axis,
                rect: divider_rect,
                ratio,
                span,
            });
            path.push(false);
            layout_node(first, first_rect, path, rects, order, dividers);
            path.pop();
            path.push(true);
            layout_node(second, second_rect, path, rects, order, dividers);
            path.pop();
        }
    }
}

/// Return the one active tab that the recursive view would mount per pane.
///
/// This pure helper is useful to integration tests that cannot create a GPUI
/// application.  Inactive tabs deliberately never appear in the result.
pub fn active_pane_mount_plan<D, V>(workspace: &PaneWorkspace<D, V>) -> Vec<(PaneId, TabId)> {
    workspace
        .pane_ids()
        .into_iter()
        .filter_map(|pane| {
            workspace
                .pane(pane)
                .and_then(|state| state.active_tab_id())
                .map(|tab| (pane, tab))
        })
        .collect()
}
