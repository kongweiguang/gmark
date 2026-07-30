// @author kongweiguang

//! Shared UI components and Markdown editing primitives.

mod actions;
pub(crate) use crate::editor::block;
pub(crate) use crate::editor::document::markdown;
pub(crate) use crate::editor::render::{latex, mermaid};
pub(crate) mod switch;

pub use actions::*;
pub use block::*;
pub(crate) use latex::*;
pub(crate) use markdown::html::*;
pub use markdown::resource::{ResourceKind, ResourceLocation, ResourceRecord, ResourceStatus};
pub use markdown::table::*;
pub(crate) use markdown::toc::*;
pub(crate) use mermaid::*;
