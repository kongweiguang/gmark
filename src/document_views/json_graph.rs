// @author kongweiguang

use super::*;
use gpui::{PathBuilder, canvas, point};
use std::collections::{HashMap, HashSet};

#[path = "json_graph/model.rs"]
pub(super) mod model;
#[path = "json_graph/style.rs"]
mod style;
pub(super) use model::GraphLayoutCache;
use model::{
    CARD_HEADER_HEIGHT as GRAPH_CARD_HEADER_HEIGHT, CARD_ROW_HEIGHT as GRAPH_CARD_ROW_HEIGHT,
    GraphLayoutKey, MAX_ZOOM as GRAPH_MAX_ZOOM, MIN_ZOOM as GRAPH_MIN_ZOOM, READABLE_MIN_ZOOM,
    ROW_LIMIT_STEP, SEARCH_REVEAL_ZOOM, edge_intersects_viewport, fit_camera, graph_layout,
    initial_collapsed_items, node_intersects_viewport, row_limit, selected_path_edges,
};
use style::JsonGraphPalette;

#[derive(Clone, Copy)]
enum GraphCardRow<'a> {
    Field(&'a JsonGraphField),
    Child(&'a JsonGraphEdge),
}

impl GraphCardRow<'_> {
    fn source_start(self) -> u64 {
        match self {
            Self::Field(field) => field.source.range.start,
            Self::Child(edge) => edge.source.range.start,
        }
    }
}

fn graph_card_rows<'a>(
    node: &'a JsonGraphNode,
    edges: impl IntoIterator<Item = &'a JsonGraphEdge>,
) -> Vec<GraphCardRow<'a>> {
    let mut rows = node
        .fields
        .iter()
        .map(GraphCardRow::Field)
        .chain(edges.into_iter().map(GraphCardRow::Child))
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.source_start());
    rows
}

fn search_reveal_row_limit(
    graph: &JsonGraphProjection,
    selected: &JsonGraphItemId,
) -> Option<(JsonGraphItemId, usize)> {
    graph.nodes.iter().find_map(|node| {
        let outgoing = graph.edges.iter().filter(|edge| edge.from == node.id);
        graph_card_rows(node, outgoing)
            .iter()
            .position(|row| match row {
                GraphCardRow::Field(field) => field.id == *selected,
                GraphCardRow::Child(edge) => edge.to == *selected,
            })
            .map(|row| {
                let required = row + 1;
                let limit = if required <= model::DEFAULT_ROW_LIMIT {
                    model::DEFAULT_ROW_LIMIT
                } else {
                    model::DEFAULT_ROW_LIMIT
                        + (required - model::DEFAULT_ROW_LIMIT).div_ceil(ROW_LIMIT_STEP)
                            * ROW_LIMIT_STEP
                };
                (node.id.clone(), limit)
            })
    })
}

pub(super) fn json_graph_node_matches_query(node: &JsonGraphNode, query: &str) -> bool {
    node.label.to_lowercase().contains(query)
        || node.json_path.to_lowercase().contains(query)
        || node.fields.iter().any(|field| {
            field.json_path.to_lowercase().contains(query)
                || field.label.to_lowercase().contains(query)
                || field.display_value.to_lowercase().contains(query)
        })
}

fn zoom_camera_around(
    camera_x: f32,
    camera_y: f32,
    old_zoom: f32,
    new_zoom: f32,
    pointer_x: f32,
    pointer_y: f32,
) -> (f32, f32) {
    let scale = new_zoom / old_zoom.max(f32::EPSILON);
    (
        pointer_x - (pointer_x - camera_x) * scale,
        pointer_y - (pointer_y - camera_y) * scale,
    )
}

fn expand_ancestors(
    graph: &JsonGraphProjection,
    selected: &JsonGraphItemId,
    collapsed_items: &mut Vec<Arc<str>>,
) {
    let parent_by_child = graph
        .edges
        .iter()
        .map(|edge| (edge.to.as_str(), edge.from.as_str()))
        .collect::<HashMap<_, _>>();
    let mut cursor = selected.as_str();
    while let Some(parent) = parent_by_child.get(cursor) {
        collapsed_items.retain(|item| item.as_ref() != *parent);
        cursor = parent;
    }
}

fn bounded_node_content(document: Option<&DocumentSession>, node: &JsonGraphNode) -> SharedString {
    bounded_graph_content(document, node.source.range.clone(), &node.label)
}

fn bounded_graph_content(
    document: Option<&DocumentSession>,
    range: Range<u64>,
    fallback: &str,
) -> SharedString {
    let byte_len = range.end.saturating_sub(range.start);
    if byte_len <= 32 * 1024 {
        return document
            .and_then(|document| document.read_range(range).ok())
            .map(|bytes| SharedString::from(String::from_utf8_lossy(&bytes).into_owned()))
            .unwrap_or_else(|| fallback.to_owned().into());
    }
    format!("{byte_len} bytes · {fallback}").into()
}

/// 图投影路径包含同级物理序号，用于稳定定位重复键；详情和剪贴板只暴露标准 JSONPath。
fn jsonpath_for_display(internal_path: &str) -> String {
    let Some(path) = internal_path.strip_prefix('$') else {
        return internal_path.to_owned();
    };
    let mut jsonpath = String::from("$");
    let path = path.strip_prefix('/').unwrap_or(path);
    if path.is_empty() {
        return jsonpath;
    }

    for segment in path.split('/') {
        if !segment.contains('#') && segment.chars().all(|character| character.is_ascii_digit()) {
            jsonpath.push('[');
            jsonpath.push_str(segment);
            jsonpath.push(']');
            continue;
        }

        let key = segment
            .rsplit_once('#')
            .filter(|(_, ordinal)| {
                !ordinal.is_empty() && ordinal.chars().all(|character| character.is_ascii_digit())
            })
            .map_or(segment, |(key, _)| key)
            .replace("~1", "/")
            .replace("~0", "~");
        let shorthand = key
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
            && key
                .chars()
                .skip(1)
                .all(|character| character.is_ascii_alphanumeric() || character == '_');
        if shorthand {
            jsonpath.push('.');
            jsonpath.push_str(&key);
        } else {
            jsonpath.push_str("['");
            jsonpath.push_str(&key.replace('\\', "\\\\").replace('\'', "\\'"));
            jsonpath.push_str("']");
        }
    }
    jsonpath
}

fn node_edit_target(snapshot: &JsonGraphSnapshot, node: &JsonGraphNode) -> JsonGraphEditTarget {
    JsonGraphEditTarget {
        item_id: node.id.clone(),
        range: node.source.range.clone(),
        document_epoch: snapshot.document_epoch(),
        base_revision: snapshot.revision(),
        label: node.label.clone(),
        kind: node.kind,
    }
}

fn field_edit_target(snapshot: &JsonGraphSnapshot, field: &JsonGraphField) -> JsonGraphEditTarget {
    JsonGraphEditTarget {
        item_id: field.id.clone(),
        range: field.source.range.clone(),
        document_epoch: snapshot.document_epoch(),
        base_revision: snapshot.revision(),
        label: field.label.clone(),
        kind: field.kind,
    }
}

impl DocumentHost {
    pub(super) fn begin_json_graph_edit(
        &mut self,
        target: JsonGraphEditTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        const MAX_GRAPH_EDIT_BYTES: u64 = 256 * 1024;
        let byte_len = target.range.end.saturating_sub(target.range.start);
        let content = if byte_len <= MAX_GRAPH_EDIT_BYTES {
            self.document
                .as_ref()
                .and_then(|document| document.read_range(target.range.clone()).ok())
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        } else {
            None
        };
        let Some(content) = content else {
            let strings = cx.global::<I18nManager>().strings();
            self.graph_edit_error = Some(
                strings
                    .json_graph_edit_too_large_template
                    .replace("{bytes}", &byte_len.to_string())
                    .into(),
            );
            self.graph_edit_issue = Some(JsonGraphEditIssue::TooLarge);
            self.graph_edit_original = None;
            self.graph_edit_target = Some(target);
            self.graph_edit_input.update(cx, |block, cx| {
                let len = block.display_text().len();
                block.replace_text_in_visible_range(0..len, "", None, false, cx);
                block.focus_handle.focus(window);
            });
            cx.notify();
            return;
        };
        self.graph_context_menu = None;
        self.graph_edit_error = None;
        self.graph_edit_issue = None;
        self.graph_edit_original = Some(Arc::from(content.as_str()));
        self.graph_edit_target = Some(target);
        self.graph_edit_input.update(cx, |block, cx| {
            let len = block.display_text().len();
            block.replace_text_in_visible_range(0..len, &content, None, false, cx);
            block.focus_handle.focus(window);
        });
        cx.notify();
    }

    fn commit_json_graph_edit(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.graph_edit_target.clone() else {
            return;
        };
        let replacement = self.graph_edit_input.read(cx).display_text().to_owned();
        let current_revision = self.document.as_ref().map(DocumentSession::revision);
        if target.document_epoch != self.document_epoch
            || current_revision != Some(target.base_revision)
        {
            let strings = cx.global::<I18nManager>().strings();
            self.graph_edit_error = Some(strings.json_graph_source_changed.clone().into());
            self.graph_edit_issue = Some(JsonGraphEditIssue::Stale);
            cx.notify();
            return;
        }
        let parsed = serde_json::from_str::<serde_json::Value>(replacement.trim());
        let valid = parsed.as_ref().is_ok_and(|value| match target.kind {
            JsonValueKind::Object => value.is_object(),
            JsonValueKind::Array => value.is_array(),
            JsonValueKind::String
            | JsonValueKind::Number
            | JsonValueKind::Boolean
            | JsonValueKind::Null => !value.is_object() && !value.is_array(),
        });
        if !valid {
            let strings = cx.global::<I18nManager>().strings();
            self.graph_edit_error = Some(strings.json_graph_edit_invalid.clone().into());
            self.graph_edit_issue = Some(JsonGraphEditIssue::Invalid);
            cx.notify();
            return;
        }
        if self.replace_source_range_from_graph(
            target.base_revision,
            target.range,
            replacement.trim(),
            cx,
        ) {
            self.graph_edit_target = None;
            self.graph_edit_error = None;
            self.graph_edit_issue = None;
            self.graph_edit_original = None;
        }
    }

    fn cancel_json_graph_edit(&mut self, cx: &mut Context<Self>) {
        self.graph_edit_target = None;
        self.graph_edit_error = None;
        self.graph_edit_issue = None;
        self.graph_edit_original = None;
        cx.notify();
    }

    fn cancel_json_graph_edit_if_pristine(&mut self, cx: &mut Context<Self>) {
        let draft = self.graph_edit_input.read(cx).display_text();
        if self
            .graph_edit_original
            .as_deref()
            .is_some_and(|original| original != draft)
        {
            return;
        }
        self.cancel_json_graph_edit(cx);
    }

    fn reload_json_graph_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item_id) = self
            .graph_edit_target
            .as_ref()
            .map(|target| target.item_id.clone())
        else {
            return;
        };
        let Some(target) = self.resolve_json_graph_edit_target(&item_id) else {
            self.cancel_json_graph_edit(cx);
            return;
        };
        self.begin_json_graph_edit(target, window, cx);
    }

    pub(super) fn resolve_json_graph_edit_target(
        &self,
        item_id: &JsonGraphItemId,
    ) -> Option<JsonGraphEditTarget> {
        let snapshot = self
            .derived_projection_snapshot
            .as_ref()?
            .as_any()
            .downcast_ref::<JsonGraphSnapshot>()?;
        if let Some(node) = snapshot
            .projection()
            .nodes
            .iter()
            .find(|node| node.id == *item_id)
        {
            return Some(node_edit_target(snapshot, node));
        }
        snapshot
            .projection()
            .nodes
            .iter()
            .flat_map(|node| node.fields.iter())
            .find(|field| field.id == *item_id)
            .map(|field| field_edit_target(snapshot, field))
    }

    pub(crate) fn begin_selected_json_graph_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self
            .graph_selected_item
            .as_ref()
            .and_then(|item| self.resolve_json_graph_edit_target(item))
        else {
            self.graph_focus_handle.focus(window);
            return;
        };
        self.begin_json_graph_edit(target, window, cx);
    }

    fn select_json_graph_item(
        &mut self,
        id: JsonGraphItemId,
        source: Range<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.graph_focus_handle.focus(window);
        self.graph_selected_item = Some(id.clone());
        document_view_state_mut(&mut self.document, &mut self.tab_view_state)
            .derived
            .entry(DocumentViewId::json_graph())
            .or_default()
            .selected_item = Some(Arc::from(id.as_str()));
        if self.view_mode == DocumentHostViewMode::Split {
            self.select_json_source_range(source, true, cx);
        }
        cx.notify();
    }

    pub(super) fn dismiss_json_graph_details(&mut self) {
        self.graph_selected_item = None;
        if let Some(state) = document_view_state_mut(&mut self.document, &mut self.tab_view_state)
            .derived
            .get_mut(&DocumentViewId::json_graph())
        {
            state.selected_item = None;
        }
    }

    fn navigate_json_graph_search(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.graph_search_matches.is_empty() {
            return;
        }
        let len = self.graph_search_matches.len();
        self.graph_search_selected = if delta < 0 {
            (self.graph_search_selected + len - 1) % len
        } else {
            (self.graph_search_selected + 1) % len
        };
        let selected = self.graph_search_matches[self.graph_search_selected].clone();
        self.graph_selected_item = Some(selected.clone());
        self.graph_pending_center = Some(selected.clone());
        self.reveal_graph_item(&selected);
        cx.notify();
    }

    /// 编辑遮罩必须挂在 SourceBacked 内容根层，不能成为可缩放、可裁剪画布的子元素。
    /// 这样 Preview 与 Split 共享同一套焦点和尺寸语义，窗口变化时也不会丢失草稿。
    pub(super) fn render_json_graph_edit_overlay(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let target = self.graph_edit_target.clone()?;
        let theme = cx.global::<ThemeManager>().current();
        let colors = &theme.colors;
        let strings = cx.global::<I18nManager>().strings();
        let container = matches!(target.kind, JsonValueKind::Object | JsonValueKind::Array);
        let error = self.graph_edit_error.clone();
        let issue = self.graph_edit_issue;
        let title = format!("{} · {}", strings.json_graph_edit_value, target.label);
        Some(
            div()
                .id("json-graph-edit-overlay")
                .debug_selector(|| "json-graph-edit-overlay".to_owned())
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .items_center()
                .justify_center()
                .occlude()
                .bg(colors.editor_background.opacity(0.42))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.cancel_json_graph_edit_if_pristine(cx)),
                )
                .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        cx.stop_propagation();
                        this.cancel_json_graph_edit(cx);
                    } else if event.keystroke.key == "enter"
                        && (!container
                            || event.keystroke.modifiers.control
                            || event.keystroke.modifiers.platform)
                    {
                        cx.stop_propagation();
                        this.commit_json_graph_edit(cx);
                    }
                }))
                .child(
                    div()
                        .id("json-graph-edit-panel")
                        .debug_selector(|| "json-graph-edit-panel".to_owned())
                        .w(px(viewport_width.min(560.0).max(300.0)))
                        .max_h(px((viewport_height - 64.0).max(280.0)))
                        .p(px(14.0))
                        .flex()
                        .flex_col()
                        .gap(px(9.0))
                        .rounded(px(9.0))
                        .border(px(1.0))
                        .border_color(colors.dialog_border)
                        .bg(colors.dialog_surface)
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(colors.text_default)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(colors.dialog_muted)
                                .child(strings.json_graph_edit_help.clone()),
                        )
                        .child(
                            div()
                                .id("json-graph-edit-input")
                                .debug_selector(|| "json-graph-edit-input".to_owned())
                                .min_h(px(if container { 180.0 } else { 44.0 }))
                                .max_h(px(320.0))
                                .p(px(9.0))
                                .overflow_y_scroll()
                                .rounded(px(6.0))
                                .border(px(1.0))
                                .border_color(if error.is_some() {
                                    colors.callout_warning_border
                                } else {
                                    colors.dialog_border
                                })
                                .bg(colors.editor_background)
                                .child(self.graph_edit_input.clone()),
                        )
                        .children(error.map(|error| {
                            div()
                                .id("json-graph-edit-error")
                                .debug_selector(|| "json-graph-edit-error".to_owned())
                                .text_size(px(11.0))
                                .text_color(colors.text_default)
                                .child(error)
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_end()
                                .gap(px(7.0))
                                .children((issue == Some(JsonGraphEditIssue::Stale)).then(|| {
                                    div()
                                        .id("json-graph-edit-reload")
                                        .debug_selector(|| "json-graph-edit-reload".to_owned())
                                        .h(px(30.0))
                                        .px(px(11.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.0))
                                        .cursor_pointer()
                                        .bg(colors.dialog_secondary_button_bg)
                                        .hover(|button| {
                                            button.bg(colors.dialog_secondary_button_hover)
                                        })
                                        .text_size(px(11.0))
                                        .text_color(colors.dialog_body)
                                        .child(strings.json_graph_reload_value.clone())
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.reload_json_graph_edit(window, cx)
                                        }))
                                }))
                                .children((issue == Some(JsonGraphEditIssue::TooLarge)).then(
                                    || {
                                        let range = target.range.clone();
                                        div()
                                            .id("json-graph-edit-source")
                                            .debug_selector(|| "json-graph-edit-source".to_owned())
                                            .h(px(30.0))
                                            .px(px(11.0))
                                            .flex()
                                            .items_center()
                                            .rounded(px(6.0))
                                            .cursor_pointer()
                                            .bg(colors.dialog_secondary_button_bg)
                                            .hover(|button| {
                                                button.bg(colors.dialog_secondary_button_hover)
                                            })
                                            .text_size(px(11.0))
                                            .text_color(colors.dialog_body)
                                            .child(strings.json_graph_edit_source.clone())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.cancel_json_graph_edit(cx);
                                                this.select_json_source_range(
                                                    range.clone(),
                                                    false,
                                                    cx,
                                                );
                                                cx.emit(DocumentHostEvent::ViewModeChanged(
                                                    DocumentHostMode::Source,
                                                ));
                                            }))
                                    },
                                ))
                                .child(
                                    div()
                                        .id("json-graph-edit-cancel")
                                        .debug_selector(|| "json-graph-edit-cancel".to_owned())
                                        .h(px(30.0))
                                        .px(px(11.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.0))
                                        .cursor_pointer()
                                        .bg(colors.dialog_secondary_button_bg)
                                        .hover(|button| {
                                            button.bg(colors.dialog_secondary_button_hover)
                                        })
                                        .text_size(px(11.0))
                                        .text_color(colors.dialog_body)
                                        .child(strings.unsaved_changes_cancel.clone())
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.cancel_json_graph_edit(cx)
                                        })),
                                )
                                .children((issue != Some(JsonGraphEditIssue::TooLarge)).then(
                                    || {
                                        div()
                                            .id("json-graph-edit-save")
                                            .debug_selector(|| "json-graph-edit-save".to_owned())
                                            .h(px(30.0))
                                            .px(px(11.0))
                                            .flex()
                                            .items_center()
                                            .rounded(px(6.0))
                                            .cursor_pointer()
                                            .bg(colors.dialog_primary_button_bg)
                                            .hover(|button| {
                                                button.bg(colors.dialog_primary_button_hover)
                                            })
                                            .text_size(px(11.0))
                                            .text_color(colors.dialog_primary_button_text)
                                            .child(strings.menu_save.clone())
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.commit_json_graph_edit(cx)
                                            }))
                                    },
                                )),
                        ),
                ),
        )
    }

    pub(super) fn reveal_graph_item(&mut self, selected: &JsonGraphItemId) {
        let Some(graph) = self
            .derived_projection_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.as_any().downcast_ref::<JsonGraphSnapshot>())
            .map(JsonGraphSnapshot::projection)
        else {
            return;
        };
        // 搜索可以命中高密度卡片中尚未构造的行；先提升该卡片的运行时行预算，
        // 再展开祖先，保证随后布局得到真实端口并能把命中项居中。
        if let Some((parent, required)) = search_reveal_row_limit(graph, selected) {
            let limit = self
                .graph_row_limits
                .entry(parent)
                .or_insert(model::DEFAULT_ROW_LIMIT);
            if required > *limit {
                *limit = required;
                self.graph_layout_cache = None;
            }
        }
        let state = document_view_state_mut(&mut self.document, &mut self.tab_view_state)
            .derived
            .entry(DocumentViewId::json_graph())
            .or_default();
        expand_ancestors(graph, selected, &mut state.collapsed_items);
    }

    pub(super) fn render_json_graph_panel(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = cx.global::<ThemeManager>().current_arc();
        let strings = cx.global::<I18nManager>().strings().clone();
        let colors = &theme.colors;
        let palette = JsonGraphPalette::from_theme(colors);
        let installed_snapshot = self
            .derived_projection_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.as_any().downcast_ref::<JsonGraphSnapshot>());
        let Some(installed_snapshot) = installed_snapshot else {
            let (title, detail): (SharedString, SharedString) =
                if let Some(error) = &self.derived_projection_error {
                    (
                        strings.json_graph_preview_unavailable.clone().into(),
                        error.clone(),
                    )
                } else {
                    (
                        strings.json_graph_generating.clone().into(),
                        strings.json_graph_generating_detail.clone().into(),
                    )
                };
            return div()
                .id("json-graph-empty-state")
                .debug_selector(|| "json-graph-empty-state".to_owned())
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .bg(colors.editor_background)
                .text_color(colors.text_default)
                .child(div().text_size(px(14.0)).child(title))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(colors.text_placeholder)
                        .child(detail),
                )
                .children(self.derived_projection_error_offset.map(|offset| {
                    div()
                        .id("json-graph-error-jump")
                        .debug_selector(|| "json-graph-error-jump".to_owned())
                        .mt(px(4.0))
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .bg(colors.dialog_secondary_button_bg)
                        .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                        .child(
                            strings
                                .json_graph_locate_byte_template
                                .replace("{offset}", &offset.to_string()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.jump_byte_offset_to_source(offset, cx);
                            cx.emit(DocumentHostEvent::ViewModeChanged(DocumentHostMode::Source));
                        }))
                }));
        };
        let graph = installed_snapshot.projection();
        let projection_epoch = installed_snapshot.document_epoch();
        let projection_revision = installed_snapshot.revision();
        let projection_identity = (
            installed_snapshot.document_epoch(),
            installed_snapshot.revision(),
            installed_snapshot.generation(),
        );
        if self.graph_projection_identity != Some(projection_identity) {
            self.graph_projection_identity = Some(projection_identity);
            self.graph_row_limits.clear();
            self.graph_layout_cache = None;
            self.graph_state_initialized = false;
            self.graph_needs_fit = true;
            self.graph_fit_all_requested = false;
        }

        let view_id = DocumentViewId::json_graph();
        let view_state = document_view_state_mut(&mut self.document, &mut self.tab_view_state)
            .derived
            .entry(view_id.clone())
            .or_default();
        let viewport = (viewport_width.max(1.0), viewport_height.max(1.0));
        if self.graph_last_viewport.is_none_or(|last| {
            (last.0 - viewport.0).abs() > 1.0 || (last.1 - viewport.1).abs() > 1.0
        }) {
            if let Some(last) = self.graph_last_viewport
                && !self.graph_needs_fit
            {
                let zoom = view_state.zoom.max(f32::EPSILON);
                let world_x = (last.0 * 0.5 - view_state.camera_x) / zoom;
                let world_y = (last.1 * 0.5 - view_state.camera_y) / zoom;
                view_state.camera_x = viewport.0 * 0.5 - world_x * zoom;
                view_state.camera_y = viewport.1 * 0.5 - world_y * zoom;
            }
            self.graph_last_viewport = Some(viewport);
        }
        if !self.graph_state_initialized {
            if view_state.collapsed_items.is_empty() {
                view_state.collapsed_items = initial_collapsed_items(graph, &self.graph_row_limits);
            }
            self.graph_state_initialized = true;
        }
        let collapsed = view_state
            .collapsed_items
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        let layout_key = GraphLayoutKey::new(
            projection_identity.0,
            projection_identity.1,
            projection_identity.2,
            &collapsed,
            &self.graph_row_limits,
        );
        let layout = if let Some(cache) = self
            .graph_layout_cache
            .as_ref()
            .filter(|cache| cache.key == layout_key)
        {
            cache.layout.clone()
        } else {
            let layout = Arc::new(graph_layout(graph, &collapsed, &self.graph_row_limits));
            self.graph_layout_cache = Some(GraphLayoutCache {
                key: layout_key,
                layout: layout.clone(),
            });
            layout
        };
        if self.graph_needs_fit
            || (view_state.camera_x == 0.0 && view_state.camera_y == 0.0 && view_state.zoom == 1.0)
        {
            let minimum_zoom = if self.graph_fit_all_requested {
                GRAPH_MIN_ZOOM
            } else {
                READABLE_MIN_ZOOM
            };
            let (x, y, zoom) = fit_camera(&layout, viewport_width, viewport_height, minimum_zoom);
            view_state.camera_x = x;
            view_state.camera_y = y;
            view_state.zoom = zoom;
            self.graph_needs_fit = false;
            self.graph_fit_all_requested = false;
        }
        if let Some((anchor_id, anchor_position)) = self.graph_recenter_anchor.take()
            && let Some(position) = layout
                .nodes
                .iter()
                .find(|position| graph.nodes[position.index].id == anchor_id)
        {
            view_state.camera_x = f32::from(anchor_position.x)
                - (position.x + position.width * 0.5) * view_state.zoom;
            view_state.camera_y = f32::from(anchor_position.y)
                - (position.y + position.height * 0.5) * view_state.zoom;
        }
        if let Some(target) = self.graph_pending_center.take()
            && let Some(position) = layout
                .nodes
                .iter()
                .find(|position| graph.nodes[position.index].id == target)
        {
            view_state.zoom = view_state.zoom.max(SEARCH_REVEAL_ZOOM);
            view_state.camera_x =
                viewport_width * 0.5 - (position.x + position.width * 0.5) * view_state.zoom;
            view_state.camera_y =
                viewport_height * 0.5 - (position.y + position.height * 0.5) * view_state.zoom;
        }
        let camera_x = view_state.camera_x;
        let camera_y = view_state.camera_y;
        let zoom = view_state.zoom.clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM);
        let query = self
            .structured_filter_input
            .read(cx)
            .display_text()
            .to_lowercase();
        let index_by_id = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.as_str(), index))
            .collect::<HashMap<_, _>>();
        let selected_id = self
            .graph_selected_item
            .as_ref()
            .map(JsonGraphItemId::as_str);
        let selected_node_index = selected_id.and_then(|id| {
            index_by_id.get(id).copied().or_else(|| {
                graph
                    .nodes
                    .iter()
                    .position(|node| node.fields.iter().any(|field| field.id.as_str() == id))
            })
        });
        let keyboard_nodes = layout
            .visible_order
            .iter()
            .filter_map(|index| {
                let node = graph.nodes.get(*index)?;
                let parent = layout
                    .parent_by_node
                    .get(*index)
                    .and_then(|parent| *parent)
                    .and_then(|parent| graph.nodes.get(parent))
                    .map(|parent| parent.id.clone());
                let first_child = layout
                    .children_by_node
                    .get(*index)
                    .and_then(|children| children.first())
                    .and_then(|child| graph.nodes.get(*child))
                    .map(|child| child.id.clone());
                Some((
                    node.id.clone(),
                    node.source.range.clone(),
                    parent,
                    first_child,
                ))
            })
            .collect::<Vec<_>>();
        let keyboard_selected_position = selected_node_index.and_then(|selected| {
            keyboard_nodes
                .iter()
                .position(|(id, _, _, _)| graph.nodes[selected].id == *id)
        });
        let selected_detail = self.graph_selected_item.as_ref().and_then(|selected| {
            graph.nodes.iter().find_map(|node| {
                if node.id == *selected {
                    return Some((
                        node.json_path.clone(),
                        bounded_node_content(self.document.as_ref(), node),
                        node_edit_target(installed_snapshot, node),
                    ));
                }
                let field = node.fields.iter().find(|field| field.id == *selected)?;
                Some((
                    field.json_path.clone(),
                    bounded_graph_content(
                        self.document.as_ref(),
                        field.source.range.clone(),
                        &field.label,
                    ),
                    field_edit_target(installed_snapshot, field),
                ))
            })
        });
        let selected_edge_color = palette.accent.opacity(0.96);
        let grid_color = palette.grid;
        let graph_bounds = Arc::new(Mutex::new(None));
        let selected_edges = selected_path_edges(&layout, selected_node_index);
        let mut branch_by_index = vec![None; graph.nodes.len()];
        for node in &layout.nodes {
            branch_by_index[node.index] = node.branch;
        }
        let edge_paths = layout
            .edges
            .iter()
            .filter(|edge| {
                edge_intersects_viewport(
                    edge,
                    camera_x,
                    camera_y,
                    zoom,
                    viewport_width,
                    viewport_height,
                )
            })
            .map(|edge| {
                let from = point(
                    px(camera_x + edge.from_x * zoom),
                    px(camera_y + edge.from_y * zoom),
                );
                let to = point(
                    px(camera_x + edge.to_x * zoom),
                    px(camera_y + edge.to_y * zoom),
                );
                let branch = branch_by_index.get(edge.to_index).copied().flatten();
                (
                    from,
                    to,
                    selected_edges.contains(&edge.edge_index),
                    palette.branch(branch, palette.edge),
                )
            })
            .collect::<Vec<_>>();
        let graph_bounds_for_prepaint = graph_bounds.clone();
        let edges = canvas(
            move |bounds, _, _| {
                if let Ok(mut current) = graph_bounds_for_prepaint.lock() {
                    *current = Some(bounds);
                }
            },
            move |bounds, _, window, _| {
                let spacing = (32.0 * zoom).clamp(18.0, 56.0);
                let width = f32::from(bounds.size.width);
                let height = f32::from(bounds.size.height);
                let mut grid = PathBuilder::stroke(px(1.0));
                let mut x = camera_x.rem_euclid(spacing);
                while x <= width {
                    let mut y = camera_y.rem_euclid(spacing);
                    while y <= height {
                        let center = point(bounds.origin.x + px(x), bounds.origin.y + px(y));
                        grid.move_to(point(center.x - px(1.5), center.y));
                        grid.line_to(point(center.x + px(1.5), center.y));
                        grid.move_to(point(center.x, center.y - px(1.5)));
                        grid.line_to(point(center.x, center.y + px(1.5)));
                        y += spacing;
                    }
                    x += spacing;
                }
                if let Ok(path) = grid.build() {
                    window.paint_path(path, grid_color);
                }
                for (from, to, selected, branch_color) in edge_paths {
                    let from = point(bounds.origin.x + from.x, bounds.origin.y + from.y);
                    let to = point(bounds.origin.x + to.x, bounds.origin.y + to.y);
                    let control = ((f32::from(to.x - from.x) * 0.5).max(24.0)) as f32;
                    let mut builder = PathBuilder::stroke(px(if selected { 1.8 } else { 1.1 }));
                    builder.move_to(from);
                    builder.cubic_bezier_to(
                        to,
                        point(from.x + px(control), from.y),
                        point(to.x - px(control), to.y),
                    );
                    if let Ok(path) = builder.build() {
                        window.paint_path(
                            path,
                            if selected {
                                selected_edge_color
                            } else if selected_node_index.is_some() {
                                branch_color.opacity(0.28)
                            } else {
                                branch_color.opacity(0.62)
                            },
                        );
                    }
                }
            },
        )
        .absolute()
        .size_full();

        let mut outgoing_by_parent = HashMap::<&str, Vec<&JsonGraphEdge>>::new();
        for edge in graph.edges.iter() {
            outgoing_by_parent
                .entry(edge.from.as_str())
                .or_default()
                .push(edge);
        }
        let node_elements = layout
            .nodes
            .iter()
            .filter(|position| {
                node_intersects_viewport(
                    position,
                    camera_x,
                    camera_y,
                    zoom,
                    viewport_width,
                    viewport_height,
                )
            })
            .map(|position| {
                let node = &graph.nodes[position.index];
                let id = node.id.clone();
                let source = node.source.range.clone();
                let node_kind = node.kind;
                let node_label = node.label.clone();
                let node_edit_range = node.source.range.clone();
                let collapsible = node.child_count > 0;
                let collapsed = collapsed.contains(node.id.as_str());
                let selected = selected_id == Some(node.id.as_str());
                let matches_query =
                    !query.is_empty() && json_graph_node_matches_query(node, &query);
                let branch_color = palette.branch(position.branch, colors.dialog_border);
                let left = camera_x + position.x * zoom;
                let top = camera_y + position.y * zoom;
                let width = position.width * zoom;
                let header_height = GRAPH_CARD_HEADER_HEIGHT * zoom;
                let row_height = GRAPH_CARD_ROW_HEIGHT * zoom;
                let toggle_id = id.clone();
                let context_id = id.clone();
                let context_bounds = graph_bounds.clone();
                let toggle_anchor = point(
                    px(left + width * 0.5),
                    px(top + position.height * zoom * 0.5),
                );
                let all_rows = graph_card_rows(
                    node,
                    outgoing_by_parent
                        .get(node.id.as_str())
                        .into_iter()
                        .flatten()
                        .copied(),
                );
                let visible_limit = row_limit(&node.id, &self.graph_row_limits);
                let hidden_rows = all_rows.len().saturating_sub(visible_limit);
                let mut row_elements = all_rows
                    .into_iter()
                    .take(visible_limit)
                    .map(|row| match row {
                        GraphCardRow::Field(field) => {
                            let edit_target = field_edit_target(installed_snapshot, field);
                            let field_id = field.id.clone();
                            let field_source = field.source.range.clone();
                            let row_selected = selected_id == Some(field.id.as_str());
                            let field_value_color = palette.value(field.kind);
                            let field_label: SharedString = field.label.to_string().into();
                            let field_value: SharedString = field.display_value.to_string().into();
                            div()
                        .id(SharedString::from(format!(
                            "json-graph-field-element-{}",
                            field.id.as_str()
                        )))
                        .debug_selector({
                            let id = field.id.as_str().to_owned();
                            move || format!("json-graph-field-{id}")
                        })
                        .relative()
                        .h(px(row_height))
                        .px(px(10.0 * zoom))
                        .flex()
                        .items_center()
                        .gap(px(6.0 * zoom))
                        .border_t(px(1.0))
                        .border_color(colors.dialog_border.opacity(0.58))
                        .bg(if row_selected {
                            palette.accent.opacity(0.11)
                        } else {
                            palette.surface
                        })
                        .text_size(px((11.0 * zoom).clamp(8.5, 16.0)))
                        .cursor_pointer()
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "json-graph-field-label-{}",
                                    field.id.as_str()
                                )))
                                .max_w(relative(0.46))
                                .overflow_hidden()
                                .truncate()
                                .text_color(palette.text)
                                .tooltip({
                                    let text = field_label.clone();
                                    move |_window, cx| crate::ui::ui_tooltip(text.clone(), cx)
                                })
                                .child(field_label),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "json-graph-field-value-{}",
                                    field.id.as_str()
                                )))
                                .min_w(px(0.0))
                                .flex_1()
                                .overflow_hidden()
                                .truncate()
                                .text_color(field_value_color)
                                .tooltip({
                                    let text = field_value.clone();
                                    move |_window, cx| crate::ui::ui_tooltip(text.clone(), cx)
                                })
                                .child(field_value),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "json-graph-field-hit-{}",
                                    field.id.as_str()
                                )))
                                .debug_selector({
                                    let id = field.id.as_str().to_owned();
                                    move || format!("json-graph-field-hit-{id}")
                                })
                                .absolute()
                                .top_0()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &gpui::MouseDownEvent, window, cx| {
                                            cx.stop_propagation();
                                            this.select_json_graph_item(
                                                field_id.clone(),
                                                field_source.clone(),
                                                window,
                                                cx,
                                            );
                                            if event.click_count >= 2 {
                                                this.begin_json_graph_edit(
                                                    edit_target.clone(),
                                                    window,
                                                    cx,
                                                );
                                            }
                                        },
                                    ),
                                ),
                        )
                        }
                        GraphCardRow::Child(edge) => {
                            let child = index_by_id
                                .get(edge.to.as_str())
                                .and_then(|index| graph.nodes.get(*index));
                            let child_summary = child
                                .map(|child| {
                                    let marker = match child.kind {
                                        JsonValueKind::Array => "[…]",
                                        JsonValueKind::Object => "{…}",
                                        _ => "→",
                                    };
                                    format!("{marker} · {}", child.fields.len() + child.child_count)
                                })
                                .unwrap_or_else(|| "→".to_owned());
                            let child_id = edge.to.clone();
                            let child_source = edge.source.range.clone();
                            let edit_target =
                                child.map(|child| node_edit_target(installed_snapshot, child));
                            let row_selected = selected_id == Some(edge.to.as_str());
                            let child_branch = child
                                .and_then(|child| index_by_id.get(child.id.as_str()))
                                .and_then(|index| branch_by_index.get(*index).copied().flatten());
                            let child_color = palette.branch(child_branch, branch_color);
                            let child_label: SharedString = edge.label.to_string().into();
                            let row_selector =
                                format!("json-graph-child-row-{}", edge.parent_port.as_str());
                            let port_selector =
                                format!("json-graph-port-{}", edge.parent_port.as_str());
                            div()
                        .id(SharedString::from(row_selector.clone()))
                        .debug_selector(move || row_selector.clone())
                        .relative()
                        .h(px(row_height))
                        .pl(px(10.0 * zoom))
                        .pr(px(14.0 * zoom))
                        .flex()
                        .items_center()
                        .gap(px(6.0 * zoom))
                        .border_t(px(1.0))
                        .border_color(colors.dialog_border.opacity(0.58))
                        .bg(if row_selected {
                            palette.accent.opacity(0.11)
                        } else {
                            palette.surface
                        })
                        .text_size(px((11.0 * zoom).clamp(8.5, 16.0)))
                        .cursor_pointer()
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "json-graph-child-label-{}",
                                    edge.parent_port.as_str()
                                )))
                                .min_w(px(0.0))
                                .flex_1()
                                .overflow_hidden()
                                .truncate()
                                .text_color(palette.text)
                                .tooltip({
                                    let text = child_label.clone();
                                    move |_window, cx| crate::ui::ui_tooltip(text.clone(), cx)
                                })
                                .child(child_label),
                        )
                        .child(div().text_color(colors.dialog_muted).child(child_summary))
                        .child(
                            div()
                                .id(SharedString::from(port_selector.clone()))
                                .debug_selector(move || port_selector.clone())
                                .absolute()
                                .right(px(-5.0 * zoom))
                                .size(px((10.0 * zoom).max(7.0)))
                                .rounded_full()
                                .border(px(1.0))
                                .border_color(child_color.opacity(0.72))
                                .bg(child_color.opacity(0.2)),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "json-graph-child-hit-{}",
                                    edge.parent_port.as_str()
                                )))
                                .absolute()
                                .top_0()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &gpui::MouseDownEvent, window, cx| {
                                            cx.stop_propagation();
                                            this.select_json_graph_item(
                                                child_id.clone(),
                                                child_source.clone(),
                                                window,
                                                cx,
                                            );
                                            if event.click_count >= 2
                                                && let Some(target) = edit_target.clone()
                                            {
                                                this.begin_json_graph_edit(target, window, cx);
                                            }
                                        },
                                    ),
                                ),
                        )
                        }
                    })
                    .collect::<Vec<_>>();
                if hidden_rows > 0 {
                    let reveal_id = id.clone();
                    let reveal_anchor = toggle_anchor;
                    let reveal_count = hidden_rows.min(ROW_LIMIT_STEP);
                    row_elements.push(
                        div()
                            .id(SharedString::from(format!(
                                "json-graph-show-more-{}",
                                node.id.as_str()
                            )))
                            .debug_selector({
                                let id = node.id.as_str().to_owned();
                                move || format!("json-graph-show-more-{id}")
                            })
                            .h(px(row_height))
                            .px(px(10.0 * zoom))
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_t(px(1.0))
                            .border_color(colors.dialog_border.opacity(0.58))
                            .bg(palette.surface)
                            .text_size(px((10.5 * zoom).clamp(8.5, 15.0)))
                            .text_color(colors.text_link)
                            .cursor_pointer()
                            .hover(|row| row.bg(colors.dialog_secondary_button_hover))
                            .child(
                                strings
                                    .json_graph_show_more_template
                                    .replace("{count}", &reveal_count.to_string()),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.graph_recenter_anchor =
                                    Some((reveal_id.clone(), reveal_anchor));
                                let limit = this
                                    .graph_row_limits
                                    .entry(reveal_id.clone())
                                    .or_insert(model::DEFAULT_ROW_LIMIT);
                                *limit = limit.saturating_add(ROW_LIMIT_STEP);
                                this.graph_layout_cache = None;
                                cx.notify();
                            })),
                    );
                }
                div()
                    .id(SharedString::from(format!(
                        "json-graph-node-{}",
                        node.id.as_str()
                    )))
                    .debug_selector({
                        let id = node.id.as_str().to_owned();
                        move || format!("json-graph-node-{id}")
                    })
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(width))
                    .rounded(px(10.0 * zoom.max(0.75)))
                    .border(px(if selected || matches_query { 2.0 } else { 1.0 }))
                    .border_color(if selected {
                        palette.accent
                    } else if matches_query {
                        palette.search
                    } else {
                        branch_color.opacity(0.52)
                    })
                    .bg(palette.surface)
                    .when(selected, |card| card.shadow_md())
                    .cursor_pointer()
                    .child(
                        div()
                            .h(px(header_height))
                            .px(px(10.0 * zoom))
                            .flex()
                            .items_center()
                            .justify_between()
                            .rounded_t(px(9.0 * zoom.max(0.75)))
                            .bg(if matches_query && !selected {
                                palette.search.opacity(0.13)
                            } else {
                                branch_color.opacity(0.18)
                            })
                            .text_size(px((12.0 * zoom).clamp(9.0, 18.0)))
                            .text_color(colors.text_default)
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "json-graph-node-label-{}",
                                        node.id.as_str()
                                    )))
                                    .min_w(px(0.0))
                                    .truncate()
                                    .tooltip({
                                        let text: SharedString = node.label.to_string().into();
                                        move |_window, cx| crate::ui::ui_tooltip(text.clone(), cx)
                                    })
                                    .child(node.label.to_string()),
                            )
                            .children(collapsible.then(|| {
                                div()
                                    .id(SharedString::from(format!(
                                        "json-graph-collapse-{}",
                                        node.id.as_str()
                                    )))
                                    .size(px((20.0 * zoom).max(16.0)))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(4.0))
                                    .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                                    .child(if collapsed { "+" } else { "−" })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.graph_recenter_anchor =
                                            Some((toggle_id.clone(), toggle_anchor));
                                        let state = document_view_state_mut(
                                            &mut this.document,
                                            &mut this.tab_view_state,
                                        )
                                        .derived
                                        .entry(DocumentViewId::json_graph())
                                        .or_default();
                                        if let Some(index) = state
                                            .collapsed_items
                                            .iter()
                                            .position(|item| item.as_ref() == toggle_id.as_str())
                                        {
                                            state.collapsed_items.remove(index);
                                        } else {
                                            state
                                                .collapsed_items
                                                .push(Arc::from(toggle_id.as_str()));
                                        }
                                        cx.notify();
                                    }))
                            })),
                    )
                    .children(row_elements)
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                            let origin = context_bounds
                                .lock()
                                .ok()
                                .and_then(|bounds| *bounds)
                                .map(|bounds| bounds.origin)
                                .unwrap_or_default();
                            this.graph_context_menu = Some(JsonGraphContextMenu {
                                node: context_id.clone(),
                                position: point(
                                    event.position.x - origin.x,
                                    event.position.y - origin.y,
                                ),
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.graph_context_menu = None;
                            this.select_json_graph_item(id.clone(), source.clone(), window, cx);
                            if event.click_count() >= 2 {
                                this.begin_json_graph_edit(
                                    JsonGraphEditTarget {
                                        item_id: id.clone(),
                                        range: node_edit_range.clone(),
                                        document_epoch: projection_epoch,
                                        base_revision: projection_revision,
                                        label: node_label.clone(),
                                        kind: node_kind,
                                    },
                                    window,
                                    cx,
                                );
                            }
                        }),
                    )
            });

        let control_button = |id: &'static str,
                              icon: &'static str,
                              glyph_size: f32,
                              glyph_offset_x: f32,
                              glyph_offset_y: f32,
                              tooltip: SharedString| {
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .size(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .bg(colors.dialog_surface)
                .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                .cursor_pointer()
                .occlude()
                .tooltip(move |_window, cx| crate::ui::ui_tooltip(tooltip.clone(), cx))
                .child(
                    svg()
                        .path(icon)
                        .size(px(glyph_size))
                        .relative()
                        .left(px(glyph_offset_x))
                        .top(px(glyph_offset_y))
                        .text_color(colors.dialog_body),
                )
        };
        let zoom_out = control_button(
            "json-graph-zoom-out",
            "icon/ui/minus.svg",
            14.0,
            0.0,
            0.0,
            strings.json_graph_zoom_out.clone().into(),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
                .derived
                .entry(DocumentViewId::json_graph())
                .or_default();
            state.zoom = (state.zoom - 0.1).clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM);
            cx.notify();
        }));
        let zoom_in = control_button(
            "json-graph-zoom-in",
            "icon/ui/plus.svg",
            14.0,
            0.0,
            0.0,
            strings.json_graph_zoom_in.clone().into(),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
                .derived
                .entry(DocumentViewId::json_graph())
                .or_default();
            state.zoom = (state.zoom + 0.1).clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM);
            cx.notify();
        }));
        let actual_size = div()
            .id("json-graph-actual-size")
            .debug_selector(|| "json-graph-actual-size".to_owned())
            .h(px(28.0))
            .min_w(px(48.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .cursor_pointer()
            .text_size(px(11.0))
            .text_color(colors.dialog_body)
            .hover(|button| button.bg(colors.dialog_secondary_button_hover))
            .tooltip(|_window, cx| crate::ui::ui_tooltip("实际大小（100%）", cx))
            .child(format!("{}%", (zoom * 100.0).round() as i32))
            .on_click(cx.listener(move |this, _, _, cx| {
                let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
                    .derived
                    .entry(DocumentViewId::json_graph())
                    .or_default();
                let world_x =
                    (viewport_width * 0.5 - state.camera_x) / state.zoom.max(f32::EPSILON);
                let world_y =
                    (viewport_height * 0.5 - state.camera_y) / state.zoom.max(f32::EPSILON);
                state.zoom = 1.0;
                state.camera_x = viewport_width * 0.5 - world_x;
                state.camera_y = viewport_height * 0.5 - world_y;
                cx.notify();
            }));
        let fit_layout = layout.clone();
        let fit_bounds = graph_bounds.clone();
        // refresh.svg 的右侧弧线靠近 viewBox 边缘；缩小后居中绘制，避免高 DPI 下被裁剪。
        let fit = control_button(
            "json-graph-fit",
            "icon/ui/refresh.svg",
            12.0,
            0.0,
            0.0,
            strings.json_graph_fit.clone().into(),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            let (actual_width, actual_height) = fit_bounds
                .lock()
                .ok()
                .and_then(|bounds| *bounds)
                .map(|bounds| (f32::from(bounds.size.width), f32::from(bounds.size.height)))
                .unwrap_or((viewport_width, viewport_height));
            let (x, y, zoom) = fit_camera(&fit_layout, actual_width, actual_height, GRAPH_MIN_ZOOM);
            let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
                .derived
                .entry(DocumentViewId::json_graph())
                .or_default();
            state.camera_x = x;
            state.camera_y = y;
            state.zoom = zoom;
            cx.notify();
        }));
        let search = div()
            .id("json-graph-search")
            .debug_selector(|| "json-graph-search".to_owned())
            .flex_1()
            .min_w(px(112.0))
            .max_w(px(210.0))
            .h(px(28.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(colors.dialog_border)
            .bg(colors.dialog_surface)
            .child(
                svg()
                    .path("icon/ui/search.svg")
                    .size(px(13.0))
                    .text_color(colors.dialog_muted),
            )
            .child(self.structured_filter_input.clone());
        let search_count = (!query.is_empty()).then(|| {
            div()
                .id("json-graph-search-count")
                .debug_selector(|| "json-graph-search-count".to_owned())
                .min_w(px(42.0))
                .text_size(px(11.0))
                .text_color(colors.dialog_muted)
                .child(if self.graph_search_matches.is_empty() {
                    "0 / 0".to_owned()
                } else {
                    format!(
                        "{} / {}",
                        self.graph_search_selected + 1,
                        self.graph_search_matches.len()
                    )
                })
        });
        let search_previous = (!query.is_empty()).then(|| {
            control_button(
                "json-graph-search-previous",
                "icon/ui/chevron-up.svg",
                14.0,
                0.0,
                0.0,
                strings.json_graph_search_previous.clone().into(),
            )
            .on_click(cx.listener(|this, _, _, cx| this.navigate_json_graph_search(-1, cx)))
        });
        let search_next = (!query.is_empty()).then(|| {
            control_button(
                "json-graph-search-next",
                "icon/ui/chevron-down.svg",
                14.0,
                0.0,
                0.0,
                strings.json_graph_search_next.clone().into(),
            )
            .on_click(cx.listener(|this, _, _, cx| this.navigate_json_graph_search(1, cx)))
        });
        let selected_root = self.graph_selected_item.as_ref().and_then(|selected| {
            graph
                .nodes
                .iter()
                .find(|node| {
                    node.id == *selected
                        && matches!(node.kind, JsonValueKind::Object | JsonValueKind::Array)
                })
                .map(|node| {
                    JsonGraphRoot::new(
                        node.source.clone(),
                        node.json_path.clone(),
                        node.label.clone(),
                    )
                })
        });
        let focus_subtree = selected_root.map(|root| {
            div()
                .id("json-graph-focus-subtree")
                .debug_selector(|| "json-graph-focus-subtree".to_owned())
                .h(px(28.0))
                .px(px(9.0))
                .flex()
                .items_center()
                .rounded(px(6.0))
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .bg(colors.dialog_surface)
                .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                .cursor_pointer()
                .text_size(px(11.0))
                .text_color(colors.dialog_body)
                .child(strings.json_graph_focus_subtree.clone())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.derived_projection_root = Some(root.clone());
                    this.graph_selected_item = None;
                    this.graph_state_initialized = false;
                    this.graph_needs_fit = true;
                    this.derived_projection_stale = this.derived_projection_snapshot.is_some();
                    this.request_registered_projection(cx);
                }))
        });
        let reset_root = self.derived_projection_root.is_some().then(|| {
            div()
                .id("json-graph-reset-root")
                .debug_selector(|| "json-graph-reset-root".to_owned())
                .h(px(28.0))
                .px(px(9.0))
                .flex()
                .items_center()
                .rounded(px(6.0))
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .bg(colors.dialog_surface)
                .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                .cursor_pointer()
                .text_size(px(11.0))
                .text_color(colors.dialog_body)
                .child(strings.json_graph_reset_root.clone())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.derived_projection_root = None;
                    this.graph_selected_item = None;
                    this.graph_state_initialized = false;
                    this.graph_needs_fit = true;
                    this.derived_projection_stale = this.derived_projection_snapshot.is_some();
                    this.request_registered_projection(cx);
                }))
        });
        let toolbar = div()
            .absolute()
            .top(px(10.0))
            .left(px(10.0))
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .occlude()
            .child(search)
            .children(search_count)
            .children(search_previous)
            .children(search_next)
            .children(reset_root)
            .children(focus_subtree);
        let zoom_toolbar = div()
            .id("json-graph-zoom-toolbar")
            .debug_selector(|| "json-graph-zoom-toolbar".to_owned())
            .absolute()
            .bottom(px(12.0))
            .left(relative(0.5))
            .ml(px(-77.0))
            .h(px(36.0))
            .px(px(4.0))
            .flex()
            .items_center()
            .gap(px(3.0))
            .rounded(px(9.0))
            .border(px(1.0))
            .border_color(colors.dialog_border)
            .bg(colors.dialog_surface)
            .shadow_md()
            .occlude()
            .child(zoom_out)
            .child(actual_size)
            .child(zoom_in)
            .child(fit);
        let stale_banner = self.derived_projection_stale.then(|| {
            let detail = self
                .derived_projection_error
                .clone()
                .unwrap_or_else(|| strings.json_graph_source_changed.clone().into());
            div()
                .id("json-graph-stale-banner")
                .debug_selector(|| "json-graph-stale-banner".to_owned())
                .absolute()
                .top(px(50.0))
                .left(px(10.0))
                .right(px(10.0))
                .h(px(34.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(6.0))
                .border(px(1.0))
                .border_color(colors.callout_warning_border)
                .bg(colors.callout_warning_bg)
                .text_size(px(11.0))
                .text_color(colors.text_default)
                .child(strings.json_graph_stale.clone())
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .truncate()
                        .text_color(colors.dialog_muted)
                        .child(detail),
                )
        });
        let truncated_banner = graph.truncated.then(|| {
            div()
                .absolute()
                .bottom(px(56.0))
                .left(px(10.0))
                .px(px(10.0))
                .h(px(30.0))
                .flex()
                .items_center()
                .rounded(px(6.0))
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .bg(colors.dialog_surface)
                .text_size(px(11.0))
                .text_color(colors.dialog_muted)
                .child(strings.json_graph_truncated.clone())
        });
        let detail_panel = selected_detail.map(|(json_path, content, edit_target)| {
            let json_path = jsonpath_for_display(&json_path);
            let copy_path = json_path.clone();
            let wide = viewport_width >= 820.0;
            let panel_width = 360.0_f32.min((viewport_width - 24.0).max(280.0));
            let panel_top = if wide {
                54.0
            } else {
                (viewport_height - viewport_height.min(320.0) - 12.0).max(54.0)
            };
            div()
                .id("json-graph-node-details")
                .debug_selector(|| "json-graph-node-details".to_owned())
                .absolute()
                .top(px(panel_top))
                .when(wide, |panel| panel.right(px(12.0)).w(px(panel_width)))
                .when(!wide, |panel| panel.left(px(12.0)).right(px(12.0)))
                .max_h(px(if wide {
                    (viewport_height - 66.0).max(240.0)
                } else {
                    viewport_height.min(320.0)
                }))
                .p(px(14.0))
                .flex()
                .flex_col()
                .gap(px(10.0))
                .occlude()
                .rounded(px(9.0))
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .bg(colors.dialog_surface)
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation()
                })
                .child(
                    div()
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_size(px(13.0))
                        .text_color(colors.text_default)
                        .child(strings.json_graph_details_title.clone())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .child(
                                    div()
                                        .id("json-graph-node-details-copy-path")
                                        .h(px(26.0))
                                        .px(px(8.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(5.0))
                                        .cursor_pointer()
                                        .text_size(px(11.0))
                                        .text_color(colors.dialog_body)
                                        .hover(|button| {
                                            button.bg(colors.dialog_secondary_button_hover)
                                        })
                                        .child(strings.json_graph_copy_path.clone())
                                        .on_click(cx.listener(move |_, _, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                copy_path.clone(),
                                            ));
                                        })),
                                )
                                .child(
                                    div()
                                        .id("json-graph-node-details-edit")
                                        .h(px(26.0))
                                        .px(px(8.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(5.0))
                                        .cursor_pointer()
                                        .text_size(px(11.0))
                                        .text_color(colors.dialog_body)
                                        .hover(|button| {
                                            button.bg(colors.dialog_secondary_button_hover)
                                        })
                                        .child(strings.json_graph_edit_value.clone())
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.begin_json_graph_edit(
                                                edit_target.clone(),
                                                window,
                                                cx,
                                            );
                                        })),
                                )
                                .child(
                                    div()
                                        .id("json-graph-node-details-close")
                                        .size(px(26.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(5.0))
                                        .cursor_pointer()
                                        .hover(|button| {
                                            button.bg(colors.dialog_secondary_button_hover)
                                        })
                                        .tooltip({
                                            let label: SharedString =
                                                strings.ui_close.clone().into();
                                            move |_window, cx| {
                                                crate::ui::ui_tooltip(label.clone(), cx)
                                            }
                                        })
                                        .child(
                                            svg()
                                                .path(CLOSE_ICON)
                                                .size(px(14.0))
                                                .text_color(colors.dialog_muted),
                                        )
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.dismiss_json_graph_details();
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(5.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(colors.dialog_muted)
                                .child(strings.json_graph_content.clone()),
                        )
                        .child(
                            div()
                                .id("json-graph-node-details-content")
                                .max_h(px(210.0))
                                .p(px(10.0))
                                .overflow_y_scroll()
                                .rounded(px(6.0))
                                .bg(colors.editor_background)
                                .font_family(source_monospace_font_family())
                                .text_size(px(11.0))
                                .text_color(colors.text_default)
                                .child(content),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(5.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(colors.dialog_muted)
                                .child(strings.json_graph_path.clone()),
                        )
                        .child(
                            div()
                                .id("json-graph-node-details-path")
                                .debug_selector(|| "json-graph-node-details-path".to_owned())
                                .p(px(9.0))
                                .overflow_hidden()
                                .truncate()
                                .rounded(px(6.0))
                                .bg(colors.editor_background)
                                .font_family(source_monospace_font_family())
                                .text_size(px(11.0))
                                .text_color(colors.text_link)
                                .child(json_path),
                        ),
                )
        });
        let context_menu = self.graph_context_menu.as_ref().and_then(|menu| {
            let node = graph.nodes.iter().find(|node| node.id == menu.node)?;
            let source = node.source.range.clone();
            let json_path = jsonpath_for_display(&node.json_path);
            let content = bounded_node_content(self.document.as_ref(), node);
            let node_id = node.id.clone();
            let edit_target = node_edit_target(installed_snapshot, node);
            let collapsible = node.child_count > 0;
            let is_collapsed = collapsed.contains(node.id.as_str());
            let focus_root = matches!(node.kind, JsonValueKind::Object | JsonValueKind::Array)
                .then(|| {
                    JsonGraphRoot::new(
                        node.source.clone(),
                        node.json_path.clone(),
                        node.label.clone(),
                    )
                });
            let panel_width = 210.0;
            let panel_height = 30.0 * (4.0 + f32::from(collapsible)) + 16.0;
            let left = f32::from(menu.position.x)
                .clamp(8.0, (viewport_width - panel_width - 8.0).max(8.0));
            let top = f32::from(menu.position.y)
                .clamp(8.0, (viewport_height - panel_height - 8.0).max(8.0));
            let item = |id: &'static str, label: String| {
                div()
                    .id(id)
                    .debug_selector(move || id.to_owned())
                    .h(px(30.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(5.0))
                    .text_size(px(11.0))
                    .text_color(colors.dialog_body)
                    .hover(|item| item.bg(colors.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .child(label)
            };
            Some(
                div()
                    .id("json-graph-context-menu-overlay")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.graph_context_menu = None;
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .id("json-graph-context-menu")
                            .debug_selector(|| "json-graph-context-menu".to_owned())
                            .absolute()
                            .left(px(left))
                            .top(px(top))
                            .w(px(panel_width))
                            .p(px(7.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .rounded(px(8.0))
                            .border(px(1.0))
                            .border_color(colors.dialog_border)
                            .bg(colors.dialog_surface)
                            .shadow_lg()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                            .child(
                                item(
                                    "json-graph-context-edit",
                                    strings.json_graph_edit_value.clone(),
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.begin_json_graph_edit(edit_target.clone(), window, cx);
                                    },
                                )),
                            )
                            .child(
                                item(
                                    "json-graph-context-locate",
                                    strings.json_graph_locate_source.clone(),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        let preserve_split =
                                            this.view_mode == DocumentHostViewMode::Split;
                                        this.graph_context_menu = None;
                                        this.select_json_source_range(
                                            source.clone(),
                                            preserve_split,
                                            cx,
                                        );
                                        if !preserve_split {
                                            cx.emit(DocumentHostEvent::ViewModeChanged(
                                                DocumentHostMode::Source,
                                            ));
                                        }
                                    },
                                )),
                            )
                            .children(focus_root.map(|root| {
                                item(
                                    "json-graph-context-focus",
                                    strings.json_graph_focus_subtree.clone(),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.graph_context_menu = None;
                                        this.derived_projection_root = Some(root.clone());
                                        this.graph_selected_item = None;
                                        this.graph_state_initialized = false;
                                        this.graph_needs_fit = true;
                                        this.derived_projection_stale =
                                            this.derived_projection_snapshot.is_some();
                                        this.request_registered_projection(cx);
                                    },
                                ))
                            }))
                            .children(collapsible.then(|| {
                                item(
                                    "json-graph-context-toggle",
                                    if is_collapsed {
                                        strings.json_graph_expand.clone()
                                    } else {
                                        strings.json_graph_collapse.clone()
                                    },
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.graph_context_menu = None;
                                        let state = document_view_state_mut(
                                            &mut this.document,
                                            &mut this.tab_view_state,
                                        )
                                        .derived
                                        .entry(DocumentViewId::json_graph())
                                        .or_default();
                                        if is_collapsed {
                                            state
                                                .collapsed_items
                                                .retain(|item| item.as_ref() != node_id.as_str());
                                        } else {
                                            state.collapsed_items.push(Arc::from(node_id.as_str()));
                                        }
                                        cx.notify();
                                    },
                                ))
                            }))
                            .child(
                                item(
                                    "json-graph-context-copy-path",
                                    strings.json_graph_copy_path.clone(),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            json_path.clone(),
                                        ));
                                        this.graph_context_menu = None;
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                item(
                                    "json-graph-context-copy-content",
                                    strings.json_graph_copy_content.clone(),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            content.to_string(),
                                        ));
                                        this.graph_context_menu = None;
                                        cx.notify();
                                    },
                                )),
                            ),
                    ),
            )
        });
        let graph_background = div()
            .id("json-graph-background-hit-target")
            .debug_selector(|| "json-graph-background-hit-target".to_owned())
            .absolute()
            .size_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.graph_selected_item.is_some() {
                        cx.stop_propagation();
                        this.dismiss_json_graph_details();
                        cx.notify();
                    }
                }),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                if this.graph_selected_item.is_some() {
                    cx.stop_propagation();
                    this.dismiss_json_graph_details();
                    cx.notify();
                }
            }));

        let graph_bounds_for_scroll = graph_bounds.clone();
        let graph_bounds_for_capture = graph_bounds.clone();
        let split_canvas = self.view_mode == DocumentHostViewMode::Split;
        div()
            .id("json-graph-canvas")
            .debug_selector(|| "json-graph-canvas".to_owned())
            .size_full()
            .relative()
            .overflow_hidden()
            .border(px(if split_canvas { 0.0 } else { 1.0 }))
            .border_color(hsla(0.0, 0.0, 0.0, 0.0))
            .bg(palette.canvas)
            .tab_index(0)
            .track_focus(&self.graph_focus_handle)
            .focus(move |canvas| {
                canvas.border_color(if split_canvas {
                    hsla(0.0, 0.0, 0.0, 0.0)
                } else {
                    colors.text_link
                })
            })
            .capture_any_mouse_down(cx.listener(
                move |this, event: &gpui::MouseDownEvent, _, cx| {
                    if this.graph_selected_item.is_none() {
                        return;
                    }
                    let origin = graph_bounds_for_capture
                        .lock()
                        .ok()
                        .and_then(|bounds| *bounds)
                        .map(|bounds| bounds.origin)
                        .unwrap_or_default();
                    let x = f32::from(event.position.x - origin.x);
                    let y = f32::from(event.position.y - origin.y);
                    let wide = viewport_width >= 820.0;
                    let left = if wide {
                        (viewport_width - 372.0).max(12.0)
                    } else {
                        12.0
                    };
                    let right = viewport_width - 12.0;
                    let top = if wide {
                        54.0
                    } else {
                        (viewport_height - viewport_height.min(320.0) - 12.0).max(54.0)
                    };
                    let bottom = viewport_height - 12.0;
                    if x < left || x > right || y < top || y > bottom {
                        this.dismiss_json_graph_details();
                        cx.notify();
                    }
                },
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.graph_focus_handle.focus(window);
                    this.graph_context_menu = None;
                    this.dismiss_json_graph_details();
                    let state =
                        document_view_state_mut(&mut this.document, &mut this.tab_view_state)
                            .derived
                            .entry(DocumentViewId::json_graph())
                            .or_default();
                    this.graph_pan_session = Some((event.position, state.camera_x, state.camera_y));
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if !event.dragging() {
                    return;
                }
                let Some((origin, camera_x, camera_y)) = this.graph_pan_session else {
                    return;
                };
                let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
                    .derived
                    .entry(DocumentViewId::json_graph())
                    .or_default();
                state.camera_x = camera_x + f32::from(event.position.x - origin.x);
                state.camera_y = camera_y + f32::from(event.position.y - origin.y);
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.graph_pan_session.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                let delta = event.delta.pixel_delta(px(28.0));
                let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
                    .derived
                    .entry(DocumentViewId::json_graph())
                    .or_default();
                if event.modifiers.control || event.modifiers.platform {
                    let old_zoom = state.zoom.clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM);
                    let new_zoom = (old_zoom + (-f32::from(delta.y) / 700.0))
                        .clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM);
                    let origin = graph_bounds_for_scroll
                        .lock()
                        .ok()
                        .and_then(|bounds| *bounds)
                        .map(|bounds| bounds.origin)
                        .unwrap_or_default();
                    let pointer_x = f32::from(event.position.x - origin.x);
                    let pointer_y = f32::from(event.position.y - origin.y);
                    (state.camera_x, state.camera_y) = zoom_camera_around(
                        state.camera_x,
                        state.camera_y,
                        old_zoom,
                        new_zoom,
                        pointer_x,
                        pointer_y,
                    );
                    state.zoom = new_zoom;
                } else {
                    state.camera_x += f32::from(delta.x);
                    state.camera_y += f32::from(delta.y);
                }
                cx.notify();
            }))
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    let key = event.keystroke.key.as_str();
                    if key == "escape" {
                        if this.graph_selected_item.is_some() {
                            this.dismiss_json_graph_details();
                        } else if this.graph_context_menu.take().is_none() {
                            return;
                        }
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }

                    let current = keyboard_selected_position.unwrap_or(0);
                    let mut target = None;
                    match key {
                        "up" if !keyboard_nodes.is_empty() => {
                            target = keyboard_nodes.get(current.saturating_sub(1)).cloned();
                        }
                        "down" if !keyboard_nodes.is_empty() => {
                            target = keyboard_nodes
                                .get((current + 1).min(keyboard_nodes.len() - 1))
                                .cloned();
                        }
                        "left" | "right" | "space" if !keyboard_nodes.is_empty() => {
                            let (id, _, parent, first_child) = &keyboard_nodes[current];
                            let state = document_view_state_mut(
                                &mut this.document,
                                &mut this.tab_view_state,
                            )
                            .derived
                            .entry(DocumentViewId::json_graph())
                            .or_default();
                            let collapsed = state
                                .collapsed_items
                                .iter()
                                .any(|item| item.as_ref() == id.as_str());
                            if key == "left" && first_child.is_some() && !collapsed {
                                state.collapsed_items.push(Arc::from(id.as_str()));
                            } else if key == "left" {
                                target = parent.as_ref().and_then(|parent| {
                                    keyboard_nodes
                                        .iter()
                                        .find(|(id, _, _, _)| id == parent)
                                        .cloned()
                                });
                            } else if key == "right" && collapsed {
                                state
                                    .collapsed_items
                                    .retain(|item| item.as_ref() != id.as_str());
                            } else if key == "right" {
                                target = first_child.as_ref().and_then(|child| {
                                    keyboard_nodes
                                        .iter()
                                        .find(|(id, _, _, _)| id == child)
                                        .cloned()
                                });
                            } else if first_child.is_some() {
                                if collapsed {
                                    state
                                        .collapsed_items
                                        .retain(|item| item.as_ref() != id.as_str());
                                } else {
                                    state.collapsed_items.push(Arc::from(id.as_str()));
                                }
                            }
                        }
                        "enter" if !keyboard_nodes.is_empty() => {
                            // 选中即展示检查器；无内部游标时 Enter 从首节点开始。
                            if keyboard_selected_position.is_none() {
                                target = keyboard_nodes.first().cloned();
                            }
                        }
                        _ => return,
                    }
                    if let Some((id, range, _, _)) = target {
                        this.graph_pending_center = Some(id.clone());
                        this.select_json_graph_item(id, range, window, cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(edges)
            .child(graph_background)
            .children(node_elements)
            .child(toolbar)
            .child(zoom_toolbar)
            .children(stale_banner)
            .children(truncated_banner)
            .children(detail_panel)
            .children(context_menu)
    }

    pub(super) fn select_json_source_range(
        &mut self,
        range: Range<u64>,
        preserve_split: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let len = document.len();
        let start = range.start.min(len);
        let end = range.end.min(len).max(start);
        document.set_selection(start..end, false);
        let line = document
            .line_for_offset(start)
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or_default();
        self.selection_anchor = Some(line);
        self.selected_lines = Some(line..line.saturating_add(1));
        self.anchor_source_window_for_byte(line as u64, start);
        self.scroll_source_line(line, ScrollStrategy::Center);
        if !preserve_split {
            self.view_mode = DocumentHostViewMode::Source;
            self.sync_tab_active_view();
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmark_json_graph::SourceLocator as JsonSourceLocator;
    fn node(id: &str, fields: usize) -> JsonGraphNode {
        JsonGraphNode {
            id: JsonGraphItemId::new(id),
            json_path: Arc::from(id),
            source: JsonSourceLocator::new(0..1),
            kind: JsonValueKind::Object,
            label: Arc::from(id),
            fields: (0..fields)
                .map(|index| JsonGraphField {
                    id: JsonGraphItemId::new(format!("{id}:{index}")),
                    json_path: Arc::from(format!("{id}/{index}")),
                    label: Arc::from(format!("k{index}")),
                    display_value: Arc::from("value"),
                    source: JsonSourceLocator::new(0..1),
                    kind: JsonValueKind::String,
                })
                .collect::<Vec<_>>()
                .into(),
            child_count: 0,
        }
    }

    #[test]
    fn tree_layout_is_deterministic_and_never_overlaps_siblings() {
        let mut root = node("root", 2);
        root.child_count = 2;
        let mut root_fields = root.fields.to_vec();
        root_fields[0].source = JsonSourceLocator::new(10..11);
        root_fields[1].source = JsonSourceLocator::new(40..41);
        root.fields = root_fields.into();
        let graph = JsonGraphProjection {
            nodes: vec![root, node("a", 1), node("b", 5)].into(),
            edges: vec![
                JsonGraphEdge {
                    id: JsonGraphItemId::new("e1"),
                    from: JsonGraphItemId::new("root"),
                    to: JsonGraphItemId::new("a"),
                    parent_port: JsonGraphItemId::new("port:a"),
                    source: JsonSourceLocator::new(20..21),
                    kind: JsonGraphEdgeKind::ObjectMember,
                    label: Arc::from("a"),
                },
                JsonGraphEdge {
                    id: JsonGraphItemId::new("e2"),
                    from: JsonGraphItemId::new("root"),
                    to: JsonGraphItemId::new("b"),
                    parent_port: JsonGraphItemId::new("port:b"),
                    source: JsonSourceLocator::new(50..51),
                    kind: JsonGraphEdgeKind::ObjectMember,
                    label: Arc::from("b"),
                },
            ]
            .into(),
            truncated: false,
        };
        let first = graph_layout(&graph, &HashSet::<Arc<str>>::new(), &HashMap::new());
        let second = graph_layout(&graph, &HashSet::<Arc<str>>::new(), &HashMap::new());
        assert_eq!(first, second);
        let a = first.nodes.iter().find(|node| node.index == 1).unwrap();
        let b = first.nodes.iter().find(|node| node.index == 2).unwrap();
        assert!(a.y + a.height + model::ROW_GAP <= b.y || b.y + b.height + model::ROW_GAP <= a.y);
        assert_eq!(first.edges.len(), 2);
        let root = first.nodes.iter().find(|node| node.index == 0).unwrap();
        let edge = &first.edges[0];
        assert_eq!(edge.from_x, root.x + root.width);
        assert_eq!(
            edge.from_y,
            root.y + GRAPH_CARD_HEADER_HEIGHT + 1.5 * GRAPH_CARD_ROW_HEIGHT
        );
        assert_eq!(edge.to_x, a.x);
        assert_eq!(edge.to_y, a.y + GRAPH_CARD_HEADER_HEIGHT * 0.5);
    }

    #[test]
    fn collapsed_node_removes_descendants_and_fit_clamps_zoom() {
        let mut root = node("root", 0);
        root.child_count = 1;
        let graph = JsonGraphProjection {
            nodes: vec![root, node("child", 0)].into(),
            edges: vec![JsonGraphEdge {
                id: JsonGraphItemId::new("e"),
                from: JsonGraphItemId::new("root"),
                to: JsonGraphItemId::new("child"),
                parent_port: JsonGraphItemId::new("port:child"),
                source: JsonSourceLocator::new(0..1),
                kind: JsonGraphEdgeKind::ObjectMember,
                label: Arc::from("child"),
            }]
            .into(),
            truncated: false,
        };
        let collapsed = HashSet::from([Arc::<str>::from("root")]);
        let layout = graph_layout(&graph, &collapsed, &HashMap::new());
        assert_eq!(layout.nodes.len(), 1);
        assert!(layout.edges.is_empty());
        let (_, _, zoom) = fit_camera(&layout, 320.0, 200.0, GRAPH_MIN_ZOOM);
        assert!((GRAPH_MIN_ZOOM..=1.0).contains(&zoom));
    }

    #[test]
    fn pointer_zoom_keeps_the_world_point_under_the_cursor() {
        let camera = (37.0, -12.0);
        let pointer = (420.0, 180.0);
        let old_zoom = 0.75;
        let new_zoom = 1.4;
        let world_before = (
            (pointer.0 - camera.0) / old_zoom,
            (pointer.1 - camera.1) / old_zoom,
        );
        let (camera_x, camera_y) =
            zoom_camera_around(camera.0, camera.1, old_zoom, new_zoom, pointer.0, pointer.1);
        let world_after = (
            (pointer.0 - camera_x) / new_zoom,
            (pointer.1 - camera_y) / new_zoom,
        );
        assert!((world_before.0 - world_after.0).abs() < 0.001);
        assert!((world_before.1 - world_after.1).abs() < 0.001);
    }

    #[test]
    fn internal_graph_paths_are_presented_as_standard_jsonpath() {
        assert_eq!(jsonpath_for_display("$"), "$");
        assert_eq!(
            jsonpath_for_display("$/paths#3/~1v1~1planning~1route#2/post#0"),
            "$.paths['/v1/planning/route'].post"
        );
        assert_eq!(
            jsonpath_for_display("$/items#0/2/name#1"),
            "$.items[2].name"
        );
        assert_eq!(
            jsonpath_for_display("$/owner~0name#0/it\u{27}s\\fine#1"),
            "$['owner~name']['it\\\u{27}s\\\\fine']"
        );
    }

    #[test]
    fn search_selection_expands_every_collapsed_ancestor() {
        let graph = JsonGraphProjection {
            nodes: vec![node("root", 0), node("child", 0), node("leaf", 0)].into(),
            edges: vec![
                JsonGraphEdge {
                    id: JsonGraphItemId::new("root-child"),
                    from: JsonGraphItemId::new("root"),
                    to: JsonGraphItemId::new("child"),
                    parent_port: JsonGraphItemId::new("port:child"),
                    source: JsonSourceLocator::new(0..1),
                    kind: JsonGraphEdgeKind::ObjectMember,
                    label: Arc::from("child"),
                },
                JsonGraphEdge {
                    id: JsonGraphItemId::new("child-leaf"),
                    from: JsonGraphItemId::new("child"),
                    to: JsonGraphItemId::new("leaf"),
                    parent_port: JsonGraphItemId::new("port:leaf"),
                    source: JsonSourceLocator::new(0..1),
                    kind: JsonGraphEdgeKind::ObjectMember,
                    label: Arc::from("leaf"),
                },
            ]
            .into(),
            truncated: false,
        };
        let mut collapsed = vec![Arc::from("root"), Arc::from("child")];
        expand_ancestors(&graph, &JsonGraphItemId::new("leaf"), &mut collapsed);
        assert!(collapsed.is_empty());
    }

    #[test]
    fn search_reveals_a_hidden_dense_card_row() {
        let graph = JsonGraphProjection {
            nodes: vec![node("root", 37)].into(),
            edges: Arc::from([]),
            truncated: false,
        };
        let selected = JsonGraphItemId::new("root:36");
        let (parent, limit) = search_reveal_row_limit(&graph, &selected).unwrap();
        assert_eq!(parent.as_str(), "root");
        assert_eq!(limit, 60);
    }
}
