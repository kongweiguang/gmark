// @author kongweiguang

use super::{RenderedRowSpacingInfo, ThemeDimensions};

/// 在渲染树中消费空段落代理，但只让它影响相邻语义块的间距。
///
/// 源码空行仍留在 DocumentTree 中承担序列化和光标映射；这里不把它交给普通块
/// 布局。连续空行只设置一次 pending 标记，文档首尾空行则不会制造额外边距。
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::editor::render) struct RenderedRowGapState {
    previous_content: Option<RenderedRowSpacingInfo>,
    pending_blank: bool,
}

impl RenderedRowGapState {
    pub(in crate::editor::render) fn root_gap(
        &mut self,
        current: RenderedRowSpacingInfo,
        default_gap: f32,
    ) -> Option<f32> {
        if current.is_empty_paragraph {
            self.pending_blank |= self.previous_content.is_some();
            return None;
        }
        let gap = if self.pending_blank && self.previous_content.is_some() {
            default_gap
        } else {
            rendered_row_top_gap(self.previous_content, current, default_gap)
        };
        self.previous_content = Some(current);
        self.pending_blank = false;
        Some(gap)
    }

    pub(in crate::editor::render) fn callout_gap(
        &mut self,
        current: RenderedRowSpacingInfo,
        dimensions: &ThemeDimensions,
    ) -> Option<f32> {
        if current.is_empty_paragraph {
            self.pending_blank |= self.previous_content.is_some();
            return None;
        }
        let gap = if self.pending_blank {
            self.previous_content.map_or(0.0, |previous| {
                if previous.is_callout_header {
                    dimensions.callout_header_margin_bottom
                } else {
                    dimensions.callout_body_gap
                }
            })
        } else {
            callout_row_top_gap(self.previous_content, current, dimensions)
        };
        self.previous_content = Some(current);
        self.pending_blank = false;
        Some(gap)
    }

    pub(in crate::editor::render) fn footnote_gap(
        &mut self,
        current: RenderedRowSpacingInfo,
        default_gap: f32,
    ) -> Option<f32> {
        if current.is_empty_paragraph {
            self.pending_blank |= self.previous_content.is_some();
            return None;
        }
        let gap = footnote_row_top_gap(self.previous_content, default_gap);
        self.previous_content = Some(current);
        self.pending_blank = false;
        Some(gap)
    }

    pub(in crate::editor::render) fn finish_group(
        &mut self,
        last_content: Option<RenderedRowSpacingInfo>,
    ) {
        if let Some(last_content) = last_content {
            self.previous_content = Some(last_content);
        }
        self.pending_blank = false;
    }

    pub(in crate::editor::render) fn last_content(&self) -> Option<RenderedRowSpacingInfo> {
        self.previous_content
    }
}

pub(in crate::editor::render) fn rendered_row_top_gap(
    previous: Option<RenderedRowSpacingInfo>,
    current: RenderedRowSpacingInfo,
    default_gap: f32,
) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };

    if previous.quote_group_anchor.is_some()
        && previous.quote_group_anchor == current.quote_group_anchor
    {
        0.0
    } else {
        default_gap
    }
}

pub(in crate::editor::render) fn callout_row_top_gap(
    previous: Option<RenderedRowSpacingInfo>,
    current: RenderedRowSpacingInfo,
    dimensions: &ThemeDimensions,
) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };

    if previous.visible_quote_group_anchor.is_some()
        && previous.visible_quote_group_anchor == current.visible_quote_group_anchor
    {
        return 0.0;
    }

    if previous.is_callout_header {
        dimensions.callout_header_margin_bottom
    } else {
        dimensions.callout_body_gap
    }
}

fn footnote_row_top_gap(previous: Option<RenderedRowSpacingInfo>, default_gap: f32) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };

    if previous.is_footnote_header {
        default_gap * 0.75
    } else {
        default_gap
    }
}
