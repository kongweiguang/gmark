// @author kongweiguang

use gmark_source_tools::{
    FoldKind, IncrementalFoldParser, SourceLanguage, fold_ranges, fold_ranges_in_window,
};

#[test]
fn delimiter_folding_uses_real_byte_coordinates_and_ignores_strings() {
    let source = "fn main() {\n    let label = \"界 }\";\n    if true {\n        println!(\"ok\");\n    }\n}\n";
    let ranges = fold_ranges_in_window(SourceLanguage::Rust, source, 100, 10);
    let outer_start = source
        .find('{')
        .and_then(|offset| u64::try_from(offset).ok())
        .unwrap_or(0)
        + 100;
    let outer_end = source
        .rfind('}')
        .and_then(|offset| u64::try_from(offset.saturating_add(1)).ok())
        .unwrap_or(0)
        + 100;
    assert!(ranges.iter().any(|range| {
        range.kind == FoldKind::Delimiter
            && range.byte_range.start() == outer_start
            && range.byte_range.end() == outer_end
            && range.start_line == 10
            && range.end_line == 15
    }));
    assert!(ranges.iter().any(|range| {
        range.kind == FoldKind::Delimiter && range.start_line == 12 && range.end_line == 14
    }));
}

#[test]
fn markdown_headings_and_fences_are_foldable_without_a_grammar() {
    let source = "# Intro\ntext\n```rust\nfn main() {}\n```\n## Detail\nmore\n";
    let ranges = fold_ranges(SourceLanguage::Markdown, source);
    assert!(ranges.iter().any(|range| {
        range.kind == FoldKind::MarkdownHeading && range.start_line == 0 && range.end_line == 6
    }));
    assert!(ranges.iter().any(|range| {
        range.kind == FoldKind::MarkdownFence && range.start_line == 2 && range.end_line == 4
    }));
}

#[test]
fn indentation_fallback_folds_python_blocks() {
    let source = "def greet():\n    if ready:\n        return 'ok'\n    return 'later'\n";
    let ranges = fold_ranges(SourceLanguage::Python, source);
    assert!(ranges.iter().any(|range| {
        range.kind == FoldKind::Indentation && range.start_line == 0 && range.end_line == 3
    }));
    assert!(ranges.iter().any(|range| {
        range.kind == FoldKind::Indentation && range.start_line == 1 && range.end_line == 2
    }));
}

#[test]
fn keyword_and_html_fallbacks_do_not_require_a_grammar() {
    let cases = [
        (SourceLanguage::Bash, "if true; then\n  echo ok\nfi\n"),
        (SourceLanguage::Mermaid, "subgraph A\n  X --> Y\nend\n"),
        (SourceLanguage::Ruby, "def answer\n  42\nend\n"),
        (SourceLanguage::Html, "<div>\n  <span>x</span>\n</div>\n"),
    ];
    for (language, source) in cases {
        assert!(
            !fold_ranges(language, source).is_empty(),
            "{language:?} should retain a structural fallback"
        );
    }
}

#[test]
fn incremental_parser_keeps_the_full_document_fallback_contract() {
    let mut parser = IncrementalFoldParser::default();
    let initial = "fn main() {\n  run();\n}\n";
    assert!(!parser.parse(7, SourceLanguage::Rust, initial).is_empty());
    assert!(!parser.last_parse_was_incremental());

    let edited = "fn main() {\n  run_twice();\n}\n";
    assert!(!parser.parse(7, SourceLanguage::Rust, edited).is_empty());
    #[cfg(feature = "code-highlight-core")]
    assert!(parser.last_parse_was_incremental());

    assert!(!parser.parse(8, SourceLanguage::Rust, edited).is_empty());
    assert!(!parser.last_parse_was_incremental());
}
