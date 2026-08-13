// @author kongweiguang

//! Internal model partitions kept private so the public pane API remains flat.

mod errors;
mod helpers;
mod ids;
mod state;
mod tree;
mod workspace;

pub use errors::*;
pub use helpers::*;
#[cfg(test)]
pub(crate) use helpers::{Rect, collect_ids, collect_rects};
pub use ids::*;
pub use state::*;
pub use tree::*;
pub use workspace::*;
