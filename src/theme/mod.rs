// @author kongweiguang

//! Backwards-compatible theme facade.
//!
//! Theme implementation lives in `crate::ui::theme`; this module preserves
//! established crate-local imports while callers migrate gradually.

pub use crate::ui::theme::*;
