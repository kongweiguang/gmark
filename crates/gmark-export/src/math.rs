// @author kongweiguang

//! LaTeX and Mermaid source projections for browser-based document export.

use anyhow::anyhow;
use gmark_markdown::escape_html;

use crate::ExportTheme;
use crate::markup::{data_uri_for_bytes, is_closing_fence, opening_fence};

const INLINE_MATH_SCALE: f32 = 1.12;

pub(crate) fn rewrite_display_math_blocks(markdown: &str, theme: &ExportTheme) -> String {
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
        if !is_root_display_math_start(line) {
            rewritten.push(line.to_owned());
            index += 1;
            continue;
        }
        let end = collect_display_math_region(&lines, index);
        let raw = lines[index..end].join("\n");
        match parse_display_math_source(&raw)
            .and_then(|source| render_latex_to_svg(&source, theme, theme.typography.text_size).ok())
        {
            Some(svg) => rewritten.push(format!("<div class=\"vlt-math\">{svg}</div>")),
            None if parse_display_math_source(&raw).is_some() => rewritten.push(format!(
                "<pre class=\"vlt-math-error\">{}</pre>",
                escape_html(&raw)
            )),
            None => rewritten.push(raw),
        }
        index = end;
    }
    rewritten.join("\n")
}

pub(crate) fn rewrite_mermaid_blocks(markdown: &str, theme: &ExportTheme) -> String {
    let lines = markdown.split('\n').collect::<Vec<_>>();
    let mut rewritten = Vec::with_capacity(lines.len());
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let Some(fence) = parse_mermaid_fence_start(line) else {
            rewritten.push(line.to_owned());
            index += 1;
            continue;
        };
        let mut end = index + 1;
        while end < lines.len() && !is_mermaid_closing_fence(lines[end], fence) {
            end += 1;
        }
        if end >= lines.len() {
            rewritten.push(line.to_owned());
            index += 1;
            continue;
        }
        let raw = lines[index..=end].join("\n");
        match parse_mermaid_fence_source(&raw)
            .and_then(|source| render_mermaid_to_svg(&source, theme).ok())
        {
            Some(svg) => {
                let source = data_uri_for_bytes("image/svg+xml", svg.as_bytes());
                rewritten.push(format!(
                    "<div class=\"vlt-mermaid\"><img alt=\"Mermaid diagram\" src=\"{source}\"></div>"
                ));
            }
            None if parse_mermaid_fence_source(&raw).is_some() => rewritten.push(format!(
                "<pre class=\"vlt-mermaid-error\">{}</pre>",
                escape_html(&raw)
            )),
            None => rewritten.push(raw),
        }
        index = end + 1;
    }
    rewritten.join("\n")
}

pub(crate) fn rewrite_inline_math(markdown: &str, theme: &ExportTheme) -> String {
    let mut rewritten = Vec::with_capacity(markdown.lines().count());
    let mut active_fence = None;
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
        rewritten.push(rewrite_inline_math_line(line, theme));
    }
    rewritten.join("\n")
}

fn rewrite_inline_math_line(line: &str, theme: &ExportTheme) -> String {
    let mut output = String::with_capacity(line.len());
    let mut index = 0;
    while index < line.len() {
        if line[index..].starts_with('`') {
            let length = line[index..]
                .bytes()
                .take_while(|byte| *byte == b'`')
                .count();
            if let Some(close) = find_backtick_run(line, index + length, length) {
                output.push_str(&line[index..close + length]);
                index = close + length;
                continue;
            }
        }
        if let Some((end, body)) =
            locate_inline_dollar_math(line, index).or_else(|| locate_inline_paren_math(line, index))
        {
            match render_latex_to_svg(&body, theme, theme.typography.text_size * INLINE_MATH_SCALE)
            {
                Ok(svg) => {
                    output.push_str(&format!("<span class=\"vlt-inline-math\">{svg}</span>"))
                }
                Err(_) => output.push_str(&escape_html(&line[index..end])),
            }
            index = end;
            continue;
        }
        if let Some((end, body, tag)) = locate_inline_script(line, index) {
            output.push_str(&format!("<{tag}>{}</{tag}>", escape_html(&body)));
            index = end;
            continue;
        }
        let Some(character) = line[index..].chars().next() else {
            break;
        };
        output.push(character);
        index += character.len_utf8();
    }
    output
}

fn render_latex_to_svg(
    source: &str,
    theme: &ExportTheme,
    font_size: f32,
) -> anyhow::Result<String> {
    let parsed = ratex_parser::parse(source).map_err(|error| anyhow!("{error}"))?;
    let layout = ratex_layout::layout(&parsed, &ratex_layout::LayoutOptions::default());
    let display_list = ratex_layout::to_display_list(&layout);
    let mut svg = ratex_svg::render_to_svg(
        &display_list,
        &ratex_svg::SvgOptions {
            font_size: f64::from(font_size.max(1.0)),
            padding: f64::from((font_size * 0.35).max(4.0)),
            embed_glyphs: true,
            ..ratex_svg::SvgOptions::default()
        },
    );
    let color = theme.colors.text.css();
    svg = svg
        .replace("rgba(0,0,0,1)", &color)
        .replace("rgba(0, 0, 0, 1)", &color);
    Ok(svg)
}

fn is_root_display_math_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("$$") && line.len() - trimmed.len() <= 3
}

fn collect_display_math_region(lines: &[&str], start: usize) -> usize {
    let opener = lines[start].trim_start().trim_end();
    if opener != "$$" && opener[2..].contains("$$") {
        return start + 1;
    }
    let mut index = start + 1;
    while index < lines.len() {
        if lines[index].trim() == "$$" {
            return index + 1;
        }
        if lines[index].trim().is_empty() {
            return index;
        }
        index += 1;
    }
    lines.len()
}

fn parse_display_math_source(raw: &str) -> Option<String> {
    let raw = raw.trim_matches('\n');
    let lines = raw.split('\n').collect::<Vec<_>>();
    if lines.len() == 1 {
        let line = strip_indent(lines[0])?.trim_end();
        let source = line.strip_prefix("$$")?;
        return source
            .find("$$")
            .map(|close| source[..close].trim().to_owned());
    }
    (lines.len() >= 2 && strip_indent(lines[0])?.trim_end() == "$$" && lines.last()?.trim() == "$$")
        .then(|| lines[1..lines.len() - 1].join("\n"))
}

#[derive(Clone, Copy)]
struct MermaidFence {
    marker: char,
    length: usize,
}

fn parse_mermaid_fence_start(line: &str) -> Option<MermaidFence> {
    let trimmed = strip_indent(line)?.trim_end();
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let length = trimmed
        .chars()
        .take_while(|character| *character == marker)
        .count();
    let info = trimmed[marker.len_utf8() * length..].trim();
    (length >= 3
        && !(marker == '`' && info.contains('`'))
        && info.split_whitespace().next().is_some_and(|kind| {
            kind.eq_ignore_ascii_case("mermaid") || kind.eq_ignore_ascii_case("mmd")
        }))
    .then_some(MermaidFence { marker, length })
}

fn is_mermaid_closing_fence(line: &str, fence: MermaidFence) -> bool {
    let Some(trimmed) = strip_indent(line).map(str::trim_end) else {
        return false;
    };
    let length = trimmed
        .chars()
        .take_while(|character| *character == fence.marker)
        .count();
    length >= fence.length
        && trimmed[fence.marker.len_utf8() * length..]
            .trim()
            .is_empty()
}

fn parse_mermaid_fence_source(raw: &str) -> Option<String> {
    let raw = raw.trim_matches('\n');
    let lines = raw.split('\n').collect::<Vec<_>>();
    let fence = parse_mermaid_fence_start(lines.first()?)?;
    is_mermaid_closing_fence(lines.last()?, fence).then(|| lines[1..lines.len() - 1].join("\n"))
}

fn render_mermaid_to_svg(source: &str, theme: &ExportTheme) -> anyhow::Result<String> {
    if !looks_like_mermaid(source) {
        return Err(anyhow!("unsupported Mermaid diagram"));
    }
    let mut options = mermaid_rs_renderer::RenderOptions::modern();
    if theme.color_scheme == crate::ExportColorScheme::Dark {
        options.theme = mermaid_rs_renderer::Theme::dark();
    }
    let parsed =
        mermaid_rs_renderer::parse_mermaid_strict(source).map_err(|error| anyhow!("{error}"))?;
    let layout =
        mermaid_rs_renderer::compute_layout(&parsed.graph, &options.theme, &options.layout);
    let svg = mermaid_rs_renderer::render_svg(&layout, &options.theme, &options.layout);
    (!svg.contains("class=\"error-text\"") && !svg.contains("Syntax error in text"))
        .then_some(svg)
        .ok_or_else(|| anyhow!("Mermaid syntax error"))
}

fn looks_like_mermaid(source: &str) -> bool {
    source.lines().any(|line| {
        let lower = line.trim().to_ascii_lowercase();
        !lower.is_empty()
            && !lower.starts_with("%%")
            && [
                "flowchart",
                "graph",
                "sequencediagram",
                "classdiagram",
                "statediagram",
                "erdiagram",
                "pie",
                "mindmap",
                "journey",
                "timeline",
                "gantt",
                "gitgraph",
                "architecture",
            ]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
    })
}

fn strip_indent(line: &str) -> Option<&str> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    (indent <= 3).then_some(&line[indent..])
}

fn find_backtick_run(line: &str, mut index: usize, length: usize) -> Option<usize> {
    while index < line.len() {
        if line[index..].starts_with(&"`".repeat(length)) {
            return Some(index);
        }
        index += line[index..].chars().next()?.len_utf8();
    }
    None
}

fn locate_inline_dollar_math(line: &str, index: usize) -> Option<(usize, String)> {
    if !line[index..].starts_with('$') || line[index..].starts_with("$$") || is_escaped(line, index)
    {
        return None;
    }
    let mut cursor = index + 1;
    while cursor < line.len() {
        if line[cursor..].starts_with('$')
            && !line[cursor..].starts_with("$$")
            && !is_escaped(line, cursor)
        {
            let body = &line[index + 1..cursor];
            return (valid_math_body(body) && !looks_like_currency(line, index, cursor, body))
                .then(|| (cursor + 1, body.to_owned()));
        }
        cursor += line[cursor..].chars().next()?.len_utf8();
    }
    None
}

fn locate_inline_paren_math(line: &str, index: usize) -> Option<(usize, String)> {
    if !line[index..].starts_with("\\(") {
        return None;
    }
    let mut cursor = index + 2;
    while cursor + 1 < line.len() {
        if line[cursor..].starts_with("\\)") {
            let body = &line[index + 2..cursor];
            return valid_math_body(body).then(|| (cursor + 2, body.to_owned()));
        }
        cursor += line[cursor..].chars().next()?.len_utf8();
    }
    None
}

fn locate_inline_script(line: &str, index: usize) -> Option<(usize, String, &'static str)> {
    if is_escaped(line, index) {
        return None;
    }
    if line[index..].starts_with('^') {
        locate_script_close(line, index, '^').map(|(end, body)| (end, body, "sup"))
    } else if is_single_tilde(line, index) {
        locate_script_close(line, index, '~').map(|(end, body)| (end, body, "sub"))
    } else {
        None
    }
}

fn locate_script_close(line: &str, index: usize, marker: char) -> Option<(usize, String)> {
    if !line
        .get(..index)?
        .chars()
        .next_back()?
        .is_ascii_alphanumeric()
    {
        return None;
    }
    let start = index + marker.len_utf8();
    if !line[start..].chars().next()?.is_ascii_alphanumeric() {
        return None;
    }
    let mut cursor = start;
    while cursor < line.len() {
        if line[cursor..].starts_with(marker)
            && !is_escaped(line, cursor)
            && (marker != '~' || is_single_tilde(line, cursor))
        {
            let body = &line[start..cursor];
            return body
                .chars()
                .all(char::is_alphanumeric)
                .then(|| (cursor + marker.len_utf8(), body.to_owned()));
        }
        cursor += line[cursor..].chars().next()?.len_utf8();
    }
    None
}

fn valid_math_body(body: &str) -> bool {
    !body.is_empty() && !body.contains(['\n', '\r']) && body.trim() == body
}

fn looks_like_currency(line: &str, open: usize, close: usize, body: &str) -> bool {
    (open > 0 && line.as_bytes()[open - 1].is_ascii_digit())
        || (close + 1 < line.len() && line.as_bytes()[close + 1].is_ascii_digit())
        || (body
            .chars()
            .all(|character| character.is_ascii_digit() || matches!(character, '.' | ',' | '_'))
            && body.chars().any(|character| character.is_ascii_digit())
            && body.len() > 1)
}

fn is_single_tilde(line: &str, index: usize) -> bool {
    line[index..].starts_with('~')
        && line
            .get(..index)
            .and_then(|value| value.chars().next_back())
            != Some('~')
        && !line[index + 1..].starts_with('~')
}

fn is_escaped(line: &str, index: usize) -> bool {
    let mut slash_count = 0;
    let mut cursor = index;
    while cursor > 0 && line.as_bytes()[cursor - 1] == b'\\' {
        slash_count += 1;
        cursor -= 1;
    }
    slash_count % 2 == 1
}
