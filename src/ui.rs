// @author kongweiguang

//! UI foundation and its temporary crate-local composition point.
//!
//! `lib.rs` remains outside this refactor's ownership boundary. It already
//! declares this module, so Wave 3 composes both the UI and platform trees here
//! while the established root modules expose compatibility facades.

#[path = "ui/actions/mod.rs"]
pub(crate) mod actions;
#[path = "ui/controls/mod.rs"]
pub(crate) mod controls;
#[path = "ui/i18n/mod.rs"]
pub(crate) mod i18n;
#[path = "ui/theme/mod.rs"]
pub(crate) mod theme;

#[path = "platform/mod.rs"]
pub(crate) mod platform;

#[cfg(test)]
pub(crate) use controls::centered_column_ratio;
pub(crate) use controls::{centered_column_width, ui_tooltip};

#[cfg(test)]
#[path = "../tests/unit/ui.rs"]
mod tests;
