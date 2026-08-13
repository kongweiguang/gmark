// @author kongweiguang

//! Internal view partitions whose re-exports keep the pane API unchanged.

mod document;
mod layout;
mod pane;
mod workspace;
mod workspace_render;

pub use document::*;
pub use layout::*;
pub use pane::*;
pub use workspace::*;
