// @author kongweiguang

use super::*;

impl Editor {
    /// 构建文档滚动面、虚拟化行与自定义滚动条。
    pub(super) fn render_document_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport_bounds = self.scroll_handle.bounds();
        let viewport_size = viewport_bounds.size;
        self.sync_scroll_viewport(viewport_size, cx);

        let theme = cx.global::<ThemeManager>().current_arc();
        let strings = cx.global::<I18nManager>().strings_arc();
        self.sync_window_title(window, &strings);
        self.sync_workspace_visibility_for_viewport(f32::from(window.viewport_size().width));

        let d = &theme.dimensions;
        let editor = cx.entity().downgrade();
        let max_scroll_y = f32::from(self.scroll_handle.max_offset().height.max(px(0.0)));
        let viewport_height = f32::from(viewport_bounds.size.height.max(px(1.0)));
        let viewport_width = f32::from(viewport_bounds.size.width.max(px(1.0)));
        let has_overflow = max_scroll_y > 0.5;

        let centered_width = crate::ui::centered_column_width(viewport_width, &theme.dimensions);
        let current_scroll_y = (-f32::from(self.scroll_handle.offset().y)).clamp(0.0, max_scroll_y);
        self.sync_virtual_surface_mounts(current_scroll_y, viewport_height, RENDER_OVERDRAW_PX, cx);
        let virtual_layout = self.virtual_surface_layout();
        let virtual_top_h = virtual_layout
            .as_ref()
            .map_or(0.0, |layout| layout.top_spacer);
        let virtual_bottom_h = virtual_layout
            .as_ref()
            .map_or(0.0, |layout| layout.bottom_spacer);
        let virtual_pinned_top = virtual_layout.as_ref().and_then(|layout| layout.pinned_top);
        let virtual_pinned_roots = virtual_layout
            .map(|layout| layout.pinned_roots)
            .unwrap_or_default();
        let local_scroll_y = (current_scroll_y - virtual_top_h).max(0.0);
        let visible_blocks = self.document.visible_blocks().to_vec();
        let scrollbar_geometry =
            Self::scrollbar_geometry(viewport_height, max_scroll_y, current_scroll_y);
        let track_height = scrollbar_geometry.track_height;
        let thumb_height = scrollbar_geometry.thumb_height;
        let thumb_top = scrollbar_geometry.thumb_top;

        let show_custom_scrollbar = has_overflow
            && (self.scrollbar_drag.is_some()
                || self.scrollbar_hovered
                || Instant::now() <= self.scrollbar_visible_until);
        let scrollbar_visual_width =
            if self.scrollbar_drag.is_some() || self.scrollbar_thumb_hovered {
                EDITOR_SCROLLBAR_HOVER_WIDTH
            } else {
                d.scrollbar_width
            };

        // Spacing metadata is read on demand instead of pre-collected into a
        // Vec<RenderedRowSpacingInfo> sized to all visible blocks. For long
        // documents this skips a ~tens-of-KB allocation per frame; per-block
        // entity.read_with is a cheap immutable lock + 7-field struct copy.
        let spacing_for = |index: usize| -> RenderedRowSpacingInfo {
            visible_blocks[index]
                .entity
                .read_with(cx, |block, _cx| RenderedRowSpacingInfo::from_block(block))
        };
        let cached_rows = self
            .render_row_cache
            .as_ref()
            .filter(|cache| cache.matches(&visible_blocks, d.block_gap))
            .map(|cache| cache.rows.clone());
        let rows = if let Some(rows) = cached_rows {
            rows
        } else {
            let mut previous_row_spacing = None;
            // 第一遍只在语义变化后扫描轻量行描述；滚动帧直接复用结果。
            let mut rows = Vec::new();
            let mut index = 0usize;
            while index < visible_blocks.len() {
                let first_spacing = spacing_for(index);
                let top_gap =
                    rendered_row_top_gap(previous_row_spacing, first_spacing, d.block_gap);

                if let (Some(callout_anchor), Some(variant)) =
                    (first_spacing.callout_anchor, first_spacing.callout_variant)
                {
                    let mut group_end = index;
                    while group_end < visible_blocks.len()
                        && spacing_for(group_end).callout_anchor == Some(callout_anchor)
                    {
                        group_end += 1;
                    }
                    rows.push(RenderedRowDescriptor {
                        start: index,
                        end: group_end,
                        top_gap,
                        kind: RenderedRowKind::Callout(variant),
                    });
                    previous_row_spacing = Some(spacing_for(group_end - 1));
                    index = group_end;
                    continue;
                }

                if let Some(footnote_anchor) = first_spacing.footnote_anchor {
                    let mut group_end = index;
                    while group_end < visible_blocks.len()
                        && spacing_for(group_end).footnote_anchor == Some(footnote_anchor)
                    {
                        group_end += 1;
                    }
                    rows.push(RenderedRowDescriptor {
                        start: index,
                        end: group_end,
                        top_gap,
                        kind: RenderedRowKind::Footnote,
                    });
                    previous_row_spacing = Some(spacing_for(group_end - 1));
                    index = group_end;
                    continue;
                }

                rows.push(RenderedRowDescriptor {
                    start: index,
                    end: index + 1,
                    top_gap,
                    kind: RenderedRowKind::Plain,
                });
                previous_row_spacing = Some(first_spacing);
                index += 1;
            }

            let rows = std::sync::Arc::<[RenderedRowDescriptor]>::from(rows);
            self.render_row_cache = Some(RenderedRowCache {
                visible_len: visible_blocks.len(),
                first_id: visible_blocks.first().map(|block| block.entity.entity_id()),
                last_id: visible_blocks.last().map(|block| block.entity.entity_id()),
                block_gap: d.block_gap,
                rows: rows.clone(),
            });
            rows
        };

        let row_starts: Vec<usize> = rows.iter().map(|row| row.start).collect();

        // A table cell maps to its containing table block's row. The viewport
        // stays contiguous; a far focused row is mounted absolutely below so it
        // keeps its GPUI input handler without forcing every intervening row in.
        let focus_row = self
            .focused_edit_target_entity_id(window, cx)
            .or(self.active_entity_id)
            .and_then(|id| {
                self.document.visible_index_for_entity_id(id).or_else(|| {
                    self.table_cell_binding(id).and_then(|binding| {
                        self.document
                            .visible_index_for_entity_id(binding.table_block.entity_id())
                    })
                })
            })
            .map(|visible_index| {
                row_starts
                    .partition_point(|&start| start <= visible_index)
                    .saturating_sub(1)
            });

        // A row's first block keys its cached height; its painted top (from last
        // frame) feeds the footprints below.
        let row_first_ids: Vec<EntityId> = row_starts
            .iter()
            .map(|&start| visible_blocks[start].entity.entity_id())
            .collect();
        // On a structural edit the row indices no longer match last frame, so the
        // cache refresh below is skipped; its block-keyed entries still hold.
        let structural_change = visible_blocks.len() != self.prev_visible_block_ids.len()
            || visible_blocks
                .iter()
                .zip(&self.prev_visible_block_ids)
                .any(|(visible, prev)| visible.entity.entity_id() != *prev);
        if structural_change {
            self.prev_visible_block_ids = visible_blocks
                .iter()
                .map(|v| v.entity.entity_id())
                .collect();
        }

        // Rows mounted together last frame shared one scroll offset, so their
        // adjacent painted-top differences are scroll-free heights. Caching those,
        // not raw positions, is what keeps the window stable while scrolling.
        if !structural_change {
            if let Some((prev_start, prev_end)) = self.prev_render_window {
                let prev_end = prev_end.min(row_first_ids.len());
                for row in prev_start..prev_end.saturating_sub(1) {
                    let top = visible_blocks[row_starts[row]]
                        .entity
                        .read_with(cx, |block, _cx| block.last_bounds)
                        .map(|bounds| f32::from(bounds.top()));
                    let next_top = visible_blocks[row_starts[row + 1]]
                        .entity
                        .read_with(cx, |block, _cx| block.last_bounds)
                        .map(|bounds| f32::from(bounds.top()));
                    if let (Some(top), Some(next_top)) = (top, next_top) {
                        let stride = next_top - top;
                        if stride > 0.0 && stride.is_finite() {
                            self.row_stride_cache.insert(row_first_ids[row], stride);
                        }
                    }
                }
            }
        }

        // Unmeasured rows use the minimum block height: a lower bound, so the
        // window over-mounts rather than ever landing on a spacer.
        let estimate = d.block_min_height.max(1.0);
        let strides: Vec<f32> = row_first_ids
            .iter()
            .map(|id| self.row_stride_cache.get(id).copied().unwrap_or(estimate))
            .collect();

        // Bound the cache against block churn, only when it outgrows the live rows.
        if self.row_stride_cache.len() > row_first_ids.len().saturating_mul(2) {
            let live: std::collections::HashSet<EntityId> = row_first_ids.iter().copied().collect();
            self.row_stride_cache.retain(|id, _| live.contains(id));
        }

        let render_window = Self::rendered_window(
            &strides,
            local_scroll_y,
            viewport_height,
            RENDER_OVERDRAW_PX,
            None,
        );
        self.prev_render_window = Some((render_window.run_start, render_window.run_end));
        let detached_focus = focus_row
            .filter(|row| *row < render_window.run_start || *row >= render_window.run_end)
            .map(|row| {
                let top = strides[..row]
                    .iter()
                    .map(|stride| stride.max(0.0))
                    .sum::<f32>();
                (row, top)
            });

        // The first mounted row re-applies its `mt`, so drop it from the top
        // spacer to avoid shifting content down by a gap.
        let top_h = virtual_top_h
            + match rows.get(render_window.run_start) {
                Some(row) => (render_window.top_h - row.top_gap).max(0.0),
                None => render_window.top_h,
            };
        let mut block_rows: Vec<AnyElement> =
            Vec::with_capacity(render_window.run_end - render_window.run_start + 2);
        if top_h > 0.5 {
            block_rows.push(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .h(px(top_h))
                    .into_any_element(),
            );
        }
        for (mounted_index, descriptor) in rows[render_window.run_start..render_window.run_end]
            .iter()
            .enumerate()
        {
            let descriptor_index = render_window.run_start + mounted_index;
            let render_entity_row = |entity: Entity<Block>, top_gap: f32| {
                let row = div()
                    .w_full()
                    .flex_shrink_0()
                    .mt(px(top_gap))
                    .child(entity.clone());
                if self.view_mode == super::ViewMode::Rendered {
                    let row_editor = editor.clone();
                    let entity_id = entity.entity_id();
                    row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                        let _ = row_editor.update(cx, |editor, cx| {
                            editor.on_block_context_menu_mouse_down(entity_id, event, window, cx);
                        });
                    })
                    .into_any_element()
                } else {
                    row.into_any_element()
                }
            };

            let heading = visible_blocks[descriptor.start]
                .entity
                .read_with(cx, |block, _cx| {
                    matches!(block.kind(), crate::components::BlockKind::Heading { .. })
                });
            let opacity = super::focus_modes::focus_row_opacity(
                self.focus_mode,
                focus_row == Some(descriptor_index),
                heading,
            );
            let element = match descriptor.kind {
                RenderedRowKind::Plain => div()
                    .w(px(centered_width))
                    .max_w(relative(1.0))
                    .flex_shrink_0()
                    .opacity(opacity)
                    .mt(px(descriptor.top_gap))
                    .child(render_entity_row(
                        visible_blocks[descriptor.start].entity.clone(),
                        0.0,
                    ))
                    .into_any_element(),
                RenderedRowKind::Footnote => {
                    let mut children = Vec::with_capacity(descriptor.end - descriptor.start);
                    let mut previous = None;
                    for row_index in descriptor.start..descriptor.end {
                        let spacing = spacing_for(row_index);
                        children.push(render_entity_row(
                            visible_blocks[row_index].entity.clone(),
                            footnote_row_top_gap(previous, d.block_gap),
                        ));
                        previous = Some(spacing);
                    }
                    div()
                        .w(px(centered_width))
                        .max_w(relative(1.0))
                        .flex_shrink_0()
                        .opacity(opacity)
                        .mt(px(descriptor.top_gap))
                        .child(footnote_group_shell(children, &theme, d))
                        .into_any_element()
                }
                RenderedRowKind::Callout(variant) => {
                    let mut children = Vec::new();
                    let mut previous_callout = None;
                    let mut row_index = descriptor.start;
                    while row_index < descriptor.end {
                        let spacing = spacing_for(row_index);
                        if let Some(footnote_anchor) = spacing.footnote_anchor {
                            let mut footnote_children = Vec::new();
                            let mut previous_footnote = None;
                            let group_start_spacing = spacing;
                            while row_index < descriptor.end
                                && spacing_for(row_index).footnote_anchor == Some(footnote_anchor)
                            {
                                let footnote_spacing = spacing_for(row_index);
                                footnote_children.push(render_entity_row(
                                    visible_blocks[row_index].entity.clone(),
                                    footnote_row_top_gap(previous_footnote, d.block_gap),
                                ));
                                previous_footnote = Some(footnote_spacing);
                                row_index += 1;
                            }
                            children.push(
                                div()
                                    .w_full()
                                    .flex_shrink_0()
                                    .mt(px(callout_row_top_gap(
                                        previous_callout,
                                        group_start_spacing,
                                        d,
                                    )))
                                    .child(footnote_group_shell(footnote_children, &theme, d))
                                    .into_any_element(),
                            );
                            previous_callout = previous_footnote;
                            continue;
                        }
                        children.push(render_entity_row(
                            visible_blocks[row_index].entity.clone(),
                            callout_row_top_gap(previous_callout, spacing, d),
                        ));
                        previous_callout = Some(spacing);
                        row_index += 1;
                    }
                    let (accent, background) = callout_colors(variant, &theme);
                    div()
                        .w(px(centered_width))
                        .max_w(relative(1.0))
                        .flex_shrink_0()
                        .opacity(opacity)
                        .mt(px(descriptor.top_gap))
                        .px(px(crate::components::rendered_content_inset(d)))
                        .child(
                            div()
                                .debug_selector(|| "callout-surface".to_owned())
                                .w_full()
                                .min_w(px(0.0))
                                .flex()
                                .flex_col()
                                .gap(px(0.0))
                                .px(px(d.callout_padding_x))
                                .py(px(d.callout_padding_y))
                                .rounded(px(d.callout_radius))
                                .border_l(px(d.callout_border_width))
                                .border_color(accent)
                                .bg(background)
                                .children(children),
                        )
                        .into_any_element()
                }
            };
            block_rows.push(element);
        }
        let bottom_h = render_window.bottom_h + virtual_bottom_h;
        if bottom_h > 0.5 {
            block_rows.push(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .h(px(bottom_h))
                    .into_any_element(),
            );
        }

        let scroll_content = div()
            .id("editor-scroll-inner")
            .relative()
            .flex()
            .flex_col()
            .flex_grow()
            .h_full()
            .items_center()
            .bg(theme.colors.editor_background)
            .overflow_y_scroll()
            .scrollbar_width(px(0.0))
            .track_scroll(&self.scroll_handle)
            .on_hover(cx.listener(Self::on_editor_hover))
            .capture_any_mouse_down(cx.listener(Self::on_editor_capture_mouse_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_editor_mouse_down))
            .on_mouse_move(cx.listener(Self::on_editor_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_editor_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_editor_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_editor_scroll_wheel))
            .px(px(d.editor_padding))
            .pt(px(editor_top_padding(
                self.typewriter_mode,
                viewport_height,
            )))
            .pb(px(editor_bottom_padding(viewport_height, d)))
            .children(block_rows);
        let scroll_content = if let Some((focus_row, top)) = detached_focus {
            let entity = visible_blocks[row_starts[focus_row]].entity.clone();
            scroll_content.child(
                div()
                    .absolute()
                    .top(px(top))
                    .left_0()
                    .right_0()
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w(px(centered_width))
                            .max_w(relative(1.0))
                            .child(entity),
                    ),
            )
        } else {
            scroll_content
        };
        let scroll_content = if let Some(top) = virtual_pinned_top
            && !virtual_pinned_roots.is_empty()
        {
            scroll_content.child(
                div()
                    .absolute()
                    .top(px(top))
                    .left_0()
                    .right_0()
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w(px(centered_width))
                            .max_w(relative(1.0))
                            .flex()
                            .flex_col()
                            .children(virtual_pinned_roots),
                    ),
            )
        } else {
            scroll_content
        };
        let scroll_content = if self.view_mode == super::ViewMode::Rendered {
            scroll_content.on_mouse_down(
                MouseButton::Right,
                cx.listener(Self::on_editor_context_menu_mouse_down),
            )
        } else {
            scroll_content
        };

        let content_area = div()
            .id("editor-scroll")
            .debug_selector(|| "editor-source-pane".to_owned())
            .w_full()
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .bg(theme.colors.editor_background)
            .relative()
            .child(scroll_content);

        let content_area = if show_custom_scrollbar {
            let scrollbar_editor = editor.clone();
            let track_origin_y = f32::from(viewport_bounds.origin.y);
            content_area.child(
                div()
                    .id("editor-scrollbar-hitbox")
                    .debug_selector(|| "editor-scrollbar-hitbox".to_owned())
                    .absolute()
                    .occlude()
                    .top(px(thumb_top))
                    .right(px(d.scrollbar_right))
                    .w(px(EDITOR_SCROLLBAR_HIT_WIDTH))
                    .h(px(thumb_height))
                    .cursor_pointer()
                    .on_hover(cx.listener(Self::on_scrollbar_thumb_hover))
                    .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                        let pointer_offset_y =
                            f32::from(event.position.y) - track_origin_y - thumb_top;
                        let _ = scrollbar_editor.update(cx, |editor, cx| {
                            cx.stop_propagation();
                            editor.start_scrollbar_drag(
                                pointer_offset_y,
                                track_height,
                                thumb_height,
                                max_scroll_y,
                                cx,
                            );
                        });
                    })
                    .child(
                        div()
                            .id("editor-scrollbar-thumb")
                            .debug_selector(|| "editor-scrollbar-thumb".to_owned())
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .right_0()
                            .w(px(scrollbar_visual_width))
                            .rounded(px(999.0))
                            .bg(theme.colors.scrollbar_thumb),
                    )
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |_thumb_bounds, _, window, _| {
                                window.on_mouse_event({
                                    let editor = editor.clone();
                                    move |_event: &MouseUpEvent, phase, _window, cx| {
                                        if !phase.bubble() {
                                            return;
                                        }
                                        let _ = editor.update(cx, |editor, cx| {
                                            editor.end_scrollbar_drag(cx);
                                        });
                                    }
                                });

                                window.on_mouse_event({
                                    let editor = editor.clone();
                                    move |event: &MouseMoveEvent, phase, _window, cx| {
                                        if !phase.bubble() || !event.dragging() {
                                            return;
                                        }

                                        let pointer_y_in_track =
                                            f32::from(event.position.y) - track_origin_y;
                                        let _ = editor.update(cx, |editor, cx| {
                                            editor.update_scrollbar_drag(pointer_y_in_track, cx);
                                        });
                                    }
                                });
                            },
                        )
                        .size_full(),
                    ),
            )
        } else {
            content_area
        };
        content_area.into_any_element()
    }
}
