// @author kongweiguang

//! Block-level Markdown values with no runtime or UI dependencies.

use crate::html::HtmlDocument;
use crate::inline::Inline;
use crate::resource::ResourceRecord;
use crate::source::SourceRange;
use crate::table::Table;

/// Supported GitHub-style alert kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalloutKind {
    /// Informational note.
    Note,
    /// Helpful tip.
    Tip,
    /// High-emphasis important alert.
    Important,
    /// Warning alert.
    Warning,
    /// Caution alert.
    Caution,
}

impl CalloutKind {
    /// Stable uppercase marker used in Markdown alert headers.
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Note => "NOTE",
            Self::Tip => "TIP",
            Self::Important => "IMPORTANT",
            Self::Warning => "WARNING",
            Self::Caution => "CAUTION",
        }
    }
}

/// One heading attribute preserved from pulldown-cmark.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeadingAttribute {
    /// Attribute name, without Markdown attribute delimiters.
    pub name: String,
    /// Optional assignment value.
    pub value: Option<String>,
}

/// Heading metadata independent of its inline title content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Heading {
    /// CommonMark level, always between one and six for parsed documents.
    pub level: u8,
    /// Explicit heading identifier when present.
    pub id: Option<String>,
    /// CSS-like classes supplied in heading attributes.
    pub classes: Vec<String>,
    /// Additional heading attributes.
    pub attributes: Vec<HeadingAttribute>,
}

impl Heading {
    /// Creates a plain heading without custom attributes.
    pub fn new(level: u8) -> Self {
        Self {
            level: level.clamp(1, 6),
            id: None,
            classes: Vec::new(),
            attributes: Vec::new(),
        }
    }
}

/// Fence form for a code block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeFence {
    /// Indented CommonMark code block.
    Indented,
    /// Backtick or tilde fenced block.
    Fenced,
}

/// Code-block metadata; content is held in the enclosing block's inlines.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeBlock {
    /// Original block form.
    pub fence: CodeFence,
    /// Optional fence info string, preserved without fence characters.
    pub info: Option<String>,
}

/// List container metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct List {
    /// First ordinal for an ordered list; `None` means unordered.
    pub start: Option<u64>,
}

/// Metadata fence dialect recognized by pulldown-cmark.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataKind {
    /// YAML-style `---` metadata fence.
    Yaml,
    /// Plus-delimited metadata fence.
    Pluses,
}

/// A pure Markdown block, including source bytes and nested structural blocks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    /// Semantic block kind.
    pub kind: BlockKind,
    /// Exact byte interval in the original document.
    pub source: SourceRange,
    /// Direct inline content, if this kind accepts it.
    pub inlines: Vec<Inline>,
    /// Direct structural children, preserving parser order.
    pub children: Vec<Block>,
    /// Original source text for this exact block range.
    pub raw_source: String,
    /// Standalone GMark resource-card metadata when this block represents one.
    pub resource: Option<ResourceRecord>,
}

impl Block {
    /// Creates a synthetic block for adapters constructing a new document.
    pub fn synthetic(kind: BlockKind, inlines: Vec<Inline>) -> Self {
        Self {
            kind,
            source: SourceRange::empty(0),
            inlines,
            children: Vec::new(),
            raw_source: String::new(),
            resource: None,
        }
    }

    /// Creates a synthetic paragraph from plain text.
    pub fn paragraph(text: impl Into<String>) -> Self {
        Self::synthetic(
            BlockKind::Paragraph,
            vec![Inline::synthetic(crate::InlineKind::Text(text.into()))],
        )
    }

    /// Returns direct text plus nested inline text without Markdown markers.
    pub fn plain_text(&self) -> String {
        self.inlines.iter().map(Inline::plain_text).collect()
    }

    /// Returns the task state for a list item, if any.
    pub fn task_state(&self) -> Option<bool> {
        match self.kind {
            BlockKind::ListItem { task } => task,
            _ => None,
        }
    }

    /// Returns heading metadata when this is a heading.
    pub fn heading(&self) -> Option<&Heading> {
        match &self.kind {
            BlockKind::Heading(heading) => Some(heading),
            _ => None,
        }
    }

    pub(crate) fn parsed(
        kind: BlockKind,
        source: SourceRange,
        inlines: Vec<Inline>,
        children: Vec<Block>,
        raw_source: String,
        resource: Option<ResourceRecord>,
    ) -> Self {
        Self {
            kind,
            source,
            inlines,
            children,
            raw_source,
            resource,
        }
    }
}

/// Semantic type of a block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlockKind {
    /// Normal paragraph.
    Paragraph,
    /// Heading with level and attribute metadata.
    Heading(Heading),
    /// Quote container; `Some` represents a GFM alert kind.
    BlockQuote { callout: Option<CalloutKind> },
    /// Code block.
    CodeBlock(CodeBlock),
    /// HTML block with a separate safety classification.
    Html(HtmlDocument),
    /// List container.
    List(List),
    /// List item, with task state when a GFM checkbox was parsed.
    ListItem { task: Option<bool> },
    /// Footnote definition container.
    FootnoteDefinition { label: String },
    /// Definition-list container.
    DefinitionList,
    /// Definition-list title.
    DefinitionListTitle,
    /// Definition-list definition.
    DefinitionListDefinition,
    /// Native value-model representation of a GFM table.
    Table(Table),
    /// YAML or plus-delimited metadata fence.
    Metadata(MetadataKind),
    /// Horizontal/thematic rule.
    ThematicBreak,
    /// Display math source without dollar delimiters.
    DisplayMath,
    /// Parsed syntax that intentionally remains opaque to this value model.
    RawMarkdown,
}
