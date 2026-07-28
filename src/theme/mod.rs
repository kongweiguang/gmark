// @author kongweiguang

//! Theme configuration and global theme access.
//!
//! Built-in themes are JSON-serializable for export and regression fixtures so
//! editor colors, spacing, and typography stay separate from render logic.

mod theme;
pub use theme::*;
