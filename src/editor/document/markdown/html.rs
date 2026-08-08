// @author kongweiguang

//! Native-safe HTML classification for Markdown raw HTML blocks.
//!
//! The parser keeps the original source as the serialization truth and builds
//! a conservative semantic tree only for tags that can be rendered safely in
//! GPUI. Anything risky, unknown, malformed, or ambiguous becomes raw text.

use std::ops::Range;

use cssparser::color::{parse_hash_color, parse_named_color};

/// Safety classification for an HTML fragment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HtmlSafetyClass {
    /// The fragment has at least one safe semantic node.
    Semantic,
    /// The entire fragment must be shown and stored as plain raw text.
    RawTextBlock,
}

/// Broad rendering category of a parsed HTML node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HtmlNodeKind {
    /// Safe inline tag or text that can be represented with text runs.
    InlineSemantic,
    /// Safe block tag that maps to a native block-like GPUI element.
    BlockSemantic,
    /// Opaque raw source that must not be interpreted as HTML.
    RawTextBlock,
}

/// One source attribute from an HTML tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HtmlAttr {
    /// Lowercase attribute name used for safety checks.
    pub(crate) name: String,
    /// Parsed attribute value without surrounding quotes.
    pub(crate) value: Option<String>,
    /// Exact attribute source text.
    pub(crate) raw_source: String,
}

/// Parsed CSS color value from a safe inline `style` attribute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HtmlCssColor {
    /// The CSS `currentColor` keyword.
    CurrentColor,
    /// An sRGB color with alpha.
    Rgba(HtmlCssRgba),
}

/// RGBA channels normalized enough for both GPUI rendering and export CSS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HtmlCssRgba {
    pub(crate) red: u8,
    pub(crate) green: u8,
    pub(crate) blue: u8,
    pub(crate) alpha: f32,
}

/// Parsed CSS font-size value from a safe inline `style` attribute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HtmlCssFontSize {
    Px(f32),
    Em(f32),
    Rem(f32),
    Percent(f32),
    Keyword(HtmlCssFontSizeKeyword),
}

/// CSS absolute and relative font-size keywords supported by rendered HTML.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HtmlCssFontSizeKeyword {
    XxSmall,
    XSmall,
    Small,
    Medium,
    Large,
    XLarge,
    XxLarge,
    Smaller,
    Larger,
}

/// Whitelisted visual CSS parsed from a safe HTML `style` attribute.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HtmlInlineStyle {
    pub(crate) color: Option<HtmlCssColor>,
    pub(crate) background_color: Option<HtmlCssColor>,
    pub(crate) font_size: Option<HtmlCssFontSize>,
}

impl Eq for HtmlInlineStyle {}

/// Safe data extracted from a standalone HTML `<img>` block.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HtmlImageBlock {
    pub(crate) src: String,
    pub(crate) alt: String,
    pub(crate) title: Option<String>,
    pub(crate) zoom: f32,
    pub(crate) width_percent: Option<u8>,
}

impl HtmlImageBlock {
    pub(crate) fn zoom_factor(&self) -> f32 {
        self.zoom.clamp(0.1, 3.0)
    }
}

/// A classified HTML node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HtmlNode {
    /// Domain node identity; inline compatibility nodes have no identity.
    pub(crate) id: Option<gmark_markdown::HtmlNodeId>,
    /// Rendering category selected by the safety policy.
    pub(crate) kind: HtmlNodeKind,
    /// Lowercase tag name, or `#text` for text nodes.
    pub(crate) tag_name: String,
    /// Safe attributes retained as semantic data.
    pub(crate) attrs: Vec<HtmlAttr>,
    /// Classified child nodes. Empty for raw text nodes.
    pub(crate) children: Vec<HtmlNode>,
    /// Exact source text covered by this node.
    pub(crate) raw_source: String,
    /// Byte range in the original HTML fragment.
    pub(crate) source_range: Range<usize>,
}

/// Classified HTML fragment plus its preserved source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HtmlDocument {
    /// Rendering-neutral HTML value used for Markdown parsing, source
    /// preservation, and export-safe classification. The remaining fields are
    /// the editor's narrow GPUI rendering projection.
    pub(crate) domain: gmark_markdown::HtmlDocument,
    /// Exact source string used for serialization and raw editing.
    pub(crate) raw_source: String,
    /// Root-level classified nodes.
    pub(crate) nodes: Vec<HtmlNode>,
    /// Overall fragment safety.
    pub(crate) safety: HtmlSafetyClass,
    /// Diagnostics from the shared domain sanitizer.  The renderer uses this
    /// to show a compact warning without exposing dangerous source as markup.
    pub(crate) diagnostics: Vec<gmark_markdown::HtmlDiagnostic>,
}

impl HtmlDocument {
    fn raw_with_markdown_value(domain: gmark_markdown::HtmlDocument) -> Self {
        let raw_source = domain.raw_source.clone();
        let diagnostics = domain.diagnostics().to_vec();
        Self {
            domain,
            nodes: vec![raw_node(&raw_source, 0..raw_source.len())],
            safety: HtmlSafetyClass::RawTextBlock,
            raw_source,
            diagnostics,
        }
    }

    pub(crate) fn is_semantic(&self) -> bool {
        self.safety == HtmlSafetyClass::Semantic
    }

    /// Returns the shared pure HTML value without exposing editor rendering
    /// nodes to the domain crate.
    #[cfg(test)]
    pub(crate) fn markdown_value(&self) -> &gmark_markdown::HtmlDocument {
        &self.domain
    }
}

impl HtmlCssColor {
    pub(crate) fn to_css(self) -> String {
        match self {
            Self::CurrentColor => "currentColor".to_string(),
            Self::Rgba(color) => format!(
                "rgba({},{},{},{:.3})",
                color.red,
                color.green,
                color.blue,
                color.alpha.clamp(0.0, 1.0)
            ),
        }
    }
}

impl HtmlCssFontSize {
    pub(crate) fn resolve(self, parent_px: f32, root_px: f32) -> f32 {
        let resolved = match self {
            Self::Px(value) => value,
            Self::Em(value) => parent_px * value,
            Self::Rem(value) => root_px * value,
            Self::Percent(value) => parent_px * value / 100.0,
            Self::Keyword(keyword) => match keyword {
                HtmlCssFontSizeKeyword::XxSmall => root_px * 0.6,
                HtmlCssFontSizeKeyword::XSmall => root_px * 0.75,
                HtmlCssFontSizeKeyword::Small => root_px * 0.875,
                HtmlCssFontSizeKeyword::Medium => root_px,
                HtmlCssFontSizeKeyword::Large => root_px * 1.125,
                HtmlCssFontSizeKeyword::XLarge => root_px * 1.5,
                HtmlCssFontSizeKeyword::XxLarge => root_px * 2.0,
                HtmlCssFontSizeKeyword::Smaller => parent_px * 0.833,
                HtmlCssFontSizeKeyword::Larger => parent_px * 1.2,
            },
        };

        if resolved.is_finite() {
            resolved.clamp(8.0, 48.0)
        } else {
            parent_px
        }
    }

    pub(crate) fn to_css(self) -> String {
        match self {
            Self::Px(value) => format!("{}px", css_number(value)),
            Self::Em(value) => format!("{}em", css_number(value)),
            Self::Rem(value) => format!("{}rem", css_number(value)),
            Self::Percent(value) => format!("{}%", css_number(value)),
            Self::Keyword(keyword) => match keyword {
                HtmlCssFontSizeKeyword::XxSmall => "xx-small",
                HtmlCssFontSizeKeyword::XSmall => "x-small",
                HtmlCssFontSizeKeyword::Small => "small",
                HtmlCssFontSizeKeyword::Medium => "medium",
                HtmlCssFontSizeKeyword::Large => "large",
                HtmlCssFontSizeKeyword::XLarge => "x-large",
                HtmlCssFontSizeKeyword::XxLarge => "xx-large",
                HtmlCssFontSizeKeyword::Smaller => "smaller",
                HtmlCssFontSizeKeyword::Larger => "larger",
            }
            .to_string(),
        }
    }
}

impl HtmlInlineStyle {
    pub(crate) fn is_empty(&self) -> bool {
        self.color.is_none() && self.background_color.is_none() && self.font_size.is_none()
    }

    pub(crate) fn to_css(self) -> Option<String> {
        if self.is_empty() {
            return None;
        }

        let mut declarations = Vec::new();
        if let Some(color) = self.color {
            declarations.push(format!("color: {}", color.to_css()));
        }
        if let Some(color) = self.background_color {
            declarations.push(format!("background-color: {}", color.to_css()));
        }
        if let Some(font_size) = self.font_size {
            declarations.push(format!("font-size: {}", font_size.to_css()));
        }
        Some(format!("{};", declarations.join("; ")))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TagKind {
    Open,
    Close,
    CommentLike,
}

#[derive(Clone, Debug)]
pub(super) struct TagToken {
    pub(super) kind: TagKind,
    pub(super) name: String,
    pub(super) attrs: Vec<HtmlAttr>,
    pub(super) source_range: Range<usize>,
}

/// Parses and classifies a raw HTML fragment. The returned document always
/// preserves `raw_source` exactly, even when semantic parsing succeeds.
pub(crate) fn parse_html_document(raw_source: &str) -> HtmlDocument {
    let markdown_value = gmark_markdown::HtmlDocument::parse(raw_source);
    if should_keep_raw_source(raw_source) {
        return HtmlDocument::raw_with_markdown_value(markdown_value);
    }
    let diagnostics = markdown_value.diagnostics().to_vec();
    let Some(tree) = markdown_value.render_status.tree() else {
        return HtmlDocument::raw_with_markdown_value(markdown_value);
    };

    let nodes = tree
        .roots
        .iter()
        .map(|node| editor_node_from_domain(node, raw_source))
        .collect::<Vec<_>>();
    if nodes.is_empty() {
        return HtmlDocument::raw_with_markdown_value(markdown_value);
    }

    HtmlDocument {
        domain: markdown_value,
        raw_source: raw_source.to_string(),
        nodes,
        safety: HtmlSafetyClass::Semantic,
        diagnostics,
    }
}

fn should_keep_raw_source(raw_source: &str) -> bool {
    let trimmed = raw_source.trim();
    let Some(token) = parse_tag_token(trimmed, 0) else {
        return false;
    };
    if token.kind != TagKind::Open {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    let trailing = &trimmed[token.source_range.end..];
    let void = matches!(
        token.name.as_str(),
        "br" | "hr" | "img" | "input" | "link" | "meta"
    );
    let has_close = lower[token.source_range.end..].contains(&format!("</{}", token.name));

    // A single active URL/event attribute has no safe semantic node left to
    // display. Preserve the complete source so the user can edit it, while
    // allowing a safe sibling under a larger fragment to continue rendering.
    // Keep an anchor with a dangerous target as editable source when the
    // sanitizer has no safe navigation target left. Other nodes (including
    // event-bearing containers) still render their sanitized text/children;
    // html5ever is intentionally allowed to repair omitted closing tags.
    token.name == "a"
        && has_dangerous_attrs(&token.attrs)
        && token.attrs.iter().any(|attr| attr.name == "href")
        && (void && trailing.trim().is_empty() || !void && has_close)
}

fn editor_node_from_domain(node: &gmark_markdown::HtmlRenderNode, raw_source: &str) -> HtmlNode {
    let tag_name = node.tag_name.clone();
    let kind = if node.kind == gmark_markdown::HtmlRenderNodeKind::Text || is_inline_tag(&tag_name)
    {
        HtmlNodeKind::InlineSemantic
    } else {
        HtmlNodeKind::BlockSemantic
    };
    let attrs = node
        .attrs
        .iter()
        .map(|attr| HtmlAttr {
            name: attr.name.clone(),
            value: Some(attr.value.clone()),
            raw_source: format!("{}=\"{}\"", attr.name, attr.value),
        })
        .collect::<Vec<_>>();
    let raw = if node.kind == gmark_markdown::HtmlRenderNodeKind::Text {
        node.text.clone()
    } else if node.tag_name == "#text" {
        node.text.clone()
    } else {
        raw_source.to_owned()
    };
    HtmlNode {
        id: Some(node.id),
        kind,
        tag_name,
        attrs,
        children: node
            .children
            .iter()
            .map(|child| editor_node_from_domain(child, raw_source))
            .collect(),
        raw_source: raw,
        source_range: 0..raw_source.len(),
    }
}

/// Parses the safe visual subset of a semantic node's `style` attribute.
pub(crate) fn style_for_node(node: &HtmlNode) -> HtmlInlineStyle {
    if node.kind == HtmlNodeKind::RawTextBlock {
        return HtmlInlineStyle::default();
    }

    let Some(style) = attr_value(node, "style") else {
        return HtmlInlineStyle::default();
    };

    parse_inline_style(style)
}

#[path = "html_parts/parser.rs"]
mod parser;

pub(crate) use parser::{
    attr_value, has_dangerous_attrs, is_inline_tag, parse_html_attrs, parse_html_image_block,
};
use parser::{css_number, parse_inline_style, parse_tag_token, raw_node};
#[cfg(test)]
#[path = "../../../../tests/unit/components/markdown/html.rs"]
mod tests;
