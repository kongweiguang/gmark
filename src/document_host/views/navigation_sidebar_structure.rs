// @author kongweiguang

//! Structured sidebar rows and reveal actions.

use super::*;
use crate::theme::ThemeColors;
use gpui::AnyElement;

impl DocumentHost {
    pub(super) fn render_document_sidebar_structure(
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
                    .border(px(1.0))
                    .border_color(colors.workbench.control_surface.opacity(0.0))
                    .hover(|this| this.bg(colors.workbench.control_hover))
                    .focus(|this| this.border_color(colors.workbench.focus_ring))
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
                                    .text_color(colors.workbench.text_secondary),
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
                            .id(("document-sidebar-structure-label", index))
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .truncate()
                            .text_color(colors.workbench.text_primary)
                            .child(label.clone())
                            .tooltip(move |_window, cx| crate::ui::ui_tooltip(label.clone(), cx)),
                    )
                    .child(
                        div()
                            .id(("document-sidebar-structure-value", index))
                            .max_w(px(110.0))
                            .overflow_hidden()
                            .truncate()
                            .text_color(colors.workbench.text_secondary)
                            .child(value.clone())
                            .tooltip(move |_window, cx| crate::ui::ui_tooltip(value.clone(), cx)),
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
                    .text_color(colors.workbench.text_tertiary)
                    .child(
                        strings
                            .document_sidebar_items_limit
                            .replace("{count}", &count.to_string()),
                    ),
            );
        }
        body.into_any_element()
    }

    pub(super) fn render_document_sidebar_loading(
        &self,
        colors: &ThemeColors,
        strings: &I18nStrings,
    ) -> AnyElement {
        div()
            .id("document-sidebar-loading")
            .debug_selector(|| "document-sidebar-loading".to_owned())
            .w_full()
            .min_h(px(96.0))
            .px(px(8.0))
            .py(px(12.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.0))
            .text_color(colors.workbench.text_secondary)
            .child(strings.document_sidebar_loading.clone())
            .into_any_element()
    }

    pub(super) fn render_document_sidebar_empty(
        &self,
        colors: &ThemeColors,
        strings: &I18nStrings,
    ) -> AnyElement {
        div()
            .id("document-sidebar-empty")
            .debug_selector(|| "document-sidebar-empty".to_owned())
            .w_full()
            .min_h(px(96.0))
            .px(px(8.0))
            .py(px(12.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.0))
            .text_color(colors.workbench.text_secondary)
            .child(strings.document_sidebar_empty.clone())
            .into_any_element()
    }

    pub(super) fn reveal_document_sidebar_column(
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

    pub(super) fn reveal_document_sidebar_structure(
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
}
