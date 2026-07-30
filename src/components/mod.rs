// @author kongweiguang

//! Shared UI components and Markdown editing primitives.
//!
//! Block、Markdown 与预览渲染的源码按编辑器领域归档，但由这个下层组合根唯一挂载；
//! Editor 与 DocumentHost 只消费同一组类型，避免二者互相依赖或重复编译实体类型。

#[path = "../editor/block/mod.rs"]
pub(crate) mod block;
#[path = "../editor/render/latex/mod.rs"]
pub(crate) mod latex;
#[path = "../editor/document/markdown/mod.rs"]
pub(crate) mod markdown;
#[path = "../editor/render/mermaid/mod.rs"]
pub(crate) mod mermaid;

pub(crate) use crate::ui::actions;
pub(crate) use crate::ui::controls::switch;

pub use actions::*;
pub use block::*;
pub(crate) use latex::*;
pub(crate) use markdown::html::*;
pub use markdown::resource::{ResourceKind, ResourceLocation, ResourceRecord, ResourceStatus};
pub use markdown::table::*;
pub(crate) use markdown::toc::*;
pub(crate) use mermaid::*;
