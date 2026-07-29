// @author kongweiguang

//! GMark 的纯 Markdown 值模型、解析、序列化和源码映射基础。

#![forbid(unsafe_code)]

mod block;
mod event;
mod html;
mod inline;
mod parser;
mod parser_builder;
mod resource;
mod serializer;
mod source;
mod table;
mod toc;

pub use block::{
    Block, BlockKind, CalloutKind, CodeBlock, CodeFence, Heading, HeadingAttribute, List,
    MetadataKind,
};
pub use event::{MarkdownEvent, MarkdownEventKind, MarkdownTag, MarkdownTagEnd};
pub use html::{
    HtmlDocument, HtmlFragment, HtmlParserKind, HtmlSafety, escape_html, sanitize_html_for_export,
};
pub use inline::{Inline, InlineKind, LinkKind, LinkTarget};
pub use parser::{MarkdownDocument, MarkdownParser, parse_markdown};
pub use resource::{
    RESOURCE_MARKER, ResourceKind, ResourceLocation, ResourceRecord, ResourceStatus,
};
pub use serializer::{
    MarkdownSerializer, SerializationMode, serialize_canonical_markdown, serialize_markdown,
};
pub use source::{
    LineEnding, LineEndingSummary, SourceFormat, SourceFormatError, SourceMap, SourceRange,
    SourceRangeError,
};
pub use table::{Table, TableAlignment, TableCell};
pub use toc::{TableOfContents, TocEntry, slugify};
