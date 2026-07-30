// @author kongweiguang

#[test]
fn minimal_projection_edit_keeps_utf8_common_prefix_and_suffix() {
    let edit = Editor::minimal_projection_edit("标题 alpha 结尾", "标题 beta 结尾")
        .expect("changed text should produce an edit");
    assert_eq!(edit.range(), &(7..11));
    assert_eq!(edit.replacement(), "bet");

    let insertion = Editor::minimal_projection_edit("前后", "前中后")
        .expect("insertion should produce an edit");
    assert_eq!(insertion.range(), &(3..3));
    assert_eq!(insertion.replacement(), "中");

    let deletion =
        Editor::minimal_projection_edit("前中后", "前后").expect("deletion should produce an edit");
    assert_eq!(deletion.range(), &(3..6));
    assert_eq!(deletion.replacement(), "");
    assert!(Editor::minimal_projection_edit("相同", "相同").is_none());
}

#[test]
fn prepared_projection_uses_stable_snapshot_and_preserves_lines() {
    let mut document = gmark_document::SourceDocument::new("alpha\n中文\n");
    let snapshot = document.snapshot();
    document
        .apply_transaction(gmark_document::Transaction::new(
            document.revision(),
            vec![gmark_document::TextEdit::new(0..5, "changed")],
        ))
        .expect("newer edit should apply");

    let prepared = PreparedSplitProjection::from_snapshot(snapshot);
    assert_eq!(prepared.revision, gmark_document::Revision::INITIAL);
    assert_eq!(prepared.lines, ["alpha", "中文", ""]);
    assert_eq!(prepared.regions.len(), 2);
    assert_eq!(prepared.regions[0].kind, ProjectionRegionKind::Paragraph);
    assert_eq!(prepared.regions[0].lines, 0..2);
    assert_eq!(prepared.regions[0].bytes, 0..12);
    assert_eq!(prepared.regions[1].kind, ProjectionRegionKind::Blank);
    assert_eq!(prepared.regions[1].lines, 2..3);
    assert_eq!(prepared.regions[1].bytes, 13..13);
    assert_eq!(document.text(), "changed\n中文\n");

    let empty =
        PreparedSplitProjection::from_snapshot(gmark_document::SourceDocument::new("").snapshot());
    assert_eq!(empty.lines, [""]);
    assert_eq!(empty.regions[0].kind, ProjectionRegionKind::Blank);
    assert_eq!(empty.regions[0].bytes, 0..0);
}

#[test]
fn prepared_projection_is_safe_to_share_with_background_workers() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PreparedSplitProjection>();
}

#[test]
fn prepared_projection_classifies_top_level_regions_without_losing_source_ranges() {
    let source =
        "# Title\n\n- one\n- two\n\n```rust\nfn main() {}\n```\n\n<!-- note -->\n\n$$\nx + y\n$$";
    let prepared = PreparedSplitProjection::from_snapshot(
        gmark_document::SourceDocument::new(source).snapshot(),
    );
    let kinds = prepared
        .regions
        .iter()
        .map(|region| region.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            ProjectionRegionKind::AtxHeading,
            ProjectionRegionKind::Blank,
            ProjectionRegionKind::List,
            ProjectionRegionKind::Blank,
            ProjectionRegionKind::FencedCode,
            ProjectionRegionKind::Blank,
            ProjectionRegionKind::Comment,
            ProjectionRegionKind::Blank,
            ProjectionRegionKind::DisplayMath,
        ]
    );

    for region in &prepared.regions {
        assert!(source.is_char_boundary(region.bytes.start));
        assert!(source.is_char_boundary(region.bytes.end));
        assert_eq!(
            &source[region.bytes.clone()],
            prepared.lines[region.lines.clone()].join("\n")
        );
    }
}

fn assert_incremental_projection_matches_full(
    previous_source: &str,
    current_source: &str,
) -> PreparedSplitProjection {
    let previous = PreparedSplitProjection::from_snapshot(
        gmark_document::SourceDocument::new(previous_source).snapshot(),
    );
    let incremental = PreparedSplitProjection::from_snapshot_incremental(
        gmark_document::SourceDocument::new(current_source).snapshot(),
        &previous,
    );
    let full = PreparedSplitProjection::from_snapshot(
        gmark_document::SourceDocument::new(current_source).snapshot(),
    );
    assert_eq!(incremental.source, full.source);
    assert_eq!(incremental.lines, full.lines);
    assert_eq!(incremental.regions, full.regions);
    let signatures = |prepared: &PreparedSplitProjection| {
        prepared
            .nodes
            .iter()
            .map(|nodes| {
                nodes.as_ref().map(|nodes| {
                    nodes
                        .iter()
                        .map(|node| (node.record.kind.clone(), node.record.markdown_line(0, None)))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(signatures(&incremental), signatures(&full));
    incremental
}
