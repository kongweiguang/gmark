// @author kongweiguang

//! Pure pane/split state.
//!
//! The tree stores only durable PaneId references. Pane state lives in a
//! map keyed by those ids so persistence, rendering, and close/merge logic all
//! observe one authoritative copy of each pane.

#[path = "model_parts/mod.rs"]
mod model_parts;

pub use model_parts::*;
#[cfg(test)]
pub(crate) use model_parts::{Rect, collect_ids, collect_rects};

#[cfg(test)]
#[path = "../../../tests/unit/editor/panes_model_private.rs"]
mod tests;
