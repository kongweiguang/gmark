// @author kongweiguang

//! Process bootstrap assets, command-line parsing, and runtime composition.

mod assets;
pub(crate) mod cli;
mod runtime;

pub(crate) use runtime::run_app;
