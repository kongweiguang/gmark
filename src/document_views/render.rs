// @author kongweiguang

use super::*;

impl Render for DocumentHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.displayed_screen_lines.rows.is_empty()
            && let Some(started) = self.first_render_started.take()
        {
            let profile = self.probe.profile();
            let plan = session_plan(&profile, &self.probe, self.probe.strategy, false);
            crate::perf::emit_document(
                "document_first_source_frame",
                started,
                usize::try_from(self.probe.len).ok(),
                Some(true),
                &profile.format,
                &plan,
                Some("GPUI render boundary; not platform present"),
            );
        }
        if self.soak_ready_published && self.displayed_screen_lines.rows.is_empty() {
            self.metrics.blank_frames_after_content =
                self.metrics.blank_frames_after_content.saturating_add(1);
        }
        let (layout_hits, layout_misses, layout_entries) = self.source_row_blocks.values().fold(
            (0u64, 0u64, 0usize),
            |(hits, misses, entries), block| {
                let block = block.read(cx);
                (
                    hits.saturating_add(block.source_layout_cache_hits),
                    misses.saturating_add(block.source_layout_cache_misses),
                    entries + usize::from(block.source_layout_cache_key.is_some()),
                )
            },
        );
        self.metrics.layout_cache_hits = layout_hits;
        self.metrics.layout_cache_misses = layout_misses;
        if layout_entries > self.metrics.max_layout_cache_entries {
            self.metrics.max_layout_cache_entries = layout_entries;
            let profile = self.probe.profile();
            let plan = session_plan(&profile, &self.probe, self.probe.strategy, false);
            crate::perf::emit_document_value(
                "document_layout_cache_peak",
                layout_entries as u64,
                &profile.format,
                &plan,
            );
        }
        let theme = cx.global::<ThemeManager>().current_arc();
        let strings = cx.global::<I18nManager>().strings_arc();
        let colors = &theme.colors;
        let dimensions = &theme.dimensions;
        let source_text_size = theme.typography.text_size;
        let source_line_height = theme.typography.text_line_height;
        let line_text_color = colors.text_default;
        let line_number_color = colors.text_placeholder;
        let gutter_separator_color = colors.dialog_border.opacity(0.7);
        let active_line_color = colors.source_mode_block_bg.opacity(0.55);
        let source_background = colors.editor_background;
        window.set_window_edited(document_dirty_state(&self.document, &self.pending_dirty));
        let line_count = self.line_count();
        self.source_list_origin = self
            .source_list_origin
            .min(line_count.saturating_sub(SOURCE_LIST_WINDOW_ROWS));
        let observed_line_bytes = self
            .index
            .as_ref()
            .map_or(0, LineIndex::max_line_bytes)
            .max(
                self.preview_lines
                    .iter()
                    .map(|line| line.len() as u64)
                    .max()
                    .unwrap_or(0),
            )
            .max(
                self.active_edit
                    .as_ref()
                    .map(|edit| edit.block.read(cx).display_text().len() as u64)
                    .unwrap_or(0),
            );
        let viewport_width = f32::from(window.viewport_size().width).max(1.0);
        let viewport_height = f32::from(window.viewport_size().height).max(1.0);
        let source_content_width = (viewport_width - 2.0 * dimensions.editor_padding).max(1.0);
        let source_row_height = (source_line_height * f32::from(window.rem_size())).max(1.0);
        self.source_row_height = source_row_height;
        let source_top_padding = crate::editor::source_editor_top_padding(dimensions);
        let source_gutter_width = f32::from(source_line_number_gutter_width(
            line_count,
            px(source_text_size),
        ));
        let source_list_len = self.source_list_len();
        let source_list = uniform_list(
            "document-host-lines",
            source_list_len,
            cx.processor(
                move |this, local_range: std::ops::Range<usize>, _window, _cx| {
                    // keyed uniform_list 可跨 render 复用 processor；全局 origin 必须在
                    // 调用时读取，不能捕获创建该 element 时的旧窗口。
                    let source_list_origin = this.source_list_origin;
                    let range = source_list_origin.saturating_add(local_range.start)
                        ..source_list_origin.saturating_add(local_range.end);
                    this.request_source_rows(range.clone(), _cx);
                    let requested_visible = range.clone();
                    let first_requested = range.start;
                    let retain_previous_frame = this
                        .displayed_screen_lines
                        .should_retain_previous_frame(&requested_visible);
                    let retained_rows = retain_previous_frame
                        .then(|| {
                            this.displayed_screen_lines
                                .retained_rows(this.show_line_endings)
                        })
                        .unwrap_or_default();
                    range
                        .map(|line| {
                            let exact_row = this.displayed_screen_lines.row(line).map(|row| {
                                (
                                    row.leading_truncated,
                                    row.trailing_truncated,
                                    (!row.trailing_truncated && this.show_line_endings)
                                        .then(|| rendered_line_ending(&row.ending))
                                        .filter(|marker| !marker.is_empty()),
                                    row.rendered(this.show_line_endings),
                                )
                            });
                            let retained_row = exact_row
                                .is_none()
                                .then(|| {
                                    let ordinal = line.checked_sub(requested_visible.start)?;
                                    retained_rows.get(ordinal).cloned()
                                })
                                .flatten();
                            let retained_old_frame = retained_row.is_some();
                            let display_line = retained_row
                                .as_ref()
                                .map_or(line, |(display_line, _)| *display_line);
                            let source_block = (!retained_old_frame)
                                .then(|| this.ensure_source_row_block(line, _cx))
                                .flatten();
                            let active_line = this
                                .active_edit
                                .as_ref()
                                .is_some_and(|edit| edit.line == line);
                            div()
                                .id(("document-host-line", line))
                                .h(px(source_row_height))
                                .min_w_full()
                                .flex()
                                .items_center()
                                .text_size(px(source_text_size))
                                .line_height(rems(source_line_height))
                                .text_color(line_text_color)
                                .bg(if active_line {
                                    active_line_color
                                } else {
                                    source_background
                                })
                                .child(
                                    div()
                                        .w(px(source_gutter_width))
                                        .pr(px(12.0))
                                        .border_r(px(1.0))
                                        .border_color(gutter_separator_color)
                                        .text_align(gpui::TextAlign::Right)
                                        .text_color(line_number_color)
                                        .child((display_line + 1).to_string()),
                                )
                                .child({
                                    let mut body = div()
                                        .debug_selector(move || {
                                            format!("document-host-line-body-{line}")
                                        })
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .h_full()
                                        .overflow_hidden();
                                    if let Some(block) = source_block {
                                        let (
                                            leading_truncated,
                                            trailing_truncated,
                                            ending_marker,
                                            _,
                                        ) = exact_row
                                            .as_ref()
                                            .expect("source block requires exact row");
                                        body = body.child(
                                            div()
                                                .size_full()
                                                .flex()
                                                .items_center()
                                                .whitespace_nowrap()
                                                .text_color(line_text_color)
                                                .children((*leading_truncated).then_some("… "))
                                                .child(
                                                    div()
                                                        .h_full()
                                                        .flex_1()
                                                        .min_w(px(0.0))
                                                        .overflow_hidden()
                                                        .child(block),
                                                )
                                                .children((*trailing_truncated).then_some(" …"))
                                                .children(*ending_marker),
                                        );
                                    } else {
                                        let display = retained_row
                                            .as_ref()
                                            .map(|(_, display)| display.clone())
                                            .or_else(|| {
                                                exact_row
                                                    .as_ref()
                                                    .map(|(_, _, _, display)| display.clone())
                                            })
                                            .unwrap_or_else(|| this.line_text(line));
                                        body = body.whitespace_nowrap().child(display);
                                    }
                                    if retained_old_frame && line == first_requested {
                                        body = body.relative().child(
                                            div()
                                                .debug_selector(|| {
                                                    "document-host-retained-frame-progress"
                                                        .to_owned()
                                                })
                                                .absolute()
                                                .top_0()
                                                .right(px(8.0))
                                                .text_color(line_number_color)
                                                .child(
                                                    _cx.global::<I18nManager>()
                                                        .strings()
                                                        .large_document_text(
                                                            "loading_next_viewport",
                                                        )
                                                        .to_owned(),
                                                ),
                                        );
                                    }
                                    body
                                })
                                .when(!retained_old_frame, |row| {
                                    row.on_mouse_down(
                                        MouseButton::Left,
                                        _cx.listener(move |this, event, window, cx| {
                                            this.select_or_edit_line(line, event, window, cx);
                                        }),
                                    )
                                })
                        })
                        .collect::<Vec<_>>()
                },
            ),
        )
        .track_scroll(self.scroll_handle.clone())
        .font_family(source_monospace_font_family())
        .h_full()
        .w(px(source_content_width))
        .max_w(relative(1.0))
        .px(px(dimensions.block_padding_x))
        .bg(colors.editor_background);
        let source_scroll = self.scroll_handle.0.borrow().base_handle.clone();
        let source_scroll_bounds = source_scroll.bounds();
        let source_viewport_height = f32::from(source_scroll_bounds.size.height.max(px(1.0)));
        let source_visible_rows =
            (source_viewport_height / source_row_height).ceil().max(1.0) as usize;
        let source_local_top = (-f32::from(source_scroll.offset().y) / source_row_height)
            .max(0.0)
            .floor() as usize;
        let source_global_top = self
            .source_list_origin
            .saturating_add(source_local_top)
            .min(line_count.saturating_sub(1));
        let source_max_top_line = line_count.saturating_sub(source_visible_rows);
        let source_thumb_height = if source_max_top_line > 0 {
            (source_viewport_height * source_visible_rows as f32 / line_count.max(1) as f32)
                .clamp(28.0_f32.min(source_viewport_height), source_viewport_height)
        } else {
            source_viewport_height
        };
        let source_thumb_top = if source_max_top_line > 0 {
            (source_viewport_height - source_thumb_height)
                * (source_global_top as f64 / source_max_top_line as f64) as f32
        } else {
            0.0
        };
        let source_scrollbar = (source_max_top_line > 0).then(|| {
            let track_top = source_scroll_bounds.top();
            div()
                .id("document-host-scrollbar")
                .debug_selector(|| "document-host-scrollbar".to_owned())
                .absolute()
                .top_0()
                .bottom_0()
                .right(px(3.0))
                .w(px(12.0))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, _cx| {
                        let line = source_line_from_scrollbar_pointer(
                            event.position.y,
                            track_top,
                            source_viewport_height,
                            source_thumb_height,
                            source_max_top_line,
                        );
                        this.scroll_source_line_strict(line, ScrollStrategy::Top);
                        window.refresh();
                    }),
                )
                .on_mouse_move(cx.listener(
                    move |this, event: &gpui::MouseMoveEvent, window, _cx| {
                        if event.dragging() {
                            let line = source_line_from_scrollbar_pointer(
                                event.position.y,
                                track_top,
                                source_viewport_height,
                                source_thumb_height,
                                source_max_top_line,
                            );
                            this.scroll_source_line_strict(line, ScrollStrategy::Top);
                            window.refresh();
                        }
                    },
                ))
                .child(
                    div()
                        .absolute()
                        .top(px(source_thumb_top))
                        .right(px(2.0))
                        .w(px(7.0))
                        .h(px(source_thumb_height))
                        .rounded(px(999.0))
                        .bg(colors.scrollbar_thumb),
                )
        });
        let source_max_window_start = observed_line_bytes.saturating_sub(MAX_RENDERED_LINE_BYTES);
        self.source_window_start = self.source_window_start.min(source_max_window_start);
        let source_horizontal_track_left = source_scroll_bounds.left()
            + px(dimensions.block_padding_x + source_gutter_width + 2.0);
        let source_horizontal_track_width =
            f32::from((source_scroll_bounds.size.width - px(96.0)).max(px(40.0)));
        let source_horizontal_thumb_width = if source_max_window_start > 0 {
            (source_horizontal_track_width * MAX_RENDERED_LINE_BYTES as f32
                / observed_line_bytes.max(1) as f32)
                .clamp(28.0, source_horizontal_track_width)
        } else {
            source_horizontal_track_width
        };
        let source_horizontal_thumb_left = if source_max_window_start > 0 {
            (source_horizontal_track_width - source_horizontal_thumb_width)
                * (self.source_window_start as f64 / source_max_window_start as f64) as f32
        } else {
            0.0
        };
        let source_horizontal_scrollbar = (source_max_window_start > 0).then(|| {
            div()
                .id("document-host-horizontal-scrollbar")
                .debug_selector(|| "document-host-horizontal-scrollbar".to_owned())
                .absolute()
                .left(px(dimensions.block_padding_x + source_gutter_width + 2.0))
                .right(px(18.0))
                .bottom(px(2.0))
                .h(px(12.0))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                        let next = source_window_start_from_pointer(
                            event.position.x,
                            source_horizontal_track_left,
                            source_horizontal_track_width,
                            source_horizontal_thumb_width,
                            source_max_window_start,
                        );
                        this.set_source_window_start(next, cx);
                    }),
                )
                .on_mouse_move(cx.listener(
                    move |this, event: &gpui::MouseMoveEvent, _window, cx| {
                        if event.dragging() {
                            let next = source_window_start_from_pointer(
                                event.position.x,
                                source_horizontal_track_left,
                                source_horizontal_track_width,
                                source_horizontal_thumb_width,
                                source_max_window_start,
                            );
                            this.set_source_window_start(next, cx);
                        }
                    },
                ))
                .child(
                    div()
                        .absolute()
                        .left(px(source_horizontal_thumb_left))
                        .bottom(px(2.0))
                        .w(px(source_horizontal_thumb_width))
                        .h(px(7.0))
                        .rounded(px(999.0))
                        .bg(colors.scrollbar_thumb),
                )
        });

        let structured_panel_available = self.structured_index.is_some();

        let json_graph = self.probe.format == DocumentFormat::Json;
        let body = if self.view_mode == DocumentHostViewMode::Split && json_graph {
            let split_ratio = self.json_split_ratio.clamp(0.3, 0.7);
            let split_divider_active =
                self.json_split_drag.is_some() || self.json_split_focus_handle.is_focused(window);
            div()
                .id("json-graph-split-view")
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .relative()
                .on_mouse_move(
                    cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                        if !event.dragging() {
                            return;
                        }
                        let Some((origin_x, origin_ratio)) = this.json_split_drag else {
                            return;
                        };
                        let delta = f32::from(event.position.x) - origin_x;
                        let ratio =
                            (origin_ratio + delta / viewport_width.max(1.0)).clamp(0.3, 0.7);
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
                        .w(relative(split_ratio))
                        .h_full()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .child(
                            div()
                                .size_full()
                                .flex()
                                .justify_center()
                                .bg(colors.editor_background)
                                .px(px(dimensions.editor_padding))
                                .pt(px(source_top_padding))
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
                        .w(px(7.0))
                        .h_full()
                        .flex_shrink_0()
                        .relative()
                        .cursor_col_resize()
                        .tab_index(0)
                        .track_focus(&self.json_split_focus_handle)
                        .hover(|divider| divider.bg(colors.text_link.opacity(0.08)))
                        .focus(|divider| divider.bg(colors.text_link.opacity(0.08)))
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(px(3.0))
                                .w(px(1.0))
                                .bg(if split_divider_active {
                                    colors.text_link.opacity(0.72)
                                } else {
                                    colors.dialog_border
                                })
                                .debug_selector(|| "json-graph-split-divider-line".to_owned()),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                this.json_split_drag =
                                    Some((f32::from(event.position.x), this.json_split_ratio));
                                cx.notify();
                            }),
                        )
                        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                            let step = if event.keystroke.modifiers.shift {
                                0.05
                            } else {
                                0.01
                            };
                            let next = match event.keystroke.key.as_str() {
                                "left" => Some(this.json_split_ratio - step),
                                "right" => Some(this.json_split_ratio + step),
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
                        })),
                )
                .child(
                    div()
                        .id("json-graph-split-preview")
                        .debug_selector(|| "json-graph-split-preview".to_owned())
                        .flex_1()
                        .h_full()
                        .min_w(px(0.0))
                        .child(self.render_json_graph_panel(
                            (viewport_width * (1.0 - split_ratio) - 7.0).max(1.0),
                            viewport_height,
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
                                .px(px(dimensions.editor_padding))
                                .pt(px(source_top_padding))
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
                        .px(px(dimensions.editor_padding))
                        .pt(px(source_top_padding))
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
        };
        let search_panel = self.search_visible.then(|| {
            let count: SharedString = if let Some(error) = &self.search_error {
                error.clone()
            } else if self.search_running {
                strings.large_document_text("searching").into()
            } else if self.search_results.is_empty() {
                strings.large_document_text("no_results").into()
            } else {
                format!(
                    "{} / {}{}",
                    self.search_selected + 1,
                    self.search_results.len(),
                    if self.search_results.len() == self.search_options.result_limit {
                        "+"
                    } else {
                        ""
                    }
                )
                .into()
            };
            let option_button = |id: &'static str, icon: &'static str, active: bool| {
                div()
                    .id(id)
                    .debug_selector(move || id.to_owned())
                    .debug_selector(move || id.to_owned())
                    .size(px(26.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .border(px(1.0))
                    .border_color(if active {
                        colors.text_link
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .bg(if active {
                        colors.dialog_secondary_button_hover
                    } else {
                        colors.dialog_surface
                    })
                    .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .child(
                        svg()
                            .path(icon)
                            .size(px(15.0))
                            .text_color(colors.dialog_body),
                    )
            };
            div()
                .id("document-host-find-panel")
                .debug_selector(|| "document-host-find-panel".to_owned())
                .absolute()
                .top(px(8.0))
                .right(px(12.0))
                .w(px(540.0))
                .max_w(relative(0.94))
                .h(px(46.0))
                .p(px(6.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .occlude()
                .bg(colors.dialog_surface)
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .rounded(px(7.0))
                .shadow_md()
                .child(
                    div()
                        .id("document-host-search-input")
                        .debug_selector(|| "document-host-search-input".to_owned())
                        .w(px(210.0))
                        .h(px(30.0))
                        .px(px(7.0))
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .rounded(px(5.0))
                        .border(px(1.0))
                        .border_color(colors.dialog_border)
                        .child(self.search_input.clone()),
                )
                .child(
                    div()
                        .id("document-host-search-count")
                        .w(px(74.0))
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .text_color(colors.dialog_muted)
                        .child(count),
                )
                .child(
                    option_button(
                        "document-host-search-case",
                        FIND_CASE_ICON,
                        self.search_options.case_sensitive,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_search_option(
                            |options| options.case_sensitive = !options.case_sensitive,
                            cx,
                        );
                    })),
                )
                .child(
                    option_button(
                        "document-host-search-word",
                        FIND_WORD_ICON,
                        self.search_options.whole_word,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_search_option(
                            |options| options.whole_word = !options.whole_word,
                            cx,
                        );
                    })),
                )
                .child(
                    option_button(
                        "document-host-search-regex",
                        FIND_REGEX_ICON,
                        self.search_options.regex,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_search_option(|options| options.regex = !options.regex, cx);
                    })),
                )
                .child(
                    option_button("document-host-search-previous", CHEVRON_UP_ICON, false)
                        .on_click(cx.listener(|this, _, _, cx| this.navigate_search(-1, cx))),
                )
                .child(
                    option_button("document-host-search-next", CHEVRON_DOWN_ICON, false)
                        .on_click(cx.listener(|this, _, _, cx| this.navigate_search(1, cx))),
                )
                .child(
                    option_button("document-host-search-close", CLOSE_ICON, false).on_click(
                        cx.listener(|this, _, window, cx| {
                            this.search_visible = false;
                            this.focus_handle.focus(window);
                            cx.notify();
                        }),
                    ),
                )
        });

        let navigation_panel = self.navigation_visible.then(|| {
            div()
                .id("document-host-navigation-panel")
                .debug_selector(|| "document-host-navigation-panel".to_owned())
                .absolute()
                .top(px(8.0))
                .right(px(12.0))
                .w(px(330.0))
                .max_w(relative(0.94))
                .h(px(46.0))
                .p(px(6.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .occlude()
                .bg(colors.dialog_surface)
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .rounded(px(7.0))
                .shadow_md()
                .child(
                    div()
                        .id("document-host-navigation-kind")
                        .w(px(54.0))
                        .h(px(30.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.0))
                        .cursor_pointer()
                        .bg(colors.dialog_secondary_button_bg)
                        .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                        .text_size(px(12.0))
                        .text_color(colors.dialog_secondary_button_text)
                        .child(
                            strings
                                .large_document_text(if self.navigation_is_byte {
                                    "byte"
                                } else {
                                    "line"
                                })
                                .to_owned(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.navigation_is_byte = !this.navigation_is_byte;
                            let placeholder = cx
                                .global::<I18nManager>()
                                .strings()
                                .large_document_text(if this.navigation_is_byte {
                                    "go_to_byte"
                                } else {
                                    "go_to_line"
                                })
                                .to_owned();
                            this.navigation_input
                                .update(cx, |input, _cx| input.set_input_placeholder(placeholder));
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id("document-host-navigation-input")
                        .debug_selector(|| "document-host-navigation-input".to_owned())
                        .flex_1()
                        .min_w(px(0.0))
                        .h(px(30.0))
                        .px(px(7.0))
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .rounded(px(5.0))
                        .border(px(1.0))
                        .border_color(colors.dialog_border)
                        .child(self.navigation_input.clone()),
                )
                .child(
                    div()
                        .id("document-host-navigation-close")
                        .debug_selector(|| "document-host-navigation-close".to_owned())
                        .size(px(26.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                        .child(
                            svg()
                                .path(CLOSE_ICON)
                                .size(px(15.0))
                                .text_color(colors.dialog_body),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.navigation_visible = false;
                            this.focus_handle.focus(window);
                            cx.notify();
                        })),
                )
        });

        let structure_banner = self.structure_error.as_ref().map(|message| {
            let byte_offset = self.structure_error_byte;
            div()
                .id("document-host-structure-notice")
                .debug_selector(|| "document-host-structure-notice".to_owned())
                .h(px(36.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .border_b(px(1.0))
                .border_color(colors.callout_warning_border)
                .bg(colors.callout_warning_bg)
                .text_color(colors.text_default)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .child(message.clone()),
                )
                .children(byte_offset.map(|offset| {
                    div()
                        .id("document-host-structure-error-jump")
                        .debug_selector(|| "document-host-structure-error-jump".to_owned())
                        .px(px(9.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .bg(colors.dialog_secondary_button_bg)
                        .text_color(colors.dialog_secondary_button_text)
                        .child(
                            strings
                                .large_document_text("go_to_byte_template")
                                .replace("{offset}", &offset.to_string()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.jump_byte_offset_to_source(offset, cx);
                        }))
                }))
        });

        let oversized_selection_banner = self
            .selected_source_byte_range()
            .filter(|range| {
                selection_transfer_for_len(range.end.saturating_sub(range.start))
                    == SelectionTransfer::ExportFile
            })
            .map(|range| {
                let selected_mib = (range.end - range.start) as f64 / (1024.0 * 1024.0);
                div()
                    .id("document-host-selection-export-notice")
                    .h(px(36.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .border_b(px(1.0))
                    .border_color(colors.callout_warning_border)
                    .bg(colors.callout_warning_bg)
                    .text_color(colors.text_default)
                    .child(
                        div().flex_1().min_w(px(0.0)).truncate().child(
                            strings
                                .large_document_text("selected_clipboard_template")
                                .replace("{mib}", &format!("{selected_mib:.1}")),
                        ),
                    )
                    .child(
                        div()
                            .id("document-host-export-selection")
                            .px(px(9.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .bg(colors.dialog_primary_button_bg)
                            .text_color(colors.dialog_primary_button_text)
                            .child(strings.large_document_text("export_selection").to_owned())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_export_selection(&ExportSelection, window, cx);
                            })),
                    )
            });

        let external_banner =
            self.coordinator.pending_external_change.as_ref().map(|_| {
                div()
                    .id("document-host-external-change-banner")
                    .debug_selector(|| "document-host-external-change-banner".to_owned())
                    .h(px(36.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .border_b(px(1.0))
                    .border_color(colors.callout_warning_border)
                    .bg(colors.callout_warning_bg)
                    .text_color(colors.text_default)
                    .child(div().flex_1().min_w(px(0.0)).truncate().child(
                        self.coordinator.external_status.clone().unwrap_or_else(|| {
                            strings.large_document_text("file_changed_disk").into()
                        }),
                    ))
                    .child(
                        div()
                            .id("document-host-external-reload")
                            .px(px(9.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .bg(colors.dialog_primary_button_bg)
                            .text_color(colors.dialog_primary_button_text)
                            .child(strings.large_document_text("reload").to_owned())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.reload_from_disk(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("document-host-external-keep-local")
                            .px(px(9.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .bg(colors.dialog_secondary_button_bg)
                            .text_color(colors.dialog_secondary_button_text)
                            .child(strings.large_document_text("keep_local").to_owned())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.keep_local_after_external_change(cx);
                            })),
                    )
            });

        let source_context_menu = self.source_context_menu.map(|position| {
            let selected_bytes = self
                .selected_source_byte_range()
                .map(|range| range.end.saturating_sub(range.start));
            let has_selection = selected_bytes.is_some_and(|bytes| bytes > 0);
            let cut_enabled = selected_bytes.is_some_and(|bytes| {
                selection_transfer_for_len(bytes) == SelectionTransfer::Clipboard
            });
            let menu_width = 190.0;
            let menu_height = 195.0;
            let left =
                f32::from(position.x).clamp(8.0, (viewport_width - menu_width - 8.0).max(8.0));
            let top =
                f32::from(position.y).clamp(8.0, (viewport_height - menu_height - 8.0).max(8.0));
            let item =
                |id: &'static str, label: String, command: SourceContextCommand, enabled: bool| {
                    div()
                        .id(id)
                        .debug_selector(move || id.to_owned())
                        .h(px(30.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .rounded(px(4.0))
                        .text_color(if enabled {
                            colors.dialog_body
                        } else {
                            colors.text_placeholder
                        })
                        .when(enabled, |row| {
                            row.cursor_pointer()
                                .hover(|row| row.bg(colors.dialog_secondary_button_hover))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.run_source_context_command(command, window, cx);
                                }))
                        })
                        .child(label)
                };
            div()
                .id("document-host-source-context-menu")
                .debug_selector(|| "document-host-source-context-menu".to_owned())
                .key_context(DOCUMENT_HOST_KEY_CONTEXT)
                .tab_index(0)
                .track_focus(&self.source_context_menu_focus_handle)
                .capture_key_down(cx.listener(Self::on_source_surface_key_down))
                .on_action(cx.listener(Self::on_dismiss_transient_ui))
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(menu_width))
                .h(px(menu_height))
                .p(px(5.0))
                .flex()
                .flex_col()
                .gap(px(1.0))
                .occlude()
                .rounded(px(6.0))
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .bg(colors.dialog_surface)
                .shadow_lg()
                .child(item(
                    "large-source-context-copy",
                    strings.large_document_text("copy").to_owned(),
                    SourceContextCommand::Copy,
                    has_selection,
                ))
                .child(item(
                    "large-source-context-cut",
                    strings.large_document_text("cut").to_owned(),
                    SourceContextCommand::Cut,
                    cut_enabled,
                ))
                .child(item(
                    "large-source-context-paste",
                    strings.large_document_text("paste").to_owned(),
                    SourceContextCommand::Paste,
                    true,
                ))
                .child(item(
                    "large-source-context-select-all",
                    strings.large_document_text("select_all").to_owned(),
                    SourceContextCommand::SelectAll,
                    true,
                ))
                .child(item(
                    "large-source-context-export",
                    strings.large_document_text("export_selection").to_owned(),
                    SourceContextCommand::ExportSelection,
                    has_selection,
                ))
                .child(item(
                    "large-source-context-export-utf8",
                    strings.large_document_text("export_utf8").to_owned(),
                    SourceContextCommand::ExportSelectionUtf8,
                    has_selection,
                ))
        });

        let graph_edit_overlay = json_graph
            .then(|| self.render_json_graph_edit_overlay(viewport_width, viewport_height, cx));
        let content = div()
            .size_full()
            .flex()
            .flex_col()
            // 宿主接管活动行焦点后仍需沿用文本编辑快捷键上下文，否则 Ctrl+Y 等
            // 仅绑定在 BlockEditor 的动作无法到达这里。
            .key_context(DOCUMENT_HOST_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            // 右键打开菜单时，焦点路径里可能仍包含行内 Block；在捕获阶段关闭
            // 瞬态菜单，避免 Block 先消费 Escape 导致菜单残留。
            .capture_key_down(cx.listener(Self::on_source_surface_key_down))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_save_document))
            .on_action(cx.listener(Self::on_find_in_document))
            .on_action(cx.listener(Self::on_go_to_line))
            .on_action(cx.listener(Self::on_find_next))
            .on_action(cx.listener(Self::on_find_previous))
            .on_action(cx.listener(Self::on_dismiss_transient_ui))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_delete_back))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_export_selection))
            .on_action(cx.listener(Self::on_page_up))
            .on_action(cx.listener(Self::on_page_down))
            .on_action(cx.listener(Self::on_jump_to_top))
            .on_action(cx.listener(Self::on_jump_to_bottom))
            .bg(colors.editor_background)
            .children(external_banner)
            .children(oversized_selection_banner)
            .children(structure_banner)
            .child(body);
        div()
            .size_full()
            .relative()
            .key_context(DOCUMENT_HOST_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_dismiss_transient_ui))
            .capture_key_down(cx.listener(Self::on_source_surface_key_down))
            .child(content)
            .children(source_context_menu)
            .children(search_panel)
            .children(navigation_panel)
            .children(graph_edit_overlay.flatten())
    }
}
