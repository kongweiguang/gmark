// @author kongweiguang

use super::{
    is_valid_footnote_id, parse_footnote_definition_head, parse_inline_footnote_reference,
    superscript_ordinal,
};

#[test]
fn validates_and_parses_reference_footnote_syntax() {
    assert!(is_valid_footnote_id("long-note"));
    assert!(!is_valid_footnote_id("bad id"));
    assert_eq!(
        parse_inline_footnote_reference("[^ref-1]"),
        Some("ref-1".to_string())
    );
    assert_eq!(
        parse_footnote_definition_head("[^ref-1]: body"),
        Some(("ref-1".to_string(), "body".to_string()))
    );
}

#[test]
fn formats_superscript_ordinals() {
    assert_eq!(superscript_ordinal(1), "\u{00B9}");
    assert_eq!(superscript_ordinal(12), "\u{00B9}\u{00B2}");
}
