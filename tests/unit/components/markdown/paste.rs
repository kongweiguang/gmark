// @author kongweiguang

use super::should_split_plain_multiline_paste;

#[test]
fn accepts_plain_lines_with_script_syntax() {
    let lines = vec![
        "H~2~O".to_string(),
        "CO<sub>2</sub>".to_string(),
        "x<sup>n</sup>".to_string(),
    ];

    assert!(should_split_plain_multiline_paste(&lines));
}

#[test]
fn accepts_closed_safe_inline_html_at_line_start() {
    let lines = vec![
        "<sub>2</sub>".to_string(),
        "<sup>n</sup>".to_string(),
        "<span style=\"color:red\">x</span>".to_string(),
        "<strong>y</strong>".to_string(),
    ];

    assert!(should_split_plain_multiline_paste(&lines));
}

#[test]
fn rejects_block_or_unclosed_html_at_line_start() {
    let lines = vec!["<div>x</div>".to_string(), "<p>y</p>".to_string()];
    assert!(!should_split_plain_multiline_paste(&lines));

    let lines = vec!["<script>x</script>".to_string(), "<sup>n</sup>".to_string()];
    assert!(!should_split_plain_multiline_paste(&lines));

    let lines = vec!["<style>x</style>".to_string(), "<sup>n</sup>".to_string()];
    assert!(!should_split_plain_multiline_paste(&lines));

    let lines = vec!["<span>x".to_string(), "<sup>n</sup>".to_string()];
    assert!(!should_split_plain_multiline_paste(&lines));
}

#[test]
fn rejects_structural_markdown() {
    let lines = vec!["```mermaid".to_string(), "flowchart LR".to_string()];
    assert!(!should_split_plain_multiline_paste(&lines));

    let lines = vec!["- item".to_string(), "- next".to_string()];
    assert!(!should_split_plain_multiline_paste(&lines));

    let lines = vec!["| A |".to_string(), "| --- |".to_string()];
    assert!(!should_split_plain_multiline_paste(&lines));

    let lines = vec![
        "```rust".to_string(),
        "fn main() {}".to_string(),
        "```".to_string(),
    ];
    assert!(!should_split_plain_multiline_paste(&lines));

    let lines = vec!["> quote".to_string(), "> more".to_string()];
    assert!(!should_split_plain_multiline_paste(&lines));

    let lines = vec!["# Title".to_string(), "body".to_string()];
    assert!(!should_split_plain_multiline_paste(&lines));
}

#[test]
fn rejects_setext_underline_pairs() {
    // "=" underline must route to the structural importer (-> H1), like the
    // "-" underline (-> H2) already did, rather than splitting into two
    // plain paragraphs.
    let lines = vec!["Title".to_string(), "=====".to_string()];
    assert!(!should_split_plain_multiline_paste(&lines));

    let lines = vec!["Title".to_string(), "-----".to_string()];
    assert!(!should_split_plain_multiline_paste(&lines));
}

#[test]
fn rejects_pipeless_table() {
    // A pipeless GFM table has no leading `|`, so its rows look like plain
    // lines; the header-plus-delimiter shape must still route to the block
    // builder instead of becoming one paragraph per row.
    let lines = vec![
        "Header 1 | Header 2 | Header 3".to_string(),
        "-------- | -------- | --------".to_string(),
        "Cell 1   | Cell 2   | Cell 3".to_string(),
    ];
    assert!(!should_split_plain_multiline_paste(&lines));

    // Prose with a stray `|` and no delimiter row still splits normally.
    let lines = vec![
        "see foo | bar for details".to_string(),
        "and another | line here".to_string(),
    ];
    assert!(should_split_plain_multiline_paste(&lines));
}
