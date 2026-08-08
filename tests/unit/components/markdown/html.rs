// @author kongweiguang

use super::*;

#[test]
fn safe_inline_html_classifies_as_semantic() {
    let doc = parse_html_document("<span style='color:blue;'>Blue</span>");
    assert!(doc.is_semantic());
    assert!(doc.markdown_value().is_semantic());
    assert_eq!(doc.nodes[0].tag_name, "span");
    assert_eq!(doc.raw_source, "<span style='color:blue;'>Blue</span>");
}

#[test]
fn risky_tag_classifies_as_raw_text() {
    let doc = parse_html_document("<script>alert(1)</script>");
    assert_eq!(doc.safety, HtmlSafetyClass::RawTextBlock);
    assert!(doc.markdown_value().is_unsafe());
    assert!(doc.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == gmark_markdown::HtmlDiagnosticKind::BlockedTag
            && diagnostic.tag.as_deref() == Some("script")
    }));
    assert_eq!(doc.nodes[0].raw_source, "<script>alert(1)</script>");
}

#[test]
fn dangerous_attribute_classifies_as_raw_text() {
    let doc = parse_html_document("<a href=\"javascript:alert(1)\">bad</a>");
    assert_eq!(doc.safety, HtmlSafetyClass::RawTextBlock);
}

#[test]
fn parses_standalone_html_image_block() {
    let image = parse_html_image_block(
        "<img src=\"./xxx/abc.png\" alt=\"alt text\" style=\"zoom:80%;\" />",
    )
    .expect("html image");

    assert_eq!(image.src, "./xxx/abc.png");
    assert_eq!(image.alt, "alt text");
    assert_eq!(image.zoom, 0.8);
}

#[test]
fn invalid_html_image_blocks_are_not_images() {
    assert!(parse_html_image_block("<img alt=\"missing src\" />").is_none());
    assert!(parse_html_image_block("<img src=\"\" />").is_none());
    assert!(parse_html_image_block("<span><img src=\"x.png\" /></span>").is_none());
}

#[test]
fn risky_child_is_local_raw_inside_safe_parent() {
    let doc = parse_html_document("<div>safe<script>alert(1)</script>tail</div>");
    assert!(doc.is_semantic());
    let div = &doc.nodes[0];
    assert_eq!(
        div.children
            .iter()
            .filter(|child| child.tag_name == "#text")
            .map(|child| child.raw_source.as_str())
            .collect::<String>(),
        "safetail"
    );
    assert!(doc.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == gmark_markdown::HtmlDiagnosticKind::BlockedTag
            && diagnostic.tag.as_deref() == Some("script")
    }));
}

#[test]
fn html5ever_repairs_malformed_html_nesting() {
    let doc = parse_html_document("<details><summary>x</details>");
    assert!(doc.is_semantic());
    assert_eq!(
        doc.markdown_value().parser,
        gmark_markdown::HtmlParserKind::Html5ever
    );
    assert_eq!(
        doc.markdown_value()
            .render_status
            .tree()
            .unwrap()
            .plain_text,
        "x"
    );
}

#[test]
fn html5ever_repairs_an_omitted_block_closing_tag() {
    let doc = parse_html_document("<div>repaired");

    assert!(doc.is_semantic());
    assert_eq!(doc.nodes[0].tag_name, "div");
    assert_eq!(doc.nodes[0].children[0].raw_source, "repaired");
}

#[test]
fn same_line_section_block_is_semantic_in_editor_projection() {
    let doc =
        parse_html_document("<section><h2>Heading</h2><p><strong>safe</strong></p></section>");

    assert!(doc.is_semantic(), "diagnostics: {:?}", doc.diagnostics);
    assert_eq!(doc.nodes[0].tag_name, "section");
}

#[test]
fn dangerous_event_attributes_keep_safe_container_content() {
    let doc = parse_html_document("<div onclick=alert(1)>safe</div>");

    assert!(doc.is_semantic());
    assert_eq!(doc.nodes[0].children[0].raw_source, "safe");
    assert!(doc.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == gmark_markdown::HtmlDiagnosticKind::BlockedAttribute
    }));
}

#[test]
fn html_section_with_safe_siblings_stays_semantic() {
    let source = concat!(
        "<section>\n",
        "  <h2 style=\"color:#2563eb\">原生 HTML 渲染</h2>\n",
        "  <p><strong>安全文本</strong>、<em>斜体</em>、<code>inline</code>、<a href=\"https://example.com\">链接</a> 😀</p>\n",
        "  <blockquote>引用 <mark>高亮</mark></blockquote>\n",
        "  <details open><summary>展开详情</summary><p>可折叠内容</p></details>\n",
        "  <table><caption>表格</caption><thead><tr><th>列一</th><th>列二</th></tr></thead><tbody><tr><td>中文</td><td>long-url-example.example/very-long-word</td></tr></tbody></table>\n",
        "  <script>alert('blocked')</script><p>危险节点后的安全兄弟</p>\n",
        "</section>"
    );
    let doc = parse_html_document(source);

    assert!(doc.is_semantic(), "diagnostics: {:?}", doc.diagnostics);
    assert_eq!(doc.nodes[0].tag_name, "section");
    assert!(parse_html_document(&format!("{source}\n")).is_semantic());
}

#[test]
fn parses_whitelisted_style_color_background_and_font_size() {
    let doc = parse_html_document(
        "<span style=\"color:blue; background-color:#fff8; font-size:20px\">x</span>",
    );
    let style = style_for_node(&doc.nodes[0]);

    assert_eq!(
        style.color,
        Some(HtmlCssColor::Rgba(HtmlCssRgba {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 1.0,
        }))
    );
    assert_eq!(
        style.background_color,
        Some(HtmlCssColor::Rgba(HtmlCssRgba {
            red: 255,
            green: 255,
            blue: 255,
            alpha: 0.53333336,
        }))
    );
    assert_eq!(style.font_size, Some(HtmlCssFontSize::Px(20.0)));
}

#[test]
fn parses_rgb_hsl_currentcolor_and_font_size_units() {
    let doc = parse_html_document(
        "<span style=\"color:rgba(255, 0, 0, .5); background-color:hsl(120 100% 50% / 25%); font-size:1.25em\">x</span>",
    );
    let style = style_for_node(&doc.nodes[0]);
    assert_eq!(
        style.color,
        Some(HtmlCssColor::Rgba(HtmlCssRgba {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 0.5,
        }))
    );
    assert_eq!(
        style.background_color,
        Some(HtmlCssColor::Rgba(HtmlCssRgba {
            red: 0,
            green: 255,
            blue: 0,
            alpha: 0.25,
        }))
    );
    assert_eq!(style.font_size, Some(HtmlCssFontSize::Em(1.25)));

    let doc = parse_html_document(
        "<span style=\"color:currentColor; font-size:120%; background-color:transparent\">x</span>",
    );
    let style = style_for_node(&doc.nodes[0]);
    assert_eq!(style.color, Some(HtmlCssColor::CurrentColor));
    assert_eq!(style.font_size, Some(HtmlCssFontSize::Percent(120.0)));
    assert_eq!(
        style.background_color,
        Some(HtmlCssColor::Rgba(HtmlCssRgba {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0.0,
        }))
    );

    let doc = parse_html_document("<span style=\"font-size:large\">x</span>");
    assert_eq!(
        style_for_node(&doc.nodes[0]).font_size,
        Some(HtmlCssFontSize::Keyword(HtmlCssFontSizeKeyword::Large))
    );
}

#[test]
fn ignores_unrecognized_or_invalid_style_declarations() {
    let doc = parse_html_document(
        "<span style=\"background-image:url(javascript:bad); color:not-a-real-color; font-size:-1px\">x</span>",
    );
    let style = style_for_node(&doc.nodes[0]);
    assert_eq!(style, HtmlInlineStyle::default());
    assert!(doc.is_semantic());
}
