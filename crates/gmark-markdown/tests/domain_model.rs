// @author kongweiguang

use std::path::Path;

use gmark_markdown::{
    BlockKind, HtmlDiagnosticKind, HtmlFallbackReason, HtmlParserKind, HtmlRenderLimits,
    HtmlSafety, InlineKind, LineEnding, LineEndingSummary, ResourceKind, ResourceLocation,
    ResourceRecord, SerializationMode, SourceRange, TableAlignment, parse_markdown,
    sanitize_html_for_export,
};

#[test]
fn parses_block_inline_table_resource_and_toc_values() {
    let source = concat!(
        "# Overview\n\n",
        "A **strong** [link](https://example.test) and [movie](clip.mp4 \"gmark:resource\").\n\n",
        "- [x] completed\n",
        "- [ ] pending\n\n",
        "| Name | Score |\n",
        "| :--- | ---: |\n",
        "| Ada | `42` |\n\n",
        "> [!NOTE]\n",
        "> quoted text\n"
    );
    let document = parse_markdown(source);

    assert!(
        document
            .blocks
            .iter()
            .any(|block| matches!(block.kind, BlockKind::Heading(_)))
    );
    assert!(
        document.blocks.iter().any(|block| {
            matches!(block.kind, BlockKind::Table(ref table)
                if table.alignments == vec![TableAlignment::Left, TableAlignment::Right]
                    && table.header.len() == 2
                    && table.rows.len() == 1)
        }),
        "unexpected table event stream: {:#?}",
        document.events
    );
    assert!(document.blocks.iter().any(|block| {
        matches!(block.kind, BlockKind::List(_))
            && block
                .children
                .iter()
                .any(|item| item.task_state() == Some(true))
            && block
                .children
                .iter()
                .any(|item| item.task_state() == Some(false))
    }));
    assert!(
        document
            .blocks
            .iter()
            .any(|block| { matches!(block.kind, BlockKind::BlockQuote { callout: Some(_) }) })
    );

    let paragraph = document
        .blocks
        .iter()
        .find(|block| matches!(block.kind, BlockKind::Paragraph))
        .expect("test fixture has a paragraph");
    assert!(paragraph.resource.is_none());
    assert!(
        paragraph
            .inlines
            .iter()
            .any(|inline| matches!(inline.kind, InlineKind::Strong))
    );
    assert!(
        paragraph
            .inlines
            .iter()
            .any(|inline| matches!(inline.kind, InlineKind::Link(_)))
    );

    let toc = document.toc();
    assert_eq!(toc.entries.len(), 1);
    assert_eq!(toc.entries[0].title, "Overview");
    assert_eq!(toc.entries[0].id, "overview");
}

#[test]
fn detects_standalone_resource_and_refuses_unsafe_opening() {
    let local = ResourceRecord::parse(
        "[movie](assets/clip.MP4 \"gmark:resource\")",
        Some(Path::new("/workspace/document")),
    )
    .expect("valid standalone resource syntax");
    assert_eq!(local.kind, ResourceKind::Video);
    assert!(local.is_local());
    assert_eq!(
        local.local_path(),
        Some(Path::new("/workspace/document/assets/clip.MP4"))
    );

    let unsafe_url = ResourceRecord::parse(
        "[click](javascript:alert(1) \"gmark:resource;type=file\")",
        None,
    )
    .expect("resource syntax remains valid even for an unsafe URL");
    assert!(unsafe_url.is_unsafe_url());
    assert!(matches!(unsafe_url.location, ResourceLocation::Url(_)));

    let document = parse_markdown("[clip](clip.mp4 \"gmark:resource\")");
    assert_eq!(document.blocks.len(), 1);
    assert!(document.blocks[0].resource.is_some());
    assert_eq!(
        document.blocks[0]
            .resource
            .as_ref()
            .map(|resource| resource.kind),
        Some(ResourceKind::Video)
    );
}

#[test]
fn resource_url_policy_matches_the_existing_editor_contract() {
    let record = ResourceRecord::parse("[legacy](vbscript:legacy \"gmark:resource\")", None)
        .expect("a custom URL syntax remains representable");

    assert!(!record.is_unsafe_url());
}

#[test]
fn html_is_value_only_sanitized_and_has_feature_compatible_fallback() {
    let source =
        "<div class=\"safe\">ok</div>\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>";
    let document = parse_markdown(source);
    let html = document
        .blocks
        .iter()
        .filter_map(|block| match &block.kind {
            BlockKind::Html(document) => Some(document),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(html.len(), 3);
    assert!(html[0].is_semantic());
    assert_eq!(html[0].parser, HtmlParserKind::Html5ever);
    assert_eq!(html[1].safety, HtmlSafety::Unsafe);
    assert_eq!(html[2].safety, HtmlSafety::Unsafe);

    let exported = sanitize_html_for_export("<img src=x onerror=alert(1)><script>x()</script>");
    assert!(!exported.to_ascii_lowercase().contains("onerror"));
    assert!(!exported.to_ascii_lowercase().contains("<script"));
}

#[test]
fn html_export_sanitization_rejects_styles_urls_events_and_nested_active_content() {
    let styled = sanitize_html_for_export(
        "<span style=\"color:blue; background-image:url(javascript:bad); background-color:rgb(255 255 0); font-size:120%\">x</span>",
    );
    assert!(!styled.to_ascii_lowercase().contains("background-image"));
    assert!(!styled.to_ascii_lowercase().contains("javascript"));

    let script = sanitize_html_for_export("<script style=\"color:blue\">alert(1)</script>");
    assert!(!script.to_ascii_lowercase().contains("<script"));

    let unsafe_url = sanitize_html_for_export("<a href=\"java&#x73;cript:alert(1)\">bad</a>");
    assert!(!unsafe_url.to_ascii_lowercase().contains("javascript"));
    assert!(!unsafe_url.to_ascii_lowercase().contains("href="));

    let nested = sanitize_html_for_export(
        "<div><span title=\"ok\" onclick=\"alert(1)\">safe</span><script>bad()</script></div>",
    );
    assert!(!nested.to_ascii_lowercase().contains("onclick"));
    assert!(!nested.to_ascii_lowercase().contains("<script"));
}

#[test]
fn code_literals_never_create_html_render_trees() {
    let fixtures = [
        "```html\n<div>fenced</div>\n```",
        "~~~html\n<div>tilde</div>\n~~~",
        "```HTML\n<div>uppercase</div>\n```",
        "~~~HtMl\n<div>mixed case</div>\n~~~",
        "- ```html\n  <div>list</div>\n  ```",
        "> ```html\n> <div>quote</div>\n> ```",
        "    <div>indented</div>",
        "`<div>inline</div>`,",
    ];

    for source in fixtures {
        let document = parse_markdown(source);
        assert_no_html_render_trees(&document.blocks, source);
    }
}

fn assert_no_html_render_trees(blocks: &[gmark_markdown::Block], source: &str) {
    for block in blocks {
        if let BlockKind::Html(document) = &block.kind {
            assert!(
                document.render_status.tree().is_none(),
                "HTML render tree created for code literal: {source:?}"
            );
        }
        assert_no_inline_html_render_trees(&block.inlines, source);
        assert_no_html_render_trees(&block.children, source);
    }
}

fn assert_no_inline_html_render_trees(inlines: &[gmark_markdown::Inline], source: &str) {
    for inline in inlines {
        if let InlineKind::Html(document) = &inline.kind {
            assert!(
                document.render_status.tree().is_none(),
                "inline HTML render tree created for code literal: {source:?}"
            );
        }
        assert_no_inline_html_render_trees(&inline.children, source);
    }
}

#[test]
fn source_mapping_preserves_bom_unicode_and_mixed_line_endings() {
    let source = "\u{feff}# 中文标题\r\n\r\nemoji 😀 paragraph\n- [x] 完成\r";
    let document = parse_markdown(source);
    assert!(document.format.has_utf8_bom);
    assert_eq!(
        document.format.line_ending_summary(),
        LineEndingSummary::Mixed
    );
    assert_eq!(
        document.format.line_endings,
        vec![
            LineEnding::CrLf,
            LineEnding::CrLf,
            LineEnding::Lf,
            LineEnding::Cr
        ]
    );
    document
        .source_map
        .validate(&document.source)
        .expect("all parser and block ranges are valid UTF-8 boundaries");
    assert!(
        document
            .source_map
            .event_ranges()
            .iter()
            .all(|range| range.end <= source.len())
    );
    assert!(
        document
            .blocks
            .iter()
            .all(|block| document.source_slice(block.source).is_ok())
    );
    assert_eq!(document.to_markdown(), source);

    let normalized = document
        .format
        .normalize(source)
        .expect("matching format snapshot normalizes source");
    assert_eq!(normalized, "# 中文标题\n\nemoji 😀 paragraph\n- [x] 完成\n");
    assert_eq!(
        document
            .format
            .restore(&normalized)
            .expect("unchanged line count restores exact original bytes"),
        source
    );
}

#[test]
fn parsed_serialization_is_byte_exact_and_canonical_mode_uses_values() {
    let source =
        "## Same\r\n\r\n[ref][id]\r\n\r\n[id]: https://example.test \"Title\"\r\n\r\n$$x^2$$\r\n";
    let document = parse_markdown(source);
    assert_eq!(document.to_markdown(), source);
    assert_eq!(
        gmark_markdown::MarkdownSerializer::new(SerializationMode::PreserveSource)
            .serialize(&document),
        source
    );

    let canonical = document.to_canonical_markdown();
    assert!(canonical.contains("## Same"));
    assert!(canonical.contains("[ref][id]"));
    assert_ne!(canonical, "");
}

#[test]
fn canonical_table_serialization_retains_inline_values() {
    let document = parse_markdown(
        "| Name | Link |\n| --- | ---: |\n| **Ada** | [site](https://example.test) |",
    );
    let table = document
        .blocks
        .iter()
        .find_map(|block| match &block.kind {
            BlockKind::Table(table) => Some(table),
            _ => None,
        })
        .expect("fixture contains a table");

    assert_eq!(
        gmark_markdown::serialize_table_canonical(table),
        "| Name | Link |\n| --- | ---: |\n| **Ada** | [site](https://example.test) |"
    );
}

#[test]
fn html5ever_decodes_entities_and_builds_tree() {
    let document = gmark_markdown::HtmlDocument::parse("<p title='x'>&lt;safe&gt; &amp; text</p>");
    assert!(document.render_status.is_renderable());
    let tree = document.render_status.tree().expect("tree");
    assert_eq!(tree.plain_text, "<safe> & text");
    assert_eq!(tree.roots[0].tag_name, "p");
}

#[test]
fn html5ever_repairs_an_omitted_closing_tag() {
    let document = gmark_markdown::HtmlDocument::parse("<p>repaired");
    let tree = document.render_status.tree().expect("HTML5 fragment tree");

    assert_eq!(tree.roots[0].tag_name, "p");
    assert_eq!(tree.plain_text, "repaired");
}

#[test]
fn table_cell_limit_is_applied_per_table() {
    let source = "<table><tr><td>a</td></tr></table><table><tr><td>b</td></tr></table>";
    let document = gmark_markdown::HtmlDocument::parse_with_limits(
        source,
        HtmlRenderLimits {
            max_table_cells: 1,
            ..HtmlRenderLimits::default()
        },
    );

    assert!(document.render_status.is_renderable());
    assert_eq!(
        document.render_status.tree().expect("tree").plain_text,
        "ab"
    );
}

#[test]
fn indented_html_fragment_is_classified_as_a_html_block() {
    let document = parse_markdown("# title\n\n<section>\n  <p>safe</p>\n</section>\n");

    assert!(document.blocks.iter().any(|block| {
        matches!(block.kind, BlockKind::Html(ref html) if html.render_status.is_renderable())
    }));
}

#[test]
fn smoke_fixture_html_block_shape() {
    let source = concat!(
        "# HTML render smoke\n\n",
        "<section>\n",
        "  <h2 style=\"color:#2563eb\">原生 HTML 渲染</h2>\n",
        "  <p><strong>安全文本</strong>、<em>斜体</em>、<code>inline</code>、<a href=\"https://example.com\">链接</a> 😀</p>\n",
        "  <blockquote>引用 <mark>高亮</mark></blockquote>\n",
        "  <details open><summary>展开详情</summary><p>可折叠内容</p></details>\n",
        "  <table><caption>表格</caption><thead><tr><th>列一</th><th>列二</th></tr></thead><tbody><tr><td>中文</td><td>long-url-example.example/very-long-word</td></tr></tbody></table>\n",
        "  <script>alert('blocked')</script><p>危险节点后的安全兄弟</p>\n",
        "</section>\n"
    );
    let document = parse_markdown(source);
    assert!(
        document
            .blocks
            .iter()
            .any(|block| matches!(block.kind, BlockKind::Html(_)))
    );
}

#[test]
fn same_line_section_fragment_is_renderable() {
    let document = gmark_markdown::HtmlDocument::parse(
        "<section><h2>Heading</h2><p><strong>safe</strong></p></section>",
    );
    assert!(document.render_status.is_renderable());
    assert_eq!(
        document.render_status.tree().unwrap().roots[0].tag_name,
        "section"
    );
}

#[test]
fn dangerous_nodes_are_removed_but_safe_siblings_remain() {
    let document =
        gmark_markdown::HtmlDocument::parse("<div>safe<script>alert(1)</script>tail</div>");
    assert!(document.is_unsafe());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.kind == HtmlDiagnosticKind::BlockedTag
            && diagnostic.tag.as_deref() == Some("script")
    }));
    assert_eq!(
        document.render_status.tree().expect("tree").plain_text,
        "safetail"
    );
    assert!(
        !document
            .sanitized_html
            .to_ascii_lowercase()
            .contains("<script")
    );
}

#[test]
fn encoded_dangerous_url_is_rejected() {
    let document =
        gmark_markdown::HtmlDocument::parse("<a href='java&#x73;cript:alert(1)'>bad</a>");
    assert!(document.is_unsafe());
    assert!(
        document
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind == HtmlDiagnosticKind::InvalidUrl)
    );
    assert!(
        !document
            .sanitized_html
            .to_ascii_lowercase()
            .contains("javascript:")
    );
}

#[test]
fn limits_fall_back_without_panicking() {
    let document = gmark_markdown::HtmlDocument::parse_with_limits(
        "<div><span>x</span></div>",
        HtmlRenderLimits {
            max_nodes: 1,
            ..HtmlRenderLimits::default()
        },
    );
    assert_eq!(
        document.render_status,
        gmark_markdown::HtmlRenderStatus::Fallback(HtmlFallbackReason::ResourceLimit)
    );
    assert_eq!(document.raw_source, "<div><span>x</span></div>");
}

#[test]
fn export_sanitization_is_idempotent() {
    let once = sanitize_html_for_export("<span style='color:blue' onclick='bad()'>x</span>");
    let twice = sanitize_html_for_export(&once);
    assert_eq!(once, twice);
    assert!(!once.contains("onclick"));
}

#[test]
fn source_range_rejects_invalid_external_boundaries() {
    assert!(SourceRange::new(8, 2).is_err());
    let range = SourceRange::new(1, 2).expect("ordered range");
    assert!(range.slice("😀").is_err());
}
