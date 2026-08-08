// @author kongweiguang

use gmark_export::{
    ClipboardSelection, ExportTheme, export_clipboard_fragment,
    render_chromium_pdf_html_with_base_dir, render_html,
};
use gmark_markdown::parse_markdown;

#[test]
fn fragment_uses_rendered_text_and_safe_html() {
    let fragment = export_clipboard_fragment(
        ClipboardSelection::Markdown {
            markdown: "**bold** <script>alert(1)</script>".to_owned(),
        },
        &ExportTheme::default(),
        None,
    );
    assert_eq!(fragment.plain_text, "bold ");
    assert!(!fragment.html.contains("<script"));
    assert!(fragment.html.contains("<strong>bold</strong>"));

    let unsafe_fragment = export_clipboard_fragment(
        ClipboardSelection::Markdown {
            markdown: concat!(
                "<a href=\"java&#x73;cript:alert(1)\" onclick=\"bad()\">danger</a>",
                "<span style=\"color:blue; background-image:url(javascript:bad)\">safe</span>"
            )
            .to_owned(),
        },
        &ExportTheme::default(),
        None,
    );
    let lower = unsafe_fragment.html.to_ascii_lowercase();
    assert!(!lower.contains("javascript:"));
    assert!(!lower.contains("onclick"));
    assert!(!lower.contains("background-image"));
    assert!(unsafe_fragment.html.contains(">danger</a>"));
    assert!(unsafe_fragment.html.contains(">safe</span>"));
}

#[test]
fn clipboard_uses_the_shared_projection_for_unicode_and_derived_values() {
    let markdown = "你好 😀 &amp; [link](https://example.test) ![alt](x.png) `code` $x$";
    let expected = parse_markdown(markdown).visible_text_projection().text;
    let fragment = export_clipboard_fragment(
        ClipboardSelection::Markdown {
            markdown: markdown.to_owned(),
        },
        &ExportTheme::default(),
        None,
    );

    assert_eq!(fragment.plain_text, expected);
    assert_eq!(fragment.plain_text, "你好 😀 & link alt code x");
    assert!(!fragment.plain_text.contains("https://example.test"));
    assert!(!fragment.plain_text.contains("x.png"));
}

#[test]
fn clipboard_keeps_generated_math_and_mermaid_html_while_blocking_raw_html() {
    let markdown = concat!(
        "before $x^2$ after\n\n",
        "```mermaid\nflowchart LR\nA --> B\n```\n\n",
        "<div>safe<script>alert(1)</script>tail</div>"
    );
    let fragment = export_clipboard_fragment(
        ClipboardSelection::Markdown {
            markdown: markdown.to_owned(),
        },
        &ExportTheme::default(),
        None,
    );

    assert!(fragment.html.contains("class=\"vlt-inline-math\""));
    assert!(fragment.html.contains("<svg"));
    assert!(fragment.html.contains("class=\"vlt-mermaid\""));
    assert!(fragment.html.contains("alt=\"Mermaid diagram\""));
    assert!(fragment.html.contains("data:image/svg+xml;base64,"));
    assert!(!fragment.html.to_ascii_lowercase().contains("<script"));
    assert!(fragment.html.contains("&lt;script&gt;"));
    assert!(fragment.plain_text.contains("safetail"));
    assert!(!fragment.plain_text.contains("alert"));
}

#[test]
fn table_plain_flavor_remains_tsv() {
    let fragment = export_clipboard_fragment(
        ClipboardSelection::Table {
            markdown: "| A | B |\n|---|---|\n| C | D |".to_owned(),
            tsv: "A\tB\nC\tD".to_owned(),
        },
        &ExportTheme::default(),
        None,
    );
    assert_eq!(fragment.plain_text, "A\tB\nC\tD");
    assert!(fragment.html.contains("<table>"));
}

#[test]
fn html_pdf_and_clipboard_share_observable_semantic_fixture() {
    let markdown = concat!(
        "# 你好 😀\n\n",
        "A **bold** [link](https://example.test) ![alt](image.png)\n\n",
        "| A | B |\n|---|---|\n| C | D |\n\n",
        "- [x] done\n\n",
        "> [!NOTE]\n> callout body\n\n",
        "formula $x^2$"
    );
    let projection = parse_markdown(markdown).visible_text_projection();
    let clipboard = export_clipboard_fragment(
        ClipboardSelection::Markdown {
            markdown: markdown.to_owned(),
        },
        &ExportTheme::default(),
        None,
    );
    let html = render_html(markdown, &ExportTheme::default(), "Doc");
    let pdf_html =
        render_chromium_pdf_html_with_base_dir(markdown, &ExportTheme::default(), "Doc", None);

    assert_eq!(clipboard.plain_text, projection.text);
    for expected in [
        "你好 😀",
        "bold",
        "link",
        "alt",
        "A",
        "B",
        "C",
        "D",
        "done",
        "callout body",
    ] {
        assert!(html.contains(expected), "HTML missing {expected:?}");
        assert!(pdf_html.contains(expected), "PDF HTML missing {expected:?}");
    }
    assert!(projection.text.contains("x^2"));
    assert!(clipboard.plain_text.contains("x^2"));
    assert!(html.contains("class=\"vlt-inline-math\""));
    assert!(pdf_html.contains("class=\"vlt-inline-math\""));
    assert!(clipboard.html.contains("<strong>bold</strong>"));
    assert!(clipboard.html.contains("<table>"));
}
