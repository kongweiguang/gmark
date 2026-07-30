// @author kongweiguang

//! Content-column sizing shared by editor surfaces.

use crate::ui::theme::ThemeDimensions;

/// 根据视口宽度在全宽与聚焦阅读宽度之间线性插值。
pub(crate) fn centered_column_ratio(viewport_width: f32, dimensions: &ThemeDimensions) -> f32 {
    if viewport_width <= dimensions.centered_shrink_start {
        return 1.0;
    }
    let progress = ((viewport_width - dimensions.centered_shrink_start)
        / (dimensions.centered_shrink_end - dimensions.centered_shrink_start))
        .clamp(0.0, 1.0);
    1.0 - progress * (1.0 - dimensions.centered_min_ratio)
}

/// 计算编辑器、块内容与大文件源码面共享的内容列宽度。
pub(crate) fn centered_column_width(viewport_width: f32, dimensions: &ThemeDimensions) -> f32 {
    let available_content_width = (viewport_width - dimensions.editor_padding * 2.0).max(1.0);
    let centered_ratio = centered_column_ratio(viewport_width, dimensions);
    (available_content_width * centered_ratio)
        .max(320.0)
        .min(dimensions.centered_max_width.max(320.0))
        .min(available_content_width)
}
