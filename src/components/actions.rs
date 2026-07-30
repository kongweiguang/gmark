// @author kongweiguang

//! Backwards-compatible action facade.
//!
//! Stable action ids and shortcut definitions now live in `crate::ui::actions`.

pub use crate::ui::actions::*;

#[cfg(test)]
#[path = "../../tests/unit/components/actions.rs"]
mod tests;
