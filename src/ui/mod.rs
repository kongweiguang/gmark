// @author kongweiguang

//! UI foundation: actions, controls, localization, and theme tokens.

pub(crate) mod actions;
pub(crate) mod controls;
pub(crate) mod i18n;
pub(crate) mod theme;

#[cfg(test)]
pub(crate) use controls::centered_column_ratio;
pub(crate) use controls::{centered_column_width, ui_tooltip};

#[cfg(test)]
#[path = "../../tests/unit/ui.rs"]
mod tests;
