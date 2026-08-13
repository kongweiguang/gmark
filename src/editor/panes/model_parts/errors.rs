// @author kongweiguang

//! Errors shared by pane model operations.

use std::error::Error;
use std::fmt;

use super::{FocusDirection, MAX_PANES, PaneId, TabId};

/// Domain errors returned by pane operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaneError {
    PaneNotFound(PaneId),
    TabNotFound(TabId),
    DuplicateDocument,
    DuplicateTabId(TabId),
    TooManyPanes,
    CannotCloseLastPane,
    SamePane,
    NoSplitForPane(PaneId),
    NoAdjacentPane {
        from: PaneId,
        direction: FocusDirection,
    },
    InvalidRatio,
    InvalidTree,
    IdCollision,
}

impl fmt::Display for PaneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PaneNotFound(id) => write!(f, "pane {:?} was not found", id),
            Self::TabNotFound(id) => write!(f, "tab {:?} was not found", id),
            Self::DuplicateDocument => {
                write!(f, "the target pane already contains the document")
            }
            Self::DuplicateTabId(id) => write!(f, "tab id {:?} is already in use", id),
            Self::TooManyPanes => write!(
                f,
                "the workspace cannot contain more than {MAX_PANES} panes"
            ),
            Self::CannotCloseLastPane => write!(f, "the last pane cannot be closed"),
            Self::SamePane => write!(f, "source and target panes must differ"),
            Self::NoSplitForPane(id) => write!(f, "pane {:?} has no enclosing split", id),
            Self::NoAdjacentPane { from, direction } => {
                write!(
                    f,
                    "no pane adjacent to {:?} in {:?} direction",
                    from, direction
                )
            }
            Self::InvalidRatio => write!(f, "split ratio must be finite"),
            Self::InvalidTree => write!(f, "pane tree and pane-state map disagree"),
            Self::IdCollision => write!(f, "could not allocate a unique UUID"),
        }
    }
}

impl Error for PaneError {}
