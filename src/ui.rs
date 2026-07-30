// @author kongweiguang

//! UI foundation: actions, controls, localization, and theme tokens.

#[path = "ui/actions/mod.rs"]
pub(crate) mod actions;
#[path = "ui/controls/mod.rs"]
pub(crate) mod controls;
#[path = "ui/i18n/mod.rs"]
pub(crate) mod i18n;
#[path = "ui/theme/mod.rs"]
pub(crate) mod theme;

#[cfg(test)]
pub(crate) use controls::centered_column_ratio;
pub(crate) use controls::{centered_column_width, ui_tooltip};

#[cfg(test)]
#[path = "../tests/unit/ui.rs"]
mod tests;
