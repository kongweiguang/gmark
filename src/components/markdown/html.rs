// @author kongweiguang

//! Native-safe HTML classification for Markdown raw HTML blocks.
//!
//! The parser keeps the original source as the serialization truth and builds
//! a conservative semantic tree only for tags that can be rendered safely in
//! GPUI. Anything risky, unknown, malformed, or ambiguous becomes raw text.

use std::collections::HashSet;
use std::ops::Range;

use cssparser::color::{parse_hash_color, parse_named_color};

#[cfg(feature = "html-native")]
use tree_sitter::Parser;

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

    pub(crate) fn to_sanitized_html_with_src(&self, src: &str) -> String {
        let mut html = format!("<img src=\"{}\"", escape_html_attr(src));
        if !self.alt.is_empty() {
            html.push_str(" alt=\"");
            html.push_str(&escape_html_attr(&self.alt));
            html.push('"');
        }
        if let Some(title) = self.title.as_deref().filter(|title| !title.is_empty()) {
            html.push_str(" title=\"");
            html.push_str(&escape_html_attr(title));
            html.push('"');
        }
        if (self.zoom_factor() - 1.0).abs() > f32::EPSILON || self.width_percent.is_some() {
            html.push_str(" style=\"zoom: ");
            html.push_str(&css_number(self.zoom_factor() * 100.0));
            html.push_str("%;");
            if let Some(width) = self.width_percent {
                html.push_str(" width: ");
                html.push_str(&width.clamp(10, 100).to_string());
                html.push_str("%;");
            }
            html.push('"');
        }
        html.push('>');
        html
    }
}

/// A classified HTML node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HtmlNode {
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
    /// Exact source string used for serialization and raw editing.
    pub(crate) raw_source: String,
    /// Root-level classified nodes.
    pub(crate) nodes: Vec<HtmlNode>,
    /// Overall fragment safety.
    pub(crate) safety: HtmlSafetyClass,
}

impl HtmlDocument {
    pub(crate) fn raw(raw_source: impl Into<String>) -> Self {
        let raw_source = raw_source.into();
        Self {
            nodes: vec![raw_node(&raw_source, 0..raw_source.len())],
            safety: HtmlSafetyClass::RawTextBlock,
            raw_source,
        }
    }

    pub(crate) fn is_semantic(&self) -> bool {
        self.safety == HtmlSafetyClass::Semantic
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
            resolved.clamp(6.0, 96.0)
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
enum TagKind {
    Open,
    Close,
    CommentLike,
}

#[derive(Clone, Debug)]
struct TagToken {
    kind: TagKind,
    name: String,
    attrs: Vec<HtmlAttr>,
    self_closing: bool,
    source_range: Range<usize>,
}

/// Parses and classifies a raw HTML fragment. The returned document always
/// preserves `raw_source` exactly, even when semantic parsing succeeds.
pub(crate) fn parse_html_document(raw_source: &str) -> HtmlDocument {
    if raw_source.trim().is_empty() {
        return HtmlDocument::raw(raw_source);
    }

    if tree_sitter_reports_error(raw_source) {
        return HtmlDocument::raw(raw_source);
    }

    let (nodes, index, ok) = parse_nodes(raw_source, 0, None);
    if !ok || index < raw_source.len() || nodes.is_empty() {
        return HtmlDocument::raw(raw_source);
    }

    if nodes
        .iter()
        .all(|node| matches!(node.kind, HtmlNodeKind::RawTextBlock))
    {
        return HtmlDocument::raw(raw_source);
    }

    HtmlDocument {
        raw_source: raw_source.to_string(),
        nodes,
        safety: HtmlSafetyClass::Semantic,
    }
}

/// Rewrites an HTML fragment for document export: safe semantic nodes keep
/// their HTML shape, while raw text nodes are escaped so browsers cannot
/// execute or interpret them.
pub(crate) fn sanitize_html_for_export(raw_source: &str) -> String {
    if let Some(image) = parse_html_image_block(raw_source) {
        return image.to_sanitized_html_with_src(&image.src);
    }

    let document = parse_html_document(raw_source);
    if !document.is_semantic() {
        return format!(
            "<pre class=\"vlt-raw-html\">{}</pre>",
            escape_html(raw_source)
        );
    }

    let semantic_html = document
        .nodes
        .iter()
        .map(sanitize_node_for_export)
        .collect::<String>();
    sanitize_semantic_fragment(&semantic_html)
}

/// Applies a standards-compliant HTML5 sanitizer after gmark's source-aware
/// projection has reduced CSS to the supported visual subset. Keeping this
/// adapter at the export boundary preserves byte ranges used by GPUI while
/// letting Ammonia handle entity decoding and URL-scheme edge cases.
fn sanitize_semantic_fragment(html: &str) -> String {
    let tags = [
        "a",
        "abbr",
        "b",
        "blockquote",
        "br",
        "code",
        "del",
        "details",
        "dfn",
        "div",
        "em",
        "figcaption",
        "figure",
        "hr",
        "i",
        "img",
        "ins",
        "kbd",
        "mark",
        "p",
        "pre",
        "q",
        "small",
        "span",
        "strong",
        "sub",
        "summary",
        "sup",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "time",
        "tr",
        "u",
    ]
    .into_iter()
    .collect::<HashSet<_>>();

    let mut policy = ammonia::Builder::new();
    policy
        .tags(tags)
        .add_generic_attributes(&["class", "style"])
        .link_rel(None);
    policy.clean(html).to_string()
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

fn sanitize_node_for_export(node: &HtmlNode) -> String {
    if node.kind == HtmlNodeKind::RawTextBlock {
        return format!(
            "<span class=\"vlt-raw-html\">{}</span>",
            escape_html(&node.raw_source)
        );
    }

    if node.tag_name == "#text" {
        return node.raw_source.clone();
    }

    if is_void_tag(&node.tag_name) {
        return sanitized_open_tag(node);
    }

    let Some(_open_end) = node.raw_source.find('>').map(|index| index + 1) else {
        return escape_html(&node.raw_source);
    };
    let close_start =
        find_closing_tag_start(&node.raw_source, &node.tag_name).unwrap_or(node.raw_source.len());
    let close = &node.raw_source[close_start..];
    let children = node
        .children
        .iter()
        .map(sanitize_node_for_export)
        .collect::<String>();
    format!("{}{children}{close}", sanitized_open_tag(node))
}

fn sanitized_open_tag(node: &HtmlNode) -> String {
    if node.tag_name == "img"
        && let Some(image) = parse_html_image_block(&node.raw_source)
    {
        return image.to_sanitized_html_with_src(&image.src);
    }

    let mut open = format!("<{}", node.tag_name);
    for attr in &node.attrs {
        if attr.name == "style" {
            continue;
        }
        open.push(' ');
        open.push_str(&attr.raw_source);
    }
    if let Some(style) = style_for_node(node).to_css() {
        open.push_str(" style=\"");
        open.push_str(&escape_html_attr(&style));
        open.push('"');
    }
    open.push('>');
    open
}

fn find_closing_tag_start(raw_source: &str, tag_name: &str) -> Option<usize> {
    let needle = format!("</{tag_name}");
    raw_source.to_ascii_lowercase().rfind(&needle)
}

fn escape_html_attr(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn parse_nodes(
    raw: &str,
    mut index: usize,
    closing_tag: Option<&str>,
) -> (Vec<HtmlNode>, usize, bool) {
    let mut nodes = Vec::new();
    while index < raw.len() {
        let Some(tag_start_relative) = raw[index..].find('<') else {
            if closing_tag.is_some() {
                push_text_node(raw, index..raw.len(), &mut nodes);
            } else {
                push_text_node(raw, index..raw.len(), &mut nodes);
            }
            return (nodes, raw.len(), closing_tag.is_none());
        };

        let tag_start = index + tag_start_relative;
        if tag_start > index {
            push_text_node(raw, index..tag_start, &mut nodes);
        }

        let Some(token) = parse_tag_token(raw, tag_start) else {
            push_text_node(raw, tag_start..tag_start + 1, &mut nodes);
            index = tag_start + 1;
            continue;
        };

        match token.kind {
            TagKind::Close => {
                if closing_tag == Some(token.name.as_str()) {
                    return (nodes, token.source_range.end, true);
                }
                nodes.push(raw_node(raw, token.source_range.clone()));
                index = token.source_range.end;
            }
            TagKind::CommentLike => {
                nodes.push(raw_node(raw, token.source_range.clone()));
                index = token.source_range.end;
            }
            TagKind::Open => {
                let class = classify_open_tag(&token);
                if class == HtmlSafetyClass::RawTextBlock {
                    let raw_end = raw_region_end(raw, &token).unwrap_or(token.source_range.end);
                    nodes.push(raw_node(raw, token.source_range.start..raw_end));
                    index = raw_end;
                    continue;
                }

                if token.self_closing || is_void_tag(&token.name) {
                    nodes.push(semantic_node(raw, token, Vec::new()));
                    index = nodes
                        .last()
                        .map(|node| node.source_range.end)
                        .unwrap_or(index);
                    continue;
                }

                let (children, child_end, closed) =
                    parse_nodes(raw, token.source_range.end, Some(&token.name));
                if !closed {
                    nodes.push(raw_node(raw, token.source_range.start..raw.len()));
                    return (nodes, raw.len(), closing_tag.is_none());
                }

                let mut node = semantic_node(raw, token, children);
                node.source_range.end = child_end;
                node.raw_source = raw[node.source_range.clone()].to_string();
                nodes.push(node);
                index = child_end;
            }
        }
    }

    (nodes, index, closing_tag.is_none())
}
#[path = "html_parts/parser.rs"]
mod parser;

pub(crate) use parser::{
    attr_value, has_dangerous_attrs, is_inline_tag, parse_html_attrs, parse_html_image_block,
};
use parser::{
    classify_open_tag, css_number, is_void_tag, parse_inline_style, parse_tag_token,
    push_text_node, raw_node, raw_region_end, semantic_node, tree_sitter_reports_error,
};
#[cfg(test)]
#[path = "../../../tests/unit/components/markdown/html.rs"]
mod tests;
