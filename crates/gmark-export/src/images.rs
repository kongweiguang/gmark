// @author kongweiguang

//! Local image inlining and standalone Markdown image projections.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use base64::{Engine as _, engine::general_purpose};
use gmark_markdown::escape_html;
use pulldown_cmark::{CowStr, Event, Tag};

use crate::markup::{is_closing_fence, opening_fence};

pub(crate) fn data_uri_for_bytes(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        general_purpose::STANDARD.encode(bytes)
    )
}

pub(crate) fn local_image_data_uri(source: &str, base_dir: Option<&Path>) -> Option<String> {
    if source.is_empty()
        || source.starts_with('#')
        || source.starts_with("data:")
        || source.starts_with("http://")
        || source.starts_with("https://")
    {
        return None;
    }
    let path = Path::new(source);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir?.join(path)
    };
    let mime = match resolved
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        _ => return None,
    };
    fs::read(resolved)
        .ok()
        .map(|bytes| data_uri_for_bytes(mime, &bytes))
}

pub(crate) fn rewrite_local_image_event<'a>(
    event: Event<'a>,
    base_dir: Option<&Path>,
) -> Event<'a> {
    match event {
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let dest_url = local_image_data_uri(dest_url.as_ref(), base_dir)
                .map(CowStr::from)
                .unwrap_or(dest_url);
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            })
        }
        event => event,
    }
}

pub(crate) fn rewrite_scaled_standalone_images(markdown: &str) -> String {
    let definitions = image_reference_definitions(markdown);
    let mut active_fence = None;
    let mut rewritten = Vec::with_capacity(markdown.lines().count());
    for line in markdown.split('\n') {
        if let Some((marker, length)) = active_fence {
            rewritten.push(line.to_owned());
            if is_closing_fence(line, marker, length) {
                active_fence = None;
            }
            continue;
        }
        if let Some(fence) = opening_fence(line) {
            active_fence = Some(fence);
            rewritten.push(line.to_owned());
            continue;
        }
        let indent_length = line.len() - line.trim_start().len();
        let indent = &line[..indent_length];
        let Some(image) = parse_scaled_image(line.trim()) else {
            rewritten.push(line.to_owned());
            continue;
        };
        let Some((source, title)) = image.resolve(&definitions) else {
            rewritten.push(line.to_owned());
            continue;
        };
        rewritten.push(format!(
            "{indent}{}",
            render_image_html(
                &source,
                &image.alt,
                title.as_deref(),
                1.0,
                Some(image.width)
            )
        ));
    }
    rewritten.join("\n")
}

pub(crate) fn render_image_html(
    source: &str,
    alt: &str,
    title: Option<&str>,
    zoom: f32,
    width: Option<u8>,
) -> String {
    let mut output = format!("<img src=\"{}\"", escape_html(source));
    if !alt.is_empty() {
        output.push_str(&format!(" alt=\"{}\"", escape_html(alt)));
    }
    if let Some(title) = title.filter(|value| !value.is_empty()) {
        output.push_str(&format!(" title=\"{}\"", escape_html(title)));
    }
    if (zoom - 1.0).abs() > f32::EPSILON || width.is_some() {
        output.push_str(&format!(" style=\"zoom: {}%;", css_number(zoom * 100.0)));
        if let Some(width) = width {
            output.push_str(&format!(" width: {}%;", width.clamp(10, 100)));
        }
        output.push('"');
    }
    output.push('>');
    output
}

/// Preserves the legacy Markdown image projection's bounded zoom and width
/// controls when a sanitized raw HTML image is emitted by the exporter.
pub(crate) fn sanitize_image_style(style: &str) -> (f32, Option<u8>) {
    let mut zoom = 1.0;
    let mut width = None;
    for declaration in style.split(';') {
        let Some((name, value)) = declaration.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.trim().eq_ignore_ascii_case("zoom") {
            zoom = value
                .strip_suffix('%')
                .and_then(|number| number.trim().parse::<f32>().ok())
                .map(|number| (number / 100.0).clamp(0.1, 3.0))
                .unwrap_or(zoom);
        } else if name.trim().eq_ignore_ascii_case("width") {
            width = value
                .strip_suffix('%')
                .and_then(|number| number.trim().parse::<u8>().ok())
                .filter(|number| (10..=100).contains(number));
        }
    }
    (zoom, width)
}

#[derive(Clone, Debug)]
struct ScaledImage {
    alt: String,
    target: ImageTarget,
    width: u8,
}

#[derive(Clone, Debug)]
enum ImageTarget {
    Direct(String, Option<String>),
    Reference(String),
}

impl ScaledImage {
    fn resolve(
        &self,
        definitions: &HashMap<String, (String, Option<String>)>,
    ) -> Option<(String, Option<String>)> {
        match &self.target {
            ImageTarget::Direct(source, title) => Some((source.clone(), title.clone())),
            ImageTarget::Reference(label) => definitions.get(label).cloned(),
        }
    }
}

fn parse_scaled_image(line: &str) -> Option<ScaledImage> {
    let attribute_start = line.rfind("{width=")?;
    let width = line[attribute_start + 7..]
        .strip_suffix("%}")?
        .parse::<u8>()
        .ok()?;
    if !(10..=100).contains(&width) {
        return None;
    }
    let image = line[..attribute_start].trim_end();
    if !image.starts_with("![") {
        return None;
    }
    let alt_end = image[2..].find(']')? + 2;
    let alt = image[2..alt_end].replace("\\]", "]");
    let tail = &image[alt_end + 1..];
    let target = if tail.starts_with('(') && tail.ends_with(')') {
        let (source, title) = parse_image_destination(&tail[1..tail.len() - 1])?;
        ImageTarget::Direct(source, title)
    } else if tail.starts_with('[') && tail.ends_with(']') {
        let label = tail[1..tail.len() - 1].trim();
        ImageTarget::Reference(normalize_label(if label.is_empty() { &alt } else { label }))
    } else {
        return None;
    };
    Some(ScaledImage { alt, target, width })
}

fn image_reference_definitions(markdown: &str) -> HashMap<String, (String, Option<String>)> {
    markdown
        .lines()
        .filter_map(|line| {
            let remainder = line.trim().strip_prefix('[')?;
            let close = remainder.find("]:")?;
            let label = remainder[..close].trim();
            let (source, title) = parse_image_destination(remainder[close + 2..].trim())?;
            Some((normalize_label(label), (source, title)))
        })
        .collect()
}

fn parse_image_destination(value: &str) -> Option<(String, Option<String>)> {
    let value = value.trim();
    let (source, rest) = if let Some(value) = value.strip_prefix('<') {
        let close = value.find('>')?;
        (value[..close].to_owned(), value[close + 1..].trim())
    } else {
        let split = value.find(char::is_whitespace).unwrap_or(value.len());
        (value[..split].to_owned(), value[split..].trim())
    };
    (!source.is_empty()).then_some((source, parse_title(rest)))
}

fn parse_title(value: &str) -> Option<String> {
    let value = value.trim();
    for quote in ['"', '\''] {
        if value.starts_with(quote) && value.ends_with(quote) && value.len() >= 2 {
            return Some(value[1..value.len() - 1].to_owned());
        }
    }
    None
}

fn normalize_label(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn css_number(value: f32) -> String {
    let formatted = format!("{value:.3}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}
