// @author kongweiguang

//! Markdown-adjacent HTML and image projections used before browser rendering.

use std::path::Path;

use gmark_markdown::{escape_html, sanitize_html_for_export};
use pulldown_cmark::{Event, Options, Parser};

use crate::images::{local_image_data_uri, render_image_html, sanitize_image_style};

pub(crate) use crate::images::{
    data_uri_for_bytes, rewrite_local_image_event, rewrite_scaled_standalone_images,
};

pub(crate) fn opening_fence(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let length = trimmed
        .chars()
        .take_while(|character| *character == marker)
        .count();
    (length >= 3).then_some((marker, length))
}

pub(crate) fn is_closing_fence(line: &str, marker: char, opening_length: usize) -> bool {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return false;
    }
    let length = trimmed
        .chars()
        .take_while(|character| *character == marker)
        .count();
    length >= opening_length && trimmed[marker.len_utf8() * length..].trim().is_empty()
}

pub(crate) fn is_root_comment_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("<!--") && line.len() - trimmed.len() <= 3
}

/// Sanitizes raw HTML events before later passes add Gmark-owned markup.
pub(crate) fn rewrite_unsafe_html_blocks(
    markdown: &str,
    base_dir: Option<&Path>,
    options: Options,
) -> String {
    let markdown = rewrite_root_html_blocks(markdown, base_dir);
    let mut replacements = Vec::new();
    for (event, range) in Parser::new_ext(&markdown, options).into_offset_iter() {
        let sanitized = sanitize_html_event(event, base_dir);
        if let Event::Html(value) | Event::InlineHtml(value) = sanitized {
            replacements.push((range, value.into_string()));
        }
    }
    if replacements.is_empty() {
        return markdown;
    }

    let mut rewritten = String::with_capacity(markdown.len());
    let mut copied_until = 0;
    for (range, replacement) in replacements {
        if range.start < copied_until {
            continue;
        }
        rewritten.push_str(&markdown[copied_until..range.start]);
        rewritten.push_str(&replacement);
        copied_until = range.end;
    }
    rewritten.push_str(&markdown[copied_until..]);
    rewritten
}

fn rewrite_root_html_blocks(markdown: &str, base_dir: Option<&Path>) -> String {
    let lines = markdown.split('\n').collect::<Vec<_>>();
    let mut rewritten = Vec::with_capacity(lines.len());
    let mut index = 0;
    let mut active_fence = None;
    while index < lines.len() {
        let line = lines[index];
        if let Some((marker, length)) = active_fence {
            rewritten.push(line.to_owned());
            if is_closing_fence(line, marker, length) {
                active_fence = None;
            }
            index += 1;
            continue;
        }
        if let Some(fence) = opening_fence(line) {
            active_fence = Some(fence);
            rewritten.push(line.to_owned());
            index += 1;
            continue;
        }
        let Some(start) = root_html_start(line) else {
            rewritten.push(line.to_owned());
            index += 1;
            continue;
        };
        let end = collect_html_region(&lines, index, &start);
        let raw = lines[index..end].join("\n");
        rewritten.push(sanitize_html_block(&raw, base_dir));
        index = end;
    }
    rewritten.join("\n")
}

/// Replaces unsafe raw HTML emitted by either pulldown-cmark HTML event variant.
pub(crate) fn sanitize_html_event<'a>(event: Event<'a>, base_dir: Option<&Path>) -> Event<'a> {
    match event {
        Event::Html(raw) if is_comment_fragment(&raw) => Event::Html(raw),
        Event::InlineHtml(raw) if is_comment_fragment(&raw) => Event::InlineHtml(raw),
        Event::Html(raw) => Event::Html(sanitize_html_block(&raw, base_dir).into()),
        Event::InlineHtml(raw) => Event::InlineHtml(sanitize_html_inline(&raw, base_dir).into()),
        event => event,
    }
}

fn is_comment_fragment(raw: &str) -> bool {
    let trimmed = raw.trim_start();
    trimmed.starts_with("<!--") || trimmed.starts_with("-->")
}

#[derive(Clone, Debug)]
struct HtmlTag {
    name: String,
    closing: bool,
    attributes: Vec<(String, Option<String>)>,
}

#[derive(Clone, Debug)]
struct HtmlStart {
    name: String,
    self_closing: bool,
    closes_same_line: bool,
}

fn root_html_start(line: &str) -> Option<HtmlStart> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 || trimmed.starts_with("<!--") {
        return None;
    }
    let end = html_tag_end(trimmed, 0)?;
    let token = &trimmed[..end];
    let tag = parse_html_tag(token)?;
    (!tag.closing).then_some(HtmlStart {
        self_closing: token.trim_end().ends_with("/>") || is_void_tag(&tag.name),
        closes_same_line: trimmed
            .to_ascii_lowercase()
            .contains(&format!("</{}>", tag.name)),
        name: tag.name,
    })
}

fn collect_html_region(lines: &[&str], start: usize, html: &HtmlStart) -> usize {
    if html.self_closing || html.closes_same_line {
        return start + 1;
    }
    let close = format!("</{}>", html.name);
    let mut index = start + 1;
    while index < lines.len() {
        if lines[index].to_ascii_lowercase().contains(&close) {
            return index + 1;
        }
        if lines[index].trim().is_empty() {
            return index;
        }
        index += 1;
    }
    lines.len()
}

fn sanitize_html_block(raw: &str, base_dir: Option<&Path>) -> String {
    sanitize_html(raw, base_dir)
}

fn sanitize_html_inline(raw: &str, base_dir: Option<&Path>) -> String {
    let trimmed = raw.trim();
    if let Some(end) = html_tag_end(trimmed, 0)
        && let Some(tag) = parse_html_tag(&trimmed[..end])
        && tag.closing
    {
        // pulldown-cmark exposes inline closing tags as standalone events.
        // The shared fragment sanitizer quite correctly drops unmatched close
        // tags, so normalize this inert boundary after the opening tag has
        // already been sanitized by the shared policy.
        return format!("</{}>", tag.name);
    }

    let sanitized = sanitize_html(raw, base_dir);
    let Some(end) = html_tag_end(trimmed, 0) else {
        return sanitized;
    };
    let Some(tag) = parse_html_tag(&trimmed[..end]) else {
        return sanitized;
    };
    if tag.closing || is_void_tag(&tag.name) || trimmed[end..].contains("</") {
        return sanitized;
    }
    let synthetic_close = format!("</{}>", tag.name);
    if let Some(stripped) = sanitized.strip_suffix(&synthetic_close) {
        stripped.to_owned()
    } else {
        sanitized
    }
}

/// Keeps comment blocks available to the export comment projection, then lets
/// the shared Markdown HTML policy own all security decisions. Image source
/// rewriting is a separate export concern because it reads local files and
/// turns them into data URIs.
fn sanitize_html(raw: &str, base_dir: Option<&Path>) -> String {
    if is_comment_fragment(raw) {
        return raw.to_owned();
    }
    let mut local_images = Vec::new();
    let rewritten = rewrite_local_image_placeholders(raw, base_dir, &mut local_images);
    let sanitized = sanitize_html_for_export(&rewritten);
    if sanitized.starts_with("<pre class=\"gmark-raw-html\">") {
        return sanitized.replacen("class=\"gmark-raw-html\"", "class=\"vlt-raw-html\"", 1);
    }

    // Native rendering drops blocked nodes from its tree. Export keeps the
    // same security policy but exposes the blocked source as escaped text so
    // a document reader can see what was rejected without executing it.
    let escaped_blocked = escape_blocked_html_nodes(&rewritten);
    let sanitized = if escaped_blocked == rewritten {
        sanitized
    } else {
        sanitize_html_for_export(&escaped_blocked)
    };
    let sanitized = rewrite_export_image_tags(&sanitized);
    restore_local_image_sources(&sanitized, &local_images)
}

fn escape_blocked_html_nodes(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut index = 0;
    while let Some(relative_start) = html[index..].find('<') {
        let start = index + relative_start;
        output.push_str(&html[index..start]);
        let Some(end) = html_tag_end(html, start) else {
            output.push_str(&html[start..]);
            return output;
        };
        let token = &html[start..end];
        let Some(tag) = parse_html_tag(token) else {
            output.push_str(token);
            index = end;
            continue;
        };
        if tag.closing || !is_blocked_tag(&tag.name) {
            output.push_str(token);
            index = end;
            continue;
        }

        let closing = format!("</{}", tag.name);
        let tail = html[end..].to_ascii_lowercase();
        let blocked_end = tail
            .find(&closing)
            .and_then(|offset| html_tag_end(html, end + offset))
            .unwrap_or(end);
        output.push_str(&escape_html(&html[start..blocked_end]));
        index = blocked_end;
    }
    output.push_str(&html[index..]);
    output
}

fn is_blocked_tag(name: &str) -> bool {
    matches!(
        name,
        "audio"
            | "base"
            | "embed"
            | "form"
            | "iframe"
            | "math"
            | "meta"
            | "object"
            | "script"
            | "style"
            | "svg"
            | "video"
    )
}

fn rewrite_export_image_tags(html: &str) -> String {
    rewrite_html_tags(html, |tag| {
        if tag.closing || tag.name != "img" {
            return None;
        }
        let value = |name: &str| {
            tag.attributes
                .iter()
                .find(|(attribute, _)| attribute == name)
                .and_then(|(_, value)| value.as_deref())
        };
        let style = value("style").unwrap_or_default();
        if !style
            .split(';')
            .filter_map(|declaration| declaration.split_once(':'))
            .any(|(property, _)| {
                matches!(
                    property.trim().to_ascii_lowercase().as_str(),
                    "zoom" | "width"
                )
            })
        {
            return None;
        }
        let source = value("src")?;
        let (zoom, width) = sanitize_image_style(style);
        Some(render_image_html(
            source,
            value("alt").unwrap_or_default(),
            value("title"),
            zoom,
            width,
        ))
    })
}

fn rewrite_html_tags<F>(html: &str, mut replacement_for: F) -> String
where
    F: FnMut(&HtmlTag) -> Option<String>,
{
    let mut output = String::with_capacity(html.len());
    let mut index = 0;
    while let Some(relative_start) = html[index..].find('<') {
        let start = index + relative_start;
        output.push_str(&html[index..start]);
        let Some(end) = html_tag_end(html, start) else {
            output.push_str(&html[start..]);
            return output;
        };
        let token = &html[start..end];
        if let Some(tag) = parse_html_tag(token)
            && let Some(replacement) = replacement_for(&tag)
        {
            output.push_str(&replacement);
        } else {
            output.push_str(token);
        }
        index = end;
    }
    output.push_str(&html[index..]);
    output
}

fn rewrite_local_image_placeholders(
    html: &str,
    base_dir: Option<&Path>,
    local_images: &mut Vec<LocalImageReplacement>,
) -> String {
    rewrite_image_sources(html, |tag| {
        let source = tag
            .attributes
            .iter()
            .find(|(name, _)| name == "src")
            .and_then(|(_, value)| value.as_deref())?;
        let data_uri = source
            .starts_with("data:image/")
            .then(|| source.to_owned())
            .or_else(|| local_image_data_uri(source, base_dir))?;
        let placeholder = format!("https://gmark.invalid/local-image/{}", local_images.len());
        local_images.push(LocalImageReplacement {
            placeholder: placeholder.clone(),
            data_uri,
        });
        Some(placeholder)
    })
}

fn restore_local_image_sources(html: &str, local_images: &[LocalImageReplacement]) -> String {
    rewrite_image_sources(html, |tag| {
        let source = tag
            .attributes
            .iter()
            .find(|(name, _)| name == "src")
            .and_then(|(_, value)| value.as_deref())?;
        local_images
            .iter()
            .find(|image| image.placeholder == source)
            .map(|image| image.data_uri.clone())
    })
}

struct LocalImageReplacement {
    placeholder: String,
    data_uri: String,
}

fn rewrite_image_sources<F>(html: &str, mut source_for: F) -> String
where
    F: FnMut(&HtmlTag) -> Option<String>,
{
    let mut output = String::with_capacity(html.len());
    let mut index = 0;
    while let Some(relative_start) = html[index..].find('<') {
        let start = index + relative_start;
        output.push_str(&html[index..start]);
        let Some(end) = html_tag_end(html, start) else {
            output.push_str(&html[start..]);
            return output;
        };
        let token = &html[start..end];
        output.push_str(&rewrite_image_tag(token, &mut source_for));
        index = end;
    }
    output.push_str(&html[index..]);
    output
}

fn rewrite_image_tag<F>(token: &str, source_for: &mut F) -> String
where
    F: FnMut(&HtmlTag) -> Option<String>,
{
    let Some(tag) = parse_html_tag(token) else {
        return token.to_owned();
    };
    if tag.closing || tag.name != "img" {
        return token.to_owned();
    }

    let source = source_for(&tag);
    let Some(source) = source else {
        return token.to_owned();
    };

    let mut output = format!("<{}", tag.name);
    for (name, value) in &tag.attributes {
        let value = if name == "src" {
            Some(source.as_str())
        } else {
            value.as_deref()
        };
        if let Some(value) = value {
            output.push_str(&format!(" {name}=\"{}\"", escape_html(value)));
        } else {
            output.push_str(&format!(" {name}"));
        }
    }
    output.push('>');
    output
}

fn is_void_tag(name: &str) -> bool {
    matches!(name, "br" | "hr" | "img")
}

fn html_tag_end(source: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, character) in source[start..].char_indices() {
        match (quote, character) {
            (None, '\'' | '"') => quote = Some(character),
            (Some(active), character) if active == character => quote = None,
            (None, '>') => return Some(start + offset + 1),
            _ => {}
        }
    }
    None
}

fn parse_html_tag(token: &str) -> Option<HtmlTag> {
    let content = token.strip_prefix('<')?.strip_suffix('>')?.trim();
    if content.starts_with('!') || content.starts_with('?') {
        return None;
    }
    let (closing, content) = content
        .strip_prefix('/')
        .map_or((false, content), |value| (true, value.trim_start()));
    let content = content.trim_end_matches('/').trim_end();
    let name_length = content
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '-')
        .map(char::len_utf8)
        .sum::<usize>();
    if name_length == 0 {
        return None;
    }
    if let Some(next) = content.as_bytes().get(name_length)
        && !next.is_ascii_whitespace()
        && *next != b'/'
    {
        return None;
    }
    let name = content[..name_length].to_ascii_lowercase();
    let attributes = parse_attributes(&content[name_length..]);
    Some(HtmlTag {
        name,
        closing,
        attributes,
    })
}

fn parse_attributes(source: &str) -> Vec<(String, Option<String>)> {
    let bytes = source.as_bytes();
    let mut attributes = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let start = index;
        while index < bytes.len()
            && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'-' | b'_' | b':'))
        {
            index += 1;
        }
        if start == index {
            index += 1;
            continue;
        }
        let name = source[start..index].to_ascii_lowercase();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let value = if bytes.get(index) == Some(&b'=') {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            let start = index;
            if matches!(bytes.get(index), Some(b'\'' | b'"')) {
                let quote = bytes[index];
                index += 1;
                let value_start = index;
                while index < bytes.len() && bytes[index] != quote {
                    index += 1;
                }
                let value = source[value_start..index].to_owned();
                index += usize::from(index < bytes.len());
                Some(value)
            } else {
                while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                Some(source[start..index].to_owned())
            }
        } else {
            None
        };
        attributes.push((name, value));
    }
    attributes
}
