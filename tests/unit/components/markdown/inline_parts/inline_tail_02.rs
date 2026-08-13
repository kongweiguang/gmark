// @author kongweiguang

// Reason: keep round-trip regressions in a separate fixture so each source
// module remains below the hard size limit without changing test scope.
#[test]
fn source_to_rendered_round_trip_preserves_code_span() {
    // Simulate Source -> Rendered: raw markdown -> from_markdown parses it.
    let raw = "`123`";
    let tree = InlineTextTree::from_markdown(raw);
    assert_eq!(tree.visible_text(), "123");
    assert!(tree.render_cache().style_at(0).code);

    // Serialize back: must produce valid markdown.
    let serialized = tree.serialize_markdown();
    assert_eq!(serialized, "`123`");

    // Re-parse: must produce same result.
    let reparsed = InlineTextTree::from_markdown(&serialized);
    assert_eq!(reparsed.visible_text(), "123");
    assert!(reparsed.render_cache().style_at(0).code);
}

// Reason: guard the source display path against introducing an extra escape
// layer when a code span is serialized and parsed more than once.
#[test]
fn raw_text_with_backticks_not_double_escaped() {
    // Simulate the Source block's display_text() path.
    let raw = "`123`";
    // display_text() returns raw text as-is; from_markdown re-parses.
    let parsed = InlineTextTree::from_markdown(raw);
    assert_eq!(parsed.visible_text(), "123");

    // A second round-trip should NOT escape or double the backticks.
    let serialized = parsed.serialize_markdown();
    assert_eq!(serialized, "`123`");
    let reparsed = InlineTextTree::from_markdown(&serialized);
    assert_eq!(reparsed.visible_text(), "123");
}

// Reason: preserve literal escaped backticks so they cannot be mistaken for
// code delimiters during inline rendering.
#[test]
fn escaped_backtick_in_code() {
    let tree = InlineTextTree::from_markdown("\\`not code\\`");
    assert_eq!(tree.visible_text(), "`not code`");
    // Escaped backticks are literal, not code delimiters.
    let cache = tree.render_cache();
    assert!(!cache.style_at(0).code);
    assert_eq!(tree.serialize_markdown(), "\\`not code\\`");
}
