// @author kongweiguang

//! Application-layer composition root.
//!
//! This tree owns GPUI bootstrap-adjacent behavior while protocol and document
//! domains remain outside it. Root modules retain temporary compatibility
//! facades until the root module registry is switched as one change.

#[path = "menu/mod.rs"]
pub(crate) mod app_menu;
pub(crate) mod bootstrap;
pub(crate) mod config;
pub(crate) mod diagnostics;
pub(crate) mod preferences;
#[path = "update/mod.rs"]
pub(crate) mod updater;
