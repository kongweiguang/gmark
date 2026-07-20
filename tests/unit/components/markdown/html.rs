// @author kongweiguang

use super::*;

#[test]
fn safe_inline_html_classifies_as_semantic() {
    let doc = parse_html_document("<span style='color:blue;'>Blue</span>");
    assert!(doc.is_semantic());
    assert_eq!(doc.nodes[0].tag_name, "span");
    assert_eq!(doc.raw_source, "<span style='color:blue;'>Blue</span>");
}

#[test]
fn risky_tag_classifies_as_raw_text() {
    let doc = parse_html_document("<script>alert(1)</script>");
    assert_eq!(doc.safety, HtmlSafetyClass::RawTextBlock);
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
fn html_image_zoom_ignores_other_style_declarations() {
    let image = parse_html_image_block(
        "<img src=\"a.png\" alt=\"a\" style=\"color:red; zoom: 120%; width:10px\" />",
    )
    .expect("html image");

    assert_eq!(image.zoom, 1.2);
    assert_eq!(
        image.to_sanitized_html_with_src("a.png"),
        "<img src=\"a.png\" alt=\"a\" style=\"zoom: 120%;\">"
    );
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
    assert!(
        div.children
            .iter()
            .any(|child| child.kind == HtmlNodeKind::RawTextBlock)
    );
}

#[test]
fn malformed_html_falls_back_to_raw_text() {
    let doc = parse_html_document("<details><summary>x</details>");
    assert_eq!(doc.safety, HtmlSafetyClass::RawTextBlock);
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

#[test]
fn export_sanitizes_style_to_whitelisted_declarations() {
    let html = sanitize_html_for_export(
        "<span style=\"color:blue; background-image:url(javascript:bad); background-color:rgb(255 255 0); font-size:120%\">x</span>",
    );

    assert!(html.contains(
            "style=\"color: rgba(0,0,255,1.000); background-color: rgba(255,255,0,1.000); font-size: 120%;\""
        ));
    assert!(!html.contains("background-image"));
}

#[test]
fn export_escapes_risky_html_even_when_style_is_present() {
    let html = sanitize_html_for_export("<script style=\"color:blue\">alert(1)</script>");

    assert!(html.contains("&lt;script style=&quot;color:blue&quot;&gt;alert(1)&lt;/script&gt;"));
    assert!(!html.contains("<script"));
}

#[test]
fn export_sanitizer_decodes_entities_before_validating_url_schemes() {
    let html = sanitize_html_for_export("<a href=\"java&#x73;cript:alert(1)\">bad</a>");

    assert_eq!(html, "<a>bad</a>");
    assert!(!html.contains("javascript"));
    assert!(!html.contains("href"));
}

#[test]
fn export_sanitizer_rejects_event_attributes_and_nested_unsafe_content() {
    let html = sanitize_html_for_export(
        "<div><span title=\"ok\" onclick=\"alert(1)\">safe</span><script>bad()</script></div>",
    );

    assert!(html.contains("title=\"ok\""));
    assert!(!html.contains("<span title=\"ok\" onclick="));
    assert!(html.contains("onclick"));
    assert!(!html.contains("<script"));
    assert!(html.contains("&lt;script&gt;bad()&lt;/script&gt;"));
}
