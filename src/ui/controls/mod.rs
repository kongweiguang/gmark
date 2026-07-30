// @author kongweiguang

//! Shared UI controls and compact layout primitives.

mod layout;
pub(crate) mod switch;
mod tooltip;

#[cfg(test)]
pub(crate) use layout::centered_column_ratio;
pub(crate) use layout::centered_column_width;
pub(crate) use tooltip::ui_tooltip;
