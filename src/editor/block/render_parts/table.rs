// @author kongweiguang

use super::*;

impl Block {
    /// 渲染原生表格及其行列选择、追加控制和轴向菜单交互。
    pub(super) fn render_table_content(
        &mut self,
        focused_base: Stateful<Div>,
        focused: bool,
        is_placeholder: bool,
        theme: &Theme,
        strings: &I18nStrings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let Some(runtime) = self.table_runtime.clone() else {
            return focused_base
                .text_size(px(t.text_size))
                .text_color(c.text_default)
                .line_height(rems(t.text_line_height))
                .child(self.render_text_or_mixed_inline_visuals(
                    theme,
                    focused,
                    is_placeholder,
                    None,
                    None,
                    c.text_default,
                    t.text_size,
                    FontWeight::NORMAL,
                    cx,
                ))
                .into_any_element();
        };

        let viewport_width = f32::from(window.viewport_size().width.max(px(1.0)));
        let table_width = effective_table_width(self, viewport_width, d);
        let column_layout = self
            .record
            .table
            .as_ref()
            .map(|table| TableColumnLayout::measure(table, table_width, window, theme))
            .unwrap_or_else(|| TableColumnLayout::equal(runtime.header.len()));
        let preview_marker = self.table_axis_preview;
        let selected_marker = self.table_axis_selection;
        let body_row_count = runtime.rows.len();
        let append_extent = px(d.table_append_button_extent);
        let append_inset = px(d.table_append_button_inset);
        let activation_band = if self.is_read_only() {
            px(0.0)
        } else {
            px(d.table_append_activation_band)
        };
        let top_gutter = if column_axis_gutter_visible(preview_marker, selected_marker) {
            activation_band
        } else {
            px(0.0)
        };
        let column_append_top = top_gutter + activation_band;
        let column_control_visible = !self.is_read_only() && self.table_append_column_hovered;
        let row_control_visible = !self.is_read_only() && self.table_append_row_hovered;
        let right_gutter = if column_control_visible {
            append_extent + append_inset
        } else {
            px(0.0)
        };
        let bottom_gutter = if row_control_visible {
            append_extent + append_inset
        } else {
            px(0.0)
        };
        let weak_table_block = cx.entity().downgrade();

        let header_cells = runtime.header;
        let column_axis_row = (top_gutter > px(0.0)).then(|| {
            div().w_full().h(top_gutter).flex().gap(px(0.0)).children(
                header_cells.iter().enumerate().map(|(column, _cell)| {
                    let hover_block = weak_table_block.clone();
                    let select_block = weak_table_block.clone();
                    let menu_block = weak_table_block.clone();
                    let marker = crate::components::TableAxisMarker {
                        kind: TableAxisKind::Column,
                        index: column,
                    };
                    let band_bg = if selected_marker == Some(marker) {
                        c.table_axis_selected_bg
                    } else if preview_marker == Some(marker) {
                        c.table_axis_preview_bg
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    };
                    div()
                        .relative()
                        .flex_none()
                        .flex_basis(relative(column_layout.fraction(column)))
                        .w(relative(column_layout.fraction(column)))
                        .h_full()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .id(ElementId::Name(
                                    format!("table-column-axis-band-{}-{}", self.record.id, column)
                                        .into(),
                                ))
                                .w_full()
                                .h_full()
                                .rounded(px(6.0))
                                .bg(band_bg)
                                .cursor_pointer()
                                .on_hover(move |hovered, _window, cx| {
                                    let _ = hover_block.update(cx, |_block, cx| {
                                        cx.emit(BlockEvent::RequestTableAxisPreview {
                                            kind: TableAxisKind::Column,
                                            index: column,
                                            hovered: *hovered,
                                        });
                                    });
                                })
                                .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                                    let _ = select_block.update(cx, |_block, cx| {
                                        cx.stop_propagation();
                                        cx.emit(BlockEvent::RequestOpenTableAxisMenu {
                                            kind: TableAxisKind::Column,
                                            index: column,
                                            position: event.position,
                                        });
                                    });
                                })
                                .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                                    let _ = menu_block.update(cx, |_block, cx| {
                                        cx.stop_propagation();
                                        cx.emit(BlockEvent::RequestOpenTableAxisMenu {
                                            kind: TableAxisKind::Column,
                                            index: column,
                                            position: event.position,
                                        });
                                    });
                                })
                                .block_mouse_except_scroll(),
                        )
                }),
            )
        });

        let header_hover_block = weak_table_block.clone();
        let header_select_block = weak_table_block.clone();
        let header_menu_block = weak_table_block.clone();
        // The header is visual row 0; its handle uses a more opaque
        // version of the body-row color to signal its distinct role.
        let header_marker = crate::components::TableAxisMarker {
            kind: TableAxisKind::Row,
            index: 0,
        };
        let header_band_bg = if selected_marker == Some(header_marker) {
            header_axis_emphasis(c.table_axis_selected_bg)
        } else if preview_marker == Some(header_marker) {
            header_axis_emphasis(c.table_axis_preview_bg)
        } else {
            hsla(0.0, 0.0, 0.0, 0.0)
        };
        let header_row = div()
            .relative()
            .w_full()
            .flex()
            .gap(px(0.0))
            .child(
                // Left-edge band mirrors the body rows so the header row
                // can be hovered, selected, and right-clicked just like
                // them, with the Header Row toggle added to its menu.
                div()
                    .id(ElementId::Name(
                        format!("table-header-axis-band-{}", self.record.id).into(),
                    ))
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(-activation_band)
                    .w(activation_band)
                    .rounded(px(6.0))
                    .bg(header_band_bg)
                    .cursor_pointer()
                    .on_hover(move |hovered, _window, cx| {
                        let _ = header_hover_block.update(cx, |_block, cx| {
                            cx.emit(BlockEvent::RequestTableAxisPreview {
                                kind: TableAxisKind::Row,
                                index: 0,
                                hovered: *hovered,
                            });
                        });
                    })
                    .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                        let _ = header_select_block.update(cx, |_block, cx| {
                            cx.stop_propagation();
                            cx.emit(BlockEvent::RequestOpenTableAxisMenu {
                                kind: TableAxisKind::Row,
                                index: 0,
                                position: event.position,
                            });
                        });
                    })
                    .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                        let _ = header_menu_block.update(cx, |_block, cx| {
                            cx.stop_propagation();
                            cx.emit(BlockEvent::RequestOpenTableAxisMenu {
                                kind: TableAxisKind::Row,
                                index: 0,
                                position: event.position,
                            });
                        });
                    })
                    .block_mouse_except_scroll(),
            )
            .children(header_cells.into_iter().enumerate().map(|(column, cell)| {
                let hover_block = weak_table_block.clone();
                let select_block = weak_table_block.clone();
                let menu_block = weak_table_block.clone();
                div()
                    .relative()
                    .flex_none()
                    .flex_basis(relative(column_layout.fraction(column)))
                    .w(relative(column_layout.fraction(column)))
                    .h_full()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .id(ElementId::Name(
                                format!(
                                    "table-column-axis-activation-{}-{}",
                                    self.record.id, column
                                )
                                .into(),
                            ))
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(activation_band)
                            .cursor_pointer()
                            .on_hover(move |hovered, _window, cx| {
                                let _ = hover_block.update(cx, |_block, cx| {
                                    cx.emit(BlockEvent::RequestTableAxisPreview {
                                        kind: TableAxisKind::Column,
                                        index: column,
                                        hovered: *hovered,
                                    });
                                });
                            })
                            .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                                let _ = select_block.update(cx, |_block, cx| {
                                    cx.stop_propagation();
                                    cx.emit(BlockEvent::RequestOpenTableAxisMenu {
                                        kind: TableAxisKind::Column,
                                        index: column,
                                        position: event.position,
                                    });
                                });
                            })
                            .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                                let _ = menu_block.update(cx, |_block, cx| {
                                    cx.stop_propagation();
                                    cx.emit(BlockEvent::RequestOpenTableAxisMenu {
                                        kind: TableAxisKind::Column,
                                        index: column,
                                        position: event.position,
                                    });
                                });
                            })
                            .block_mouse_except_scroll(),
                    )
                    .child(cell)
            }));

        let body_rows = runtime
            .rows
            .into_iter()
            .enumerate()
            .map(|(body_row_index, row)| {
                let hover_block = weak_table_block.clone();
                let select_block = weak_table_block.clone();
                let menu_block = weak_table_block.clone();
                // Row selections are addressed by visual index, where
                // the header is `0` and body rows follow at `1..`.
                let visual_row = body_row_index + 1;
                let marker = crate::components::TableAxisMarker {
                    kind: TableAxisKind::Row,
                    index: visual_row,
                };
                let band_bg = if selected_marker == Some(marker) {
                    c.table_axis_selected_bg
                } else if preview_marker == Some(marker) {
                    c.table_axis_preview_bg
                } else {
                    hsla(0.0, 0.0, 0.0, 0.0)
                };
                div()
                    .relative()
                    .w_full()
                    .flex()
                    .gap(px(0.0))
                    .child(
                        div()
                            .id(ElementId::Name(
                                format!(
                                    "table-row-axis-band-{}-{}",
                                    self.record.id, body_row_index
                                )
                                .into(),
                            ))
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(-activation_band)
                            .w(activation_band)
                            .rounded(px(6.0))
                            .bg(band_bg)
                            .cursor_pointer()
                            .on_hover(move |hovered, _window, cx| {
                                let _ = hover_block.update(cx, |_block, cx| {
                                    cx.emit(BlockEvent::RequestTableAxisPreview {
                                        kind: TableAxisKind::Row,
                                        index: visual_row,
                                        hovered: *hovered,
                                    });
                                });
                            })
                            .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                                let _ = select_block.update(cx, |_block, cx| {
                                    cx.stop_propagation();
                                    cx.emit(BlockEvent::RequestOpenTableAxisMenu {
                                        kind: TableAxisKind::Row,
                                        index: visual_row,
                                        position: event.position,
                                    });
                                });
                            })
                            .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                                let _ = menu_block.update(cx, |_block, cx| {
                                    cx.stop_propagation();
                                    cx.emit(BlockEvent::RequestOpenTableAxisMenu {
                                        kind: TableAxisKind::Row,
                                        index: visual_row,
                                        position: event.position,
                                    });
                                });
                            })
                            .block_mouse_except_scroll(),
                    )
                    .children(row.into_iter().enumerate().map(|(column, cell)| {
                        div()
                            .flex_none()
                            .flex_basis(relative(column_layout.fraction(column)))
                            .w(relative(column_layout.fraction(column)))
                            .h_full()
                            .min_w(px(0.0))
                            .child(cell)
                    }))
            });

        {
            let mut rows = Vec::with_capacity(2 + body_row_count);
            if let Some(column_axis_row) = column_axis_row {
                rows.push(column_axis_row.into_any_element());
            }
            rows.push(header_row.into_any_element());
            rows.extend(body_rows.map(|row| row.into_any_element()));

            let column_edge_band = div()
                .id(ElementId::Name(
                    format!("table-append-column-edge-{}", self.record.id).into(),
                ))
                .absolute()
                .top(column_append_top)
                .bottom(bottom_gutter)
                .right(right_gutter)
                .w(activation_band)
                .on_hover(cx.listener(Self::on_table_append_column_edge_hover));

            let row_edge_band = div()
                .id(ElementId::Name(
                    format!("table-append-row-edge-{}", self.record.id).into(),
                ))
                .absolute()
                .left_0()
                .right(right_gutter)
                .bottom(bottom_gutter)
                .h(activation_band)
                .on_hover(cx.listener(Self::on_table_append_row_edge_hover));

            let column_control = {
                let append_column_tooltip: SharedString =
                    strings.table_append_column.clone().into();
                let base = div()
                    .id(ElementId::Name(
                        format!("table-append-column-zone-{}", self.record.id).into(),
                    ))
                    .absolute()
                    .top(column_append_top)
                    .bottom(bottom_gutter)
                    .right_0()
                    .w(right_gutter)
                    .on_hover(cx.listener(Self::on_table_append_column_zone_hover));

                if column_control_visible {
                    base.child(
                        div()
                            .id(ElementId::Name(
                                format!("table-append-column-button-{}", self.record.id).into(),
                            ))
                            .debug_selector(|| "table-append-column-button".to_owned())
                            .absolute()
                            .top(append_inset)
                            .bottom_0()
                            .right_0()
                            .w(append_extent)
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(999.0))
                            .bg(c.table_append_button_bg)
                            .hover(|this| this.bg(c.table_append_button_hover))
                            .cursor_pointer()
                            .tooltip(move |_window, cx| {
                                crate::ui::ui_tooltip(append_column_tooltip.clone(), cx)
                            })
                            .text_size(px(t.text_size))
                            .text_color(c.table_append_button_text)
                            .block_mouse_except_scroll()
                            .on_hover(cx.listener(Self::on_table_append_column_button_hover))
                            .on_click(cx.listener(Self::on_append_table_column))
                            .child(svg().path(PLUS_ICON).size(px(14.0))),
                    )
                } else {
                    base
                }
            };

            let row_control = {
                let append_row_tooltip: SharedString = strings.table_append_row.clone().into();
                let base = div()
                    .id(ElementId::Name(
                        format!("table-append-row-zone-{}", self.record.id).into(),
                    ))
                    .absolute()
                    .left_0()
                    .right(right_gutter)
                    .bottom_0()
                    .h(bottom_gutter)
                    .on_hover(cx.listener(Self::on_table_append_row_zone_hover));

                if row_control_visible {
                    base.child(
                        div()
                            .id(ElementId::Name(
                                format!("table-append-row-button-{}", self.record.id).into(),
                            ))
                            .debug_selector(|| "table-append-row-button".to_owned())
                            .absolute()
                            .left(append_inset)
                            .right(append_inset)
                            .bottom_0()
                            .h(append_extent)
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(999.0))
                            .bg(c.table_append_button_bg)
                            .hover(|this| this.bg(c.table_append_button_hover))
                            .cursor_pointer()
                            .tooltip(move |_window, cx| {
                                crate::ui::ui_tooltip(append_row_tooltip.clone(), cx)
                            })
                            .text_size(px(t.text_size))
                            .text_color(c.table_append_button_text)
                            .block_mouse_except_scroll()
                            .on_hover(cx.listener(Self::on_table_append_row_button_hover))
                            .on_click(cx.listener(Self::on_append_table_row))
                            .child(svg().path(PLUS_ICON).size(px(14.0))),
                    )
                } else {
                    base
                }
            };

            let table_surface = div()
                .debug_selector(|| "table-surface".to_owned())
                .w_full()
                .min_w(px(0.0))
                .relative()
                .flex()
                .flex_col()
                .pr(right_gutter)
                .pb(bottom_gutter)
                .gap(px(0.0))
                .children(rows)
                .child(column_edge_band)
                .child(row_edge_band)
                .child(column_control)
                .child(row_control);
            focused_base.child(table_surface).into_any_element()
        }
    }
}
