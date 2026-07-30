// @author kongweiguang

use super::*;

pub(super) fn parse_tag_token(raw: &str, start: usize) -> Option<TagToken> {
    let rest = raw.get(start..)?;
    if !rest.starts_with('<') {
        return None;
    }

    if rest.starts_with("<!--") {
        let end = rest.find("-->").map(|offset| start + offset + 3)?;
        return Some(TagToken {
            kind: TagKind::CommentLike,
            name: "#comment".into(),
            attrs: Vec::new(),
            self_closing: true,
            source_range: start..end,
        });
    }

    if rest.starts_with("<!") || rest.starts_with("<?") {
        let end = rest.find('>').map(|offset| start + offset + 1)?;
        return Some(TagToken {
            kind: TagKind::CommentLike,
            name: "#raw".into(),
            attrs: Vec::new(),
            self_closing: true,
            source_range: start..end,
        });
    }

    let bytes = raw.as_bytes();
    let mut index = start + 1;
    let closing = bytes.get(index) == Some(&b'/');
    if closing {
        index += 1;
    }

    let name_start = index;
    while index < raw.len() {
        let ch = raw[index..].chars().next()?;
        if ch.is_ascii_alphanumeric() || ch == '-' {
            index += ch.len_utf8();
        } else {
            break;
        }
    }
    if index == name_start {
        return None;
    }

    let name = raw[name_start..index].to_ascii_lowercase();
    let attrs_start = index;
    let mut quote: Option<char> = None;
    while index < raw.len() {
        let ch = raw[index..].chars().next()?;
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            }
            index += ch.len_utf8();
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            index += ch.len_utf8();
            continue;
        }

        if ch == '>' {
            let source_range = start..index + 1;
            let attrs_source = &raw[attrs_start..index];
            let self_closing = attrs_source.trim_end().ends_with('/');
            return Some(TagToken {
                kind: if closing {
                    TagKind::Close
                } else {
                    TagKind::Open
                },
                name,
                attrs: if closing {
                    Vec::new()
                } else {
                    parse_html_attrs(attrs_source)
                },
                self_closing,
                source_range,
            });
        }

        index += ch.len_utf8();
    }

    None
}

/// Peek the next char at `index` without advancing. Returns `None` at EOF.
#[inline]
fn peek_char(source: &str, index: usize) -> Option<char> {
    source[index..].chars().next()
}

/// Advance `index` past the next char and return it. Returns `None` at EOF.
/// Encapsulates the byte-index ↔ UTF-8-boundary invariant so callers that
/// don't need the char's value can't drift into a panic by hand-incrementing
/// `index` by anything other than `ch.len_utf8()`. Loops that *do* need the
/// char for a check should peek with [`peek_char`], inspect the value, and
/// then advance with `index += ch.len_utf8()` — see [`parse_html_attrs`] —
/// so the char is read only once per iteration.
#[inline]
fn advance_char(source: &str, index: &mut usize) -> Option<char> {
    let ch = source[*index..].chars().next()?;
    *index += ch.len_utf8();
    Some(ch)
}

pub(crate) fn parse_html_attrs(source: &str) -> Vec<HtmlAttr> {
    let mut attrs = Vec::new();
    let mut index = 0usize;
    while index < source.len() {
        while let Some(ch) = peek_char(source, index).filter(|c| c.is_whitespace() || *c == '/') {
            index += ch.len_utf8();
        }
        if index >= source.len() {
            break;
        }

        let start = index;
        while let Some(ch) = peek_char(source, index) {
            if ch.is_whitespace() || ch == '=' || ch == '/' {
                break;
            }
            index += ch.len_utf8();
        }
        let name_end = index;
        if name_end == start {
            // Lone separator we couldn't classify — consume one char and retry.
            advance_char(source, &mut index);
            continue;
        }

        while let Some(ch) = peek_char(source, index).filter(|c| c.is_whitespace()) {
            index += ch.len_utf8();
        }

        let mut value = None;
        if source[index..].starts_with('=') {
            index += 1;
            while let Some(ch) = peek_char(source, index).filter(|c| c.is_whitespace()) {
                index += ch.len_utf8();
            }

            if let Some(quote) = peek_char(source, index).filter(|c| *c == '"' || *c == '\'') {
                index += quote.len_utf8();
                let value_start = index;
                while let Some(ch) = peek_char(source, index) {
                    if ch == quote {
                        break;
                    }
                    index += ch.len_utf8();
                }
                value = Some(source[value_start..index].to_string());
                if index < source.len() {
                    index += quote.len_utf8();
                }
            } else if peek_char(source, index).is_some() {
                let value_start = index;
                while let Some(ch) = peek_char(source, index) {
                    if ch.is_whitespace() || ch == '/' {
                        break;
                    }
                    index += ch.len_utf8();
                }
                value = Some(source[value_start..index].to_string());
            }
        }

        attrs.push(HtmlAttr {
            name: source[start..name_end].to_ascii_lowercase(),
            value,
            raw_source: source[start..index].to_string(),
        });
    }

    attrs
}

pub(super) fn classify_open_tag(token: &TagToken) -> HtmlSafetyClass {
    if !is_safe_tag(&token.name) || has_dangerous_attrs(&token.attrs) {
        HtmlSafetyClass::RawTextBlock
    } else {
        HtmlSafetyClass::Semantic
    }
}

pub(super) fn semantic_node(raw: &str, token: TagToken, children: Vec<HtmlNode>) -> HtmlNode {
    HtmlNode {
        kind: if is_inline_tag(&token.name) {
            HtmlNodeKind::InlineSemantic
        } else {
            HtmlNodeKind::BlockSemantic
        },
        tag_name: token.name,
        attrs: token.attrs,
        children,
        raw_source: raw[token.source_range.clone()].to_string(),
        source_range: token.source_range,
    }
}

pub(super) fn push_text_node(raw: &str, range: Range<usize>, nodes: &mut Vec<HtmlNode>) {
    if range.is_empty() {
        return;
    }
    nodes.push(HtmlNode {
        kind: HtmlNodeKind::InlineSemantic,
        tag_name: "#text".into(),
        attrs: Vec::new(),
        children: Vec::new(),
        raw_source: raw[range.clone()].to_string(),
        source_range: range,
    });
}

pub(super) fn raw_node(raw: &str, range: Range<usize>) -> HtmlNode {
    HtmlNode {
        kind: HtmlNodeKind::RawTextBlock,
        tag_name: "#raw".into(),
        attrs: Vec::new(),
        children: Vec::new(),
        raw_source: raw[range.clone()].to_string(),
        source_range: range,
    }
}

pub(super) fn raw_region_end(raw: &str, token: &TagToken) -> Option<usize> {
    if token.self_closing || is_void_tag(&token.name) {
        return Some(token.source_range.end);
    }

    let close = format!("</{}>", token.name);
    let close_upper = close.to_ascii_uppercase();
    let rest = &raw[token.source_range.end..];
    let lower = rest.to_ascii_lowercase();
    let upper = rest.to_ascii_uppercase();
    lower
        .find(&close)
        .or_else(|| upper.find(&close_upper))
        .map(|offset| token.source_range.end + offset + close.len())
        .or(Some(raw.len()))
}

pub(crate) fn has_dangerous_attrs(attrs: &[HtmlAttr]) -> bool {
    attrs.iter().any(|attr| {
        attr.name.starts_with("on")
            || attr.value.as_deref().is_some_and(|value| {
                let normalized = value
                    .chars()
                    .filter(|ch| !ch.is_whitespace() && *ch != '\0')
                    .collect::<String>()
                    .to_ascii_lowercase();
                matches!(
                    attr.name.as_str(),
                    "href" | "src" | "action" | "formaction" | "xlink:href"
                ) && normalized.starts_with("javascript:")
            })
    })
}

pub(crate) fn attr_value<'a>(node: &'a HtmlNode, name: &str) -> Option<&'a str> {
    node.attrs
        .iter()
        .find(|attr| attr.name == name)
        .and_then(|attr| attr.value.as_deref())
}

pub(crate) fn parse_html_image_block(raw_source: &str) -> Option<HtmlImageBlock> {
    let trimmed = raw_source.trim();
    if trimmed.is_empty() {
        return None;
    }

    let token = parse_tag_token(trimmed, 0)?;
    if token.kind != TagKind::Open
        || token.name != "img"
        || token.source_range != (0..trimmed.len())
    {
        return None;
    }
    if has_dangerous_attrs(&token.attrs) {
        return None;
    }

    let src = attr_value_in_attrs(&token.attrs, "src")?.trim().to_string();
    if src.is_empty() {
        return None;
    }

    let alt = attr_value_in_attrs(&token.attrs, "alt")
        .unwrap_or_default()
        .to_string();
    let title = attr_value_in_attrs(&token.attrs, "title").map(str::to_string);
    let zoom = attr_value_in_attrs(&token.attrs, "style")
        .and_then(parse_html_zoom)
        .unwrap_or(1.0);
    let width_percent =
        attr_value_in_attrs(&token.attrs, "style").and_then(parse_html_width_percent);

    Some(HtmlImageBlock {
        src,
        alt,
        title,
        zoom,
        width_percent,
    })
}

fn attr_value_in_attrs<'a>(attrs: &'a [HtmlAttr], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|attr| attr.name == name)
        .and_then(|attr| attr.value.as_deref())
}

fn parse_html_zoom(style: &str) -> Option<f32> {
    for declaration in style.split(';') {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        if !property.trim().eq_ignore_ascii_case("zoom") {
            continue;
        }

        let value = value.trim();
        let parsed = if let Some(percent) = value.strip_suffix('%') {
            parse_css_number(percent)? / 100.0
        } else {
            parse_css_number(value)?
        };
        return Some(parsed.clamp(0.1, 3.0));
    }
    None
}

fn parse_html_width_percent(style: &str) -> Option<u8> {
    for declaration in style.split(';') {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        if !property.trim().eq_ignore_ascii_case("width") {
            continue;
        }
        let percent = value.trim().strip_suffix('%')?.trim().parse::<u8>().ok()?;
        return (10..=100).contains(&percent).then_some(percent);
    }
    None
}

pub(super) fn parse_inline_style(style: &str) -> HtmlInlineStyle {
    let mut parsed = HtmlInlineStyle::default();
    for declaration in style.split(';') {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        let property = property.trim().to_ascii_lowercase();
        let value = value.trim();
        match property.as_str() {
            "color" => {
                if let Some(color) = parse_css_color(value) {
                    parsed.color = Some(color);
                }
            }
            "background-color" => {
                if let Some(color) = parse_css_color(value) {
                    parsed.background_color = Some(color);
                }
            }
            "font-size" => {
                if let Some(size) = parse_css_font_size(value) {
                    parsed.font_size = Some(size);
                }
            }
            _ => {}
        }
    }
    parsed
}

fn parse_css_color(value: &str) -> Option<HtmlCssColor> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("currentcolor") {
        return Some(HtmlCssColor::CurrentColor);
    }
    if value.eq_ignore_ascii_case("transparent") {
        return Some(HtmlCssColor::Rgba(HtmlCssRgba {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0.0,
        }));
    }
    if let Some(hex) = value.strip_prefix('#')
        && let Ok((red, green, blue, alpha)) = parse_hash_color(hex.as_bytes())
    {
        return Some(HtmlCssColor::Rgba(HtmlCssRgba {
            red,
            green,
            blue,
            alpha,
        }));
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphabetic() || ch == '-')
        && let Ok((red, green, blue)) = parse_named_color(value)
    {
        return Some(HtmlCssColor::Rgba(HtmlCssRgba {
            red,
            green,
            blue,
            alpha: 1.0,
        }));
    }
    parse_rgb_color(value).or_else(|| parse_hsl_color(value))
}

fn parse_rgb_color(value: &str) -> Option<HtmlCssColor> {
    let args = css_function_args(value, &["rgb", "rgba"])?;
    let parts = css_function_parts(args);
    if parts.len() < 3 {
        return None;
    }

    let red = parse_rgb_component(&parts[0])?;
    let green = parse_rgb_component(&parts[1])?;
    let blue = parse_rgb_component(&parts[2])?;
    let alpha = parts
        .get(3)
        .and_then(|part| parse_alpha_component(part))
        .unwrap_or(1.0);
    Some(HtmlCssColor::Rgba(HtmlCssRgba {
        red,
        green,
        blue,
        alpha,
    }))
}

fn parse_hsl_color(value: &str) -> Option<HtmlCssColor> {
    let args = css_function_args(value, &["hsl", "hsla"])?;
    let parts = css_function_parts(args);
    if parts.len() < 3 {
        return None;
    }

    let hue = parse_hue(&parts[0])?;
    let saturation = parse_percent_component(&parts[1])?;
    let lightness = parse_percent_component(&parts[2])?;
    let alpha = parts
        .get(3)
        .and_then(|part| parse_alpha_component(part))
        .unwrap_or(1.0);
    let (red, green, blue) = hsl_to_rgb(hue, saturation, lightness);
    Some(HtmlCssColor::Rgba(HtmlCssRgba {
        red,
        green,
        blue,
        alpha,
    }))
}

fn css_function_args<'a>(value: &'a str, names: &[&str]) -> Option<&'a str> {
    let open = value.find('(')?;
    let close = value.rfind(')')?;
    if close <= open || !value[close + 1..].trim().is_empty() {
        return None;
    }
    let name = value[..open].trim();
    names
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
        .then_some(&value[open + 1..close])
}

fn css_function_parts(args: &str) -> Vec<String> {
    if args.contains(',') {
        return args
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect();
    }

    let normalized = args.replace('/', " / ");
    normalized
        .split_whitespace()
        .filter(|token| *token != "/")
        .map(str::to_string)
        .collect()
}

fn parse_rgb_component(value: &str) -> Option<u8> {
    if let Some(percent) = value.trim().strip_suffix('%') {
        let value = parse_css_number(percent)?;
        return Some((value.clamp(0.0, 100.0) * 255.0 / 100.0).round() as u8);
    }

    let value = parse_css_number(value)?;
    Some(value.clamp(0.0, 255.0).round() as u8)
}

fn parse_percent_component(value: &str) -> Option<f32> {
    let value = value.trim().strip_suffix('%')?;
    Some((parse_css_number(value)? / 100.0).clamp(0.0, 1.0))
}

fn parse_alpha_component(value: &str) -> Option<f32> {
    if let Some(percent) = value.trim().strip_suffix('%') {
        return Some((parse_css_number(percent)? / 100.0).clamp(0.0, 1.0));
    }
    Some(parse_css_number(value)?.clamp(0.0, 1.0))
}

fn parse_hue(value: &str) -> Option<f32> {
    let trimmed = value.trim().to_ascii_lowercase();
    if let Some(value) = trimmed.strip_suffix("deg") {
        return parse_css_number(value);
    }
    if let Some(value) = trimmed.strip_suffix("turn") {
        return Some(parse_css_number(value)? * 360.0);
    }
    if let Some(value) = trimmed.strip_suffix("rad") {
        return Some(parse_css_number(value)? * 180.0 / std::f32::consts::PI);
    }
    parse_css_number(&trimmed)
}

fn hsl_to_rgb(hue_degrees: f32, saturation: f32, lightness: f32) -> (u8, u8, u8) {
    let hue = hue_degrees.rem_euclid(360.0) / 60.0;
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let x = chroma * (1.0 - (hue % 2.0 - 1.0).abs());
    let (red, green, blue) = match hue.floor() as i32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    (
        ((red + m).clamp(0.0, 1.0) * 255.0).round() as u8,
        ((green + m).clamp(0.0, 1.0) * 255.0).round() as u8,
        ((blue + m).clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn parse_css_font_size(value: &str) -> Option<HtmlCssFontSize> {
    let trimmed = value.trim().to_ascii_lowercase();
    match trimmed.as_str() {
        "xx-small" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::XxSmall)),
        "x-small" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::XSmall)),
        "small" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::Small)),
        "medium" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::Medium)),
        "large" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::Large)),
        "x-large" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::XLarge)),
        "xx-large" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::XxLarge)),
        "smaller" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::Smaller)),
        "larger" => return Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::Larger)),
        _ => {}
    }

    if let Some(value) = trimmed.strip_suffix("rem") {
        return Some(HtmlCssFontSize::Rem(parse_non_negative_css_number(value)?));
    }
    if let Some(value) = trimmed.strip_suffix("em") {
        return Some(HtmlCssFontSize::Em(parse_non_negative_css_number(value)?));
    }
    if let Some(value) = trimmed.strip_suffix("px") {
        return Some(HtmlCssFontSize::Px(parse_non_negative_css_number(value)?));
    }
    if let Some(value) = trimmed.strip_suffix('%') {
        return Some(HtmlCssFontSize::Percent(parse_non_negative_css_number(
            value,
        )?));
    }
    None
}

fn parse_non_negative_css_number(value: &str) -> Option<f32> {
    let value = parse_css_number(value)?;
    (value >= 0.0).then_some(value)
}

fn parse_css_number(value: &str) -> Option<f32> {
    let value = value.trim().parse::<f32>().ok()?;
    value.is_finite().then_some(value)
}

pub(super) fn css_number(value: f32) -> String {
    let mut formatted = format!("{:.3}", value);
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

fn is_safe_tag(name: &str) -> bool {
    is_inline_tag(name) || is_block_tag(name)
}

pub(crate) fn is_inline_tag(name: &str) -> bool {
    matches!(
        name,
        "a" | "strong"
            | "em"
            | "b"
            | "i"
            | "u"
            | "mark"
            | "del"
            | "ins"
            | "code"
            | "kbd"
            | "sup"
            | "sub"
            | "small"
            | "abbr"
            | "dfn"
            | "time"
            | "q"
            | "span"
    )
}

fn is_block_tag(name: &str) -> bool {
    matches!(
        name,
        "div"
            | "p"
            | "blockquote"
            | "hr"
            | "br"
            | "details"
            | "summary"
            | "figure"
            | "figcaption"
            | "table"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
            | "th"
            | "td"
            | "img"
            | "pre"
    )
}

pub(super) fn is_void_tag(name: &str) -> bool {
    matches!(name, "br" | "hr" | "img")
}

#[cfg(feature = "html-native")]
pub(super) fn tree_sitter_reports_error(raw_source: &str) -> bool {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .is_err()
    {
        return true;
    }
    parser
        .parse(raw_source, None)
        .is_none_or(|tree| tree.root_node().has_error())
}

#[cfg(not(feature = "html-native"))]
pub(super) fn tree_sitter_reports_error(_: &str) -> bool {
    true
}
