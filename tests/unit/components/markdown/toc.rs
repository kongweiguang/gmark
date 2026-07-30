// @author kongweiguang

use super::{heading_slug, is_toc_marker};

#[test]
fn recognizes_only_standalone_marker_and_normalizes_slug() {
    assert!(is_toc_marker("  [TOC]  "));
    assert!(!is_toc_marker("text [TOC]"));
    assert_eq!(heading_slug(" A / B! "), "a-b");
}
