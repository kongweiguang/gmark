// @author kongweiguang

use super::*;

use gpui::{AnyElement, KeyDownEvent, MouseButton, WeakEntity};

use crate::theme::{Theme, ThemeColors};

const MAX_STRUCTURED_CACHED_ROWS: usize = STRUCTURED_OVERSCAN_ROWS * 6;

/// Stable targets exposed to the editor shell. The shell never needs to know
/// how JSON or delimited indexes are stored; a target is resolved by the host
/// against the current document epoch and view mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DocumentSidebarTarget {
    Column { column: usize },
    StructuredRow { row: u64, offset: u64, json: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocumentSidebarNodeSnapshot {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) secondary: String,
    pub(crate) depth: usize,
    pub(crate) expandable: bool,
    pub(crate) target: DocumentSidebarTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocumentSidebarMetadata {
    pub(crate) length: u64,
    pub(crate) lines: u64,
    pub(crate) encoding: String,
    pub(crate) line_endings: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocumentSidebarSnapshot {
    pub(crate) format: DocumentMenuFormat,
    pub(crate) metadata: DocumentSidebarMetadata,
    pub(crate) document_epoch: u64,
    pub(crate) revision: u64,
    pub(crate) generation: u64,
    pub(crate) nodes: Vec<DocumentSidebarNodeSnapshot>,
}

pub(super) fn prune_structured_row_cache<T>(
    rows: &mut BTreeMap<u64, T>,
    requested_center: u64,
    max_rows: usize,
) {
    while rows.len() > max_rows {
        let first = rows.first_key_value().map(|(row, _)| *row);
        let last = rows.last_key_value().map(|(row, _)| *row);
        let evicted = match (first, last) {
            (Some(first), Some(last))
                if requested_center.saturating_sub(first)
                    >= last.saturating_sub(requested_center) =>
            {
                first
            }
            (_, Some(last)) => last,
            _ => break,
        };
        rows.remove(&evicted);
    }
}

impl DocumentHost {
    fn structured_sidebar_row(&self, display_row: u64) -> Option<StructuredRow> {
        if let Some(node) = self.json_node_at(display_row) {
            return self.json_rows.get(&node.path()).cloned();
        }
        self.structured_rows
            .values()
            .find(|row| row.index == display_row)
            .cloned()
    }

    /// Read-only navigation projection. Only already indexed/cached rows are
    /// returned; rendering a large document therefore never triggers a full
    /// parse or a second complete in-memory copy.
    pub(crate) fn document_sidebar_snapshot(&self) -> DocumentSidebarSnapshot {
        let format = self.document_menu_format();
        let mut nodes = Vec::new();
        match format {
            DocumentMenuFormat::Csv | DocumentMenuFormat::Tsv => {
                if let Some(StructuredIndex::Delimited(index)) = self.structured_index.as_ref() {
                    nodes.extend(index.headers().iter().enumerate().map(|(column, header)| {
                        let label = if header.trim().is_empty() {
                            format!("Column {}", column + 1)
                        } else {
                            header.clone()
                        };
                        DocumentSidebarNodeSnapshot {
                            id: format!("column:{column}"),
                            label,
                            secondary: (column + 1).to_string(),
                            depth: 0,
                            expandable: false,
                            target: DocumentSidebarTarget::Column { column },
                        }
                    }));
                }
            }
            DocumentMenuFormat::Json | DocumentMenuFormat::JsonLines => {
                let count = self
                    .json_root_index()
                    .map_or_else(
                        || {
                            self.structured_index
                                .as_ref()
                                .map_or(0, StructuredIndex::row_count)
                        },
                        |root| self.json_visible_count(&[], root),
                    )
                    .min(128);
                for display_row in 0..count {
                    let Some(row) = self.structured_sidebar_row(display_row) else {
                        continue;
                    };
                    let path = self
                        .json_node_at(display_row)
                        .map(|node| node.path())
                        .unwrap_or_default();
                    nodes.push(DocumentSidebarNodeSnapshot {
                        id: if path.is_empty() {
                            format!("row:{display_row}")
                        } else {
                            format!(
                                "json:{}",
                                path.iter()
                                    .map(u64::to_string)
                                    .collect::<Vec<_>>()
                                    .join("/")
                            )
                        },
                        label: row.cells.first().cloned().unwrap_or_default(),
                        secondary: row.cells.get(1).cloned().unwrap_or_default(),
                        depth: row.depth,
                        expandable: self
                            .json_node_at(display_row)
                            .is_some_and(|node| self.json_child_indexes.contains_key(&node.path())),
                        target: DocumentSidebarTarget::StructuredRow {
                            row: display_row,
                            offset: row.byte_range.start,
                            json: format == DocumentMenuFormat::Json,
                        },
                    });
                }
            }
            DocumentMenuFormat::Markdown | DocumentMenuFormat::Text => {}
        }
        DocumentSidebarSnapshot {
            format,
            metadata: DocumentSidebarMetadata {
                length: self.document_length(),
                lines: self.document_line_count(),
                encoding: self.encoding_label(),
                line_endings: self.document_line_ending_label(),
            },
            document_epoch: self.document_epoch,
            revision: self
                .document
                .as_ref()
                .map_or(0, |document| document.revision()),
            generation: self.structured_generation,
            nodes,
        }
    }

    /// Resolve a stable navigation target against the current host state.
    /// Stale rows are ignored by the existing structured/source epoch checks.
    pub(crate) fn reveal_document_sidebar_target(
        &mut self,
        target: DocumentSidebarTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            DocumentSidebarTarget::Column { column } => {
                self.reveal_document_sidebar_column(column, window, cx)
            }
            DocumentSidebarTarget::StructuredRow { row, offset, json } => {
                self.reveal_document_sidebar_structure(row, offset, json, cx)
            }
        }
    }

    /// 渲染右侧文档导航的只读投影。条目只读取当前已建立的结构索引，正文与主视图仍由
    /// DocumentHost 独占；后台索引尚未完成时保持稳定的 loading 状态。
    pub(crate) fn render_document_sidebar(
        &mut self,
        theme: &Theme,
        strings: &I18nStrings,
        _editor: &WeakEntity<crate::editor::Editor>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = &theme.colors;
        let format = self.document_sidebar_snapshot().format;
        if self.is_paged_document() {
            return self.render_document_sidebar_info(colors, strings, cx);
        }
        match format {
            DocumentMenuFormat::Csv | DocumentMenuFormat::Tsv => {
                self.render_document_sidebar_columns(colors, strings, cx)
            }
            DocumentMenuFormat::Json | DocumentMenuFormat::JsonLines => {
                self.render_document_sidebar_structure(colors, strings, cx)
            }
            DocumentMenuFormat::Markdown | DocumentMenuFormat::Text => {
                self.render_document_sidebar_info(colors, strings, cx)
            }
        }
    }

    fn render_document_sidebar_info(
        &self,
        colors: &ThemeColors,
        strings: &I18nStrings,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let format = match self.document_menu_format() {
            DocumentMenuFormat::Text => strings.document_sidebar_text.clone(),
            format => format.label(false).to_owned(),
        };
        let rows = [
            (strings.document_sidebar_format.clone(), format),
            (
                strings.document_sidebar_size.clone(),
                format_byte_count(self.probe.len),
            ),
            (
                strings.document_sidebar_lines.clone(),
                self.document_line_count().to_string(),
            ),
            (
                strings.document_sidebar_encoding.clone(),
                self.encoding_label(),
            ),
            (
                strings.document_sidebar_line_endings.clone(),
                self.document_line_ending_label(),
            ),
        ];
        div()
            .id("document-sidebar-info")
            .debug_selector(|| "document-sidebar-info".to_owned())
            .w_full()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(rows.into_iter().map(|(label, value)| {
                let selector_label = label.clone();
                div()
                    .id(SharedString::from(format!("document-sidebar-info-{label}")))
                    .debug_selector(move || format!("document-sidebar-info-{selector_label}"))
                    .w_full()
                    .min_h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(px(6.0))
                    .text_size(px(12.0))
                    .child(div().text_color(colors.text_placeholder).child(label))
                    .child(
                        div()
                            .max_w(px(150.0))
                            .overflow_hidden()
                            .truncate()
                            .text_color(colors.text_default)
                            .child(value),
                    )
            }))
            .into_any_element()
    }

    fn render_document_sidebar_columns(
        &mut self,
        colors: &ThemeColors,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.is_paged_document() || self.structure_error.is_some() {
            return self.render_document_sidebar_info(colors, strings, cx);
        }
        let Some(StructuredIndex::Delimited(index)) = self.structured_index.as_ref() else {
            return self.render_document_sidebar_loading(colors, strings);
        };
        let headers = index.headers().to_vec();
        if headers.is_empty() {
            return self.render_document_sidebar_empty(colors, strings);
        }
        let row_count = index.record_count();
        let mut body = div()
            .id("document-sidebar-columns")
            .debug_selector(|| "document-sidebar-columns".to_owned())
            .w_full()
            .flex()
            .flex_col()
            .gap(px(2.0));
        for (column, header) in headers.into_iter().enumerate() {
            let label = if header.trim().is_empty() {
                strings
                    .document_sidebar_column_fallback
                    .replace("{count}", &(column + 1).to_string())
            } else {
                header
            };
            body = body.child(
                div()
                    .id(SharedString::from(format!(
                        "document-sidebar-column-{column}"
                    )))
                    .debug_selector(move || format!("document-sidebar-column-{column}"))
                    .w_full()
                    .min_h(px(30.0))
                    .tab_index(0)
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(7.0))
                    .hover(|this| this.bg(colors.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .child(
                        div()
                            .w(px(24.0))
                            .flex_shrink_0()
                            .text_color(colors.text_placeholder)
                            .child(format!("{}", column + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .truncate()
                            .text_color(colors.text_default)
                            .child(label),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.reveal_document_sidebar_target(
                            DocumentSidebarTarget::Column { column },
                            window,
                            cx,
                        );
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.reveal_document_sidebar_target(
                                DocumentSidebarTarget::Column { column },
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }
                    })),
            );
        }
        body = body.child(
            div()
                .id("document-sidebar-column-count")
                .debug_selector(|| "document-sidebar-column-count".to_owned())
                .mt(px(8.0))
                .px(px(8.0))
                .text_size(px(11.0))
                .text_color(colors.text_placeholder)
                .child(
                    strings
                        .document_sidebar_rows_template
                        .replace("{count}", &row_count.to_string()),
                ),
        );
        body.into_any_element()
    }

    fn render_document_sidebar_structure(
        &mut self,
        colors: &ThemeColors,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.is_paged_document() || self.structure_error.is_some() {
            return self.render_document_sidebar_info(colors, strings, cx);
        }
        let is_json = self.is_json_document();
        let (count, rows) = if is_json {
            let Some(root) = self.json_root_index() else {
                return self.render_document_sidebar_loading(colors, strings);
            };
            let count = usize::try_from(self.json_visible_count(&[], root))
                .unwrap_or(usize::MAX)
                .min(128);
            self.request_structured_rows(0..count, cx);
            let rows = (0..count)
                .filter_map(|index| {
                    let node = self.json_node_at(index as u64)?;
                    let row = self.json_rows.get(&node.path())?;
                    Some((index as u64, row.clone()))
                })
                .collect::<Vec<_>>();
            (count, rows)
        } else {
            let Some(index) = self.structured_index.as_ref() else {
                return self.render_document_sidebar_loading(colors, strings);
            };
            let count = usize::try_from(index.row_count())
                .unwrap_or(usize::MAX)
                .min(128);
            self.request_structured_rows(0..count, cx);
            let rows = self
                .structured_rows
                .values()
                .filter(|row| row.index < count as u64)
                .cloned()
                .map(|row| (row.index, row))
                .collect::<Vec<_>>();
            (count, rows)
        };
        if count == 0 {
            return self.render_document_sidebar_empty(colors, strings);
        }
        let mut by_index = BTreeMap::new();
        for (index, row) in rows {
            by_index.insert(index, row);
        }
        let mut body = div()
            .id("document-sidebar-structure")
            .debug_selector(|| "document-sidebar-structure".to_owned())
            .w_full()
            .flex()
            .flex_col()
            .gap(px(2.0));
        for index in 0..count {
            let row = by_index.get(&(index as u64)).cloned();
            let (label, value, depth, offset) = row.as_ref().map_or_else(
                || ("Loading…".to_owned(), String::new(), 0usize, None),
                |row| {
                    (
                        row.cells.first().cloned().unwrap_or_default(),
                        row.cells.get(1).cloned().unwrap_or_default(),
                        row.depth,
                        Some(row.byte_range.start),
                    )
                },
            );
            let json_node = is_json.then(|| self.json_node_at(index as u64)).flatten();
            let expandable = json_node
                .as_ref()
                .is_some_and(|node| self.json_child_indexes.contains_key(&node.path()));
            let expanded = json_node
                .as_ref()
                .is_some_and(|node| self.json_expanded_nodes.contains(&node.path()));
            let expand_editor = cx.entity().downgrade();
            body = body.child(
                div()
                    .id(SharedString::from(format!(
                        "document-sidebar-structure-{index}"
                    )))
                    .debug_selector(move || format!("document-sidebar-structure-{index}"))
                    .w_full()
                    .min_h(px(30.0))
                    .tab_index(0)
                    .pl(px(8.0 + depth as f32 * 14.0))
                    .pr(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(7.0))
                    .hover(|this| this.bg(colors.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .children(expandable.then(|| {
                        div()
                            .id(SharedString::from(format!(
                                "document-sidebar-structure-toggle-{index}"
                            )))
                            .debug_selector(move || {
                                format!("document-sidebar-structure-toggle-{index}")
                            })
                            .size(px(18.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .child(
                                svg()
                                    .path(if expanded {
                                        "icon/ui/chevron-down.svg"
                                    } else {
                                        "icon/ui/chevron-right.svg"
                                    })
                                    .size(px(13.0))
                                    .text_color(colors.text_placeholder),
                            )
                            .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                                let _ = expand_editor.update(cx, |host, cx| {
                                    host.activate_json_node(index as u64, cx);
                                });
                                cx.stop_propagation();
                            })
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .truncate()
                            .text_color(colors.text_default)
                            .child(label),
                    )
                    .child(
                        div()
                            .max_w(px(110.0))
                            .overflow_hidden()
                            .truncate()
                            .text_color(colors.text_placeholder)
                            .child(value),
                    )
                    .when_some(offset, |row, offset| {
                        let row = row.on_click(cx.listener(move |this, _, window, cx| {
                            this.reveal_document_sidebar_target(
                                DocumentSidebarTarget::StructuredRow {
                                    row: index as u64,
                                    offset,
                                    json: is_json,
                                },
                                window,
                                cx,
                            );
                        }));
                        row.on_key_down(cx.listener(
                            move |this, event: &KeyDownEvent, window, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    this.reveal_document_sidebar_target(
                                        DocumentSidebarTarget::StructuredRow {
                                            row: index as u64,
                                            offset,
                                            json: is_json,
                                        },
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            },
                        ))
                    }),
            );
        }
        if count
            < if is_json {
                self.json_root_index()
                    .map_or(count as u64, JsonIndex::item_count)
            } else {
                self.structured_index
                    .as_ref()
                    .map_or(count as u64, StructuredIndex::row_count)
            } as usize
        {
            body = body.child(
                div()
                    .id("document-sidebar-structure-limit")
                    .debug_selector(|| "document-sidebar-structure-limit".to_owned())
                    .mt(px(8.0))
                    .px(px(8.0))
                    .text_size(px(11.0))
                    .text_color(colors.text_placeholder)
                    .child(
                        strings
                            .document_sidebar_items_limit
                            .replace("{count}", &count.to_string()),
                    ),
            );
        }
        body.into_any_element()
    }

    fn render_document_sidebar_loading(
        &self,
        colors: &ThemeColors,
        strings: &I18nStrings,
    ) -> AnyElement {
        div()
            .id("document-sidebar-loading")
            .debug_selector(|| "document-sidebar-loading".to_owned())
            .w_full()
            .px(px(8.0))
            .py(px(12.0))
            .text_size(px(12.0))
            .text_color(colors.text_placeholder)
            .child(strings.document_sidebar_loading.clone())
            .into_any_element()
    }

    fn render_document_sidebar_empty(
        &self,
        colors: &ThemeColors,
        strings: &I18nStrings,
    ) -> AnyElement {
        div()
            .id("document-sidebar-empty")
            .debug_selector(|| "document-sidebar-empty".to_owned())
            .w_full()
            .px(px(8.0))
            .py(px(12.0))
            .text_size(px(12.0))
            .text_color(colors.text_placeholder)
            .child(strings.document_sidebar_empty.clone())
            .into_any_element()
    }

    fn reveal_document_sidebar_column(
        &mut self,
        column: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_delimited_document() {
            return;
        }
        if self.view_mode == DocumentHostViewMode::Source {
            let offset = self
                .structured_index
                .as_ref()
                .and_then(|index| match index {
                    StructuredIndex::Delimited(index) => index
                        .read_header()
                        .ok()
                        .flatten()
                        .map(|record| record.byte_range.start)
                        .or_else(|| {
                            index.read_records(0, 1).ok().and_then(|records| {
                                records.first().map(|record| record.byte_range.start)
                            })
                        }),
                    _ => None,
                });
            if let Some(offset) = offset {
                self.jump_byte_offset_to_source(offset, cx);
            }
        }
        self.set_structured_column_window_start(column, cx);
        self.structured_selected_cell = Some(StructuredCellEdit {
            record: None,
            column,
        });
        self.focus_handle.focus(window);
        cx.notify();
    }

    fn reveal_document_sidebar_structure(
        &mut self,
        row: u64,
        offset: u64,
        is_json: bool,
        cx: &mut Context<Self>,
    ) {
        match self.view_mode {
            DocumentHostViewMode::Source => self.jump_byte_offset_to_source(offset, cx),
            DocumentHostViewMode::Split => {
                if is_json {
                    self.activate_json_node(row, cx);
                } else {
                    self.reveal_structured_row_in_split(row, cx);
                }
            }
            DocumentHostViewMode::Structure if is_json => {
                let selected = self
                    .derived_projection_snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.as_any().downcast_ref::<JsonGraphSnapshot>())
                    .and_then(|snapshot| {
                        snapshot
                            .projection()
                            .nodes
                            .iter()
                            .find(|node| {
                                node.source.range.start <= offset && node.source.range.end > offset
                            })
                            .map(|node| node.id.clone())
                    });
                if let Some(selected) = selected {
                    self.graph_selected_item = Some(selected.clone());
                    self.graph_pending_center = Some(selected.clone());
                    self.reveal_graph_item(&selected);
                    cx.notify();
                } else {
                    self.activate_json_node(row, cx);
                }
            }
            DocumentHostViewMode::Structure => {
                self.structured_selected_cell = Some(StructuredCellEdit {
                    record: Some(row),
                    column: 0,
                });
                cx.notify();
            }
            DocumentHostViewMode::Live => self.jump_byte_offset_to_source(offset, cx),
        }
    }

    pub(super) fn jump_to_search_result(&mut self, cx: &mut Context<Self>) {
        let Some(found_start) = self
            .search_results
            .get(self.search_selected)
            .map(|found| found.range.start)
        else {
            return;
        };
        let line = if let Some(document) = self.document.as_ref() {
            let Some(line) = document
                .line_for_offset(found_start)
                .and_then(|line| usize::try_from(line).ok())
            else {
                return;
            };
            self.anchor_source_window_for_byte(line as u64, found_start);
            line
        } else {
            let estimated = self.probe.estimated_lines.max(1);
            let line = ((found_start as u128 * estimated as u128) / self.probe.len.max(1) as u128)
                .min(usize::MAX as u128) as usize;
            self.source_window_start = 0;
            self.invalidate_source_rows();
            line.min(self.line_count().saturating_sub(1))
        };
        // CSV/TSV 的全文搜索仍以 Source 字节坐标为真值，但命中不能夺走用户当前的
        // 表格工作区；Source 选择留作随后切换或 Split 左栏同步使用。
        let keep_delimited_table = self.is_delimited_document()
            && matches!(
                self.view_mode,
                DocumentHostViewMode::Live
                    | DocumentHostViewMode::Structure
                    | DocumentHostViewMode::Split
            );
        if !keep_delimited_table {
            self.view_mode = DocumentHostViewMode::Source;
            self.sync_tab_active_view();
        }
        self.select_source_lines(line..line.saturating_add(1), false);
        self.scroll_source_line(line, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn navigate_search(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.search_results.is_empty() {
            return;
        }
        let count = self.search_results.len() as i64;
        self.search_selected =
            (self.search_selected as i64 + i64::from(delta)).rem_euclid(count) as usize;
        self.jump_to_search_result(cx);
    }

    pub(super) fn toggle_search_option(
        &mut self,
        update: impl FnOnce(&mut SearchOptions),
        cx: &mut Context<Self>,
    ) {
        update(&mut self.search_options);
        self.schedule_search(cx);
    }

    pub(crate) fn on_find_in_document(
        &mut self,
        _: &FindInDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation_visible = false;
        self.search_visible = true;
        let host = cx.entity().downgrade();
        self.search_input.update(cx, move |input, _cx| {
            input.set_host_action_handler(move |action, window, cx| {
                let _ = host.update(cx, |view, cx| {
                    view.on_search_host_action(action, window, cx)
                });
            });
            input.focus_handle.focus(window);
        });
        cx.notify();
    }

    pub(crate) fn on_go_to_line(
        &mut self,
        _: &GoToLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_visible = false;
        self.navigation_visible = true;
        let host = cx.entity().downgrade();
        self.navigation_input.update(cx, move |input, _cx| {
            input.set_host_action_handler(move |action, window, cx| {
                let _ = host.update(cx, |view, cx| {
                    view.on_navigation_host_action(action, window, cx)
                });
            });
            let len = input.display_text().len();
            input.selected_range = 0..len;
            input.focus_handle.focus(window);
        });
        cx.notify();
    }

    pub(crate) fn on_find_next(&mut self, _: &FindNext, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate_search(1, cx);
    }

    pub(crate) fn on_find_previous(
        &mut self,
        _: &FindPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_search(-1, cx);
    }

    pub(crate) fn on_dismiss_transient_ui(
        &mut self,
        _: &DismissTransientUi,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.search_visible || self.navigation_visible || self.source_context_menu.is_some() {
            self.search_visible = false;
            self.navigation_visible = false;
            self.source_context_menu = None;
            self.focus_handle.focus(window);
            cx.notify();
        }
    }

    pub(super) fn scroll_page(&mut self, toward_end: bool, cx: &mut Context<Self>) {
        let handle = self.scroll_handle.0.borrow().base_handle.clone();
        let row_height = self.source_row_height.max(1.0);
        let local_top = (-f32::from(handle.offset().y) / row_height)
            .max(0.0)
            .floor() as usize;
        let top = self.source_list_origin.saturating_add(local_top);
        let page_rows = (f32::from(handle.bounds().size.height) / row_height)
            .floor()
            .max(1.0) as usize;
        let target = if toward_end {
            top.saturating_add(page_rows)
                .min(self.line_count().saturating_sub(1))
        } else {
            top.saturating_sub(page_rows)
        };
        // UniformList 的 logical_scroll_top/bottom 只描述当前挂载子树，虚拟列表中会同时
        // 返回 0；必须把稳定行高的像素 offset 映射回全局行，PageUp/Down 才能闭环。
        self.scroll_source_line_strict(target, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn on_page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_page(false, cx);
    }

    pub(super) fn on_page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_page(true, cx);
    }

    pub(super) fn on_jump_to_top(&mut self, _: &JumpToTop, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_source_line_strict(0, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn on_jump_to_bottom(
        &mut self,
        _: &JumpToBottom,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(last) = self.line_count().checked_sub(1) {
            self.scroll_source_line_strict(last, ScrollStrategy::Bottom);
            cx.notify();
        }
    }

    pub(super) fn json_root_index(&self) -> Option<&JsonIndex> {
        match self.structured_index.as_ref() {
            Some(StructuredIndex::Json { index, .. }) => Some(index),
            _ => None,
        }
    }

    pub(super) fn json_container_index(&self, path: &[u64]) -> Option<&JsonIndex> {
        if path.is_empty() {
            self.json_root_index()
        } else {
            self.json_child_indexes.get(path)
        }
    }

    pub(super) fn json_visible_count(&self, container_path: &[u64], index: &JsonIndex) -> u64 {
        let mut count = index.item_count();
        for expanded in &self.json_expanded_nodes {
            if expanded.len() != container_path.len() + 1 || !expanded.starts_with(container_path) {
                continue;
            }
            if let Some(child) = self.json_child_indexes.get(expanded) {
                count = count.saturating_add(self.json_visible_count(expanded, child));
            }
        }
        count
    }

    pub(super) fn json_node_at(&self, display_index: u64) -> Option<JsonNode> {
        let root = self.json_root_index()?;
        self.json_node_at_in(&[], root, display_index, 0)
    }

    pub(super) fn json_node_at_in(
        &self,
        container_path: &[u64],
        index: &JsonIndex,
        display_index: u64,
        depth: usize,
    ) -> Option<JsonNode> {
        let mut inserted = 0u64;
        for expanded in &self.json_expanded_nodes {
            if expanded.len() != container_path.len() + 1 || !expanded.starts_with(container_path) {
                continue;
            }
            let item = *expanded.last()?;
            let root_position = item.saturating_add(inserted);
            if display_index < root_position {
                break;
            }
            if display_index == root_position {
                return Some(JsonNode {
                    container_path: container_path.to_vec(),
                    item,
                    depth,
                });
            }
            let child = self.json_child_indexes.get(expanded)?;
            let child_count = self.json_visible_count(expanded, child);
            if display_index <= root_position.saturating_add(child_count) {
                return self.json_node_at_in(
                    expanded,
                    child,
                    display_index - root_position - 1,
                    depth + 1,
                );
            }
            inserted = inserted.saturating_add(child_count);
        }
        let item = display_index.saturating_sub(inserted);
        (item < index.item_count()).then(|| JsonNode {
            container_path: container_path.to_vec(),
            item,
            depth,
        })
    }

    pub(super) fn request_json_rows(&mut self, visible: Range<usize>, cx: &mut Context<Self>) {
        let Some(StructuredIndex::Json { source, .. }) = self.structured_index.clone() else {
            return;
        };
        let Some(root) = self.json_root_index() else {
            return;
        };
        let row_count = self.json_visible_count(&[], root);
        let start = visible.start.saturating_sub(STRUCTURED_OVERSCAN_ROWS) as u64;
        let end = (visible.end.saturating_add(STRUCTURED_OVERSCAN_ROWS) as u64).min(row_count);
        let nodes = (start..end)
            .filter_map(|row| self.json_node_at(row))
            .filter(|node| !self.json_rows.contains_key(&node.path()))
            .filter_map(|node| {
                self.json_container_index(&node.container_path)
                    .cloned()
                    .map(|index| (node, index))
            })
            .collect::<Vec<_>>();
        if nodes.is_empty() {
            return;
        }
        self.structured_generation = self.structured_generation.wrapping_add(1);
        let generation = self.structured_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        self.structured_pending = Some(start..end);
        self.structured_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let mut rows = Vec::with_capacity(nodes.len());
                    for (node, index) in nodes {
                        let Some(range) = index.item_range(node.item)? else {
                            continue;
                        };
                        rows.push((
                            node.path(),
                            StructuredRow {
                                index: node.item,
                                byte_range: range,
                                column_start: 0,
                                cells: read_json_cells(&index, &source, node.item)?,
                                depth: node.depth,
                            },
                        ));
                    }
                    Ok::<_, gmark_paged_document::PagedDocumentError>(rows)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.structured_generation) {
                    return;
                }
                view.structured_pending = None;
                match result {
                    Ok(rows) => view.json_rows.extend(rows),
                    Err(error) => view.set_structure_error(error, cx),
                }
                cx.notify();
            });
        });
    }

    pub(super) fn activate_json_node(&mut self, display_row: u64, cx: &mut Context<Self>) {
        if let Some(cancellation) = self.json_expand_cancellation.take() {
            cancellation.cancel();
        }
        let Some(node) = self.json_node_at(display_row) else {
            return;
        };
        let path = node.path();
        if self.json_child_indexes.contains_key(&path) {
            if !self.json_expanded_nodes.remove(&path) {
                self.json_expanded_nodes.insert(path);
            }
            self.structured_pending = None;
            cx.notify();
            return;
        }
        let Some(parent) = self.json_container_index(&node.container_path).cloned() else {
            return;
        };
        self.json_expand_generation = self.json_expand_generation.wrapping_add(1);
        let generation = self.json_expand_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        let cancellation = SearchCancellation::default();
        self.json_expand_cancellation = Some(cancellation.clone());
        self.json_expand_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    parent.child_index_cancellable(
                        node.item,
                        JsonIndexOptions::default(),
                        &cancellation,
                    )
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.json_expand_generation) {
                    return;
                }
                view.json_expand_cancellation = None;
                match result {
                    Ok(Some(child)) => {
                        view.json_child_indexes.insert(path.clone(), child);
                        view.json_expanded_nodes.insert(path);
                        view.structured_pending = None;
                    }
                    Ok(None) => {
                        if let Some(byte_offset) =
                            view.json_rows.get(&path).map(|row| row.byte_range.start)
                        {
                            view.jump_byte_offset_to_source(byte_offset, cx);
                        }
                    }
                    Err(gmark_paged_document::PagedDocumentError::Cancelled) => {}
                    Err(error) => view.set_structure_error(error, cx),
                }
                cx.notify();
            });
        });
    }

    pub(super) fn request_structured_rows(
        &mut self,
        visible: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.structured_index, Some(StructuredIndex::Json { .. })) {
            self.request_json_rows(visible, cx);
            return;
        }
        let Some(index) = self.structured_index.clone() else {
            return;
        };
        let filter_active = !self
            .structured_filter_input
            .read(cx)
            .display_text()
            .trim()
            .is_empty();
        let row_count = if filter_active {
            self.structured_filtered_rows.len() as u64
        } else {
            index.row_count()
        };
        let start = visible.start.saturating_sub(STRUCTURED_OVERSCAN_ROWS) as u64;
        let end = (visible.end.saturating_add(STRUCTURED_OVERSCAN_ROWS) as u64).min(row_count);
        if start >= end {
            return;
        }
        let logical_rows = if filter_active {
            let Some(start) = usize::try_from(start).ok() else {
                return;
            };
            let Some(end) = usize::try_from(end).ok() else {
                return;
            };
            let Some(rows) = self.structured_filtered_rows.get(start..end) else {
                return;
            };
            rows.to_vec()
        } else {
            (start..end).collect::<Vec<_>>()
        };
        if logical_rows
            .iter()
            .all(|row| self.structured_rows.contains_key(row))
        {
            return;
        }
        // 同一时刻只允许一次视口读取。拖动滚动条会在相邻帧给出略有差异的范围；
        // 若每帧替换 Task，磁盘读取会持续被取消，画面只能在加载占位之间闪烁。
        if self.structured_pending.is_some() {
            return;
        }

        self.structured_generation = self.structured_generation.wrapping_add(1);
        let generation = self.structured_generation;
        let task_stamp = DocumentTaskStamp::capture(self, generation);
        let requested = start..end;
        let requested_center = logical_rows
            .get(logical_rows.len() / 2)
            .copied()
            .unwrap_or(start);
        let requested_for_read = requested.clone();
        let requested_for_completion = requested.clone();
        let column_start = self.structured_column_window_start;
        let column_end = column_start.saturating_add(STRUCTURED_COLUMN_WINDOW);
        let columns = column_start..column_end;
        self.structured_pending = Some(requested.clone());
        self.structured_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    if filter_active {
                        let mut rows = Vec::with_capacity(logical_rows.len());
                        for row in logical_rows {
                            rows.extend(index.read_rows(row, 1, columns.clone())?);
                        }
                        Ok(rows)
                    } else {
                        index.read_rows(
                            requested_for_read.start,
                            usize::try_from(requested_for_read.end - requested_for_read.start)
                                .unwrap_or(STRUCTURED_OVERSCAN_ROWS * 3),
                            columns,
                        )
                    }
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if !task_stamp.accepts_strict(view, view.structured_generation) {
                    if view.structured_pending.as_ref() == Some(&requested_for_completion) {
                        view.structured_pending = None;
                        cx.notify();
                    }
                    return;
                }
                view.structured_pending = None;
                match result {
                    Ok(rows) => {
                        view.structured_rows
                            .extend(rows.into_iter().map(|row| (row.index, row)));
                        // 保留相邻 viewport 的重叠行，避免小步滚动把上一帧重新打回占位；
                        // 超预算后只淘汰离本次请求中心最远的端点，内存仍与文件大小解耦。
                        prune_structured_row_cache(
                            &mut view.structured_rows,
                            requested_center,
                            MAX_STRUCTURED_CACHED_ROWS,
                        );
                        view.clear_structure_error();
                    }
                    Err(error) => view.set_structure_error(error, cx),
                }
                cx.notify();
            });
        });
    }

    /// Split 中只同步左侧源码位置，不改变当前模式，保证右侧预览仍留在原位。
    pub(super) fn reveal_structured_row_in_split(&mut self, row: u64, cx: &mut Context<Self>) {
        let Some(byte_offset) = self
            .structured_rows
            .get(&row)
            .map(|row| row.byte_range.start)
        else {
            return;
        };
        let Some(line) = self
            .document
            .as_ref()
            .and_then(|document| document.line_for_offset(byte_offset.min(document.len())))
            .and_then(|line| usize::try_from(line).ok())
        else {
            return;
        };
        self.anchor_source_window_for_byte(line as u64, byte_offset);
        self.select_source_lines(line..line.saturating_add(1), false);
        self.scroll_source_line(line, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn jump_byte_offset_to_source(&mut self, byte_offset: u64, cx: &mut Context<Self>) {
        let Some(line) = self
            .document
            .as_ref()
            .and_then(|document| document.line_for_offset(byte_offset.min(document.len())))
            .and_then(|line| usize::try_from(line).ok())
        else {
            return;
        };
        self.anchor_source_window_for_byte(line as u64, byte_offset);
        self.view_mode = DocumentHostViewMode::Source;
        self.sync_tab_active_view();
        self.select_source_lines(line..line.saturating_add(1), false);
        self.scroll_source_line(line, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn source_list_len(&self) -> usize {
        self.fold_projection
            .visible_line_count()
            .saturating_sub(self.source_list_origin)
            .min(SOURCE_LIST_WINDOW_ROWS)
    }

    pub(super) fn scroll_source_line(&mut self, line: usize, strategy: ScrollStrategy) {
        let local = self.prepare_source_list_target(line);
        self.scroll_handle.scroll_to_item(local, strategy);
    }

    pub(super) fn scroll_source_line_strict(&mut self, line: usize, strategy: ScrollStrategy) {
        let local = self.prepare_source_list_target(line);
        self.scroll_handle.scroll_to_item_strict(local, strategy);
    }

    fn prepare_source_list_target(&mut self, requested: usize) -> usize {
        let real_total = self.line_count().max(1);
        let real_target = requested.min(real_total.saturating_sub(1));
        self.ensure_source_line_visible(real_target);
        let total = self.fold_projection.visible_line_count().max(1);
        let target = self.fold_projection.visible_line_for_real(real_target);
        let window_end = self
            .source_list_origin
            .saturating_add(SOURCE_LIST_WINDOW_ROWS)
            .min(total);
        if target < self.source_list_origin || target >= window_end {
            self.source_list_origin = source_list_origin_for_target(total, target);
        }
        target.saturating_sub(self.source_list_origin)
    }

    pub(super) fn line_count(&self) -> usize {
        self.document.as_ref().map_or_else(
            || {
                usize::try_from(self.probe.estimated_lines)
                    .unwrap_or(usize::MAX)
                    .max(self.preview_lines.len())
            },
            |document| usize::try_from(document.line_count()).unwrap_or(usize::MAX),
        )
    }

    pub(super) fn line_window(&self, line: usize) -> Option<&BoundedLineWindow> {
        self.displayed_screen_lines.row(line)
    }

    pub(super) fn line_text(&self, line: usize) -> SharedString {
        if let Some(window) = self.line_window(line) {
            return window.rendered(self.show_line_endings);
        }
        self.preview_lines.get(line).cloned().unwrap_or_default()
    }

    pub(super) fn selected_search_range(&self, line: usize) -> Option<Range<usize>> {
        let found = self.search_results.get(self.search_selected)?;
        let document = self.document.as_ref()?;
        if document.line_for_offset(found.range.start)? != line as u64 {
            return None;
        }
        let window = self.line_window(line)?;
        if found.range.start >= window.content_range.end
            || found.range.end <= window.content_range.start
        {
            return None;
        }
        let rendered = &window.text;
        let start = usize::try_from(
            found
                .range
                .start
                .max(window.content_range.start)
                .saturating_sub(window.content_range.start),
        )
        .ok()?;
        let end = usize::try_from(
            found
                .range
                .end
                .min(window.content_range.end)
                .saturating_sub(window.content_range.start),
        )
        .ok()?;
        if start >= end
            || end > rendered.len()
            || !rendered.is_char_boundary(start)
            || !rendered.is_char_boundary(end)
        {
            return None;
        }
        Some(start..end)
    }

    #[cfg(test)]
    pub(super) fn begin_line_edit(
        &mut self,
        line: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.saving || self.reloading {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let Ok(Some(windowed)) =
            read_bounded_line_window(document, line as u64, self.source_window_start)
        else {
            return;
        };
        // Soak/tests must reuse the same bounded row entity as pointer activation. Tests may call
        // before the first row snapshot is painted, so only that first activation builds the
        // entity from the bounded document window; subsequent cycles reuse the cache entry.
        let block = if let Some(block) = self
            .source_row_blocks
            .get(&line)
            .filter(|block| block.read(cx).display_text() == windowed.text.as_ref())
            .cloned()
        {
            block
        } else {
            let text = windowed.text.to_string();
            let host = cx.entity().downgrade();
            let block = cx.new(move |cx| {
                let mut block = Block::with_record(
                    cx,
                    BlockRecord::with_plain_text(BlockKind::Paragraph, text),
                );
                block.set_compact_source_host();
                block.set_host_action_handler(move |action, window, cx| {
                    let _ = host.update(cx, |view, cx| {
                        view.on_line_edit_host_action(action, window, cx)
                    });
                });
                block
            });
            cx.subscribe(&block, Self::on_line_edit_event).detach();
            self.source_row_blocks.insert(line, block.clone());
            block
        };
        let BoundedLineWindow {
            replace_range,
            ending,
            leading_truncated,
            trailing_truncated,
            ..
        } = windowed;
        block.update(cx, |block, _cx| {
            block.selected_range = block.display_text().len()..block.display_text().len();
            block.focus_handle.focus(window);
        });
        self.active_edit = Some(SourceLineEdit {
            line,
            range: replace_range,
            ending,
            leading_truncated,
            trailing_truncated,
            block,
        });
        cx.emit(DocumentHostEvent::StateChanged);
        cx.notify();
    }

    pub(super) fn on_line_edit_host_action(
        &mut self,
        action: BlockHostAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            BlockHostAction::Submit(_) => {}
            BlockHostAction::Save => self.on_save_document(&SaveDocument, window, cx),
            BlockHostAction::Undo => self.on_undo(&Undo, window, cx),
            BlockHostAction::Redo => self.on_redo(&Redo, window, cx),
            BlockHostAction::Find => self.on_find_in_document(&FindInDocument, window, cx),
            BlockHostAction::FindNext => self.on_find_next(&FindNext, window, cx),
            BlockHostAction::FindPrevious => self.on_find_previous(&FindPrevious, window, cx),
            BlockHostAction::GoToLine => self.on_go_to_line(&GoToLine, window, cx),
            BlockHostAction::PageUp => {
                self.leave_line_edit_for_viewport_navigation(window);
                self.on_page_up(&PageUp, window, cx);
            }
            BlockHostAction::PageDown => {
                self.leave_line_edit_for_viewport_navigation(window);
                self.on_page_down(&PageDown, window, cx);
            }
            BlockHostAction::JumpToTop => {
                self.leave_line_edit_for_viewport_navigation(window);
                self.on_jump_to_top(&JumpToTop, window, cx);
            }
            BlockHostAction::JumpToBottom => {
                self.leave_line_edit_for_viewport_navigation(window);
                self.on_jump_to_bottom(&JumpToBottom, window, cx);
            }
            BlockHostAction::DismissTransientUi => {
                self.on_dismiss_transient_ui(&DismissTransientUi, window, cx)
            }
        }
    }

    pub(super) fn leave_line_edit_for_viewport_navigation(&mut self, window: &mut Window) {
        // 翻页会卸载当前虚拟行；焦点若继续留在该 Block，下一次快捷键没有可达的
        // element path。编辑已按 Changed 事件增量提交，可以安全回到宿主焦点。
        self.active_edit = None;
        self.focus_handle.focus(window);
    }

    pub(super) fn select_or_edit_line(
        &mut self,
        line: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_source_row_from_pointer(line, event, window, cx);
    }
}

fn format_byte_count(bytes: u64) -> String {
    const KIB: u64 = 1_024;
    const MIB: u64 = KIB * 1_024;
    const GIB: u64 = MIB * 1_024;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}
