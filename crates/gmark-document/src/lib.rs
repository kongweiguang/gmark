// @author kongweiguang

//! gmark 源码优先文档内核。
//!
//! Markdown 源文本是唯一持久真值。解析树、块树和布局都是可从指定
//! [`Revision`] 重建的投影，不能反向覆盖用户源码。

#![forbid(unsafe_code)]

mod atomic_save;
mod document;
mod source_format;

pub use atomic_save::{AtomicWriteError, AtomicWriteStage, atomic_write};
pub use document::{
    DocumentError, DocumentSnapshot, Revision, SourceDocument, TextEdit, Transaction,
};
pub use source_format::{LineEnding, LineEndingStatus, SourceFormatSnapshot, SourceFormatSummary};
