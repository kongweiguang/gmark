// @author kongweiguang

use super::{MAX_TABLE_CELLS, checked_table_cell_count};

/// 表格片段合并在精确上限、超限和 checked 乘加溢出时都必须拒绝。
#[::core::prelude::v1::test]
fn table_fragment_cell_limit_is_checked_before_merge() {
    assert_eq!(checked_table_cell_count(99, 100), Some(MAX_TABLE_CELLS));
    assert!(checked_table_cell_count(100, 100).is_some_and(|cells| cells > MAX_TABLE_CELLS));
    assert!(checked_table_cell_count(usize::MAX, usize::MAX).is_none());
}
