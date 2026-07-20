// @author kongweiguang

use super::{
    TableColumnAlignment, TableColumnLayout, TableData, collect_pipeless_table_region,
    collect_root_table_candidate_region, is_root_table_candidate_line, parse_root_table_region,
    serialize_table_markdown_lines,
};
use crate::components::InlineTextTree;

fn assert_close(left: f32, right: f32) {
    assert!(
        (left - right).abs() < 0.0001,
        "expected {left} to be close to {right}"
    );
}

#[test]
fn parses_valid_root_table_region() {
    let lines = vec![
        "| Left | Center | Right |".to_string(),
        "| :--- | :---: | ---: |".to_string(),
        "| a | b | c |".to_string(),
    ];
    let table = parse_root_table_region(&lines).expect("table should parse");
    assert_eq!(table.alignments.len(), 3);
    assert_eq!(
        table.alignments,
        vec![
            TableColumnAlignment::Left,
            TableColumnAlignment::Center,
            TableColumnAlignment::Right
        ]
    );
    assert_eq!(table.header[0].serialize_markdown(), "Left");
    assert_eq!(table.rows[0][2].serialize_markdown(), "c");
}

#[test]
fn rejects_invalid_alignment_row() {
    let lines = vec!["| Left | Right |".to_string(), "| nope | --- |".to_string()];
    assert!(parse_root_table_region(&lines).is_none());
}

#[test]
fn rejects_alignment_row_with_wrong_column_count() {
    let lines = vec!["| A | B | C |".to_string(), "| --- | --- |".to_string()];
    assert!(parse_root_table_region(&lines).is_none());
}

#[test]
fn preserves_explicit_left_alignment_colon() {
    // ":---" is explicit left and must survive a parse/serialize round-trip
    // instead of being silently rewritten to a bare "---".
    let lines = vec![
        "| L | D | R |".to_string(),
        "| :--- | --- | ---: |".to_string(),
        "| a | b | c |".to_string(),
    ];
    let table = parse_root_table_region(&lines).expect("table should parse");
    assert_eq!(
        table.alignments,
        vec![
            TableColumnAlignment::Left,
            TableColumnAlignment::Default,
            TableColumnAlignment::Right
        ]
    );
    assert_eq!(
        serialize_table_markdown_lines(&table)[1],
        "| :--- | --- | ---: |"
    );
}

#[test]
fn pads_short_body_rows_and_truncates_long_ones() {
    let lines = vec![
        "| A | B | C |".to_string(),
        "| --- | --- | --- |".to_string(),
        "| short |".to_string(),
        "| 1 | 2 | 3 | 4 |".to_string(),
    ];
    let table = parse_root_table_region(&lines).expect("table should parse");
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].len(), 3);
    assert_eq!(table.rows[0][0].serialize_markdown(), "short");
    assert!(table.rows[0][1].serialize_markdown().is_empty());
    assert!(table.rows[0][2].serialize_markdown().is_empty());
    assert_eq!(table.rows[1].len(), 3);
    assert_eq!(table.rows[1][2].serialize_markdown(), "3");
}

#[test]
fn parses_pipeless_table() {
    let lines = vec![
        "Name | Score".to_string(),
        "--- | ---".to_string(),
        "Alice | 10".to_string(),
        "Bob | 7".to_string(),
    ];
    let end = collect_pipeless_table_region(&lines, 0).expect("region");
    assert_eq!(end, 4);
    let table = parse_root_table_region(&lines[..end]).expect("table should parse");
    assert_eq!(table.header.len(), 2);
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.header[0].serialize_markdown(), "Name");
    assert_eq!(table.rows[1][1].serialize_markdown(), "7");
}

#[test]
fn prose_with_pipe_is_not_a_pipeless_table() {
    let lines = vec!["this | that".to_string(), "and the next line".to_string()];
    assert!(collect_pipeless_table_region(&lines, 0).is_none());
}

#[test]
fn pipeless_table_requires_valid_delimiter_row() {
    let lines = vec!["Name | Score".to_string(), "Alice | 10".to_string()];
    assert!(collect_pipeless_table_region(&lines, 0).is_none());
}

#[test]
fn single_column_pipeless_is_not_a_table() {
    // Ambiguous with a setext heading; must not be captured as a table.
    let lines = vec!["Title".to_string(), "---".to_string()];
    assert!(collect_pipeless_table_region(&lines, 0).is_none());
}

#[test]
fn serializes_canonical_pipe_table() {
    let table = TableData {
        header: vec![
            InlineTextTree::from_markdown("**bold**"),
            InlineTextTree::from_markdown("[link](https://example.com)"),
        ],
        rows: vec![vec![
            InlineTextTree::plain("A | B".to_string()),
            InlineTextTree::plain("value".to_string()),
        ]],
        alignments: vec![TableColumnAlignment::Default, TableColumnAlignment::Right],
    };
    assert_eq!(
        serialize_table_markdown_lines(&table),
        vec![
            "| **bold** | [link](https://example.com) |".to_string(),
            "| --- | ---: |".to_string(),
            "| A \\| B | value |".to_string(),
        ]
    );
}

#[test]
fn detects_root_table_candidate_runs() {
    let lines = vec![
        "| A | B |".to_string(),
        "| --- | --- |".to_string(),
        "| 1 | 2 |".to_string(),
        "paragraph".to_string(),
    ];
    assert!(is_root_table_candidate_line(&lines[0]));
    assert_eq!(collect_root_table_candidate_region(&lines, 0), 3);
}

#[test]
fn equal_share_fast_path_keeps_columns_uniform() {
    let layout = TableColumnLayout::from_preferred_widths(&[32.0, 64.0, 48.0], 360.0, 60.0);
    let fractions = layout.fractions();
    assert_eq!(fractions.len(), 3);
    assert_close(fractions[0], 1.0 / 3.0);
    assert_close(fractions[1], 1.0 / 3.0);
    assert_close(fractions[2], 1.0 / 3.0);
}

#[test]
fn content_pressure_redistributes_width_across_the_whole_column() {
    let layout = TableColumnLayout::from_preferred_widths(&[48.0, 220.0, 48.0], 360.0, 60.0);
    let fractions = layout.fractions();
    assert_eq!(fractions.len(), 3);
    assert!(fractions[1] > fractions[0]);
    assert!(fractions[1] > fractions[2]);
    assert_close(fractions[0], fractions[2]);
}

#[test]
fn minimum_column_floor_prevents_neighbor_collapse() {
    let layout = TableColumnLayout::from_preferred_widths(&[16.0, 900.0, 16.0], 300.0, 70.0);
    let fractions = layout.fractions();
    let widths = fractions
        .iter()
        .map(|fraction| fraction * 300.0)
        .collect::<Vec<_>>();
    assert!(widths[0] >= 70.0 - 0.001);
    assert!(widths[2] >= 70.0 - 0.001);
    assert_close(fractions.iter().sum::<f32>(), 1.0);
}

#[test]
fn moderate_single_cell_growth_stays_equal_when_share_is_sufficient() {
    let layout = TableColumnLayout::from_preferred_widths(&[56.0, 92.0, 56.0], 360.0, 60.0);
    let fractions = layout.fractions();
    assert_close(fractions[0], 1.0 / 3.0);
    assert_close(fractions[1], 1.0 / 3.0);
    assert_close(fractions[2], 1.0 / 3.0);
}

#[test]
fn append_row_preserves_column_count_and_creates_empty_cells() {
    let mut table = TableData::new_empty(1, 3);
    table.append_row();

    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[1].len(), 3);
    assert!(
        table.rows[1]
            .iter()
            .all(|cell| cell.serialize_markdown().is_empty())
    );
}

#[test]
fn append_column_extends_every_row_and_uses_requested_alignment() {
    let mut table = TableData {
        header: vec![
            InlineTextTree::plain("A".to_string()),
            InlineTextTree::plain("B".to_string()),
        ],
        rows: vec![
            vec![
                InlineTextTree::plain("1".to_string()),
                InlineTextTree::plain("2".to_string()),
            ],
            vec![
                InlineTextTree::plain("3".to_string()),
                InlineTextTree::plain("4".to_string()),
            ],
        ],
        alignments: vec![TableColumnAlignment::Left, TableColumnAlignment::Right],
    };

    table.append_column(TableColumnAlignment::Right);

    assert_eq!(table.header.len(), 3);
    assert_eq!(table.rows[0].len(), 3);
    assert_eq!(table.rows[1].len(), 3);
    assert_eq!(
        table.alignments,
        vec![
            TableColumnAlignment::Left,
            TableColumnAlignment::Right,
            TableColumnAlignment::Right,
        ]
    );
    assert!(table.header[2].serialize_markdown().is_empty());
    assert!(table.rows[0][2].serialize_markdown().is_empty());
    assert!(table.rows[1][2].serialize_markdown().is_empty());
}

#[test]
fn append_column_pads_missing_alignments_with_default() {
    let mut table = TableData {
        header: vec![InlineTextTree::plain("A".to_string())],
        rows: vec![vec![InlineTextTree::plain("1".to_string())]],
        alignments: Vec::new(),
    };

    table.append_column(TableColumnAlignment::Left);

    assert_eq!(
        table.alignments,
        vec![TableColumnAlignment::Default, TableColumnAlignment::Left]
    );
    assert_eq!(table.header.len(), 2);
    assert_eq!(table.rows[0].len(), 2);
}

#[test]
fn set_column_alignment_updates_requested_column() {
    let mut table = TableData::new_empty(2, 3);
    table.set_column_alignment(1, TableColumnAlignment::Center);
    assert_eq!(
        table.alignments,
        vec![
            TableColumnAlignment::Default,
            TableColumnAlignment::Center,
            TableColumnAlignment::Default
        ]
    );
}

#[test]
fn swap_visual_rows_exchanges_header_with_first_body_row() {
    let mut table = TableData {
        header: vec![InlineTextTree::plain("A".to_string())],
        rows: vec![
            vec![InlineTextTree::plain("1".to_string())],
            vec![InlineTextTree::plain("2".to_string())],
        ],
        alignments: vec![TableColumnAlignment::Left],
    };
    // Visual row 0 is the header; swapping it with visual row 1 exchanges
    // header and first-body content.
    table.swap_visual_rows(0, 1);
    assert_eq!(table.header[0].serialize_markdown(), "1");
    assert_eq!(table.rows[0][0].serialize_markdown(), "A");
    assert_eq!(table.rows[1][0].serialize_markdown(), "2");

    // Two body rows (visual 1 and 2) swap like ordinary rows.
    table.swap_visual_rows(1, 2);
    assert_eq!(table.rows[0][0].serialize_markdown(), "2");
    assert_eq!(table.rows[1][0].serialize_markdown(), "A");
}

#[test]
fn swap_columns_exchanges_header_body_and_alignment() {
    let mut table = TableData {
        header: vec![
            InlineTextTree::plain("A".to_string()),
            InlineTextTree::plain("B".to_string()),
        ],
        rows: vec![vec![
            InlineTextTree::plain("1".to_string()),
            InlineTextTree::plain("2".to_string()),
        ]],
        alignments: vec![TableColumnAlignment::Left, TableColumnAlignment::Right],
    };
    table.swap_columns(0, 1);
    assert_eq!(table.header[0].serialize_markdown(), "B");
    assert_eq!(table.rows[0][0].serialize_markdown(), "2");
    assert_eq!(
        table.alignments,
        vec![TableColumnAlignment::Right, TableColumnAlignment::Left]
    );
}

#[test]
fn remove_body_row_can_empty_the_table() {
    let mut table = TableData::new_empty(2, 2);
    table.remove_body_row(0);
    assert_eq!(table.rows.len(), 1);
    table.remove_body_row(0);
    // The last body row can be removed, leaving a header-only table.
    assert!(table.rows.is_empty());
    // Out-of-range removal is a no-op.
    table.remove_body_row(0);
    assert!(table.rows.is_empty());
}

#[test]
fn remove_header_row_promotes_first_body_row() {
    let mut table = parse_root_table_region(&[
        "| A | B |".to_string(),
        "| --- | --- |".to_string(),
        "| 1 | 2 |".to_string(),
        "| 3 | 4 |".to_string(),
    ])
    .expect("valid table");

    assert!(table.remove_header_row());
    assert_eq!(table.header[0].serialize_markdown(), "1");
    assert_eq!(table.header[1].serialize_markdown(), "2");
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0][0].serialize_markdown(), "3");

    // Promoting the last remaining row leaves a header-only table.
    assert!(table.remove_header_row());
    assert!(table.rows.is_empty());
    assert!(!table.remove_header_row());
    assert_eq!(table.header[0].serialize_markdown(), "3");
}

#[test]
fn remove_column_preserves_at_least_one_column() {
    let mut table = TableData::new_empty(2, 2);
    table.remove_column(0);
    assert_eq!(table.column_count(), 1);
    table.remove_column(0);
    assert_eq!(table.column_count(), 1);
}
