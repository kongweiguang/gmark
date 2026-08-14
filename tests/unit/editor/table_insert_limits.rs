// @author kongweiguang

use super::{MAX_TABLE_CELLS, checked_table_cell_count};

/// 锁定恰好 10,000 个单元格可用、超出和算术溢出均被拒绝的边界。
#[::core::prelude::v1::test]
fn table_cell_limit_accepts_exact_boundary_and_rejects_overflow() {
    assert_eq!(checked_table_cell_count(99, 100), Some(MAX_TABLE_CELLS));
    assert!(checked_table_cell_count(100, 100).is_some_and(|cells| cells > MAX_TABLE_CELLS));
    assert!(checked_table_cell_count(usize::MAX, usize::MAX).is_none());
}
