// @author kongweiguang

//! HTML policy, sanitization, and renderer-neutral projection for Markdown.
//!
//! This module deliberately owns the only HTML safety boundary in GMark.  The
//! editor and exporters consume the resulting value; they do not parse raw
//! HTML independently or execute anything from it.

use std::sync::Arc;

use html5ever::driver::parse_fragment;
use html5ever::tendril::TendrilSink;
use html5ever::{QualName, local_name, ns};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

#[path = "html_policy.rs"]
mod policy;
use policy::{
    allowed_attribute, blocked_tag, dangerous_url_attribute, sanitize_html,
    style_has_ignored_content, supported_tag,
};

/// Which parser path classified an HTML fragment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HtmlParserKind {
    /// Compatibility marker for the former tree-sitter-backed path.
    Native,
    /// Compatibility marker for the former string-scan fallback.
    Fallback,
    /// Browser-grade HTML5 fragment parser used by the production path.
    Html5ever,
}

/// Safety classification for a preserved HTML source fragment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HtmlSafety {
    /// The source contains only allowed semantic content.
    Semantic,
    /// The source contains active content, a dangerous URL, or an event
    /// attribute.  Safe siblings may still be rendered after sanitization.
    Unsafe,
    /// The source is not a semantic HTML fragment and should be displayed as
    /// text.
    RawText,
}

/// Upper bounds applied before a fragment is converted to a render tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HtmlRenderLimits {
    pub max_source_bytes: usize,
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_attributes_per_node: usize,
    pub max_table_cells: usize,
}

impl Default for HtmlRenderLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 256 * 1024,
            max_nodes: 4_096,
            max_depth: 64,
            max_attributes_per_node: 64,
            max_table_cells: 2_048,
        }
    }
}

/// Why a fragment could not be represented as a safe visual tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HtmlFallbackReason {
    Empty,
    NotHtml,
    ResourceLimit,
    NoRenderableContent,
}

/// A renderer-neutral diagnostic emitted while cleaning a fragment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HtmlDiagnostic {
    pub kind: HtmlDiagnosticKind,
    pub tag: Option<String>,
    pub attribute: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HtmlDiagnosticKind {
    BlockedTag,
    BlockedAttribute,
    InvalidUrl,
    UnsupportedTag,
    IgnoredStyle,
    ResourceLimit,
}

/// Stable within one parsed source fragment.  UI state such as `<details>`
/// disclosure is keyed by this id plus the source hash and block id.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct HtmlNodeId(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HtmlRenderAttribute {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HtmlRenderNodeKind {
    Element,
    Text,
    Break,
    Image,
}

/// Safe, renderer-neutral HTML tree.  It contains no GPUI, filesystem, or
/// networking values and can safely cross a background-task boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HtmlRenderNode {
    pub id: HtmlNodeId,
    pub kind: HtmlRenderNodeKind,
    pub tag_name: String,
    pub text: String,
    pub attrs: Vec<HtmlRenderAttribute>,
    pub children: Vec<HtmlRenderNode>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HtmlRenderTree {
    pub roots: Arc<[HtmlRenderNode]>,
    pub plain_text: String,
    pub diagnostics: Arc<[HtmlDiagnostic]>,
}

impl HtmlRenderTree {
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    pub fn diagnostics(&self) -> &[HtmlDiagnostic] {
        &self.diagnostics
    }
}

/// Whether a fragment was rendered directly, after sanitization, or shown as
/// an escaped source fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HtmlRenderStatus {
    Ready(Arc<HtmlRenderTree>),
    Sanitized(Arc<HtmlRenderTree>),
    Fallback(HtmlFallbackReason),
}

impl HtmlRenderStatus {
    pub fn tree(&self) -> Option<&HtmlRenderTree> {
        match self {
            Self::Ready(tree) | Self::Sanitized(tree) => Some(tree),
            Self::Fallback(_) => None,
        }
    }

    pub fn is_renderable(&self) -> bool {
        matches!(self, Self::Ready(_) | Self::Sanitized(_))
    }
}

/// A pure HTML value that preserves raw Markdown source and exposes safe
/// output for both native rendering and export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HtmlDocument {
    /// Exact raw HTML as authored in Markdown.
    pub raw_source: String,
    /// Sanitized HTML for the export adapter.
    pub sanitized_html: String,
    /// Safety class before rendering or export.
    pub safety: HtmlSafety,
    /// Parser used for the current value.
    pub parser: HtmlParserKind,
    /// Native render projection or explicit raw fallback.
    pub render_status: HtmlRenderStatus,
}

/// Compatibility name for an inline or block HTML value.
pub type HtmlFragment = HtmlDocument;

impl HtmlDocument {
    /// Retains a raw source value that must be displayed as text.
    pub fn raw(raw_source: impl Into<String>) -> Self {
        let raw_source = raw_source.into();
        Self {
            sanitized_html: escape_html(&raw_source),
            raw_source,
            safety: HtmlSafety::RawText,
            parser: HtmlParserKind::Fallback,
            render_status: HtmlRenderStatus::Fallback(HtmlFallbackReason::NotHtml),
        }
    }

    /// Parses, sanitizes, and projects one Markdown HTML fragment.
    pub fn parse(raw_source: &str) -> Self {
        Self::parse_with_limits(raw_source, HtmlRenderLimits::default())
    }

    pub fn parse_with_limits(raw_source: &str, limits: HtmlRenderLimits) -> Self {
        if raw_source.trim().is_empty() {
            return Self::fallback(raw_source, HtmlFallbackReason::Empty);
        }
        if raw_source.len() > limits.max_source_bytes {
            return Self::fallback(raw_source, HtmlFallbackReason::ResourceLimit);
        }
        if !looks_like_html(raw_source) {
            return Self::fallback(raw_source, HtmlFallbackReason::NotHtml);
        }

        let mut diagnostics = Vec::new();
        let raw_dom = parse_fragment_dom(raw_source);
        let mut state = WalkState::new(limits, &mut diagnostics);
        if inspect_dom(&raw_dom.document, 0, &mut state).is_err() {
            return Self::fallback(raw_source, HtmlFallbackReason::ResourceLimit);
        }

        let unsafe_source = diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.kind,
                HtmlDiagnosticKind::BlockedTag
                    | HtmlDiagnosticKind::BlockedAttribute
                    | HtmlDiagnosticKind::InvalidUrl
            )
        });

        let sanitized_html = sanitize_html(raw_source);
        let sanitized_dom = parse_fragment_dom(&sanitized_html);
        let mut convert_state = WalkState::new(limits, &mut diagnostics);
        let mut next_id = 0u64;
        let mut table_cells = 0usize;
        let mut roots = Vec::new();
        for child in sanitized_dom.document.children.borrow().iter() {
            roots.extend(convert_nodes(
                child,
                0,
                &mut next_id,
                &mut convert_state,
                &mut table_cells,
            ));
        }

        if convert_state.limit_hit || next_id as usize > limits.max_nodes {
            return Self::fallback(raw_source, HtmlFallbackReason::ResourceLimit);
        }

        let plain_text = plain_text_for_nodes(&roots);
        if roots.is_empty() && sanitized_html.trim().is_empty() {
            diagnostics.push(HtmlDiagnostic {
                kind: HtmlDiagnosticKind::ResourceLimit,
                tag: None,
                attribute: None,
            });
        }
        let tree = Arc::new(HtmlRenderTree {
            roots: Arc::from(roots),
            plain_text,
            diagnostics: Arc::from(diagnostics.clone()),
        });
        let render_status = if diagnostics.is_empty() {
            HtmlRenderStatus::Ready(Arc::clone(&tree))
        } else {
            HtmlRenderStatus::Sanitized(Arc::clone(&tree))
        };

        Self {
            raw_source: raw_source.to_owned(),
            sanitized_html,
            safety: if unsafe_source {
                HtmlSafety::Unsafe
            } else {
                HtmlSafety::Semantic
            },
            parser: HtmlParserKind::Html5ever,
            render_status,
        }
    }

    fn fallback(raw_source: &str, reason: HtmlFallbackReason) -> Self {
        Self {
            raw_source: raw_source.to_owned(),
            sanitized_html: escape_html(raw_source),
            safety: HtmlSafety::RawText,
            parser: HtmlParserKind::Fallback,
            render_status: HtmlRenderStatus::Fallback(reason),
        }
    }

    pub const fn is_semantic(&self) -> bool {
        matches!(self.safety, HtmlSafety::Semantic)
    }

    pub const fn is_unsafe(&self) -> bool {
        matches!(self.safety, HtmlSafety::Unsafe)
    }

    pub fn diagnostics(&self) -> &[HtmlDiagnostic] {
        self.render_status
            .tree()
            .map(HtmlRenderTree::diagnostics)
            .unwrap_or_default()
    }

    /// Produces safe HTML or an escaped raw-text container for export.
    pub fn sanitized_for_export(&self) -> String {
        match &self.render_status {
            HtmlRenderStatus::Fallback(_) => escape_raw_html(&self.raw_source),
            HtmlRenderStatus::Ready(tree) | HtmlRenderStatus::Sanitized(tree)
                if tree.is_empty() =>
            {
                escape_raw_html(&self.raw_source)
            }
            HtmlRenderStatus::Ready(_) | HtmlRenderStatus::Sanitized(_) => {
                self.sanitized_html.clone()
            }
        }
    }
}

/// Parses and sanitizes HTML for an export boundary.
pub fn sanitize_html_for_export(raw_source: &str) -> String {
    HtmlDocument::parse(raw_source).sanitized_for_export()
}

/// Escapes text for an HTML text node without evaluating it.
pub fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn looks_like_html(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with('<') && trimmed.contains('>')
}

struct ParsedDom {
    document: Handle,
}

fn parse_fragment_dom(source: &str) -> ParsedDom {
    let context = QualName::new(None, ns!(html), local_name!(div));
    let dom = parse_fragment(
        RcDom::default(),
        Default::default(),
        context,
        Vec::new(),
        false,
    )
    .one(source);
    ParsedDom {
        document: dom.document,
    }
}

struct WalkState<'a> {
    limits: HtmlRenderLimits,
    diagnostics: &'a mut Vec<HtmlDiagnostic>,
    limit_hit: bool,
    nodes: usize,
}

impl<'a> WalkState<'a> {
    fn new(limits: HtmlRenderLimits, diagnostics: &'a mut Vec<HtmlDiagnostic>) -> Self {
        Self {
            limits,
            diagnostics,
            limit_hit: false,
            nodes: 0,
        }
    }

    fn enter(&mut self, depth: usize) -> bool {
        if depth > self.limits.max_depth || self.nodes >= self.limits.max_nodes {
            self.limit_hit = true;
            self.diagnostics.push(HtmlDiagnostic {
                kind: HtmlDiagnosticKind::ResourceLimit,
                tag: None,
                attribute: None,
            });
            return false;
        }
        self.nodes += 1;
        true
    }
}

fn inspect_dom(handle: &Handle, depth: usize, state: &mut WalkState<'_>) -> Result<(), ()> {
    if !state.enter(depth) {
        return Err(());
    }
    match &handle.data {
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.to_string().to_ascii_lowercase();
            if blocked_tag(&tag) {
                state.diagnostics.push(HtmlDiagnostic {
                    kind: HtmlDiagnosticKind::BlockedTag,
                    tag: Some(tag.clone()),
                    attribute: None,
                });
            } else if !supported_tag(&tag) {
                state.diagnostics.push(HtmlDiagnostic {
                    kind: HtmlDiagnosticKind::UnsupportedTag,
                    tag: Some(tag.clone()),
                    attribute: None,
                });
            }
            let attrs = attrs.borrow();
            if attrs.len() > state.limits.max_attributes_per_node {
                state.limit_hit = true;
                return Err(());
            }
            for attr in attrs.iter() {
                let name = attr.name.local.to_string().to_ascii_lowercase();
                let value = attr.value.to_string();
                if name.starts_with("on") {
                    state.diagnostics.push(HtmlDiagnostic {
                        kind: HtmlDiagnosticKind::BlockedAttribute,
                        tag: Some(tag.clone()),
                        attribute: Some(name),
                    });
                } else if dangerous_url_attribute(&name, &value) {
                    state.diagnostics.push(HtmlDiagnostic {
                        kind: HtmlDiagnosticKind::InvalidUrl,
                        tag: Some(tag.clone()),
                        attribute: Some(name),
                    });
                } else if name == "style" && style_has_ignored_content(&value) {
                    state.diagnostics.push(HtmlDiagnostic {
                        kind: HtmlDiagnosticKind::IgnoredStyle,
                        tag: Some(tag.clone()),
                        attribute: Some(name),
                    });
                }
            }
            for child in handle.children.borrow().iter() {
                inspect_dom(child, depth + 1, state)?;
            }
        }
        _ => {
            for child in handle.children.borrow().iter() {
                inspect_dom(child, depth + 1, state)?;
            }
        }
    }
    Ok(())
}

fn convert_nodes(
    handle: &Handle,
    depth: usize,
    next_id: &mut u64,
    state: &mut WalkState<'_>,
    table_cells: &mut usize,
) -> Vec<HtmlRenderNode> {
    if !state.enter(depth) {
        return Vec::new();
    }
    match &handle.data {
        NodeData::Text { contents } => vec![HtmlRenderNode {
            id: next_node_id(next_id),
            kind: HtmlRenderNodeKind::Text,
            tag_name: "#text".to_owned(),
            text: contents.borrow().to_string(),
            attrs: Vec::new(),
            children: Vec::new(),
        }],
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.to_string().to_ascii_lowercase();
            let mut children = Vec::new();
            // The limit is per table, not per fragment. A nested table gets a
            // fresh counter while its containing cell remains counted in the
            // outer table's budget.
            let mut nested_table_cells = 0usize;
            if tag == "table" {
                for child in handle.children.borrow().iter() {
                    children.extend(convert_nodes(
                        child,
                        depth + 1,
                        next_id,
                        state,
                        &mut nested_table_cells,
                    ));
                }
            } else {
                for child in handle.children.borrow().iter() {
                    children.extend(convert_nodes(child, depth + 1, next_id, state, table_cells));
                }
            }
            if !supported_tag(&tag) {
                return children;
            }
            let attrs = attrs
                .borrow()
                .iter()
                .filter_map(|attr| {
                    let name = attr.name.local.to_string().to_ascii_lowercase();
                    allowed_attribute(&tag, &name).then(|| HtmlRenderAttribute {
                        name,
                        value: attr.value.to_string(),
                    })
                })
                .collect::<Vec<_>>();
            if matches!(tag.as_str(), "td" | "th") {
                *table_cells = table_cells.saturating_add(1);
                if *table_cells > state.limits.max_table_cells {
                    state.limit_hit = true;
                    return Vec::new();
                }
            }
            let kind = match tag.as_str() {
                "br" => HtmlRenderNodeKind::Break,
                "img" => HtmlRenderNodeKind::Image,
                _ => HtmlRenderNodeKind::Element,
            };
            vec![HtmlRenderNode {
                id: next_node_id(next_id),
                kind,
                tag_name: tag,
                text: String::new(),
                attrs,
                children,
            }]
        }
        NodeData::Document
        | NodeData::Comment { .. }
        | NodeData::Doctype { .. }
        | NodeData::ProcessingInstruction { .. } => handle
            .children
            .borrow()
            .iter()
            .flat_map(|child| convert_nodes(child, depth + 1, next_id, state, table_cells))
            .collect(),
    }
}

fn next_node_id(next_id: &mut u64) -> HtmlNodeId {
    let id = HtmlNodeId(*next_id);
    *next_id = next_id.saturating_add(1);
    id
}

fn plain_text_for_nodes(nodes: &[HtmlRenderNode]) -> String {
    let mut output = String::new();
    for node in nodes {
        match node.kind {
            HtmlRenderNodeKind::Text => output.push_str(&node.text),
            HtmlRenderNodeKind::Break => output.push('\n'),
            HtmlRenderNodeKind::Image => {
                if let Some(alt) = node.attrs.iter().find(|attr| attr.name == "alt") {
                    output.push_str(&alt.value);
                }
            }
            HtmlRenderNodeKind::Element => output.push_str(&plain_text_for_nodes(&node.children)),
        }
    }
    output
}

/// Produces a conservative escaped fallback suitable for a raw source view.
pub fn escape_raw_html(raw_source: &str) -> String {
    format!(
        "<pre class=\"gmark-raw-html\">{}</pre>",
        escape_html(raw_source)
    )
}
