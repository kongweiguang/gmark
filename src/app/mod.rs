// @author kongweiguang

//! Application-layer composition root.
//!
//! This tree owns bootstrap, menus, preferences, diagnostics, and update
//! coordination while protocol and document domains remain outside it.

#[path = "menu/mod.rs"]
pub(crate) mod app_menu;
pub(crate) mod bootstrap;
pub(crate) mod config;
pub(crate) mod diagnostics;
pub(crate) mod document_service;
pub(crate) mod preferences;
#[path = "update/mod.rs"]
pub(crate) mod updater;
