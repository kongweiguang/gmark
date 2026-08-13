// @author kongweiguang

//! Recursive GPUI rendering for the durable pane model.
//!
//! The layout calculation remains pure and the public pane view API stays
//! flat while implementation responsibilities live in private partitions.

#[path = "view_parts/mod.rs"]
mod view_parts;

pub use view_parts::*;

#[cfg(test)]
#[path = "../../../tests/unit/editor/panes_view_private.rs"]
mod tests;
