// @author kongweiguang

use super::*;

pub(super) fn looks_like_export_currency(
    line: &str,
    open: usize,
    close: usize,
    body: &str,
) -> bool {
    let prev_is_digit = open > 0 && line.as_bytes()[open - 1].is_ascii_digit();
    let next_is_digit = close + 1 < line.len() && line.as_bytes()[close + 1].is_ascii_digit();
    (prev_is_digit || next_is_digit)
        || (body
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | ',' | '_'))
            && body.chars().any(|ch| ch.is_ascii_digit())
            && body.len() > 1)
}

pub(super) fn rewrite_unsafe_html_blocks(markdown: &str, base_dir: Option<&Path>) -> String {
    let lines = markdown.split('\n').collect::<Vec<_>>();
    let mut rewritten = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    let mut active_fence: Option<(char, usize)> = None;

    while index < lines.len() {
        let line = lines[index];
        if let Some((marker, run_len)) = active_fence {
            rewritten.push(line.to_string());
            if is_closing_fence(line, marker, run_len) {
                active_fence = None;
            }
            index += 1;
            continue;
        }

        if let Some(fence) = opening_fence(line) {
            active_fence = Some(fence);
            rewritten.push(line.to_string());
            index += 1;
            continue;
        }

        let Some(html_start) = root_html_start(line) else {
            rewritten.push(line.to_string());
            index += 1;
            continue;
        };

        let end = collect_export_html_region(&lines, index, &html_start);
        let raw = lines[index..end].join("\n");
        if let Some(image) = parse_html_image_block(&raw) {
            let src =
                local_image_data_uri(&image.src, base_dir).unwrap_or_else(|| image.src.clone());
            rewritten.push(image.to_sanitized_html_with_src(&src));
        } else {
            rewritten.push(sanitize_html_for_export(&raw));
        }
        index = end;
    }

    rewritten.join("\n")
}

pub(super) fn rewrite_display_math_blocks(markdown: &str, theme: &Theme) -> String {
    let lines = markdown.split('\n').collect::<Vec<_>>();
    let mut rewritten = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    let mut active_fence: Option<(char, usize)> = None;

    while index < lines.len() {
        let line = lines[index];
        if let Some((marker, run_len)) = active_fence {
            rewritten.push(line.to_string());
            if is_closing_fence(line, marker, run_len) {
                active_fence = None;
            }
            index += 1;
            continue;
        }

        if let Some(fence) = opening_fence(line) {
            active_fence = Some(fence);
            rewritten.push(line.to_string());
            index += 1;
            continue;
        }

        if !is_root_display_math_start(line) {
            rewritten.push(line.to_string());
            index += 1;
            continue;
        }

        let end = collect_display_math_region(&lines, index);
        let raw = lines[index..end].join("\n");
        if let Some(source) = parse_display_math_source(&raw) {
            match render_latex_to_svg(
                &source.body,
                theme.colors.text_default,
                theme.typography.text_size,
            ) {
                Ok(svg) => rewritten.push(format!("<div class=\"vlt-math\">{svg}</div>")),
                Err(_) => rewritten.push(format!(
                    "<pre class=\"vlt-math-error\">{}</pre>",
                    escape_html(&raw)
                )),
            }
        } else {
            rewritten.push(raw);
        }
        index = end;
    }

    rewritten.join("\n")
}

pub(super) fn rewrite_mermaid_blocks(markdown: &str, theme: &Theme) -> String {
    let lines = markdown.split('\n').collect::<Vec<_>>();
    let mut rewritten = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    let theme_mode = MermaidThemeMode::from_theme(theme);

    while index < lines.len() {
        let line = lines[index];
        let Some(fence) = parse_mermaid_fence_start(line) else {
            rewritten.push(line.to_string());
            index += 1;
            continue;
        };

        let mut end = index + 1;
        while end < lines.len() && !is_mermaid_closing_fence(lines[end], fence) {
            end += 1;
        }
        if end >= lines.len() {
            rewritten.push(line.to_string());
            index += 1;
            continue;
        }

        let raw = lines[index..=end].join("\n");
        if let Some(source) = parse_mermaid_fence_source(&raw) {
            match render_mermaid_to_svg(&source.body, theme_mode) {
                Ok(svg) => {
                    let src = data_uri_for_bytes("image/svg+xml", svg.as_bytes());
                    rewritten.push(format!(
                        "<div class=\"vlt-mermaid\"><img alt=\"Mermaid diagram\" src=\"{src}\"></div>"
                    ));
                }
                Err(_) => rewritten.push(format!(
                    "<pre class=\"vlt-mermaid-error\">{}</pre>",
                    escape_html(&raw)
                )),
            }
        } else {
            rewritten.push(raw);
        }
        index = end + 1;
    }

    rewritten.join("\n")
}

pub(super) fn rewrite_local_image_event<'a>(
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

fn local_image_data_uri(source: &str, base_dir: Option<&Path>) -> Option<String> {
    if source.is_empty()
        || source.starts_with('#')
        || source.starts_with("data:")
        || net::is_remote_image_source(source)
    {
        return None;
    }

    let path = Path::new(source);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir?.join(path)
    };
    let mime = image_mime_from_path(&resolved)?;
    let bytes = fs::read(&resolved).ok()?;
    Some(data_uri_for_bytes(mime, &bytes))
}

fn image_mime_from_path(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

fn data_uri_for_bytes(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        general_purpose::STANDARD.encode(bytes)
    )
}

#[derive(Clone, Debug)]
struct ExportHtmlStart {
    name: String,
    self_closing: bool,
    closes_same_line: bool,
}

fn root_html_start(line: &str) -> Option<ExportHtmlStart> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 || trimmed.starts_with("<!--") {
        return None;
    }

    let tagged = trimmed.strip_prefix('<')?;
    if tagged.starts_with('/') || tagged.starts_with('!') || tagged.starts_with('?') {
        return None;
    }
    let name_len = tagged
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .map(char::len_utf8)
        .sum::<usize>();
    if name_len == 0 {
        return None;
    }
    let name = tagged[..name_len].to_ascii_lowercase();
    let suffix = &tagged[name_len..];
    let next = suffix.chars().next()?;
    if !matches!(next, '>' | ' ' | '\t' | '/') {
        return None;
    }
    Some(ExportHtmlStart {
        self_closing: trimmed.ends_with("/>") || is_export_void_html_tag(&name),
        closes_same_line: trimmed.to_ascii_lowercase().contains(&format!("</{name}>")),
        name,
    })
}

fn is_export_void_html_tag(name: &str) -> bool {
    matches!(name, "br" | "hr" | "img")
}

fn collect_export_html_region(lines: &[&str], start: usize, html: &ExportHtmlStart) -> usize {
    if html.self_closing || html.closes_same_line {
        return start + 1;
    }

    let close = format!("</{}>", html.name);
    let mut index = start + 1;
    while index < lines.len() {
        let line = lines[index];
        if line.to_ascii_lowercase().contains(&close) {
            return index + 1;
        }
        if line.trim().is_empty() {
            return index;
        }
        index += 1;
    }

    lines.len()
}

pub(super) fn opening_fence(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return None;
    }

    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }

    let run_len = trimmed.chars().take_while(|ch| *ch == marker).count();
    (run_len >= 3).then_some((marker, run_len))
}

pub(super) fn is_closing_fence(line: &str, marker: char, opening_run_len: usize) -> bool {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return false;
    }

    let run_len = trimmed.chars().take_while(|ch| *ch == marker).count();
    run_len >= opening_run_len && trimmed[marker.len_utf8() * run_len..].trim().is_empty()
}

pub(super) fn is_root_comment_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("<!--") && line.len() - trimmed.len() <= 3
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

pub(super) fn theme_css(theme: &Theme) -> String {
    let c = &theme.colors;
    let d = &theme.dimensions;
    let t = &theme.typography;
    let color_scheme = if c.editor_background.l >= 0.5 {
        "light"
    } else {
        "dark"
    };
    let pre_overflow = "overflow: auto;";
    let media_overflow = "overflow-x: auto;";
    format!(
        r#":root {{
  color-scheme: {};
  --vlt-bg: {};
  --vlt-text: {};
  --vlt-muted: {};
  --vlt-link: {};
  --vlt-border: {};
  --vlt-code-bg: {};
  --vlt-code-text: {};
  --vlt-comment-bg: {};
  --vlt-table-head-bg: {};
  --vlt-table-cell-bg: {};
  --vlt-quote-border: {};
  --vlt-quote-text: {};
  --vlt-callout-note-bg: {};
  --vlt-callout-note-border: {};
  --vlt-callout-tip-bg: {};
  --vlt-callout-tip-border: {};
  --vlt-callout-important-bg: {};
  --vlt-callout-important-border: {};
  --vlt-callout-warning-bg: {};
  --vlt-callout-warning-border: {};
  --vlt-callout-caution-bg: {};
  --vlt-callout-caution-border: {};
}}

* {{ box-sizing: border-box; }}
html {{ background-color: var(--vlt-bg); color: var(--vlt-text); }}
body {{
  margin: 0;
  background-color: var(--vlt-bg);
  color: var(--vlt-text);
  font-family: {};
  font-size: {}px;
  line-height: {};
}}
{}
p, ul, ol, blockquote, pre, table, hr {{ margin: 0 0 1rem; }}
h1, h2, h3, h4, h5, h6 {{
  margin: 1.6em 0 0.65em;
  line-height: 1.2;
  font-weight: {};
}}
h1 {{ color: {}; font-size: {}px; }}
h2 {{ color: {}; font-size: {}px; }}
h3 {{ color: {}; font-size: {}px; }}
h4 {{ color: {}; font-size: {}px; }}
h5 {{ color: {}; font-size: {}px; }}
h6 {{ color: {}; font-size: {}px; }}
a {{ color: var(--vlt-link); text-decoration-thickness: 0.08em; text-underline-offset: 0.18em; }}
blockquote {{
  margin-left: 0;
  padding: 0.5rem 0 0.5rem 1rem;
  border-left: 3px solid;
  border-color: var(--vlt-quote-border);
  color: var(--vlt-quote-text);
}}
blockquote.markdown-alert-note,
blockquote.markdown-alert-tip,
blockquote.markdown-alert-important,
blockquote.markdown-alert-warning,
blockquote.markdown-alert-caution {{
  padding: 0.75rem 1rem;
  border-left: 4px solid;
  border-radius: {}px;
}}
blockquote.markdown-alert-note {{ background-color: var(--vlt-callout-note-bg); border-color: var(--vlt-callout-note-border); }}
blockquote.markdown-alert-tip {{ background-color: var(--vlt-callout-tip-bg); border-color: var(--vlt-callout-tip-border); }}
blockquote.markdown-alert-important {{ background-color: var(--vlt-callout-important-bg); border-color: var(--vlt-callout-important-border); }}
blockquote.markdown-alert-warning {{ background-color: var(--vlt-callout-warning-bg); border-color: var(--vlt-callout-warning-border); }}
blockquote.markdown-alert-caution {{ background-color: var(--vlt-callout-caution-bg); border-color: var(--vlt-callout-caution-border); }}
code {{
  background-color: var(--vlt-code-bg);
  color: var(--vlt-code-text);
  border-radius: 4px;
  padding: 0.12em 0.32em;
  font-family: {};
  font-size: {}px;
}}
pre {{
  {}
  background-color: var(--vlt-code-bg);
  color: var(--vlt-code-text);
  border-radius: {}px;
  padding: 1rem;
}}
pre code {{ padding: 0; background-color: transparent; }}
.vlt-comment {{
  white-space: pre-wrap;
  padding: 0;
  border: 0;
  background-color: transparent;
  color: var(--vlt-link);
}}
.vlt-raw-html {{
  white-space: pre-wrap;
  background-color: var(--vlt-code-bg);
  color: var(--vlt-code-text);
}}
.vlt-math {{
  display: flex;
  justify-content: center;
  margin: 1rem 0;
  {}
}}
.vlt-math svg {{
  max-width: 100%;
  height: auto;
}}
.vlt-mermaid {{
  display: flex;
  justify-content: center;
  margin: 1rem 0;
  {}
}}
.vlt-mermaid img {{
  max-width: 100%;
  height: auto;
  display: block;
  margin: 0 auto;
}}
.vlt-inline-math {{
  display: inline-flex;
  align-items: center;
  vertical-align: middle;
  max-width: 100%;
}}
.vlt-inline-math svg {{
  max-height: 1.8em;
  width: auto;
}}
.vlt-math-error {{
  white-space: pre-wrap;
  background-color: var(--vlt-code-bg);
  color: var(--vlt-code-text);
}}
.vlt-mermaid-error {{
  white-space: pre-wrap;
  background-color: var(--vlt-code-bg);
  color: var(--vlt-code-text);
}}
table {{
  width: 100%;
  border-collapse: collapse;
  display: table;
}}
th, td {{
  border: 1px solid;
  border-color: var(--vlt-border);
  padding: 0.5rem 0.65rem;
  vertical-align: top;
}}
th {{ background-color: var(--vlt-table-head-bg); font-weight: 600; }}
td {{ background-color: var(--vlt-table-cell-bg); }}
img {{ max-width: 100%; height: auto; display: block; margin: 1rem auto; }}
hr {{ border: 0; border-top: 1px solid; border-color: var(--vlt-border); }}
.gmark-toc {{
  margin: 1rem 0 1.5rem;
  padding: 0.85rem 1rem;
  border-left: 2px solid var(--vlt-link);
  background-color: var(--vlt-code-bg);
}}
.gmark-toc ol {{ margin: 0; padding-left: 1.2rem; }}
.gmark-toc li {{ margin: 0.22rem 0; }}
.gmark-toc-level-2 {{ margin-left: 1rem !important; }}
.gmark-toc-level-3 {{ margin-left: 2rem !important; }}
.gmark-toc-level-4 {{ margin-left: 3rem !important; }}
.gmark-toc-level-5 {{ margin-left: 4rem !important; }}
.gmark-toc-level-6 {{ margin-left: 5rem !important; }}
.footnote-definition {{
  color: var(--vlt-muted);
  font-size: 0.92em;
}}
"#,
        color_scheme,
        css_color(c.editor_background),
        css_color(c.text_default),
        css_color(c.dialog_muted),
        css_color(c.text_link),
        css_color(c.table_border),
        css_color(c.code_bg),
        css_color(c.code_text),
        css_color(c.comment_bg),
        css_color(c.table_header_bg),
        css_color(c.table_cell_bg),
        css_color(c.border_quote),
        css_color(c.text_quote),
        css_color(c.callout_note_bg),
        css_color(c.callout_note_border),
        css_color(c.callout_tip_bg),
        css_color(c.callout_tip_border),
        css_color(c.callout_important_bg),
        css_color(c.callout_important_border),
        css_color(c.callout_warning_bg),
        css_color(c.callout_warning_border),
        css_color(c.callout_caution_bg),
        css_color(c.callout_caution_border),
        body_font_stack(),
        t.text_size,
        t.text_line_height,
        document_layout_css(),
        css_font_weight(&t.h1_weight),
        css_color(c.text_h1),
        t.h1_size,
        css_color(c.text_h2),
        t.h2_size,
        css_color(c.text_h3),
        t.h3_size,
        css_color(c.text_h4),
        t.h4_size,
        css_color(c.text_h5),
        t.h5_size,
        css_color(c.text_h6),
        t.h6_size,
        d.callout_radius,
        "\"SFMono-Regular\", Consolas, \"Liberation Mono\", Menlo, monospace",
        t.code_size,
        pre_overflow,
        d.code_bg_radius,
        media_overflow,
        media_overflow
    )
}

pub(super) fn chromium_pdf_theme_css(theme: &Theme) -> String {
    let mut css = theme_css(theme);
    css = css.replace(
        document_layout_css(),
        ".vlt-document {\n  width: auto;\n  max-width: none;\n  margin: 0;\n  padding: 0;\n}",
    );
    css.push_str(
        r#"

@page {
  size: A4;
  margin: 15mm;
}

@media print {
  html,
  body {
    background-color: var(--vlt-bg);
    border: 0;
    outline: 0;
    box-shadow: none;
    print-color-adjust: exact;
    -webkit-print-color-adjust: exact;
  }

  .vlt-document {
    width: auto;
    max-width: none;
    margin: 0;
    padding: 0;
    border: 0;
    outline: 0;
    box-shadow: none;
  }

  pre,
  code {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  img,
  svg {
    max-width: 100%;
    height: auto;
    break-inside: avoid;
  }

  table,
  blockquote,
  pre,
  .vlt-math,
  .vlt-mermaid {
    break-inside: avoid;
  }
}
"#,
    );
    css
}

fn body_font_stack() -> &'static str {
    "system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", \"Noto Serif Tibetan\", \"Noto Sans Tibetan\", \"Microsoft Himalaya\", Kailasa, \"BabelStone Tibetan\", sans-serif"
}

fn document_layout_css() -> &'static str {
    ".vlt-document {\n  width: min(100% - 48px, 920px);\n  margin: 0 auto;\n  padding: 48px 0 72px;\n}"
}

pub(super) fn contains_tibetan_text(text: &str) -> bool {
    text.chars()
        .any(|ch| ('\u{0f00}'..='\u{0fff}').contains(&ch))
}

fn css_color(color: Hsla) -> String {
    let color = Rgba::from(color);
    format!(
        "rgba({},{},{},{:.3})",
        css_color_channel(color.r),
        css_color_channel(color.g),
        css_color_channel(color.b),
        color.a.clamp(0.0, 1.0)
    )
}

fn css_color_channel(channel: f32) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn css_font_weight(weight: &FontWeightDef) -> u16 {
    match weight {
        FontWeightDef::Thin => 100,
        FontWeightDef::Light => 300,
        FontWeightDef::Normal => 400,
        FontWeightDef::Medium => 500,
        FontWeightDef::Semibold => 600,
        FontWeightDef::Bold => 700,
        FontWeightDef::Extrabold => 800,
        FontWeightDef::Black => 900,
    }
}

pub(super) fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
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
