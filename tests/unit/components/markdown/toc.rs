// @author kongweiguang

use super::{collect_toc_entries, heading_slug, is_toc_marker};

#[test]
fn collects_source_headings_with_stable_unicode_slugs() {
    let entries = collect_toc_entries(
        "---\ntitle: ignored\n---\n# 你好 **gmark**\n## 你好 gmark\nTitle\n-----\n```md\n# ignored\n```",
    );
    assert_eq!(
        entries
            .iter()
            .map(|entry| (&entry.title, &entry.slug))
            .collect::<Vec<_>>(),
        vec![
            (&"你好 gmark".to_string(), &"你好-gmark".to_string()),
            (&"你好 gmark".to_string(), &"你好-gmark-1".to_string()),
            (&"Title".to_string(), &"title".to_string()),
        ]
    );
}

#[test]
fn recognizes_only_standalone_marker_and_normalizes_slug() {
    assert!(is_toc_marker("  [TOC]  "));
    assert!(!is_toc_marker("text [TOC]"));
    assert_eq!(heading_slug(" A / B! "), "a-b");
}
