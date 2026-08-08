// @author kongweiguang

use super::*;

use html5ever::serialize::{SerializeOpts, TraversalScope, serialize};
use markup5ever_rcdom::SerializableHandle;

pub(super) fn sanitize_html(source: &str) -> String {
    let mut builder = ammonia::Builder::default();
    builder.tags(allowed_tags().iter().copied().collect());
    builder.generic_attributes(
        ["class", "id", "lang", "title", "dir", "style", "open"]
            .into_iter()
            .collect(),
    );
    builder.url_schemes(["http", "https", "mailto"].into_iter().collect());
    builder.add_tag_attributes("a", ["href"]);
    builder.add_tag_attributes("img", ["src", "alt", "width", "height"]);
    builder.add_tag_attributes("ol", ["start"]);
    builder.add_tag_attributes("li", ["value"]);
    builder.add_tag_attributes("td", ["colspan", "rowspan", "align"]);
    builder.add_tag_attributes("th", ["colspan", "rowspan", "align"]);
    builder.add_tag_attributes("table", ["align"]);
    builder.filter_style_properties(
        [
            "background-color",
            "border",
            "border-color",
            "border-radius",
            "border-style",
            "border-width",
            "color",
            "display",
            "font-size",
            "font-style",
            "font-weight",
            "height",
            "line-height",
            "margin",
            "margin-bottom",
            "margin-left",
            "margin-right",
            "margin-top",
            "max-height",
            "max-width",
            "min-height",
            "min-width",
            "padding",
            "padding-bottom",
            "padding-left",
            "padding-right",
            "padding-top",
            "text-align",
            "text-decoration",
            "vertical-align",
            "white-space",
            "width",
            "zoom",
        ]
        .into_iter()
        .collect(),
    );
    let cleaned = builder.clean(source).to_string();
    normalize_sanitized_styles(&cleaned)
}

/// Ammonia owns the structural and URL safety policy.  The export contract
/// also historically emitted stable CSS values (for example `blue` as an
/// explicit RGBA value), so normalize only the declarations that survived
/// Ammonia after cleaning.  This keeps the editor, HTML export, and PDF export
/// on the same sanitized string without reintroducing an independent policy.
fn normalize_sanitized_styles(source: &str) -> String {
    let dom = parse_fragment_dom(source);
    let document = dom.document.clone();
    let children = document.children.borrow().clone();
    for child in &children {
        normalize_style_nodes(child);
    }

    let mut output = Vec::with_capacity(source.len());
    for child in children {
        let scope = match &child.data {
            NodeData::Element { name, .. }
                if matches!(name.local.as_ref(), "html" | "head" | "body") =>
            {
                TraversalScope::ChildrenOnly(None)
            }
            _ => TraversalScope::IncludeNode,
        };
        let serialized = serialize(
            &mut output,
            &SerializableHandle::from(child),
            SerializeOpts {
                traversal_scope: scope,
                ..SerializeOpts::default()
            },
        );
        if serialized.is_err() {
            return source.to_owned();
        }
    }
    String::from_utf8(output).unwrap_or_else(|_| source.to_owned())
}

fn normalize_style_nodes(handle: &Handle) {
    if let NodeData::Element { attrs, .. } = &handle.data {
        let mut attrs = attrs.borrow_mut();
        if let Some(index) = attrs
            .iter()
            .position(|attr| attr.name.local.as_ref() == "style")
        {
            let normalized = normalize_inline_style_value(attrs[index].value.as_ref());
            if let Some(value) = normalized {
                attrs[index].value = value.into();
            } else {
                attrs.remove(index);
            }
        }
    }

    let children = handle.children.borrow().clone();
    for child in children {
        normalize_style_nodes(&child);
    }
}

fn normalize_inline_style_value(style: &str) -> Option<String> {
    let mut declarations = Vec::new();
    for declaration in style.split(';') {
        let Some((property, raw_value)) = declaration.split_once(':') else {
            continue;
        };
        let property = property.trim().to_ascii_lowercase();
        let value = raw_value.trim();
        if value.is_empty() || value_has_unsafe_css(value) {
            continue;
        }

        let normalized = match property.as_str() {
            "color" | "background-color" => {
                let Some(color) = normalize_css_color(value) else {
                    continue;
                };
                color
            }
            "font-size" if safe_font_size_value(value) => value.to_owned(),
            property if allowed_style_property(property) => value.to_owned(),
            _ => continue,
        };
        declarations.push(format!("{property}: {normalized};"));
    }

    (!declarations.is_empty()).then(|| declarations.join(" "))
}

fn allowed_style_property(property: &str) -> bool {
    matches!(
        property,
        "background-color"
            | "border"
            | "border-color"
            | "border-radius"
            | "border-style"
            | "border-width"
            | "color"
            | "display"
            | "font-size"
            | "font-style"
            | "font-weight"
            | "height"
            | "line-height"
            | "margin"
            | "margin-bottom"
            | "margin-left"
            | "margin-right"
            | "margin-top"
            | "max-height"
            | "max-width"
            | "min-height"
            | "min-width"
            | "padding"
            | "padding-bottom"
            | "padding-left"
            | "padding-right"
            | "padding-top"
            | "text-align"
            | "text-decoration"
            | "vertical-align"
            | "white-space"
            | "width"
            | "zoom"
    )
}

fn value_has_unsafe_css(value: &str) -> bool {
    let normalized = value
        .chars()
        .filter(|character| !character.is_ascii_whitespace() && !character.is_control())
        .collect::<String>()
        .to_ascii_lowercase();
    normalized.contains("url(")
        || normalized.contains("expression(")
        || normalized.contains("javascript:")
        || normalized.contains("vbscript:")
}

fn normalize_css_color(value: &str) -> Option<String> {
    if value.eq_ignore_ascii_case("currentcolor") {
        return Some("currentColor".to_owned());
    }
    if value.eq_ignore_ascii_case("transparent") {
        return Some("rgba(0,0,0,0.000)".to_owned());
    }
    if let Some(hex) = value.strip_prefix('#')
        && let Ok((red, green, blue, alpha)) = cssparser::color::parse_hash_color(hex.as_bytes())
    {
        return Some(format!(
            "rgba({red},{green},{blue},{})",
            format_css_alpha(alpha)
        ));
    }
    if value
        .chars()
        .all(|character| character.is_ascii_alphabetic() || character == '-')
        && let Ok((red, green, blue)) = cssparser::color::parse_named_color(value)
    {
        return Some(format!("rgba({red},{green},{blue},1.000)"));
    }
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("rgb(") || lower.starts_with("rgba(") {
        return normalize_rgb_color(value);
    }
    (lower.starts_with("hsl(") || lower.starts_with("hsla(")).then(|| value.to_owned())
}

fn normalize_rgb_color(value: &str) -> Option<String> {
    let open = value.find('(')?;
    let close = value.rfind(')')?;
    if close <= open || !value[close + 1..].trim().is_empty() {
        return None;
    }
    let args = value[open + 1..close].replace('/', " / ");
    let mut parts = args
        .split(|character: char| character == ',' || character.is_ascii_whitespace())
        .filter(|part| !part.is_empty() && *part != "/")
        .collect::<Vec<_>>();
    if parts.len() < 3 || parts.len() > 4 {
        return None;
    }
    let alpha = match parts.pop() {
        Some(value) => parse_css_alpha(value)?,
        None => 1.0,
    };
    let blue = parse_css_channel(parts.pop()?)?;
    let green = parse_css_channel(parts.pop()?)?;
    let red = parse_css_channel(parts.pop()?)?;
    Some(format!(
        "rgba({red},{green},{blue},{})",
        format_css_alpha(alpha)
    ))
}

fn format_css_alpha(alpha: f32) -> String {
    if (alpha - 1.0).abs() < f32::EPSILON {
        "1.000".to_owned()
    } else if alpha.abs() < f32::EPSILON {
        "0.000".to_owned()
    } else {
        format!("{alpha:.8}")
    }
}

fn parse_css_channel(value: &str) -> Option<u8> {
    if let Some(percent) = value.strip_suffix('%') {
        let number = percent.trim().parse::<f32>().ok()?;
        return number
            .is_finite()
            .then(|| (number.clamp(0.0, 100.0) * 2.55).round() as u8);
    }
    let number = value.trim().parse::<f32>().ok()?;
    number
        .is_finite()
        .then(|| number.clamp(0.0, 255.0).round() as u8)
}

fn parse_css_alpha(value: &str) -> Option<f32> {
    let number = if let Some(percent) = value.strip_suffix('%') {
        percent.trim().parse::<f32>().ok()? / 100.0
    } else {
        value.trim().parse::<f32>().ok()?
    };
    number.is_finite().then(|| number.clamp(0.0, 1.0))
}

fn safe_font_size_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "xx-small"
            | "x-small"
            | "small"
            | "medium"
            | "large"
            | "x-large"
            | "xx-large"
            | "smaller"
            | "larger"
    ) {
        return true;
    }
    ["px", "em", "rem", "%"].iter().any(|suffix| {
        lower
            .strip_suffix(suffix)
            .and_then(|number| number.trim().parse::<f32>().ok())
            .is_some_and(|number| number.is_finite() && number >= 0.0)
    })
}

pub(super) fn allowed_tags() -> &'static [&'static str] {
    &[
        "a",
        "abbr",
        "article",
        "aside",
        "b",
        "blockquote",
        "br",
        "caption",
        "cite",
        "code",
        "col",
        "colgroup",
        "dd",
        "del",
        "details",
        "dfn",
        "div",
        "dl",
        "dt",
        "em",
        "figcaption",
        "figure",
        "footer",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "header",
        "hr",
        "i",
        "img",
        "ins",
        "kbd",
        "li",
        "main",
        "mark",
        "nav",
        "ol",
        "p",
        "pre",
        "q",
        "s",
        "section",
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
        "ul",
    ]
}

pub(super) fn supported_tag(tag: &str) -> bool {
    allowed_tags().contains(&tag)
}

pub(super) fn blocked_tag(tag: &str) -> bool {
    matches!(
        tag,
        "base"
            | "embed"
            | "form"
            | "iframe"
            | "meta"
            | "object"
            | "script"
            | "style"
            | "svg"
            | "math"
            | "video"
            | "audio"
    )
}

pub(super) fn allowed_attribute(tag: &str, name: &str) -> bool {
    if matches!(
        name,
        "class" | "id" | "lang" | "title" | "dir" | "style" | "open"
    ) {
        return true;
    }
    match tag {
        "a" => name == "href",
        "img" => matches!(name, "src" | "alt" | "width" | "height"),
        "ol" => name == "start",
        "li" => name == "value",
        "td" | "th" => matches!(name, "colspan" | "rowspan" | "align"),
        "table" => name == "align",
        _ => false,
    }
}

pub(super) fn dangerous_url_attribute(name: &str, value: &str) -> bool {
    if !matches!(
        name,
        "href" | "src" | "action" | "formaction" | "xlink:href"
    ) {
        return false;
    }
    let normalized = value
        .chars()
        .filter(|character| !character.is_whitespace() && !character.is_control())
        .collect::<String>()
        .to_ascii_lowercase();
    normalized.starts_with("javascript:")
        || normalized.starts_with("vbscript:")
        || normalized.starts_with("data:")
        || normalized.starts_with("file:")
        || normalized.starts_with("blob:")
}

pub(super) fn style_has_ignored_content(style: &str) -> bool {
    let allowed = [
        "background-color",
        "border",
        "border-color",
        "border-radius",
        "border-style",
        "border-width",
        "color",
        "display",
        "font-size",
        "font-style",
        "font-weight",
        "height",
        "line-height",
        "margin",
        "margin-bottom",
        "margin-left",
        "margin-right",
        "margin-top",
        "max-height",
        "max-width",
        "min-height",
        "min-width",
        "padding",
        "padding-bottom",
        "padding-left",
        "padding-right",
        "padding-top",
        "text-align",
        "text-decoration",
        "vertical-align",
        "white-space",
        "width",
        "zoom",
    ];
    style.split(';').any(|declaration| {
        let Some((name, value)) = declaration.split_once(':') else {
            return !declaration.trim().is_empty();
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim().to_ascii_lowercase();
        !allowed.contains(&name.as_str()) || value.contains("url(")
    })
}
