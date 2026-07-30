// @author kongweiguang

//! Vertical scrollbar geometry for virtualized structured rows.

use super::*;
use crate::theme::ThemeColors;

impl DocumentHost {
    pub(super) fn render_structured_scrollbar(
        &self,
        row_count: usize,
        colors: &ThemeColors,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let structured_count = row_count;

        let structured_scroll = self.structured_scroll_handle.0.borrow().base_handle.clone();
        let structured_scroll_bounds = structured_scroll.bounds();
        let structured_viewport_height =
            f32::from(structured_scroll_bounds.size.height.max(px(1.0)));
        // 纵向 thumb 的几何必须来自纵向列表 viewport；横向 ScrollHandle 只描述底部表格
        // 轨道，用它的高度会把任意点击都折算回第 0 行。
        let structured_track_bounds = structured_scroll_bounds;
        let structured_track_height = f32::from(structured_track_bounds.size.height.max(px(1.0)));
        let structured_visible_rows = (structured_viewport_height / 26.0).ceil().max(1.0) as usize;
        let structured_max_top_row = structured_count.saturating_sub(structured_visible_rows);
        let structured_top_row = (-f32::from(structured_scroll.offset().y) / 26.0)
            .max(0.0)
            .floor() as usize;
        let structured_thumb_height = if structured_max_top_row > 0 {
            (structured_track_height * structured_visible_rows as f32
                / structured_count.max(1) as f32)
                .clamp(
                    28.0_f32.min(structured_track_height),
                    structured_track_height,
                )
        } else {
            structured_track_height
        };
        let structured_thumb_top = if structured_max_top_row > 0 {
            (structured_track_height - structured_thumb_height)
                * (structured_top_row.min(structured_max_top_row) as f64
                    / structured_max_top_row as f64) as f32
        } else {
            0.0
        };
        (structured_max_top_row > 0).then(|| {
            let track_top = structured_track_bounds.top();
            div()
                .id("document-host-structured-scrollbar")
                .debug_selector(|| "document-host-structured-scrollbar".to_owned())
                .absolute()
                .top(px(0.0))
                .bottom(px(0.0))
                .right(px(3.0))
                .w(px(12.0))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, _cx| {
                        let row = source_line_from_scrollbar_pointer(
                            event.position.y,
                            track_top,
                            structured_track_height,
                            structured_thumb_height,
                            structured_max_top_row,
                        );
                        this.structured_scroll_handle
                            .scroll_to_item_strict(row, ScrollStrategy::Top);
                        window.refresh();
                    }),
                )
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                        let row = source_line_from_scrollbar_pointer(
                            event.position().y,
                            track_top,
                            structured_track_height,
                            structured_thumb_height,
                            structured_max_top_row,
                        );
                        this.structured_scroll_handle
                            .scroll_to_item_strict(row, ScrollStrategy::Top);
                        cx.notify();
                        window.refresh();
                    }),
                )
                .on_mouse_move(cx.listener(
                    move |this, event: &gpui::MouseMoveEvent, window, _cx| {
                        if event.dragging() {
                            let row = source_line_from_scrollbar_pointer(
                                event.position.y,
                                track_top,
                                structured_track_height,
                                structured_thumb_height,
                                structured_max_top_row,
                            );
                            this.structured_scroll_handle
                                .scroll_to_item_strict(row, ScrollStrategy::Top);
                            window.refresh();
                        }
                    },
                ))
                .child(
                    div()
                        .id("document-host-structured-scrollbar-thumb")
                        .debug_selector(|| "document-host-structured-scrollbar-thumb".to_owned())
                        .absolute()
                        .top(px(structured_thumb_top))
                        .right(px(2.0))
                        .w(px(7.0))
                        .h(px(structured_thumb_height))
                        .rounded(px(999.0))
                        .bg(colors.scrollbar_thumb),
                )
        })
    }
}
