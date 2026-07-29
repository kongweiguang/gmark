// @author kongweiguang

//! Safe, value-only HTML classification and export sanitization.

/// Which parser path classified an HTML fragment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HtmlParserKind {
    /// Tree-sitter validated the source under the optional `html-native` feature.
    Native,
    /// The dependency-free-compatible sanitizer fallback classified the source.
    Fallback,
}

/// Safety classification for a preserved HTML source fragment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HtmlSafety {
    /// The source looks like HTML and contains no immediately dangerous construct.
    Semantic,
    /// The source contains active content or a dangerous URL/attribute.
    Unsafe,
    /// The source is not a semantic HTML fragment and should be treated as text.
    RawText,
}

/// A pure HTML value that preserves raw Markdown source and exposes safe output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HtmlDocument {
    /// Exact raw HTML as authored in Markdown.
    pub raw_source: String,
    /// Sanitized HTML for an HTML-capable export adapter.
    pub sanitized_html: String,
    /// Safety class before rendering or export.
    pub safety: HtmlSafety,
    /// Classification backend used for this value.
    pub parser: HtmlParserKind,
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
        }
    }

    /// Classifies HTML without fetching resources or constructing UI elements.
    pub fn parse(raw_source: &str) -> Self {
        if raw_source.trim().is_empty() || !looks_like_html(raw_source) {
            return Self::raw(raw_source);
        }

        let parser = if native_html_is_valid(raw_source) {
            HtmlParserKind::Native
        } else {
            HtmlParserKind::Fallback
        };
        let unsafe_construct = contains_unsafe_construct(raw_source);
        let sanitized_html = ammonia::clean(raw_source);
        let safety = if unsafe_construct {
            HtmlSafety::Unsafe
        } else {
            HtmlSafety::Semantic
        };
        Self {
            raw_source: raw_source.to_owned(),
            sanitized_html,
            safety,
            parser,
        }
    }

    /// Returns whether adapters may treat this as semantic HTML before export.
    pub const fn is_semantic(&self) -> bool {
        matches!(self.safety, HtmlSafety::Semantic)
    }

    /// Returns whether the source contained an actively dangerous construct.
    pub const fn is_unsafe(&self) -> bool {
        matches!(self.safety, HtmlSafety::Unsafe)
    }

    /// Produces safe HTML or an escaped raw-text container for export.
    pub fn sanitized_for_export(&self) -> String {
        match self.safety {
            HtmlSafety::RawText => format!(
                "<pre class=\"gmark-raw-html\">{}</pre>",
                escape_html(&self.raw_source)
            ),
            HtmlSafety::Semantic | HtmlSafety::Unsafe => self.sanitized_html.clone(),
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

fn contains_unsafe_construct(raw_source: &str) -> bool {
    let lower = raw_source.to_ascii_lowercase();
    [
        "<script",
        "<iframe",
        "<object",
        "<embed",
        "<base",
        "javascript:",
        "vbscript:",
        "data:text/html",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
        || contains_event_handler_attribute(&lower)
}

fn contains_event_handler_attribute(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let starts_attribute = index == 0
            || matches!(
                bytes[index.saturating_sub(1)],
                b'<' | b'/' | b' ' | b'\t' | b'\r' | b'\n'
            );
        if starts_attribute
            && bytes.get(index) == Some(&b'o')
            && bytes.get(index + 1) == Some(&b'n')
        {
            let mut cursor = index + 2;
            while let Some(byte) = bytes.get(cursor) {
                if byte.is_ascii_alphanumeric() || *byte == b'-' || *byte == b'_' {
                    cursor += 1;
                    continue;
                }
                break;
            }
            while matches!(bytes.get(cursor), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                cursor += 1;
            }
            if bytes.get(cursor) == Some(&b'=') {
                return true;
            }
        }
        index += 1;
    }
    false
}

#[cfg(feature = "html-native")]
fn native_html_is_valid(raw_source: &str) -> bool {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .is_err()
    {
        return false;
    }
    parser
        .parse(raw_source, None)
        .is_some_and(|tree| !tree.root_node().has_error())
}

#[cfg(not(feature = "html-native"))]
fn native_html_is_valid(_: &str) -> bool {
    false
}
