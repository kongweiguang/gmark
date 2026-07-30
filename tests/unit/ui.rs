// @author kongweiguang

use super::{centered_column_ratio, centered_column_width};
use crate::ui::theme::Theme;

#[test]
fn centered_columns_keep_the_existing_viewport_bounds() {
    let dimensions = Theme::xcode_dark().dimensions;

    assert_eq!(
        centered_column_ratio(dimensions.centered_shrink_start, &dimensions),
        1.0
    );
    assert_eq!(
        centered_column_ratio(dimensions.centered_shrink_end, &dimensions),
        dimensions.centered_min_ratio
    );
    assert_eq!(centered_column_width(100.0, &dimensions), 52.0);
}
