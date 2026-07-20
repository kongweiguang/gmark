// @author kongweiguang

use super::*;

impl DiskSourceAdapter {
    /// 构建结构化 CSV/JSON/Markdown 表格面板；源码滚动面保持独立所有权。
    pub(super) fn render_structured_panel(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = cx.global::<ThemeManager>().current_arc();
        let colors = &theme.colors;
        let line_text_color = colors.text_default;
        let line_number_color = colors.text_placeholder;
        let structured_border_color = colors.dialog_border;
        let structured_headers = self
            .structured_index
            .as_ref()
            .map(StructuredIndex::headers)
            .unwrap_or_default();
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
        let structured_filter_active = !self
            .structured_filter_input
            .read(cx)
            .display_text()
            .trim()
            .is_empty();
        let json_structure = matches!(self.structured_index, Some(StructuredIndex::Json { .. }));
        let structured_count = if json_structure {
            self.json_root_index()
                .map_or(0, |root| self.json_visible_count(&[], root))
        } else if structured_filter_active {
            self.structured_filtered_rows.len() as u64
        } else {
            self.structured_index
                .as_ref()
                .map_or(0, StructuredIndex::row_count)
        };
        let structured_count = usize::try_from(structured_count).unwrap_or(usize::MAX);
        let structured_row_widths = structured_column_widths.clone();
        let structured_list = uniform_list(
            "large-file-structured-rows",
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
                        } else if structured_filter_active {
                            this.structured_filtered_rows
                                .get(row_index)
                                .copied()
                                .unwrap_or(row_index as u64)
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
                            .unwrap_or_else(|| vec!["Loading…".to_owned()]);
                        let row_column_start = row
                            .as_ref()
                            .map_or(this.structured_column_window_start, |row| row.column_start);
                        div()
                            .id(("large-file-structured-row", row_index))
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
                                    .w(px(76.0))
                                    .px(px(10.0))
                                    .text_align(gpui::TextAlign::Right)
                                    .text_color(line_number_color)
                                    .child(if json_structure {
                                        String::new()
                                    } else {
                                        (logical_row + 1).to_string()
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
                                        div()
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
                                            .pl(px(10.0 + row_depth as f32 * 14.0))
                                            .child(format!("{json_prefix}{}", truncate_cell(cell)))
                                    }),
                            )
                            .on_click(_cx.listener(move |this, _, _window, cx| {
                                if json_structure {
                                    this.activate_json_node(row_index as u64, cx);
                                } else {
                                    this.jump_structured_row_to_source(logical_row, cx);
                                }
                            }))
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .h_full()
        .w(px(structured_width));

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
                        div()
                            .id(("large-file-structured-header", column))
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
                            .child(header)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.hidden_structured_columns.insert(column);
                                cx.notify();
                            }))
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
                    .id("large-file-structured-column-pager")
                    .debug_selector(|| "large-file-structured-column-pager".to_owned())
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
                            .id("large-file-structured-columns-previous")
                            .debug_selector(|| "large-file-structured-columns-previous".to_owned())
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
                    .child(format!(
                        "Columns {}–{} / {structured_column_count}",
                        start + 1,
                        end
                    ))
                    .child(
                        div()
                            .id("large-file-structured-columns-next")
                            .debug_selector(|| "large-file-structured-columns-next".to_owned())
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
                        .id("large-file-markdown-table-switcher")
                        .debug_selector(|| "large-file-markdown-table-switcher".to_owned())
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
                                .id("large-file-markdown-table-previous")
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
                                .id("large-file-markdown-table-position")
                                .min_w(px(92.0))
                                .text_align(gpui::TextAlign::Center)
                                .child(format!("Table {} / {table_count}", selected + 1)),
                        )
                        .child(
                            div()
                                .id("large-file-markdown-table-next")
                                .debug_selector(|| "large-file-markdown-table-next".to_owned())
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

        let structured_filter_bar =
            matches!(self.structured_index, Some(StructuredIndex::Delimited(_))).then(|| {
                let column_label = self.structured_filter_column.map_or_else(
                    || "All columns".to_owned(),
                    |column| {
                        self.structured_index
                            .as_ref()
                            .and_then(|index| index.headers().get(column).cloned())
                            .unwrap_or_else(|| format!("Column {}", column + 1))
                    },
                );
                div()
                    .id("large-file-structured-filter-bar")
                    .h(px(34.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .border_b(px(1.0))
                    .border_color(colors.dialog_border)
                    .child(
                        div()
                            .id("large-file-structured-filter-column")
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .bg(colors.dialog_secondary_button_bg)
                            .text_color(colors.dialog_secondary_button_text)
                            .child(column_label)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let count = this
                                    .structured_index
                                    .as_ref()
                                    .map(|index| index.headers().len())
                                    .unwrap_or(0);
                                this.structured_filter_column = match this.structured_filter_column
                                {
                                    None if count > 0 => Some(0),
                                    Some(column) if column + 1 < count => Some(column + 1),
                                    _ => None,
                                };
                                this.schedule_structured_filter(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("large-file-structured-filter-input")
                            .debug_selector(|| "large-file-structured-filter-input".to_owned())
                            .w(px(220.0))
                            .h(px(26.0))
                            .px(px(8.0))
                            .rounded(px(4.0))
                            .border(px(1.0))
                            .border_color(colors.dialog_border)
                            .bg(colors.editor_background)
                            .overflow_hidden()
                            .child(self.structured_filter_input.clone()),
                    )
                    .when(self.structured_filter_running, |bar| {
                        bar.child("Filtering…")
                    })
                    .when(
                        structured_filter_active && !self.structured_filter_running,
                        |bar| {
                            let count = self.structured_filtered_rows.len();
                            bar.child(if count == 10_000 {
                                "10,000+ matches (result limit)".to_owned()
                            } else {
                                format!("{count} matches")
                            })
                        },
                    )
                    .when(!self.hidden_structured_columns.is_empty(), |bar| {
                        bar.child(
                            div()
                                .id("large-file-show-all-columns")
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(px(4.0))
                                .bg(colors.dialog_secondary_button_bg)
                                .child("Show all columns")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.hidden_structured_columns.clear();
                                    cx.notify();
                                })),
                        )
                    })
            });
        let content = div()
            .h_full()
            .w(px(structured_width))
            .flex()
            .flex_col()
            .children(markdown_table_switcher)
            .children(structured_column_pager)
            .children(structured_filter_bar)
            .child(structured_header)
            .child(div().flex_1().min_h(px(0.0)).child(structured_list));
        if self.view_mode == LargeViewMode::Split {
            div()
                .id("large-file-split-structure")
                .w(relative(0.5))
                .h_full()
                .min_w(px(0.0))
                .overflow_x_scroll()
                .border_r(px(1.0))
                .border_color(colors.dialog_border)
                .on_scroll_wheel(cx.listener(Self::on_horizontal_container_scroll_wheel))
                .child(content)
        } else {
            div()
                .id("large-file-structured-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_x_scroll()
                .on_scroll_wheel(cx.listener(Self::on_horizontal_container_scroll_wheel))
                .child(content)
        }
    }
}
