// @author kongweiguang

use super::*;

impl DocumentHost {
    /// 构建结构化 CSV/JSON/Markdown 表格面板；源码滚动面保持独立所有权。
    pub(super) fn render_structured_panel(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = cx.global::<ThemeManager>().current_arc();
        let strings = cx.global::<I18nManager>().strings_arc();
        let colors = &theme.colors;
        let line_text_color = colors.text_default;
        let line_number_color = colors.text_placeholder;
        let structured_border_color = colors.dialog_border;
        let structured_selection_color = colors.table_axis_selected_bg;
        let mut structured_headers = self
            .structured_index
            .as_ref()
            .map(|index| index.localized_headers(&strings))
            .unwrap_or_default();
        for (target, value) in &self.structured_cell_overrides {
            if target.record.is_none()
                && let Some(header) = structured_headers.get_mut(target.column)
            {
                header.clone_from(value);
            }
        }
        let loading_text = strings.large_document_text("loading");
        // 只从表头和已加载视口抽样列宽，避免为 10 GB CSV 扫描整列或固定宽度浪费空间。
        let structured_column_widths = structured_headers
            .iter()
            .enumerate()
            .map(|(column, header)| {
                let sampled_chars = self
                    .structured_rows
                    .values()
                    .chain(self.json_rows.values())
                    .filter_map(|row| {
                        column
                            .checked_sub(row.column_start)
                            .and_then(|relative| row.cells.get(relative))
                    })
                    .map(|cell| cell.chars().take(48).count())
                    .fold(header.chars().take(48).count(), usize::max);
                (28.0 + sampled_chars as f32 * 7.2).clamp(96.0, 374.0)
            })
            .collect::<Vec<_>>();
        let structured_column_count = structured_headers.len();
        let visible_structured_headers = structured_headers
            .into_iter()
            .enumerate()
            .skip(self.structured_column_window_start)
            .take(STRUCTURED_COLUMN_WINDOW)
            .filter(|(column, _)| !self.hidden_structured_columns.contains(column))
            .collect::<Vec<_>>();
        let structured_width = 76.0
            + visible_structured_headers
                .iter()
                .map(|(column, _)| {
                    structured_column_widths
                        .get(*column)
                        .copied()
                        .unwrap_or(STRUCTURED_CELL_WIDTH)
                })
                .sum::<f32>()
                .max(STRUCTURED_CELL_WIDTH);
        let json_structure = matches!(self.structured_index, Some(StructuredIndex::Json { .. }));
        let structured_live = self.view_mode == DocumentHostViewMode::Live
            && matches!(self.structured_index, Some(StructuredIndex::Delimited(_)));
        let structured_count = if json_structure {
            self.json_root_index()
                .map_or(0, |root| self.json_visible_count(&[], root))
        } else {
            self.structured_index
                .as_ref()
                .map_or(0, StructuredIndex::row_count)
        };
        let structured_count = usize::try_from(structured_count).unwrap_or(usize::MAX);
        let structured_row_widths = structured_column_widths.clone();
        let structured_list = uniform_list(
            "document-host-structured-rows",
            structured_count,
            cx.processor(move |this, range: Range<usize>, _window, _cx| {
                this.request_structured_rows(range.clone(), _cx);
                range
                    .map(|row_index| {
                        let json_node = json_structure
                            .then(|| this.json_node_at(row_index as u64))
                            .flatten();
                        let logical_row = if let Some(node) = &json_node {
                            node.item
                        } else {
                            row_index as u64
                        };
                        let row = if let Some(node) = &json_node {
                            this.json_rows.get(&node.path()).cloned()
                        } else {
                            this.structured_rows.get(&logical_row).cloned()
                        };
                        let row_depth = row.as_ref().map_or(0, |row| row.depth);
                        let cells = row
                            .as_ref()
                            .map(|row| row.cells.clone())
                            .unwrap_or_else(|| vec![loading_text.clone()]);
                        let row_column_start = row
                            .as_ref()
                            .map_or(this.structured_column_window_start, |row| row.column_start);
                        div()
                            .id(("document-host-structured-row", row_index))
                            .debug_selector(move || {
                                format!("document-host-structured-row-{row_index}")
                            })
                            .h(px(26.0))
                            .w(px(structured_width))
                            .flex()
                            .items_center()
                            .border_b(px(1.0))
                            .border_color(structured_border_color)
                            .text_size(px(12.0))
                            .text_color(line_text_color)
                            .child(
                                div()
                                    .id(("document-host-structured-row-number", logical_row))
                                    .w(px(76.0))
                                    .px(px(10.0))
                                    .text_align(gpui::TextAlign::Right)
                                    .text_color(line_number_color)
                                    .child(if json_structure {
                                        String::new()
                                    } else {
                                        (logical_row + 1).to_string()
                                    })
                                    .when(structured_live && !json_structure, |gutter| {
                                        gutter
                                            .cursor_context_menu()
                                            .on_mouse_down(
                                                MouseButton::Right,
                                                _cx.listener(move |this, _, _, cx| {
                                                    this.structured_context_target = Some(
                                                        StructuredMenuTarget::Row(logical_row),
                                                    );
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                }),
                                            )
                                    }),
                            )
                            .children(
                                cells
                                    .into_iter()
                                    .enumerate()
                                    .map(move |(column, cell)| {
                                        (row_column_start.saturating_add(column), cell)
                                    })
                                    .filter(|(column, _)| {
                                        !this.hidden_structured_columns.contains(column)
                                    })
                                    .map(|(column, cell)| {
                                        let cell = this
                                            .structured_cell_overrides
                                            .get(&StructuredCellEdit {
                                                record: Some(logical_row),
                                                column,
                                            })
                                            .cloned()
                                            .unwrap_or(cell);
                                        let json_prefix = if json_structure && column == 0 {
                                            let path = json_node
                                                .as_ref()
                                                .map(JsonNode::path)
                                                .unwrap_or_default();
                                            if this.json_expanded_nodes.contains(&path) {
                                                "▾ "
                                            } else {
                                                "▸ "
                                            }
                                        } else {
                                            ""
                                        };
                                        let editing = this.structured_cell_edit
                                            == Some(StructuredCellEdit {
                                                record: Some(logical_row),
                                                column,
                                            });
                                        let selected = this.structured_selected_cell
                                            == Some(StructuredCellEdit {
                                                record: Some(logical_row),
                                                column,
                                            });
                                        let selection_target = StructuredCellEdit {
                                            record: Some(logical_row),
                                            column,
                                        };
                                        let display_cell = cell.clone();
                                        div()
                                            .id(SharedString::from(format!(
                                                "document-host-structured-cell-{logical_row}-{column}"
                                            )))
                                            .debug_selector(move || {
                                                format!(
                                                    "document-host-structured-cell-{logical_row}-{column}"
                                                )
                                            })
                                            .w(px(structured_row_widths
                                                .get(column)
                                                .copied()
                                                .unwrap_or(STRUCTURED_CELL_WIDTH)))
                                            .h_full()
                                            .px(px(10.0))
                                            .flex()
                                            .items_center()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .border_l(px(1.0))
                                            .border_color(structured_border_color)
                                            .when(selected, |cell_view| {
                                                cell_view.bg(structured_selection_color)
                                            })
                                            .pl(px(10.0 + row_depth as f32 * 14.0))
                                            .child(if editing {
                                                div()
                                                    .id("document-host-structured-cell-editor")
                                                    .debug_selector(|| {
                                                        "document-host-structured-cell-editor".to_owned()
                                                    })
                                                    .size_full()
                                                    .min_w(px(0.0))
                                                    .overflow_hidden()
                                                    .child(this.structured_cell_input.clone())
                                                    .into_any_element()
                                            } else {
                                                div()
                                                    .child(format!(
                                                        "{json_prefix}{}",
                                                        truncate_cell(cell)
                                                    ))
                                                    .into_any_element()
                                            })
                                            .cursor_text()
                                            .on_click(_cx.listener(move |this, _, window, cx| {
                                                if this.structured_cell_edit
                                                    == Some(selection_target)
                                                {
                                                    return;
                                                }
                                                this.select_structured_cell(
                                                    selection_target,
                                                    window,
                                                    cx,
                                                )
                                            }))
                                            .when(structured_live && !json_structure, |cell_view| {
                                                cell_view
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        _cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                                            if event.click_count >= 2 {
                                                                this.begin_structured_cell_edit(
                                                                    Some(logical_row),
                                                                    column,
                                                                    display_cell.clone(),
                                                                    window,
                                                                    cx,
                                                                );
                                                                cx.stop_propagation();
                                                            }
                                                        }),
                                                    )
                                            })
                                    }),
                            )
                            .when(json_structure, |row| {
                                row.on_click(_cx.listener(move |this, _, _window, cx| {
                                    this.activate_json_node(row_index as u64, cx);
                                }))
                            })
                            .when(
                                !json_structure && this.view_mode == DocumentHostViewMode::Split,
                                |row| {
                                    row.on_click(_cx.listener(move |this, _, _window, cx| {
                                        this.reveal_structured_row_in_split(logical_row, cx);
                                    }))
                                },
                            )
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(self.structured_scroll_handle.clone())
        .h_full()
        .w(px(structured_width));
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
        let structured_scrollbar = (structured_max_top_row > 0).then(|| {
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
        });

        let structured_header = div()
            .h(px(30.0))
            .w(px(structured_width))
            .flex()
            .items_center()
            .bg(colors.dialog_secondary_button_bg)
            .border_b(px(1.0))
            .border_color(colors.dialog_border)
            .text_size(px(11.0))
            .text_color(colors.text_default)
            .child(div().w(px(76.0)).px(px(10.0)).child("#"))
            .children(
                visible_structured_headers
                    .into_iter()
                    .map(|(column, header)| {
                        let editing = self.structured_cell_edit
                            == Some(StructuredCellEdit {
                                record: None,
                                column,
                            });
                        let selected = self.structured_selected_cell
                            == Some(StructuredCellEdit {
                                record: None,
                                column,
                            });
                        let selection_target = StructuredCellEdit {
                            record: None,
                            column,
                        };
                        let edit_header = header.clone();
                        div()
                            .id(("document-host-structured-header", column))
                            .debug_selector(move || {
                                format!("document-host-structured-header-{column}")
                            })
                            .w(px(structured_column_widths
                                .get(column)
                                .copied()
                                .unwrap_or(STRUCTURED_CELL_WIDTH)))
                            .h_full()
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .border_l(px(1.0))
                            .border_color(colors.dialog_border)
                            .when(selected, |header_view| {
                                header_view.bg(structured_selection_color)
                            })
                            .child(if editing {
                                div()
                                    .id("document-host-structured-cell-editor")
                                    .debug_selector(|| {
                                        "document-host-structured-cell-editor".to_owned()
                                    })
                                    .size_full()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .child(self.structured_cell_input.clone())
                                    .into_any_element()
                            } else {
                                div().child(header).into_any_element()
                            })
                            .cursor_text()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if this.structured_cell_edit == Some(selection_target) {
                                    return;
                                }
                                this.select_structured_cell(selection_target, window, cx)
                            }))
                            .when(structured_live, |header_view| {
                                header_view
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(
                                            move |this, event: &MouseDownEvent, window, cx| {
                                                if event.click_count >= 2 {
                                                    this.begin_structured_cell_edit(
                                                        None,
                                                        column,
                                                        edit_header.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                }
                                            },
                                        ),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(move |this, _, _, cx| {
                                            this.structured_context_target =
                                                Some(StructuredMenuTarget::Column(column));
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    )
                            })
                            .when(!structured_live, |header_view| {
                                header_view.on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |this, _, _, cx| {
                                        this.hidden_structured_columns.insert(column);
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                )
                            })
                    }),
            );

        let structured_column_pager =
            (structured_column_count > STRUCTURED_COLUMN_WINDOW).then(|| {
                let start = self.structured_column_window_start;
                let end = start
                    .saturating_add(STRUCTURED_COLUMN_WINDOW)
                    .min(structured_column_count);
                let previous = start.saturating_sub(STRUCTURED_COLUMN_WINDOW);
                let next = start
                    .saturating_add(STRUCTURED_COLUMN_WINDOW)
                    .min(structured_column_count.saturating_sub(1));
                div()
                    .id("document-host-structured-column-pager")
                    .debug_selector(|| "document-host-structured-column-pager".to_owned())
                    .h(px(32.0))
                    .w(px(structured_width))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .border_b(px(1.0))
                    .border_color(colors.dialog_border)
                    .text_size(px(12.0))
                    .text_color(colors.dialog_muted)
                    .child(
                        div()
                            .id("document-host-structured-columns-previous")
                            .debug_selector(|| {
                                "document-host-structured-columns-previous".to_owned()
                            })
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .text_color(if start == 0 {
                                colors.text_placeholder
                            } else {
                                colors.text_default
                            })
                            .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                            .child("‹")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.set_structured_column_window_start(previous, cx);
                            })),
                    )
                    .child(
                        strings
                            .large_document_text("columns_window_template")
                            .replace("{start}", &(start + 1).to_string())
                            .replace("{end}", &end.to_string())
                            .replace("{total}", &structured_column_count.to_string()),
                    )
                    .child(
                        div()
                            .id("document-host-structured-columns-next")
                            .debug_selector(|| "document-host-structured-columns-next".to_owned())
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .text_color(if end == structured_column_count {
                                colors.text_placeholder
                            } else {
                                colors.text_default
                            })
                            .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                            .child("›")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.set_structured_column_window_start(next, cx);
                            })),
                    )
            });

        let markdown_table_switcher = match self.structured_index.as_ref() {
            Some(StructuredIndex::MarkdownTables { tables, selected }) if tables.len() > 1 => {
                let selected = *selected;
                let table_count = tables.len();
                let previous = selected.saturating_sub(1);
                let next = (selected + 1).min(table_count - 1);
                Some(
                    div()
                        .id("document-host-markdown-table-switcher")
                        .debug_selector(|| "document-host-markdown-table-switcher".to_owned())
                        .h(px(34.0))
                        .w(px(structured_width))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .border_b(px(1.0))
                        .border_color(colors.dialog_border)
                        .text_size(px(12.0))
                        .text_color(colors.dialog_muted)
                        .child(
                            div()
                                .id("document-host-markdown-table-previous")
                                .size(px(24.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .text_color(if selected == 0 {
                                    colors.text_placeholder
                                } else {
                                    colors.text_default
                                })
                                .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                                .child("‹")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.select_markdown_table(previous, cx);
                                })),
                        )
                        .child(
                            div()
                                .id("document-host-markdown-table-position")
                                .min_w(px(92.0))
                                .text_align(gpui::TextAlign::Center)
                                .child(
                                    strings
                                        .large_document_text("table_position_template")
                                        .replace("{current}", &(selected + 1).to_string())
                                        .replace("{total}", &table_count.to_string()),
                                ),
                        )
                        .child(
                            div()
                                .id("document-host-markdown-table-next")
                                .debug_selector(|| "document-host-markdown-table-next".to_owned())
                                .size(px(24.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .text_color(if selected + 1 == table_count {
                                    colors.text_placeholder
                                } else {
                                    colors.text_default
                                })
                                .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                                .child("›")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.select_markdown_table(next, cx);
                                })),
                        ),
                )
            }
            _ => None,
        };

        let column_progress = self
            .structured_column_progress
            .as_ref()
            .map(|(processed, total)| (processed.load(Ordering::Relaxed), *total));
        let structured_operation_bar =
            (column_progress.is_some() || !self.hidden_structured_columns.is_empty()).then(|| {
                div()
                    .id("document-host-structured-operation-bar")
                    .debug_selector(|| "document-host-structured-operation-bar".to_owned())
                    .h(px(34.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .border_b(px(1.0))
                    .border_color(colors.dialog_border)
                    .when_some(column_progress, |bar, (processed, total)| {
                        bar.child(
                            strings
                                .large_document_text("updating_columns_progress_template")
                                .replace("{processed}", &processed.to_string())
                                .replace("{total}", &total.to_string()),
                        )
                        .child(
                            div()
                                .id("document-host-cancel-column-update")
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .bg(colors.dialog_secondary_button_bg)
                                .child(strings.large_document_text("cancel").to_owned())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel_delimited_column_transform(cx)
                                })),
                        )
                    })
                    .when(!self.hidden_structured_columns.is_empty(), |bar| {
                        bar.child(
                            div()
                                .id("document-host-show-all-columns")
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(px(4.0))
                                .bg(colors.dialog_secondary_button_bg)
                                .child(strings.large_document_text("show_all_columns").to_owned())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.hidden_structured_columns.clear();
                                    cx.notify();
                                })),
                        )
                    })
            });
        let add_row = structured_live.then(|| {
            let row_count = self
                .structured_index
                .as_ref()
                .map_or(0, StructuredIndex::row_count);
            div()
                .id("document-host-structured-add-row")
                .debug_selector(|| "document-host-structured-add-row".to_owned())
                .h(px(30.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .border_t(px(1.0))
                .border_color(colors.dialog_border)
                .cursor_pointer()
                .text_color(colors.text_link)
                .child(strings.large_document_text("add_row").to_owned())
                .on_click(
                    cx.listener(move |this, _, _, cx| this.insert_delimited_row(row_count, cx)),
                )
        });
        let context_menu = self.structured_context_target.map(|target| {
            let row_count = self
                .structured_index
                .as_ref()
                .map_or(0, StructuredIndex::row_count);
            let menu_item = |key: &'static str| {
                div()
                    .id(key)
                    .debug_selector(move || format!("document-host-structured-menu-{key}"))
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .hover(|item| item.bg(colors.dialog_secondary_button_hover))
                    .child(strings.large_document_text(key).to_owned())
            };
            let menu = div()
                .id("document-host-structured-context-menu")
                .debug_selector(|| "document-host-structured-context-menu".to_owned())
                .absolute()
                .top(px(62.0))
                .left(px(82.0))
                .w(px(178.0))
                .p(px(4.0))
                .rounded(px(6.0))
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .bg(colors.dialog_surface)
                .shadow_md();
            match target {
                StructuredMenuTarget::Row(row) => menu
                    .child(menu_item("insert_row_before").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.insert_delimited_row(row, cx);
                        },
                    )))
                    .child(menu_item("insert_row_after").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.insert_delimited_row((row + 1).min(row_count), cx);
                        },
                    )))
                    .child(
                        menu_item("delete_row").on_click(cx.listener(move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.delete_delimited_row(row, cx);
                        })),
                    ),
                StructuredMenuTarget::Column(column) => menu
                    .child(menu_item("insert_column_before").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.transform_delimited_column(
                                DelimitedEdit::InsertColumn {
                                    before: column,
                                    header: cx
                                        .global::<I18nManager>()
                                        .strings()
                                        .large_document_text("default_column_template")
                                        .replace("{number}", &(column + 1).to_string()),
                                },
                                cx,
                            );
                        },
                    )))
                    .child(menu_item("insert_column_after").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.transform_delimited_column(
                                DelimitedEdit::InsertColumn {
                                    before: column + 1,
                                    header: cx
                                        .global::<I18nManager>()
                                        .strings()
                                        .large_document_text("default_column_template")
                                        .replace("{number}", &(column + 2).to_string()),
                                },
                                cx,
                            );
                        },
                    )))
                    .child(menu_item("delete_column").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.transform_delimited_column(
                                DelimitedEdit::DeleteColumn { column },
                                cx,
                            );
                        },
                    )))
                    .child(menu_item("hide_column").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.hidden_structured_columns.insert(column);
                            cx.notify();
                        },
                    ))),
            }
        });
        let content = div()
            .id("document-host-structured-content")
            .debug_selector(|| "document-host-structured-content".to_owned())
            .tab_index(0)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_structured_table_key_down))
            .h_full()
            .w(px(structured_width))
            .relative()
            .flex()
            .flex_col()
            .children(markdown_table_switcher)
            .children(structured_column_pager)
            .children(structured_operation_bar)
            .child(structured_header)
            .child(div().flex_1().min_h(px(0.0)).child(structured_list))
            .children(add_row)
            .children(context_menu);
        if self.view_mode == DocumentHostViewMode::Split {
            let mut horizontal_scroll = div()
                .id("document-host-split-structure-horizontal-scroll")
                .size_full()
                .overflow_x_scroll()
                .track_scroll(&self.structured_horizontal_scroll_handle)
                .on_scroll_wheel(cx.listener(Self::on_horizontal_container_scroll_wheel))
                .child(content);
            // GPUI 默认会把纯纵向滚轮转成横向；表格嵌在纵向列表时必须禁用该回退。
            horizontal_scroll.style().restrict_scroll_to_axis = Some(true);
            div()
                .id("document-host-split-structure")
                .debug_selector(|| "document-host-split-structure".to_owned())
                .w(relative(0.5))
                .h_full()
                .min_w(px(0.0))
                .relative()
                .overflow_hidden()
                .border_l(px(1.0))
                .border_color(colors.dialog_border)
                .child(horizontal_scroll)
                .children(structured_scrollbar)
        } else {
            let mut horizontal_scroll = div()
                .id("document-host-structured-horizontal-scroll")
                .size_full()
                .overflow_x_scroll()
                .track_scroll(&self.structured_horizontal_scroll_handle)
                .on_scroll_wheel(cx.listener(Self::on_horizontal_container_scroll_wheel))
                .child(content);
            horizontal_scroll.style().restrict_scroll_to_axis = Some(true);
            div()
                .id("document-host-structured-scroll")
                .debug_selector(|| "document-host-structured-scroll".to_owned())
                .flex_1()
                .min_h(px(0.0))
                .relative()
                .overflow_hidden()
                .child(horizontal_scroll)
                .children(structured_scrollbar)
        }
    }
}
