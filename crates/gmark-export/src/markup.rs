// @author kongweiguang

//! Markdown-adjacent HTML and image projections used before browser rendering.

use std::path::Path;

use gmark_markdown::escape_html;

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

/// Sanitizes root HTML blocks and inlines local image bytes as data URIs.
pub(crate) fn rewrite_unsafe_html_blocks(markdown: &str, base_dir: Option<&Path>) -> String {
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

#[derive(Clone, Debug)]
struct HtmlTag {
    name: String,
    closing: bool,
    self_closing: bool,
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
    let tag = parse_html_tag(trimmed)?;
    (!tag.closing).then_some(HtmlStart {
        self_closing: tag.self_closing || is_void_tag(&tag.name),
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
    let trimmed = raw.trim_start();
    if trimmed.starts_with("<script")
        || trimmed.starts_with("<iframe")
        || trimmed.starts_with("<object")
        || trimmed.starts_with("<embed")
    {
        return format!("<pre class=\"vlt-raw-html\">{}</pre>", escape_html(raw));
    }
    let mut output = String::with_capacity(raw.len());
    let mut index = 0;
    while index < raw.len() {
        let Some(relative_start) = raw[index..].find('<') else {
            output.push_str(&raw[index..]);
            break;
        };
        let start = index + relative_start;
        output.push_str(&raw[index..start]);
        let Some(end) = html_tag_end(raw, start) else {
            output.push_str(&escape_html(&raw[start..]));
            break;
        };
        let token = &raw[start..end];
        let Some(tag) = parse_html_tag(token) else {
            output.push_str(&escape_html(token));
            index = end;
            continue;
        };
        if is_dangerous_tag(&tag.name) && !tag.closing {
            let close = format!("</{}", tag.name);
            let tail = raw[end..].to_ascii_lowercase();
            let dangerous_end = tail
                .find(&close)
                .and_then(|offset| html_tag_end(raw, end + offset))
                .unwrap_or(end);
            output.push_str(&escape_html(&raw[start..dangerous_end]));
            index = dangerous_end;
            continue;
        }
        if !is_safe_tag(&tag.name) {
            output.push_str(&escape_html(token));
        } else if tag.closing {
            output.push_str(&format!("</{}>", tag.name));
        } else if tag.name == "img" {
            output.push_str(&sanitize_image_tag(&tag, base_dir));
        } else {
            output.push_str(&sanitize_open_tag(&tag));
        }
        index = end;
    }
    output
}

fn sanitize_open_tag(tag: &HtmlTag) -> String {
    let mut output = format!("<{}", tag.name);
    for (name, value) in &tag.attributes {
        if name == "style" {
            if let Some(style) = sanitize_text_style(value.as_deref().unwrap_or_default()) {
                output.push_str(&format!(" style=\"{}\"", escape_html(&style)));
            }
        } else if safe_attribute(&tag.name, name, value.as_deref()) {
            if let Some(value) = value {
                output.push_str(&format!(" {name}=\"{}\"", escape_html(value)));
            } else {
                output.push_str(&format!(" {name}"));
            }
        }
    }
    output.push('>');
    output
}

fn sanitize_image_tag(tag: &HtmlTag, base_dir: Option<&Path>) -> String {
    let value = |name: &str| {
        tag.attributes
            .iter()
            .find(|(attribute, _)| attribute == name)
            .and_then(|(_, value)| value.as_deref())
    };
    let source = value("src").unwrap_or_default();
    let source = local_image_data_uri(source, base_dir).unwrap_or_else(|| source.to_owned());
    let (zoom, width) = sanitize_image_style(value("style").unwrap_or_default());
    render_image_html(
        &source,
        value("alt").unwrap_or_default(),
        value("title"),
        zoom,
        width,
    )
}

fn sanitize_text_style(style: &str) -> Option<String> {
    let mut color = None;
    let mut background = None;
    let mut font_size = None;
    for declaration in style.split(';') {
        let Some((name, value)) = declaration.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "color" => color = css_color(value),
            "background-color" => background = css_color(value),
            "font-size" if safe_font_size(value) => font_size = Some(value.to_owned()),
            _ => {}
        }
    }
    let mut declarations = Vec::new();
    if let Some(value) = color {
        declarations.push(format!("color: {value}"));
    }
    if let Some(value) = background {
        declarations.push(format!("background-color: {value}"));
    }
    if let Some(value) = font_size {
        declarations.push(format!("font-size: {value}"));
    }
    (!declarations.is_empty()).then(|| format!("{};", declarations.join("; ")))
}

fn css_color(value: &str) -> Option<String> {
    let lower = value.trim().to_ascii_lowercase();
    let named = match lower.as_str() {
        "black" => Some((0, 0, 0)),
        "white" => Some((255, 255, 255)),
        "red" => Some((255, 0, 0)),
        "blue" => Some((0, 0, 255)),
        "green" => Some((0, 128, 0)),
        "yellow" => Some((255, 255, 0)),
        _ => None,
    };
    let rgb = named.or_else(|| hex_color(&lower))?;
    Some(format!("rgba({},{},{},1.000)", rgb.0, rgb.1, rgb.2))
}

fn hex_color(value: &str) -> Option<(u8, u8, u8)> {
    let source = value.strip_prefix('#')?;
    match source.len() {
        3 => Some((
            u8::from_str_radix(&source[0..1].repeat(2), 16).ok()?,
            u8::from_str_radix(&source[1..2].repeat(2), 16).ok()?,
            u8::from_str_radix(&source[2..3].repeat(2), 16).ok()?,
        )),
        6 => Some((
            u8::from_str_radix(&source[0..2], 16).ok()?,
            u8::from_str_radix(&source[2..4], 16).ok()?,
            u8::from_str_radix(&source[4..6], 16).ok()?,
        )),
        _ => None,
    }
}

fn safe_font_size(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    ["px", "em", "rem", "%"].iter().any(|suffix| {
        lower
            .strip_suffix(suffix)
            .is_some_and(|number| number.trim().parse::<f32>().is_ok())
    }) || matches!(
        lower.as_str(),
        "small" | "medium" | "large" | "smaller" | "larger"
    )
}

fn safe_attribute(tag: &str, name: &str, value: Option<&str>) -> bool {
    match name {
        "class" | "title" | "colspan" | "rowspan" | "align" => true,
        "alt" => tag == "img",
        "href" => tag == "a" && safe_url(value.unwrap_or_default(), false),
        "src" => tag == "img" && safe_url(value.unwrap_or_default(), true),
        "open" => tag == "details",
        _ => false,
    }
}

fn safe_url(value: &str, is_image: bool) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    !lower.starts_with("javascript:")
        && !lower.starts_with("vbscript:")
        && !lower.starts_with("data:text/html")
        && (!lower.starts_with("data:") || (is_image && lower.starts_with("data:image/")))
}

fn is_safe_tag(name: &str) -> bool {
    matches!(
        name,
        "a" | "abbr"
            | "b"
            | "blockquote"
            | "br"
            | "code"
            | "del"
            | "details"
            | "dfn"
            | "div"
            | "em"
            | "figcaption"
            | "figure"
            | "hr"
            | "i"
            | "img"
            | "ins"
            | "kbd"
            | "mark"
            | "p"
            | "pre"
            | "q"
            | "small"
            | "span"
            | "strong"
            | "sub"
            | "summary"
            | "sup"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "time"
            | "tr"
            | "u"
    )
}

fn is_dangerous_tag(name: &str) -> bool {
    matches!(
        name,
        "script" | "iframe" | "object" | "embed" | "base" | "style" | "link" | "meta"
    )
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
    let self_closing = content.trim_end().ends_with('/');
    let content = content.trim_end_matches('/').trim_end();
    let name_length = content
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '-')
        .map(char::len_utf8)
        .sum::<usize>();
    if name_length == 0 {
        return None;
    }
    let name = content[..name_length].to_ascii_lowercase();
    let attributes = parse_attributes(&content[name_length..]);
    Some(HtmlTag {
        name,
        closing,
        self_closing,
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
