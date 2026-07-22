// @author kongweiguang

use super::{TableCellRectangle, parse_tsv_matrix};
use crate::components::TableCellPosition;
use gpui::EntityId;

#[test]
fn tsv_parser_requires_a_rectangular_bounded_matrix() {
    assert_eq!(
        parse_tsv_matrix("a\tb\r\nc\td\r\n"),
        Some(vec![
            vec!["a".into(), "b".into()],
            vec!["c".into(), "d".into()]
        ])
    );
    assert!(parse_tsv_matrix("plain text").is_none());
    assert!(parse_tsv_matrix("a\tb\nc").is_none());
    let oversized = (0..101)
        .map(|_| vec!["x"; 100].join("\t"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(parse_tsv_matrix(&oversized).is_none());
}

#[test]
fn rectangle_normalizes_anchor_and_focus() {
    let selection = TableCellRectangle {
        table_block_id: EntityId::from(1),
        anchor: TableCellPosition { row: 3, column: 4 },
        focus: TableCellPosition { row: 1, column: 2 },
    };
    assert_eq!(selection.rows(), 1..=3);
    assert_eq!(selection.columns(), 2..=4);
    assert!(selection.contains(TableCellPosition { row: 2, column: 3 }));
}
