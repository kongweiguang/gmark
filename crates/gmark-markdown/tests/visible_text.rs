// @author kongweiguang

use gmark_markdown::{Replaceability, VisibleTextKind, parse_markdown};

#[test]
fn projects_visible_text_without_markdown_markers() {
    let document =
        parse_markdown("# Title\n\nA **bold** [link](https://example.test) ![alt](image.png)");
    let projection = document.visible_text_projection();
    assert_eq!(projection.text, "Title\nA bold link alt");
    assert!(!projection.text.contains("**"));
    assert!(projection.segments.iter().any(|segment| {
        segment.kind == VisibleTextKind::LinkLabel
            && segment.replaceability == Replaceability::Direct
    }));
}

#[test]
fn encoded_text_is_searchable_but_not_replaceable() {
    let document = parse_markdown("A &amp; B");
    let projection = document.visible_text_projection();
    assert_eq!(projection.text, "A & B");
    assert_eq!(projection.source_range_for_visible(2..3), None);
}

#[test]
fn table_cells_are_projected_in_render_order() {
    let document = parse_markdown("| A | B |\n|---|---|\n| C | D |");
    let projection = document.visible_text_projection();
    assert_eq!(projection.text, "A\tB\nC\tD");
    assert!(projection.segments.iter().all(|segment| {
        segment.kind == VisibleTextKind::TableCell || segment.kind == VisibleTextKind::Separator
    }));
}

#[test]
fn heading_fold_contains_following_section_until_same_level() {
    let document = parse_markdown("# One\nbody\n## Child\nchild body\n# Two\nlast");
    let projection = document.visible_text_projection();
    assert_eq!(projection.text, "One\nbody\nChild\nchild body\nTwo\nlast");
    let headings = projection
        .folds
        .iter()
        .filter(|fold| fold.heading_level.is_some())
        .collect::<Vec<_>>();
    assert_eq!(headings.len(), 3);
    assert_eq!(
        &projection.text[headings[0].body.clone()],
        "\nbody\nChild\nchild body"
    );
    assert_eq!(&projection.text[headings[1].body.clone()], "\nchild body");
}

#[test]
fn unicode_and_derived_ranges_are_conservative() {
    let document = parse_markdown("plain 😀&amp; `code` $x$");
    let projection = document.visible_text_projection();

    assert_eq!(projection.text, "plain 😀& code x");
    let plain_end = "plain 😀".len();
    assert_eq!(
        projection.source_range_for_visible(0..plain_end),
        Some(gmark_markdown::SourceRange::new(0, plain_end).unwrap())
    );

    let entity_start = projection.text.find('&').expect("decoded entity");
    assert_eq!(
        projection.source_range_for_visible(entity_start..entity_start + 1),
        None
    );
    assert_eq!(
        projection.source_bounds_for_visible(entity_start..entity_start + 1),
        Some(gmark_markdown::SourceRange::new(plain_end, plain_end + 5).unwrap())
    );

    let emoji_start = projection.text.find('😀').expect("emoji");
    assert_eq!(
        projection.source_range_for_visible(emoji_start + 1..emoji_start + 2),
        None,
        "a byte range splitting UTF-8 must never become a replacement range"
    );
    assert_eq!(
        projection
            .segments
            .iter()
            .find(|segment| segment.visible.contains(&entity_start))
            .map(|segment| segment.replaceability),
        Some(Replaceability::Derived)
    );
    assert!(
        projection
            .segments
            .iter()
            .any(|segment| segment.kind == VisibleTextKind::Code
                && segment.replaceability == Replaceability::Derived)
    );
    assert!(
        projection
            .segments
            .iter()
            .any(|segment| segment.kind == VisibleTextKind::Math
                && segment.replaceability == Replaceability::Derived)
    );
}

#[test]
fn semantic_fixture_covers_links_images_code_tasks_footnotes_callouts_math_and_mermaid() {
    let source = concat!(
        "# Unicode 你好 😀\n\n",
        "[link](https://example.test) ![alt](image.png) `inline`\n\n",
        "```rust\nlet value = 1;\n```\n\n",
        "- [x] done\n- [ ] pending\n\n",
        "> [!NOTE]\n> callout body\n\n",
        "formula $x^2$\n\n",
        "```mermaid\nflowchart LR\nA --> B\n```\n\n",
        "footnote[^a]\n\n[^a]: footnote body"
    );
    let projection = parse_markdown(source).visible_text_projection();

    for expected in [
        "Unicode 你好 😀",
        "link",
        "alt",
        "inline",
        "let value = 1;",
        "done",
        "pending",
        "callout body",
        "x^2",
        "flowchart LR",
        "A --> B",
        "footnote",
        "footnote body",
    ] {
        assert!(projection.text.contains(expected), "missing {expected:?}");
    }
    assert!(!projection.text.contains("https://example.test"));
    assert!(!projection.text.contains("image.png"));
    assert!(!projection.text.contains("[x]"));
    assert!(
        projection
            .segments
            .iter()
            .any(|segment| segment.kind == VisibleTextKind::LinkLabel)
    );
    assert!(
        projection
            .segments
            .iter()
            .any(|segment| segment.kind == VisibleTextKind::ImageAlt)
    );
    assert!(
        projection
            .segments
            .iter()
            .any(|segment| segment.kind == VisibleTextKind::Footnote)
    );
    assert!(
        projection
            .segments
            .iter()
            .any(|segment| segment.kind == VisibleTextKind::Math)
    );
    assert!(
        projection
            .segments
            .iter()
            .any(|segment| segment.kind == VisibleTextKind::Code)
    );
    assert!(
        projection.segments.iter().any(|segment| {
            segment.kind == VisibleTextKind::Derived
                && segment.replaceability == Replaceability::Derived
                && projection.text[segment.visible.clone()].contains("flowchart LR")
        }),
        "segments: {:#?}, text: {:?}",
        projection.segments,
        projection.text
    );
}

#[test]
fn dangerous_html_is_removed_while_sanitized_siblings_remain_searchable() {
    let document = parse_markdown("<div>safe<script>alert(1)</script>tail</div>");
    let projection = document.visible_text_projection();

    assert_eq!(projection.text, "safetail");
    assert!(!projection.text.contains("alert"));
    assert!(projection.segments.iter().any(|segment| {
        segment.kind == VisibleTextKind::Html && segment.replaceability == Replaceability::Derived
    }));
    assert_eq!(projection.source_range_for_visible(0..8), None);
}

#[test]
fn code_literals_keep_markup_like_text_out_of_the_html_block_filter() {
    let source = "```html\n<script>\nalert(1)\n</script>\n```";
    let projection = parse_markdown(source).visible_text_projection();

    assert_eq!(projection.text, "<script>\nalert(1)\n</script>\n");
    assert!(
        projection
            .segments
            .iter()
            .all(|segment| segment.kind == VisibleTextKind::Code)
    );
}

#[test]
fn callout_fold_covers_all_emitted_callout_text() {
    let projection = parse_markdown("> [!WARNING]\n> body\n>\n> more").visible_text_projection();
    let fold = projection
        .folds
        .iter()
        .find(|fold| fold.callout.is_some())
        .expect("callout fold");
    assert_eq!(&projection.text[fold.body.clone()], projection.text);
}
