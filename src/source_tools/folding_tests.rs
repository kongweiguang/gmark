// @author kongweiguang

use super::*;

#[test]
fn domain_discovery_ignores_delimiters_inside_strings_and_comments() {
    let source = "{\n  \"fake\": \"} [\",\n  // }\n  \"real\": [\n    1\n  ]\n}\n";
    let regions = discover_fold_regions(SourceLanguageId::Json, source, 0, 0);
    assert!(
        regions
            .iter()
            .any(|region| region.start_line == 0 && region.end_line == 6)
    );
    assert!(
        regions
            .iter()
            .any(|region| region.start_line == 3 && region.end_line == 5)
    );
}

#[test]
fn domain_discovery_keeps_structural_fallbacks() {
    let cases = [
        (SourceLanguageId::Markdown, "# A\nbody\nmore\n"),
        (SourceLanguageId::Bash, "if true; then\n  echo ok\nfi\n"),
        (SourceLanguageId::Html, "<div>\n  <span>x</span>\n</div>\n"),
        (SourceLanguageId::Ruby, "def f\n  1\nend\n"),
        (SourceLanguageId::Mermaid, "subgraph A\n  X --> Y\nend\n"),
    ];
    for (language, source) in cases {
        assert!(
            !discover_fold_regions(language, source, 0, 0).is_empty(),
            "{language:?} should expose a fold"
        );
    }
}

#[test]
fn projection_preserves_nested_state_and_maps_both_directions() {
    let mut projection = FoldProjectionIndex::default();
    projection.set_regions(
        10,
        vec![
            FoldRegion {
                id: 1,
                kind: "outer",
                byte_range: 0..100,
                start_line: 1,
                end_line: 8,
                depth: 0,
                structure_path: vec![0],
                closing: Some('}'),
            },
            FoldRegion {
                id: 2,
                kind: "inner",
                byte_range: 10..50,
                start_line: 3,
                end_line: 5,
                depth: 1,
                structure_path: vec![0, 0],
                closing: Some(']'),
            },
        ],
    );
    projection.toggle(1);
    projection.toggle(2);
    assert_eq!(projection.visible_line_count(), 3);
    assert_eq!(projection.real_line_for_visible(2), 9);
    assert_eq!(projection.visible_line_for_real(7), 1);
    projection.toggle(1);
    assert_eq!(projection.visible_line_count(), 8);
    assert!(projection.is_collapsed(2));
    assert_eq!(projection.visible_line_for_real(5), 3);
}

#[test]
fn reparsing_after_prefix_edit_preserves_structural_fold_state() {
    let source = "{\n  \"a\": [\n    1\n  ]\n}\n";
    let mut projection = FoldProjectionIndex::default();
    let first = discover_fold_regions(SourceLanguageId::Json, source, 0, 0);
    let inner = first
        .iter()
        .find(|region| region.start_line == 1)
        .expect("inner fold")
        .id;
    projection.set_regions(5, first);
    projection.toggle(inner);

    let changed = format!("\n{source}");
    projection.set_regions(
        6,
        discover_fold_regions(SourceLanguageId::Json, &changed, 0, 0),
    );
    let inner = projection
        .regions()
        .iter()
        .find(|region| region.start_line == 2)
        .expect("shifted inner fold");
    assert!(projection.is_collapsed(inner.id));
}

#[test]
fn edits_expand_touched_folds_and_shift_untouched_following_regions() {
    let mut projection = FoldProjectionIndex::default();
    projection.set_regions(
        12,
        vec![
            FoldRegion {
                id: 1,
                kind: "first",
                byte_range: 0..20,
                start_line: 0,
                end_line: 3,
                depth: 0,
                structure_path: vec![0],
                closing: Some('}'),
            },
            FoldRegion {
                id: 2,
                kind: "second",
                byte_range: 30..50,
                start_line: 6,
                end_line: 9,
                depth: 0,
                structure_path: vec![1],
                closing: Some('}'),
            },
        ],
    );
    projection.collapse_all();
    projection.apply_source_edit(5..5, 1, 1, "new\nline\n");

    assert!(projection.regions().iter().all(|region| region.id != 1));
    let shifted = projection
        .regions()
        .iter()
        .find(|region| region.id == 2)
        .expect("untouched following fold should remain");
    assert_eq!(shifted.byte_range, 39..59);
    assert_eq!((shifted.start_line, shifted.end_line), (8, 11));
    assert!(projection.is_collapsed(2));
}

#[test]
fn resident_parser_reuses_domain_tree_for_local_edits() {
    let mut parser = ResidentFoldParser::default();
    let first = "fn main() {\n  run();\n}\n";
    assert!(!parser.parse(1, SourceLanguageId::Rust, first).is_empty());
    assert!(!parser.last_parse_was_incremental());

    let changed = "fn main() {\n  run_twice();\n}\n";
    assert!(!parser.parse(1, SourceLanguageId::Rust, changed).is_empty());
    #[cfg(feature = "code-highlight-core")]
    assert!(parser.last_parse_was_incremental());

    assert!(!parser.parse(2, SourceLanguageId::Rust, changed).is_empty());
    assert!(!parser.last_parse_was_incremental());
}
