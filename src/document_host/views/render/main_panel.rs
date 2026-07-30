// @author kongweiguang

//! Mode-specific document content panels.

use super::*;

impl DocumentHost {
    pub(super) fn render_main_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let surface = self.prepare_source_surface(window, cx);
        let source_list = self.render_source_list(surface, cx);
        let source_scrollbar = self.render_source_scrollbar(surface, cx);
        let source_horizontal_scrollbar = self.render_source_horizontal_scrollbar(surface, cx);
        let theme = cx.global::<ThemeManager>().current_arc();
        let strings = cx.global::<I18nManager>().strings_arc();
        let colors = &theme.colors;
        let viewport_width = f32::from(window.viewport_size().width).max(1.0);
        let viewport_height = f32::from(window.viewport_size().height).max(1.0);
        let structured_panel_available = self.structured_index.is_some();
        let json_graph = self.probe.format == DocumentFormat::Json;

        if self.view_mode == DocumentHostViewMode::Split && json_graph {
            let split_ratio = self.json_split_ratio.clamp(0.3, 0.7);
            let narrow_split = viewport_width < 820.0;
            let split_divider_active =
                self.json_split_drag.is_some() || self.json_split_focus_handle.is_focused(window);
            div()
                .id("json-graph-split-view")
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .when(narrow_split, |split| split.flex_col())
                .relative()
                .on_mouse_move(
                    cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                        if !event.dragging() {
                            return;
                        }
                        let Some((origin, origin_ratio)) = this.json_split_drag else {
                            return;
                        };
                        let pointer = if narrow_split {
                            f32::from(event.position.y)
                        } else {
                            f32::from(event.position.x)
                        };
                        let extent = if narrow_split {
                            viewport_height
                        } else {
                            viewport_width
                        };
                        let delta = pointer - origin;
                        let ratio = (origin_ratio + delta / extent.max(1.0)).clamp(0.3, 0.7);
                        if (this.json_split_ratio - ratio).abs() >= f32::EPSILON {
                            this.json_split_ratio = ratio;
                            cx.emit(DocumentHostEvent::SplitRatioChanged(ratio));
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        if this.json_split_drag.take().is_some() {
                            cx.notify();
                        }
                    }),
                )
                .child(
                    div()
                        .id("json-graph-split-source")
                        .debug_selector(|| "json-graph-split-source".to_owned())
                        .relative()
                        .when(narrow_split, |panel| {
                            panel.w_full().h(relative(split_ratio)).min_h(px(220.0))
                        })
                        .when(!narrow_split, |panel| {
                            panel.w(relative(split_ratio)).h_full()
                        })
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .child(
                            div()
                                .size_full()
                                .flex()
                                .justify_center()
                                .bg(colors.editor_background)
                                .px(px(surface.horizontal_padding))
                                .pt(px(surface.top_padding))
                                .overflow_hidden()
                                .capture_any_mouse_down(
                                    cx.listener(Self::capture_source_surface_mouse_down),
                                )
                                .on_scroll_wheel(cx.listener(Self::on_source_scroll_wheel))
                                .child(source_list),
                        )
                        // JSON Split 只保留中央 1px 分隔线；源码滚动仍由滚轮和键盘驱动，
                        // 避免右侧滚动滑块贴着分隔器形成第二条视觉竖线。
                        .children(source_horizontal_scrollbar),
                )
                .child(
                    div()
                        .id("json-graph-split-divider")
                        .debug_selector(|| "json-graph-split-divider".to_owned())
                        .when(narrow_split, |divider| divider.w_full().h(px(7.0)))
                        .when(!narrow_split, |divider| divider.w(px(7.0)).h_full())
                        .flex_shrink_0()
                        .relative()
                        .when(narrow_split, |divider| divider.cursor_row_resize())
                        .when(!narrow_split, |divider| divider.cursor_col_resize())
                        .tab_index(0)
                        .track_focus(&self.json_split_focus_handle)
                        .hover(|divider| divider.bg(colors.text_link.opacity(0.08)))
                        .focus(|divider| divider.bg(colors.text_link.opacity(0.08)))
                        .child(
                            div()
                                .absolute()
                                .when(narrow_split, |line| {
                                    line.left_0().right_0().top(px(3.0)).h(px(1.0))
                                })
                                .when(!narrow_split, |line| {
                                    line.top_0().bottom_0().left(px(3.0)).w(px(1.0))
                                })
                                .bg(if split_divider_active {
                                    colors.text_link.opacity(0.72)
                                } else {
                                    colors.dialog_border
                                })
                                .debug_selector(|| "json-graph-split-divider-line".to_owned()),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                                let origin = if narrow_split {
                                    f32::from(event.position.y)
                                } else {
                                    f32::from(event.position.x)
                                };
                                this.json_split_drag = Some((origin, this.json_split_ratio));
                                cx.notify();
                            }),
                        )
                        .on_key_down(cx.listener(
                            move |this, event: &gpui::KeyDownEvent, _, cx| {
                                let step = if event.keystroke.modifiers.shift {
                                    0.05
                                } else {
                                    0.01
                                };
                                let next = match event.keystroke.key.as_str() {
                                    "up" if narrow_split => Some(this.json_split_ratio - step),
                                    "down" if narrow_split => Some(this.json_split_ratio + step),
                                    "left" if !narrow_split => Some(this.json_split_ratio - step),
                                    "right" if !narrow_split => Some(this.json_split_ratio + step),
                                    "home" => Some(0.3),
                                    "end" => Some(0.7),
                                    _ => None,
                                };
                                if let Some(next) = next {
                                    this.json_split_ratio = next.clamp(0.3, 0.7);
                                    cx.emit(DocumentHostEvent::SplitRatioChanged(
                                        this.json_split_ratio,
                                    ));
                                    cx.notify();
                                    cx.stop_propagation();
                                }
                            },
                        )),
                )
                .child(
                    div()
                        .id("json-graph-split-preview")
                        .debug_selector(|| "json-graph-split-preview".to_owned())
                        .flex_1()
                        .when(narrow_split, |panel| panel.w_full().min_h(px(220.0)))
                        .when(!narrow_split, |panel| panel.h_full())
                        .min_w(px(0.0))
                        .child(self.render_json_graph_panel(
                            if narrow_split {
                                viewport_width
                            } else {
                                (viewport_width * (1.0 - split_ratio) - 7.0).max(1.0)
                            },
                            if narrow_split {
                                (viewport_height * (1.0 - split_ratio) - 7.0).max(220.0)
                            } else {
                                viewport_height
                            },
                            cx,
                        )),
                )
        } else if self.view_mode == DocumentHostViewMode::Structure && json_graph {
            self.render_json_graph_panel(viewport_width, viewport_height, cx)
        } else if self.view_mode == DocumentHostViewMode::Split && structured_panel_available {
            div()
                .id("document-host-split-view")
                .debug_selector(|| "document-host-split-view".to_owned())
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .child(
                    div()
                        .id("document-host-split-source")
                        .debug_selector(|| "document-host-split-source".to_owned())
                        .relative()
                        .w(relative(0.5))
                        .h_full()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .child(
                            div()
                                .id("document-host-split-source-horizontal-scroll")
                                .debug_selector(|| {
                                    "document-host-split-source-horizontal-scroll".to_owned()
                                })
                                .size_full()
                                .flex()
                                .justify_center()
                                .bg(colors.editor_background)
                                .px(px(surface.horizontal_padding))
                                .pt(px(surface.top_padding))
                                .overflow_hidden()
                                .capture_any_mouse_down(
                                    cx.listener(Self::capture_source_surface_mouse_down),
                                )
                                .on_scroll_wheel(cx.listener(Self::on_source_scroll_wheel))
                                .child(source_list),
                        )
                        .children(source_scrollbar)
                        .children(source_horizontal_scrollbar),
                )
                .child(self.render_structured_panel(cx))
        } else if matches!(
            self.view_mode,
            DocumentHostViewMode::Live | DocumentHostViewMode::Structure
        ) && structured_panel_available
        {
            self.render_structured_panel(cx)
        } else if matches!(
            self.view_mode,
            DocumentHostViewMode::Live | DocumentHostViewMode::Structure
        ) && self.index.is_none()
        {
            div()
                .id("document-host-structure-loading")
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .text_color(colors.text_placeholder)
                .child(strings.large_document_text("preparing_template").replace(
                    "{mib}",
                    &format!("{:.1}", self.probe.len as f64 / (1024.0 * 1024.0)),
                ))
        } else {
            div()
                .id("document-host-source-scroll")
                .relative()
                .flex_1()
                .min_h(px(0.0))
                .overflow_hidden()
                .child(
                    div()
                        .id("document-host-source-horizontal-scroll")
                        .debug_selector(|| "document-host-source-horizontal-scroll".to_owned())
                        .size_full()
                        .flex()
                        .justify_center()
                        .bg(colors.editor_background)
                        .px(px(surface.horizontal_padding))
                        .pt(px(surface.top_padding))
                        .overflow_hidden()
                        .capture_any_mouse_down(
                            cx.listener(Self::capture_source_surface_mouse_down),
                        )
                        .on_scroll_wheel(cx.listener(Self::on_source_scroll_wheel))
                        .on_mouse_move(cx.listener(Self::on_source_surface_mouse_move))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(Self::on_source_surface_mouse_up),
                        )
                        .child(source_list),
                )
                .children(source_scrollbar)
                .children(source_horizontal_scrollbar)
        }
    }
}
