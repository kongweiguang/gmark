// @author kongweiguang

//! Renderer-neutral visible-text projection and source mapping.
//!
//! The projection deliberately sits on top of the lossless Markdown value
//! model.  It exposes the text a reader sees while retaining enough source
//! information for safe navigation and reversible edits.  Derived text is
//! still searchable, but never accidentally written back to Markdown.

use std::ops::Range;

use crate::MarkdownDocument;
use crate::block::{Block, BlockKind, CalloutKind};
use crate::html::HtmlRenderStatus;
use crate::inline::{Inline, InlineKind};
use crate::source::SourceRange;
use crate::table::Table;

/// Semantic role of one visible-text segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisibleTextKind {
    /// Ordinary paragraph or heading text.
    Text,
    /// Inline or fenced code content.
    Code,
    /// Text displayed as a link label.
    LinkLabel,
    /// Alternative text displayed for an image.
    ImageAlt,
    /// Inline or display math source exposed to search/accessibility.
    Math,
    /// Text extracted from the shared sanitized HTML render tree.
    Html,
    /// Footnote label/body text.
    Footnote,
    /// Table-cell content.
    TableCell,
    /// Mermaid or another renderer-derived representation.
    Derived,
    /// A structural separator inserted by the renderer.
    Separator,
}

/// Whether a visible segment can safely be replaced in source Markdown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Replaceability {
    /// The visible bytes are exactly one continuous source slice.
    Direct,
    /// The text is searchable but changing it would require a structural or
    /// lossy rewrite.
    Derived,
    /// No source exists, for example for a paragraph separator.
    None,
}

/// A visible range and its optional source range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleTextSegment {
    /// Byte range in [`VisibleTextProjection::text`].
    pub visible: Range<usize>,
    /// Continuous source range when one exists.
    pub source: Option<SourceRange>,
    /// Semantic role of this segment.
    pub kind: VisibleTextKind,
    /// Whether a Replace operation may target this segment.
    pub replaceability: Replaceability,
}

/// A heading or Callout region which can be hidden without changing source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleFoldRegion {
    /// Stable source range of the structural owner.
    pub source: SourceRange,
    /// Visible range hidden when this region is collapsed.
    pub body: Range<usize>,
    /// Heading level, or `None` for a Callout.
    pub heading_level: Option<u8>,
    /// Callout kind when the region is a GFM alert.
    pub callout: Option<CalloutKind>,
}

/// Rendered/semantic text plus source and folding metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VisibleTextProjection {
    /// User-visible text in renderer order.
    pub text: String,
    /// Monotonic visible segments.
    pub segments: Vec<VisibleTextSegment>,
    /// Heading and Callout body regions.
    pub folds: Vec<VisibleFoldRegion>,
}

impl VisibleTextProjection {
    /// Builds a projection from the parsed, source-ranged document.
    pub fn from_document(document: &MarkdownDocument) -> Self {
        let mut builder = ProjectionBuilder::new(document);
        for block in &document.blocks {
            builder.block(block, None);
        }
        builder.finish()
    }

    /// Returns the source range when a visible range is one direct segment.
    pub fn source_range_for_visible(&self, visible: Range<usize>) -> Option<SourceRange> {
        if !self.is_valid_visible_range(&visible) {
            return None;
        }
        let segment = self.segments.iter().find(|segment| {
            segment.visible.start <= visible.start && visible.end <= segment.visible.end
        })?;
        if segment.replaceability != Replaceability::Direct {
            return None;
        }
        let source = segment.source?;
        let visible_len = segment.visible.end.saturating_sub(segment.visible.start);
        let source_len = source.end.saturating_sub(source.start);
        if visible_len != source_len {
            return None;
        }
        Some(SourceRange {
            start: source.start + visible.start - segment.visible.start,
            end: source.start + visible.end - segment.visible.start,
        })
    }

    /// Returns a conservative source span covering all segments intersecting
    /// a visible range.  Unlike [`Self::source_range_for_visible`], this span
    /// is for navigation only and must not be used for replacement.
    pub fn source_bounds_for_visible(&self, visible: Range<usize>) -> Option<SourceRange> {
        if !self.is_valid_visible_range(&visible) {
            return None;
        }
        let mut source: Option<SourceRange> = None;
        for segment in self.segments.iter().filter(|segment| {
            segment.visible.start < visible.end && visible.start < segment.visible.end
        }) {
            let Some(segment_source) = segment.source else {
                continue;
            };
            source = Some(match source {
                Some(current) => SourceRange {
                    start: current.start.min(segment_source.start),
                    end: current.end.max(segment_source.end),
                },
                None => segment_source,
            });
        }
        source
    }

    fn is_valid_visible_range(&self, visible: &Range<usize>) -> bool {
        visible.start <= visible.end
            && visible.end <= self.text.len()
            && self.text.is_char_boundary(visible.start)
            && self.text.is_char_boundary(visible.end)
    }

    /// Finds a segment containing a visible byte offset.
    pub fn segment_at(&self, offset: usize) -> Option<&VisibleTextSegment> {
        self.segments
            .iter()
            .find(|segment| segment.visible.contains(&offset))
    }

    /// Returns all folds which contain a visible offset, innermost first.
    pub fn folds_containing(&self, offset: usize) -> impl Iterator<Item = &VisibleFoldRegion> {
        let mut folds = self
            .folds
            .iter()
            .filter(move |fold| fold.body.contains(&offset))
            .collect::<Vec<_>>();
        folds.sort_by_key(|fold| fold.body.end.saturating_sub(fold.body.start));
        folds.into_iter()
    }
}

impl MarkdownDocument {
    /// Returns the shared semantic text projection used by rendered views.
    pub fn visible_text_projection(&self) -> VisibleTextProjection {
        VisibleTextProjection::from_document(self)
    }
}

struct ProjectionBuilder<'a> {
    document: &'a MarkdownDocument,
    text: String,
    segments: Vec<VisibleTextSegment>,
    folds: Vec<VisibleFoldRegion>,
    open_headings: Vec<(usize, u8)>,
    last_block_had_text: bool,
}

impl<'a> ProjectionBuilder<'a> {
    fn new(document: &'a MarkdownDocument) -> Self {
        Self {
            document,
            text: String::new(),
            segments: Vec::new(),
            folds: Vec::new(),
            open_headings: Vec::new(),
            last_block_had_text: false,
        }
    }

    fn finish(self) -> VisibleTextProjection {
        let mut projection = VisibleTextProjection {
            text: self.text,
            segments: self.segments,
            folds: self.folds,
        };
        for (index, _) in self.open_headings {
            if let Some(fold) = projection.folds.get_mut(index) {
                fold.body.end = projection.text.len();
            }
        }
        projection
    }

    fn block(&mut self, block: &Block, parent_kind: Option<VisibleTextKind>) {
        if let BlockKind::Heading(heading) = &block.kind {
            self.close_headings_for(heading.level);
        }
        if self.last_block_had_text && !self.text.ends_with('\n') {
            self.append_separator();
        }
        // Keep structural fold ranges anchored to bytes emitted by this
        // block; a separator belongs to the boundary before it.
        let block_start = self.text.len();

        let block_kind = match (&block.kind, parent_kind) {
            (BlockKind::CodeBlock(code), _)
                if code
                    .info
                    .as_deref()
                    .and_then(|info| info.split_whitespace().next())
                    .is_some_and(|info| {
                        info.eq_ignore_ascii_case("mermaid") || info.eq_ignore_ascii_case("mmd")
                    }) =>
            {
                VisibleTextKind::Derived
            }
            (BlockKind::CodeBlock(_), _) => VisibleTextKind::Code,
            (BlockKind::FootnoteDefinition { .. }, _) => VisibleTextKind::Footnote,
            (BlockKind::Table(_), _) => VisibleTextKind::TableCell,
            (_, Some(kind)) => kind,
            _ => VisibleTextKind::Text,
        };

        match &block.kind {
            BlockKind::ThematicBreak | BlockKind::Metadata(_) => {}
            BlockKind::DisplayMath => {
                if let Some(inline) = block.inlines.first() {
                    self.inline(inline, VisibleTextKind::Math);
                }
            }
            BlockKind::Html(document) => self.html(document, block.source),
            BlockKind::Table(table) => self.table(table),
            BlockKind::RawMarkdown => self.append(
                &block.raw_source,
                block.source,
                VisibleTextKind::Derived,
                Replaceability::Derived,
            ),
            _ => {
                for inline in &block.inlines {
                    self.inline(inline, block_kind);
                }
                for child in &block.children {
                    self.block(child, Some(block_kind));
                }
            }
        }

        let block_end = self.text.len();
        let has_text = block_end > block_start;
        if let BlockKind::Heading(heading) = &block.kind {
            if has_text {
                let fold_index = self.folds.len();
                self.folds.push(VisibleFoldRegion {
                    source: block.source,
                    body: block_end..block_end,
                    heading_level: Some(heading.level),
                    callout: None,
                });
                self.open_headings.push((fold_index, heading.level));
            }
        } else if let BlockKind::BlockQuote {
            callout: Some(callout),
        } = block.kind
            && has_text
        {
            self.folds.push(VisibleFoldRegion {
                source: block.source,
                // The Markdown alert marker is structural and is not emitted
                // into the semantic text projection. Consequently every byte
                // emitted for a callout belongs to its collapsible body.
                body: block_start..block_end,
                heading_level: None,
                callout: Some(callout),
            });
        }
        self.last_block_had_text = self.last_block_had_text || has_text;
    }

    fn close_headings_for(&mut self, level: u8) {
        let end = self.text.len();
        let mut retained = Vec::with_capacity(self.open_headings.len());
        for (index, open_level) in self.open_headings.drain(..) {
            if open_level >= level {
                if let Some(fold) = self.folds.get_mut(index) {
                    fold.body.end = end;
                }
            } else {
                retained.push((index, open_level));
            }
        }
        self.open_headings = retained;
    }

    fn table(&mut self, table: &Table) {
        let mut first = true;
        for row in std::iter::once(&table.header).chain(table.rows.iter()) {
            if !first {
                self.append_separator();
            }
            first = false;
            for (column, cell) in row.iter().enumerate() {
                if column > 0 {
                    self.append_tab_separator();
                }
                for inline in &cell.inlines {
                    self.inline(inline, VisibleTextKind::TableCell);
                }
            }
        }
    }

    fn inline(&mut self, inline: &Inline, inherited_kind: VisibleTextKind) {
        match &inline.kind {
            InlineKind::Text(value) => {
                let code_literal = matches!(
                    inherited_kind,
                    VisibleTextKind::Code | VisibleTextKind::Derived
                );
                if code_literal || !self.is_inside_blocked_html(inline.source) {
                    self.append_text_value(value, inline.source, inherited_kind);
                }
            }
            InlineKind::Code(value) => {
                let kind = if inherited_kind == VisibleTextKind::Derived {
                    VisibleTextKind::Derived
                } else {
                    VisibleTextKind::Code
                };
                self.append_code_value(value, inline.source, kind);
            }
            InlineKind::InlineMath(value) => self.append(
                value,
                inline.source,
                VisibleTextKind::Math,
                Replaceability::Derived,
            ),
            InlineKind::SoftBreak | InlineKind::HardBreak => self.append_separator(),
            InlineKind::TaskListMarker(_) => {}
            InlineKind::Html(document) => self.html(document, inline.source),
            InlineKind::Image(_) => {
                for child in &inline.children {
                    self.inline(child, VisibleTextKind::ImageAlt);
                }
            }
            InlineKind::Link(_) => {
                for child in &inline.children {
                    self.inline(child, VisibleTextKind::LinkLabel);
                }
            }
            InlineKind::Emphasis
            | InlineKind::Strong
            | InlineKind::Strikethrough
            | InlineKind::Superscript
            | InlineKind::Subscript
            | InlineKind::FootnoteReference(_) => {
                for child in &inline.children {
                    self.inline(child, inherited_kind);
                }
                if let InlineKind::FootnoteReference(label) = &inline.kind
                    && inline.children.is_empty()
                {
                    self.append(
                        label,
                        inline.source,
                        VisibleTextKind::Footnote,
                        Replaceability::Derived,
                    );
                }
            }
        }
    }

    fn html(&mut self, document: &crate::HtmlDocument, source: SourceRange) {
        // Unsafe fragments still expose their sanitized tree: blocked nodes
        // and active payloads have already been removed by the shared HTML
        // policy while safe siblings remain searchable. Raw/resource-limit
        // fallbacks intentionally stay out of the semantic projection so the
        // escaped source cannot become a replacement target.
        let text = match &document.render_status {
            HtmlRenderStatus::Ready(tree) | HtmlRenderStatus::Sanitized(tree) => {
                tree.plain_text.clone()
            }
            HtmlRenderStatus::Fallback(_) => return,
        };
        if !text.is_empty() {
            self.append(
                &text,
                source,
                VisibleTextKind::Html,
                Replaceability::Derived,
            );
        }
    }

    fn append_text_value(&mut self, value: &str, source: SourceRange, kind: VisibleTextKind) {
        let replaceability = if kind == VisibleTextKind::Derived {
            Replaceability::Derived
        } else {
            self.document
                .source_slice(source)
                .ok()
                .filter(|source_text| *source_text == value)
                .map(|_| Replaceability::Direct)
                .unwrap_or(Replaceability::Derived)
        };
        self.append(value, source, kind, replaceability);
    }

    fn append_code_value(&mut self, value: &str, source: SourceRange, kind: VisibleTextKind) {
        let replaceability = if kind == VisibleTextKind::Derived {
            Replaceability::Derived
        } else {
            self.document
                .source_slice(source)
                .ok()
                .filter(|source_text| *source_text == value)
                .map(|_| Replaceability::Direct)
                .unwrap_or(Replaceability::Derived)
        };
        self.append(value, source, kind, replaceability);
    }

    fn is_inside_blocked_html(&self, source: SourceRange) -> bool {
        let Ok(prefix) = source
            .slice(&self.document.source)
            .map(|_| &self.document.source[..source.start])
        else {
            return false;
        };
        let lower = prefix.to_ascii_lowercase();
        [
            "audio", "base", "embed", "form", "iframe", "math", "meta", "object", "script",
            "style", "svg", "video",
        ]
        .into_iter()
        .any(|tag| {
            let open = find_html_tag_start(&lower, tag, false)
                .filter(|offset| !self.is_code_source_offset(*offset));
            let close = find_html_tag_start(&lower, tag, true)
                .filter(|offset| !self.is_code_source_offset(*offset));
            open.is_some_and(|open| close.is_none_or(|close| open > close))
        })
    }

    fn is_code_source_offset(&self, offset: usize) -> bool {
        self.document.events.iter().any(|event| {
            matches!(&event.kind, crate::MarkdownEventKind::Code(_))
                && event.source.start <= offset
                && offset < event.source.end
        })
    }

    fn append(
        &mut self,
        value: &str,
        source: SourceRange,
        kind: VisibleTextKind,
        replaceability: Replaceability,
    ) {
        if value.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(value);
        self.segments.push(VisibleTextSegment {
            visible: start..self.text.len(),
            source: (!source.is_empty()).then_some(source),
            kind,
            replaceability,
        });
    }

    fn append_separator(&mut self) {
        if self.text.ends_with('\n') {
            return;
        }
        let start = self.text.len();
        self.text.push('\n');
        self.segments.push(VisibleTextSegment {
            visible: start..self.text.len(),
            source: None,
            kind: VisibleTextKind::Separator,
            replaceability: Replaceability::None,
        });
    }

    fn append_tab_separator(&mut self) {
        let start = self.text.len();
        self.text.push('\t');
        self.segments.push(VisibleTextSegment {
            visible: start..self.text.len(),
            source: None,
            kind: VisibleTextKind::Separator,
            replaceability: Replaceability::None,
        });
    }
}

fn find_html_tag_start(source: &str, tag: &str, closing: bool) -> Option<usize> {
    let marker = if closing {
        format!("</{tag}")
    } else {
        format!("<{tag}")
    };
    let mut cursor = 0;
    let mut found = None;
    while let Some(relative) = source[cursor..].find(&marker) {
        let start = cursor + relative;
        let boundary = source.as_bytes().get(start + marker.len()).copied();
        if boundary.is_none_or(|byte| byte.is_ascii_whitespace() || byte == b'/' || byte == b'>') {
            found = Some(start);
        }
        cursor = start + marker.len();
    }
    found
}
