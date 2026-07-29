// @author kongweiguang

//! Pure inline Markdown values independent of rendering toolkits.

use crate::html::HtmlDocument;
use crate::source::SourceRange;

/// The syntactic form that produced a Markdown link or image target.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum LinkKind {
    /// `[label](destination "title")`.
    #[default]
    Inline,
    /// `[label][reference]`.
    Reference,
    /// An unresolved reference link.
    ReferenceUnknown,
    /// `[label][]`.
    Collapsed,
    /// An unresolved collapsed reference link.
    CollapsedUnknown,
    /// `[label]`.
    Shortcut,
    /// An unresolved shortcut reference link.
    ShortcutUnknown,
    /// `<https://example.test/>`.
    Autolink,
    /// `<name@example.test>`.
    Email,
    /// `[[target]]` or `[[target|label]]`.
    WikiLink { piped: bool },
}

/// Destination metadata shared by link and image inlines.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LinkTarget {
    /// Markdown link syntax form.
    pub kind: LinkKind,
    /// Resolved destination emitted by pulldown-cmark.
    pub destination: String,
    /// Optional title string, without Markdown delimiters.
    pub title: String,
    /// Reference identifier when the syntax has one.
    pub reference: String,
}

/// A rendering-neutral inline node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inline {
    /// The node's semantic kind.
    pub kind: InlineKind,
    /// Exact byte range in the original source.
    pub source: SourceRange,
    /// Nested values for formatting, links, and images.
    pub children: Vec<Inline>,
}

impl Inline {
    /// Creates a synthetic inline value with no original source range.
    pub fn synthetic(kind: InlineKind) -> Self {
        Self {
            kind,
            source: SourceRange::empty(0),
            children: Vec::new(),
        }
    }

    pub(crate) fn parsed(kind: InlineKind, source: SourceRange, children: Vec<Self>) -> Self {
        Self {
            kind,
            source,
            children,
        }
    }

    /// Returns visible/plain text recursively, suitable for titles and TOC labels.
    pub fn plain_text(&self) -> String {
        let child_text = || {
            self.children
                .iter()
                .map(Self::plain_text)
                .collect::<String>()
        };
        match &self.kind {
            InlineKind::Text(value)
            | InlineKind::Code(value)
            | InlineKind::InlineMath(value)
            | InlineKind::FootnoteReference(value) => value.clone(),
            InlineKind::SoftBreak | InlineKind::HardBreak => "\n".to_owned(),
            InlineKind::Html(document) => document.raw_source.clone(),
            InlineKind::TaskListMarker(_) => String::new(),
            InlineKind::Emphasis
            | InlineKind::Strong
            | InlineKind::Strikethrough
            | InlineKind::Superscript
            | InlineKind::Subscript
            | InlineKind::Link(_)
            | InlineKind::Image(_) => child_text(),
        }
    }
}

/// Semantic inline kind. Formatting is represented by `children`, never UI text runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InlineKind {
    /// Plain decoded Markdown text.
    Text(String),
    /// Inline code span content.
    Code(String),
    /// Inline math source without dollar delimiters.
    InlineMath(String),
    /// A CommonMark soft line break.
    SoftBreak,
    /// A CommonMark hard line break.
    HardBreak,
    /// Emphasis wrapper.
    Emphasis,
    /// Strong wrapper.
    Strong,
    /// GFM strikethrough wrapper.
    Strikethrough,
    /// Extension superscript wrapper.
    Superscript,
    /// Extension subscript wrapper.
    Subscript,
    /// Link wrapper.
    Link(LinkTarget),
    /// Image wrapper; children represent alternative text.
    Image(LinkTarget),
    /// Inline HTML retained as a separately sanitizable value.
    Html(HtmlDocument),
    /// A footnote reference label.
    FootnoteReference(String),
    /// A task-list checkbox marker owned by its containing list item.
    TaskListMarker(bool),
}
