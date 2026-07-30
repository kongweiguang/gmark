// @author kongweiguang

use super::*;

pub(super) fn make_mermaid_display_background_transparent(svg: &str) -> anyhow::Result<String> {
    let (_, root_end) = svg_root_tag_range(svg)?;
    let Some(rect_start) = svg[root_end..].find("<rect").map(|index| root_end + index) else {
        return Ok(svg.to_string());
    };
    let Some(rect_end) = svg[rect_start..]
        .find('>')
        .map(|index| rect_start + index + 1)
    else {
        return Ok(svg.to_string());
    };
    let rect = &svg[rect_start..rect_end];
    let Some(fill_start) = rect.find("fill=\"").map(|index| index + "fill=\"".len()) else {
        return Ok(svg.to_string());
    };
    let Some(fill_len) = rect[fill_start..].find('"') else {
        return Ok(svg.to_string());
    };
    let fill_start = rect_start + fill_start;
    let fill_end = fill_start + fill_len;
    Ok(format!("{}none{}", &svg[..fill_start], &svg[fill_end..]))
}

pub(super) fn mermaid_svg_intrinsic_size(svg: &str) -> anyhow::Result<MermaidSvgSize> {
    let (start, end) = svg_root_tag_range(svg)?;
    svg_root_size(&svg[start..end])
}

pub(super) fn mermaid_svg_display_size(svg: &str) -> anyhow::Result<MermaidSvgSize> {
    let (start, end) = svg_root_tag_range(svg)?;
    let root_tag = &svg[start..end];
    let width = svg_root_attr(root_tag, "width")
        .and_then(|value| parse_svg_length(&value))
        .ok_or_else(|| anyhow!("Mermaid display SVG root did not expose a usable width"))?;
    let height = svg_root_attr(root_tag, "height")
        .and_then(|value| parse_svg_length(&value))
        .ok_or_else(|| anyhow!("Mermaid display SVG root did not expose a usable height"))?;
    Ok(MermaidSvgSize { width, height })
}

pub(super) fn svg_root_tag_range(svg: &str) -> anyhow::Result<(usize, usize)> {
    let start = svg
        .find("<svg")
        .ok_or_else(|| anyhow!("Mermaid renderer output did not contain an SVG root"))?;
    let bytes = svg.as_bytes();
    let mut quote = None;
    let mut index = start;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active_quote) = quote {
            if byte == active_quote {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if byte == b'>' {
            return Ok((start, index + 1));
        }
        index += 1;
    }
    Err(anyhow!(
        "Mermaid renderer output had an unterminated SVG root tag"
    ))
}

pub(super) fn svg_root_size(root_tag: &str) -> anyhow::Result<MermaidSvgSize> {
    if let Some(view_box) = svg_root_attr(root_tag, "viewBox")
        && let Some(size) = parse_view_box_size(&view_box)
    {
        return Ok(size);
    }

    let width = svg_root_attr(root_tag, "width")
        .and_then(|value| parse_svg_length(&value))
        .ok_or_else(|| anyhow!("Mermaid SVG root did not expose a usable width"))?;
    let height = svg_root_attr(root_tag, "height")
        .and_then(|value| parse_svg_length(&value))
        .ok_or_else(|| anyhow!("Mermaid SVG root did not expose a usable height"))?;
    Ok(MermaidSvgSize { width, height })
}

fn parse_view_box_size(view_box: &str) -> Option<MermaidSvgSize> {
    let values = view_box
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (values.len() == 4 && values[2].is_finite() && values[3].is_finite()).then_some(
        MermaidSvgSize {
            width: values[2].max(1.0),
            height: values[3].max(1.0),
        },
    )
}

fn parse_svg_length(value: &str) -> Option<f32> {
    let value = value.trim();
    let end = value
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+' | 'e' | 'E'))
        .map(|(index, ch)| index + ch.len_utf8())
        .last()?;
    let parsed = value[..end].parse::<f32>().ok()?;
    (parsed.is_finite() && parsed > 0.0).then_some(parsed)
}

fn svg_root_attr(root_tag: &str, attr_name: &str) -> Option<String> {
    svg_root_attrs(root_tag)
        .into_iter()
        .find(|attr| attr.name.eq_ignore_ascii_case(attr_name))
        .and_then(|attr| attr.value)
}

pub(super) fn rewrite_svg_root_tag(root_tag: &str, size: MermaidSvgSize) -> anyhow::Result<String> {
    let attrs = svg_root_attrs(root_tag)
        .into_iter()
        .filter(|attr| {
            !["width", "height", "style"]
                .iter()
                .any(|name| attr.name.eq_ignore_ascii_case(name))
        })
        .map(|attr| attr.raw)
        .collect::<Vec<_>>();

    let mut rewritten = String::from("<svg");
    for attr in attrs {
        rewritten.push(' ');
        rewritten.push_str(attr.trim());
    }
    rewritten.push_str(&format!(
        " width=\"{:.3}\" height=\"{:.3}\">",
        size.width, size.height
    ));
    Ok(rewritten)
}

#[derive(Debug)]
struct SvgRootAttr {
    name: String,
    value: Option<String>,
    raw: String,
}

fn svg_root_attrs(root_tag: &str) -> Vec<SvgRootAttr> {
    let Some(mut index) = root_tag.find("<svg").map(|index| index + "<svg".len()) else {
        return Vec::new();
    };
    let end = root_tag.rfind('>').unwrap_or(root_tag.len());
    let bytes = root_tag.as_bytes();
    let mut attrs = Vec::new();

    while index < end {
        while index < end && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= end || bytes[index] == b'/' {
            break;
        }

        let attr_start = index;
        while index < end
            && !bytes[index].is_ascii_whitespace()
            && bytes[index] != b'='
            && bytes[index] != b'/'
        {
            index += 1;
        }
        let name = root_tag[attr_start..index].to_string();
        if name.is_empty() {
            break;
        }

        while index < end && bytes[index].is_ascii_whitespace() {
            index += 1;
        }

        let mut value = None;
        if index < end && bytes[index] == b'=' {
            index += 1;
            while index < end && bytes[index].is_ascii_whitespace() {
                index += 1;
            }

            if index < end && (bytes[index] == b'"' || bytes[index] == b'\'') {
                let quote = bytes[index];
                index += 1;
                let value_start = index;
                while index < end && bytes[index] != quote {
                    index += 1;
                }
                value = Some(root_tag[value_start..index].to_string());
                if index < end {
                    index += 1;
                }
            } else {
                let value_start = index;
                while index < end && !bytes[index].is_ascii_whitespace() && bytes[index] != b'/' {
                    index += 1;
                }
                value = Some(root_tag[value_start..index].to_string());
            }
        }

        let raw = root_tag[attr_start..index].trim().to_string();
        attrs.push(SvgRootAttr { name, value, raw });
    }

    attrs
}
