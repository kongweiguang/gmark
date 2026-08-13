// @author kongweiguang

//! Gmark 的纯 Markdown 值模型、解析、序列化和源码映射基础。

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
mod visible_text;

pub use block::{
    Block, BlockKind, CalloutKind, CodeBlock, CodeFence, Heading, HeadingAttribute, List,
    MetadataKind,
};
pub use event::{MarkdownEvent, MarkdownEventKind, MarkdownTag, MarkdownTagEnd};
pub use html::{
    HtmlDiagnostic, HtmlDiagnosticKind, HtmlDocument, HtmlFallbackReason, HtmlFragment, HtmlNodeId,
    HtmlParserKind, HtmlRenderAttribute, HtmlRenderLimits, HtmlRenderNode, HtmlRenderNodeKind,
    HtmlRenderStatus, HtmlRenderTree, HtmlSafety, escape_html, escape_raw_html,
    sanitize_html_for_export,
};
pub use inline::{Inline, InlineKind, LinkKind, LinkTarget};
pub use parser::{MarkdownDocument, MarkdownParser, parse_markdown};
pub use resource::{
    ParsedResource, RESOURCE_MARKER, ResourceKind, ResourceLocation, ResourceRecord,
    ResourceStatus, parse_resource_parts,
};
pub use serializer::{
    MarkdownSerializer, SerializationMode, serialize_canonical_markdown,
    serialize_inlines_canonical, serialize_markdown, serialize_table_canonical,
};
pub use source::{
    LineEnding, LineEndingSummary, SourceFormat, SourceFormatError, SourceMap, SourceRange,
    SourceRangeError,
};
pub use table::{Table, TableAlignment, TableCell};
pub use toc::{TableOfContents, TocEntry, slugify};
pub use visible_text::{
    Replaceability, VisibleFoldRegion, VisibleTextKind, VisibleTextProjection, VisibleTextSegment,
};
