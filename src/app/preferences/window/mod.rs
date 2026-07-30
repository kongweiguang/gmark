// @author kongweiguang

use super::*;
use std::collections::BTreeMap;

mod state;
use state::*;

mod constructor;
mod controls;
mod dropdowns;
mod editor_page;
mod image_page;
mod input_controls;
mod launch;
mod navigation;
mod persistence;
mod render;
mod search;
mod search_results;
mod shortcuts;
mod startup_page;
mod status_bar_page;
mod theme_page;

#[cfg(test)]
use launch::open_preferences_window_with_state;
pub(crate) use launch::{localized_shortcut_command_label, open_preferences_window};

#[cfg(test)]
#[path = "../../../../tests/unit/config/preferences.rs"]
mod tests;
