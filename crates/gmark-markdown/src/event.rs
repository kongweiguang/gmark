// @author kongweiguang

//! Lossless, source-ranged projection of pulldown-cmark events.

use std::ops::Range;

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, LinkType, MetadataBlockKind, Options, Parser,
    Tag, TagEnd,
};

use crate::block::{
    CalloutKind, CodeBlock, CodeFence, Heading, HeadingAttribute, List, MetadataKind,
};
use crate::inline::{LinkKind, LinkTarget};
use crate::source::SourceRange;
use crate::table::TableAlignment;

/// One pulldown-cmark event with its exact original source range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkdownEvent {
    /// Semantic event kind.
    pub kind: MarkdownEventKind,
    /// Exact byte range reported by pulldown-cmark, adjusted for a leading BOM.
    pub source: SourceRange,
}

/// Rendering-neutral event equivalent to a pulldown-cmark event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkdownEventKind {
    /// Opens a structural or inline tag.
    Start(MarkdownTag),
    /// Closes a tag.
    End(MarkdownTagEnd),
    /// Decoded text.
    Text(String),
    /// Inline code text.
    Code(String),
    /// Inline math text without dollar delimiters.
    InlineMath(String),
    /// Display math text without dollar delimiters.
    DisplayMath(String),
    /// Raw HTML in an HTML block.
    Html(String),
    /// Raw inline HTML.
    InlineHtml(String),
    /// Footnote reference label.
    FootnoteReference(String),
    /// Soft line break.
    SoftBreak,
    /// Hard line break.
    HardBreak,
    /// Thematic rule.
    Rule,
    /// GFM task checkbox state.
    TaskListMarker(bool),
}

/// Start-tag data from pulldown-cmark without borrowing source input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkdownTag {
    /// Paragraph.
    Paragraph,
    /// Heading metadata.
    Heading(Heading),
    /// Blockquote, optionally recognized as a GFM alert.
    BlockQuote { callout: Option<CalloutKind> },
    /// Code-block metadata.
    CodeBlock(CodeBlock),
    /// HTML block.
    HtmlBlock,
    /// List container.
    List(List),
    /// List item.
    Item,
    /// Footnote definition.
    FootnoteDefinition { label: String },
    /// Definition-list container.
    DefinitionList,
    /// Definition-list title.
    DefinitionListTitle,
    /// Definition-list definition.
    DefinitionListDefinition,
    /// GFM table with declared column alignments.
    Table(Vec<TableAlignment>),
    /// Table header wrapper.
    TableHead,
    /// Table row wrapper.
    TableRow,
    /// Table cell wrapper.
    TableCell,
    /// Emphasis wrapper.
    Emphasis,
    /// Strong wrapper.
    Strong,
    /// Strikethrough wrapper.
    Strikethrough,
    /// Superscript wrapper.
    Superscript,
    /// Subscript wrapper.
    Subscript,
    /// Link wrapper.
    Link(LinkTarget),
    /// Image wrapper.
    Image(LinkTarget),
    /// Metadata block.
    Metadata(MetadataKind),
}

/// End-tag data from pulldown-cmark.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownTagEnd {
    /// Paragraph close.
    Paragraph,
    /// Heading close.
    Heading,
    /// Blockquote close.
    BlockQuote,
    /// Code-block close.
    CodeBlock,
    /// HTML-block close.
    HtmlBlock,
    /// List close.
    List,
    /// List-item close.
    Item,
    /// Footnote-definition close.
    FootnoteDefinition,
    /// Definition-list close.
    DefinitionList,
    /// Definition-list title close.
    DefinitionListTitle,
    /// Definition-list definition close.
    DefinitionListDefinition,
    /// Table close.
    Table,
    /// Table-header close.
    TableHead,
    /// Table-row close.
    TableRow,
    /// Table-cell close.
    TableCell,
    /// Emphasis close.
    Emphasis,
    /// Strong close.
    Strong,
    /// Strikethrough close.
    Strikethrough,
    /// Superscript close.
    Superscript,
    /// Subscript close.
    Subscript,
    /// Link close.
    Link,
    /// Image close.
    Image,
    /// Metadata close.
    Metadata,
}

pub(crate) fn parse_events(source: &str, offset_base: usize) -> Vec<MarkdownEvent> {
    Parser::new_ext(source, Options::all())
        .into_offset_iter()
        .map(|(event, range)| MarkdownEvent {
            kind: event_kind(event),
            source: source_range(range, offset_base),
        })
        .collect()
}

fn source_range(range: Range<usize>, offset_base: usize) -> SourceRange {
    SourceRange::from_parser(range.start + offset_base, range.end + offset_base)
}

fn event_kind(event: Event<'_>) -> MarkdownEventKind {
    match event {
        Event::Start(tag) => MarkdownEventKind::Start(tag_kind(tag)),
        Event::End(tag) => MarkdownEventKind::End(tag_end_kind(tag)),
        Event::Text(value) => MarkdownEventKind::Text(value.into_string()),
        Event::Code(value) => MarkdownEventKind::Code(value.into_string()),
        Event::InlineMath(value) => MarkdownEventKind::InlineMath(value.into_string()),
        Event::DisplayMath(value) => MarkdownEventKind::DisplayMath(value.into_string()),
        Event::Html(value) => MarkdownEventKind::Html(value.into_string()),
        Event::InlineHtml(value) => MarkdownEventKind::InlineHtml(value.into_string()),
        Event::FootnoteReference(value) => {
            MarkdownEventKind::FootnoteReference(value.into_string())
        }
        Event::SoftBreak => MarkdownEventKind::SoftBreak,
        Event::HardBreak => MarkdownEventKind::HardBreak,
        Event::Rule => MarkdownEventKind::Rule,
        Event::TaskListMarker(checked) => MarkdownEventKind::TaskListMarker(checked),
    }
}

fn tag_kind(tag: Tag<'_>) -> MarkdownTag {
    match tag {
        Tag::Paragraph => MarkdownTag::Paragraph,
        Tag::Heading {
            level,
            id,
            classes,
            attrs,
        } => MarkdownTag::Heading(Heading {
            level: level as u8,
            id: id.map(|value| value.into_string()),
            classes: classes
                .into_iter()
                .map(|value| value.into_string())
                .collect(),
            attributes: attrs
                .into_iter()
                .map(|(name, value)| HeadingAttribute {
                    name: name.into_string(),
                    value: value.map(|value| value.into_string()),
                })
                .collect(),
        }),
        Tag::BlockQuote(kind) => MarkdownTag::BlockQuote {
            callout: kind.map(callout_kind),
        },
        Tag::CodeBlock(kind) => MarkdownTag::CodeBlock(code_block(kind)),
        Tag::HtmlBlock => MarkdownTag::HtmlBlock,
        Tag::List(start) => MarkdownTag::List(List { start }),
        Tag::Item => MarkdownTag::Item,
        Tag::FootnoteDefinition(label) => MarkdownTag::FootnoteDefinition {
            label: label.into_string(),
        },
        Tag::DefinitionList => MarkdownTag::DefinitionList,
        Tag::DefinitionListTitle => MarkdownTag::DefinitionListTitle,
        Tag::DefinitionListDefinition => MarkdownTag::DefinitionListDefinition,
        Tag::Table(alignments) => {
            MarkdownTag::Table(alignments.into_iter().map(table_alignment).collect())
        }
        Tag::TableHead => MarkdownTag::TableHead,
        Tag::TableRow => MarkdownTag::TableRow,
        Tag::TableCell => MarkdownTag::TableCell,
        Tag::Emphasis => MarkdownTag::Emphasis,
        Tag::Strong => MarkdownTag::Strong,
        Tag::Strikethrough => MarkdownTag::Strikethrough,
        Tag::Superscript => MarkdownTag::Superscript,
        Tag::Subscript => MarkdownTag::Subscript,
        Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        } => MarkdownTag::Link(link_target(
            link_type,
            dest_url.into_string(),
            title.into_string(),
            id.into_string(),
        )),
        Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        } => MarkdownTag::Image(link_target(
            link_type,
            dest_url.into_string(),
            title.into_string(),
            id.into_string(),
        )),
        Tag::MetadataBlock(kind) => MarkdownTag::Metadata(metadata_kind(kind)),
    }
}

fn tag_end_kind(tag: TagEnd) -> MarkdownTagEnd {
    match tag {
        TagEnd::Paragraph => MarkdownTagEnd::Paragraph,
        TagEnd::Heading(_) => MarkdownTagEnd::Heading,
        TagEnd::BlockQuote(_) => MarkdownTagEnd::BlockQuote,
        TagEnd::CodeBlock => MarkdownTagEnd::CodeBlock,
        TagEnd::HtmlBlock => MarkdownTagEnd::HtmlBlock,
        TagEnd::List(_) => MarkdownTagEnd::List,
        TagEnd::Item => MarkdownTagEnd::Item,
        TagEnd::FootnoteDefinition => MarkdownTagEnd::FootnoteDefinition,
        TagEnd::DefinitionList => MarkdownTagEnd::DefinitionList,
        TagEnd::DefinitionListTitle => MarkdownTagEnd::DefinitionListTitle,
        TagEnd::DefinitionListDefinition => MarkdownTagEnd::DefinitionListDefinition,
        TagEnd::Table => MarkdownTagEnd::Table,
        TagEnd::TableHead => MarkdownTagEnd::TableHead,
        TagEnd::TableRow => MarkdownTagEnd::TableRow,
        TagEnd::TableCell => MarkdownTagEnd::TableCell,
        TagEnd::Emphasis => MarkdownTagEnd::Emphasis,
        TagEnd::Strong => MarkdownTagEnd::Strong,
        TagEnd::Strikethrough => MarkdownTagEnd::Strikethrough,
        TagEnd::Superscript => MarkdownTagEnd::Superscript,
        TagEnd::Subscript => MarkdownTagEnd::Subscript,
        TagEnd::Link => MarkdownTagEnd::Link,
        TagEnd::Image => MarkdownTagEnd::Image,
        TagEnd::MetadataBlock(_) => MarkdownTagEnd::Metadata,
    }
}

fn code_block(kind: CodeBlockKind<'_>) -> CodeBlock {
    match kind {
        CodeBlockKind::Indented => CodeBlock {
            fence: CodeFence::Indented,
            info: None,
        },
        CodeBlockKind::Fenced(info) => CodeBlock {
            fence: CodeFence::Fenced,
            info: (!info.trim().is_empty()).then(|| info.into_string()),
        },
    }
}

fn callout_kind(kind: BlockQuoteKind) -> CalloutKind {
    match kind {
        BlockQuoteKind::Note => CalloutKind::Note,
        BlockQuoteKind::Tip => CalloutKind::Tip,
        BlockQuoteKind::Important => CalloutKind::Important,
        BlockQuoteKind::Warning => CalloutKind::Warning,
        BlockQuoteKind::Caution => CalloutKind::Caution,
    }
}

fn metadata_kind(kind: MetadataBlockKind) -> MetadataKind {
    match kind {
        MetadataBlockKind::YamlStyle => MetadataKind::Yaml,
        MetadataBlockKind::PlusesStyle => MetadataKind::Pluses,
    }
}

fn table_alignment(alignment: Alignment) -> TableAlignment {
    match alignment {
        Alignment::None => TableAlignment::Default,
        Alignment::Left => TableAlignment::Left,
        Alignment::Center => TableAlignment::Center,
        Alignment::Right => TableAlignment::Right,
    }
}

fn link_target(
    link_type: LinkType,
    destination: String,
    title: String,
    reference: String,
) -> LinkTarget {
    LinkTarget {
        kind: link_kind(link_type),
        destination,
        title,
        reference,
    }
}

fn link_kind(kind: LinkType) -> LinkKind {
    match kind {
        LinkType::Inline => LinkKind::Inline,
        LinkType::Reference => LinkKind::Reference,
        LinkType::ReferenceUnknown => LinkKind::ReferenceUnknown,
        LinkType::Collapsed => LinkKind::Collapsed,
        LinkType::CollapsedUnknown => LinkKind::CollapsedUnknown,
        LinkType::Shortcut => LinkKind::Shortcut,
        LinkType::ShortcutUnknown => LinkKind::ShortcutUnknown,
        LinkType::Autolink => LinkKind::Autolink,
        LinkType::Email => LinkKind::Email,
        LinkType::WikiLink { has_pothole } => LinkKind::WikiLink { piped: has_pothole },
    }
}
