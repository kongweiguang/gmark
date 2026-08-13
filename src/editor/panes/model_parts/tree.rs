// @author kongweiguang

//! Binary pane layout nodes and their public structural queries.

use super::helpers::normalize_ratio;
use super::{PaneError, PaneId, SplitAxis};

/// A leaf reference or a binary split in the workspace layout tree.
#[derive(Clone, Debug, PartialEq)]
pub enum PaneNode {
    Leaf(PaneId),
    Split {
        axis: SplitAxis,
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

impl PaneNode {
    pub fn leaf(id: PaneId) -> Self {
        Self::Leaf(id)
    }

    pub fn split(
        axis: SplitAxis,
        ratio: f32,
        first: Self,
        second: Self,
    ) -> Result<Self, PaneError> {
        let ratio = normalize_ratio(ratio)?;
        Ok(Self::Split {
            axis,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        })
    }

    pub fn as_leaf(&self) -> Option<PaneId> {
        match self {
            Self::Leaf(id) => Some(*id),
            Self::Split { .. } => None,
        }
    }

    pub fn axis(&self) -> Option<SplitAxis> {
        match self {
            Self::Leaf(_) => None,
            Self::Split { axis, .. } => Some(*axis),
        }
    }

    pub fn ratio(&self) -> Option<f32> {
        match self {
            Self::Leaf(_) => None,
            Self::Split { ratio, .. } => Some(*ratio),
        }
    }

    pub fn children(&self) -> Option<(&Self, &Self)> {
        match self {
            Self::Leaf(_) => None,
            Self::Split { first, second, .. } => Some((first, second)),
        }
    }

    pub fn leaf_count(&self) -> usize {
        match self {
            Self::Leaf(_) => 1,
            Self::Split { first, second, .. } => first.leaf_count() + second.leaf_count(),
        }
    }

    pub(super) fn contains_pane(&self, pane: PaneId) -> bool {
        match self {
            Self::Leaf(id) => *id == pane,
            Self::Split { first, second, .. } => {
                first.contains_pane(pane) || second.contains_pane(pane)
            }
        }
    }
}
