// @author kongweiguang

use super::*;
use gpui::{PathBuilder, canvas, point};
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

const GRAPH_CARD_MIN_WIDTH: f32 = 210.0;
const GRAPH_CARD_MAX_WIDTH: f32 = 340.0;
const GRAPH_CARD_HEADER_HEIGHT: f32 = 34.0;
const GRAPH_CARD_ROW_HEIGHT: f32 = 28.0;
const GRAPH_COLUMN_GAP: f32 = 150.0;
const GRAPH_ROW_GAP: f32 = 24.0;
const GRAPH_CANVAS_PADDING: f32 = 72.0;
const GRAPH_MIN_ZOOM: f32 = 0.3;
const GRAPH_MAX_ZOOM: f32 = 2.0;

#[derive(Clone, Debug, PartialEq)]
struct PositionedGraphNode {
    index: usize,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct PositionedGraphEdge {
    from: gpui::Point<gpui::Pixels>,
    to: gpui::Point<gpui::Pixels>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct GraphLayout {
    nodes: Vec<PositionedGraphNode>,
    edges: Vec<PositionedGraphEdge>,
    width: f32,
    height: f32,
}

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

pub(super) fn json_graph_node_matches_query(node: &JsonGraphNode, query: &str) -> bool {
    node.label.to_lowercase().contains(query)
        || node.json_path.to_lowercase().contains(query)
        || node.fields.iter().any(|field| {
            field.json_path.to_lowercase().contains(query)
                || field.label.to_lowercase().contains(query)
                || field.display_value.to_lowercase().contains(query)
        })
}

fn card_size(node: &JsonGraphNode) -> (f32, f32) {
    let widest = std::iter::once(node.label.chars().count())
        .chain(
            node.fields
                .iter()
                .map(|field| field.label.chars().count() + field.display_value.chars().count() + 3),
        )
        .max()
        .unwrap_or(8);
    let width =
        (64.0 + widest.min(42) as f32 * 7.0).clamp(GRAPH_CARD_MIN_WIDTH, GRAPH_CARD_MAX_WIDTH);
    let rows = (node.fields.len() + node.child_count).max(1);
    let height = GRAPH_CARD_HEADER_HEIGHT + rows as f32 * GRAPH_CARD_ROW_HEIGHT;
    (width, height)
}

fn graph_layout<T>(graph: &JsonGraphProjection, collapsed: &HashSet<T>) -> GraphLayout
where
    T: Borrow<str> + Eq + Hash,
{
    if graph.nodes.is_empty() {
        return GraphLayout::default();
    }
    let index_by_id = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut children = vec![Vec::new(); graph.nodes.len()];
    let mut parent = vec![None; graph.nodes.len()];
    for edge in graph.edges.iter() {
        let (Some(&from), Some(&to)) = (
            index_by_id.get(edge.from.as_str()),
            index_by_id.get(edge.to.as_str()),
        ) else {
            continue;
        };
        children[from].push(to);
        parent[to] = Some(from);
    }
    let roots = parent
        .iter()
        .enumerate()
        .filter_map(|(index, parent)| parent.is_none().then_some(index))
        .collect::<Vec<_>>();
    let mut visible = vec![false; graph.nodes.len()];
    let mut depth = vec![0usize; graph.nodes.len()];
    let mut stack = roots
        .iter()
        .rev()
        .map(|root| (*root, false))
        .collect::<Vec<_>>();
    let mut postorder = Vec::new();
    while let Some((index, visited)) = stack.pop() {
        if visited {
            postorder.push(index);
            continue;
        }
        visible[index] = true;
        stack.push((index, true));
        if collapsed.contains(graph.nodes[index].id.as_str()) {
            continue;
        }
        for &child in children[index].iter().rev() {
            depth[child] = depth[index].saturating_add(1);
            stack.push((child, false));
        }
    }
    let sizes = graph.nodes.iter().map(card_size).collect::<Vec<_>>();
    let mut subtree_height = vec![0.0f32; graph.nodes.len()];
    for &index in &postorder {
        let child_height = children[index]
            .iter()
            .filter(|child| visible[**child])
            .map(|child| subtree_height[*child])
            .sum::<f32>();
        let child_count = children[index]
            .iter()
            .filter(|child| visible[**child])
            .count();
        let child_height = child_height + GRAPH_ROW_GAP * child_count.saturating_sub(1) as f32;
        subtree_height[index] = sizes[index].1.max(child_height);
    }
    let max_width_by_depth = depth
        .iter()
        .enumerate()
        .filter(|(index, _)| visible[*index])
        .fold(Vec::<f32>::new(), |mut widths, (index, depth)| {
            if widths.len() <= *depth {
                widths.resize(*depth + 1, 0.0);
            }
            widths[*depth] = widths[*depth].max(sizes[index].0);
            widths
        });
    let mut x_by_depth = Vec::with_capacity(max_width_by_depth.len());
    let mut x = GRAPH_CANVAS_PADDING;
    for width in &max_width_by_depth {
        x_by_depth.push(x);
        x += *width + GRAPH_COLUMN_GAP;
    }
    let mut positions = vec![None; graph.nodes.len()];
    let mut root_top = GRAPH_CANVAS_PADDING;
    let mut queue = roots
        .iter()
        .map(|root| {
            let top = root_top;
            root_top += subtree_height[*root] + GRAPH_ROW_GAP;
            (*root, top)
        })
        .collect::<Vec<_>>();
    while let Some((index, subtree_top)) = queue.pop() {
        if !visible[index] {
            continue;
        }
        let (width, height) = sizes[index];
        let y = subtree_top + (subtree_height[index] - height) * 0.5;
        positions[index] = Some(PositionedGraphNode {
            index,
            x: x_by_depth[depth[index]],
            y,
            width,
            height,
        });
        let mut child_top = subtree_top;
        for &child in children[index].iter().filter(|child| visible[**child]) {
            queue.push((child, child_top));
            child_top += subtree_height[child] + GRAPH_ROW_GAP;
        }
    }
    let nodes = positions.iter().flatten().cloned().collect::<Vec<_>>();
    let edges = graph
        .edges
        .iter()
        .filter_map(|edge| {
            let from = *index_by_id.get(edge.from.as_str())?;
            let to = *index_by_id.get(edge.to.as_str())?;
            let from = positions[from].as_ref()?;
            let to = positions[to].as_ref()?;
            let source_row = graph_card_rows(
                &graph.nodes[from.index],
                graph
                    .edges
                    .iter()
                    .filter(|candidate| candidate.from == edge.from),
            )
            .iter()
            .position(|row| {
                matches!(row, GraphCardRow::Child(candidate) if candidate.parent_port == edge.parent_port)
            })?;
            Some(PositionedGraphEdge {
                from: point(
                    px(from.x + from.width),
                    px(from.y
                        + GRAPH_CARD_HEADER_HEIGHT
                        + (source_row as f32 + 0.5) * GRAPH_CARD_ROW_HEIGHT),
                ),
                to: point(px(to.x), px(to.y + GRAPH_CARD_HEADER_HEIGHT * 0.5)),
            })
        })
        .collect::<Vec<_>>();
    let width = nodes
        .iter()
        .map(|node| node.x + node.width)
        .fold(0.0, f32::max)
        + GRAPH_CANVAS_PADDING;
    let height = nodes
        .iter()
        .map(|node| node.y + node.height)
        .fold(0.0, f32::max)
        + GRAPH_CANVAS_PADDING;
    GraphLayout {
        nodes,
        edges,
        width,
        height,
    }
}

fn fit_camera(layout: &GraphLayout, viewport_width: f32, viewport_height: f32) -> (f32, f32, f32) {
    if layout.width <= 0.0 || layout.height <= 0.0 {
        return (0.0, 0.0, 1.0);
    }
    let zoom = ((viewport_width - 48.0).max(1.0) / layout.width)
        .min((viewport_height - 48.0).max(1.0) / layout.height)
        .clamp(GRAPH_MIN_ZOOM, 1.0);
    let camera_x = ((viewport_width - layout.width * zoom) * 0.5).max(0.0);
    let camera_y = ((viewport_height - layout.height * zoom) * 0.5).max(0.0);
    (camera_x, camera_y, zoom)
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
        document_view_state_mut(&mut self.document, &mut self.pending_view_state)
            .derived
            .entry(DocumentViewId::json_graph())
            .or_default()
            .selected_item = Some(Arc::from(id.as_str()));
        if self.view_mode == DocumentHostViewMode::Split {
            self.select_json_source_range(source, true, cx);
        }
        cx.notify();
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
        let state = document_view_state_mut(&mut self.document, &mut self.pending_view_state)
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

        let view_id = DocumentViewId::json_graph();
        let view_state = document_view_state_mut(&mut self.document, &mut self.pending_view_state)
            .derived
            .entry(view_id.clone())
            .or_default();
        let viewport = (viewport_width.max(1.0), viewport_height.max(1.0));
        if self.graph_last_viewport.is_none_or(|last| {
            (last.0 - viewport.0).abs() > 1.0 || (last.1 - viewport.1).abs() > 1.0
        }) {
            self.graph_last_viewport = Some(viewport);
            self.graph_needs_fit = true;
        }
        if !self.graph_state_initialized {
            // 1,500 项预算已经限制了首屏复杂度；默认完整展开，避免用户误以为深层数据缺失。
            view_state.collapsed_items.clear();
            self.graph_state_initialized = true;
        }
        let collapsed = view_state
            .collapsed_items
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        let layout = graph_layout(graph, &collapsed);
        if self.graph_needs_fit
            || (view_state.camera_x == 0.0 && view_state.camera_y == 0.0 && view_state.zoom == 1.0)
        {
            let (x, y, zoom) = fit_camera(&layout, viewport_width, viewport_height);
            view_state.camera_x = x;
            view_state.camera_y = y;
            view_state.zoom = zoom;
            self.graph_needs_fit = false;
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
        let selected_id = self
            .graph_selected_item
            .as_ref()
            .map(JsonGraphItemId::as_str);
        let selected_source_range = self.graph_selected_item.as_ref().and_then(|selected| {
            graph.nodes.iter().find_map(|node| {
                if node.id == *selected {
                    return Some(node.source.range.clone());
                }
                node.fields
                    .iter()
                    .find(|field| field.id == *selected)
                    .map(|field| field.source.range.clone())
            })
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
        let edge_color = colors.dialog_border.opacity(0.8);
        let grid_color = colors.dialog_border.opacity(0.18);
        let graph_bounds = Arc::new(Mutex::new(None));
        let edge_paths = layout
            .edges
            .iter()
            .map(|edge| {
                let from = point(
                    px(camera_x + f32::from(edge.from.x) * zoom),
                    px(camera_y + f32::from(edge.from.y) * zoom),
                );
                let to = point(
                    px(camera_x + f32::from(edge.to.x) * zoom),
                    px(camera_y + f32::from(edge.to.y) * zoom),
                );
                (from, to)
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
                let mut x = camera_x.rem_euclid(spacing);
                while x <= width {
                    let mut builder = PathBuilder::stroke(px(1.0));
                    builder.move_to(point(bounds.origin.x + px(x), bounds.origin.y));
                    builder.line_to(point(
                        bounds.origin.x + px(x),
                        bounds.origin.y + bounds.size.height,
                    ));
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, grid_color);
                    }
                    x += spacing;
                }
                let mut y = camera_y.rem_euclid(spacing);
                while y <= height {
                    let mut builder = PathBuilder::stroke(px(1.0));
                    builder.move_to(point(bounds.origin.x, bounds.origin.y + px(y)));
                    builder.line_to(point(
                        bounds.origin.x + bounds.size.width,
                        bounds.origin.y + px(y),
                    ));
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, grid_color);
                    }
                    y += spacing;
                }
                for (from, to) in edge_paths {
                    let from = point(bounds.origin.x + from.x, bounds.origin.y + from.y);
                    let to = point(bounds.origin.x + to.x, bounds.origin.y + to.y);
                    let control = ((f32::from(to.x - from.x) * 0.5).max(24.0)) as f32;
                    let mut builder = PathBuilder::stroke(px(1.25));
                    builder.move_to(from);
                    builder.cubic_bezier_to(
                        to,
                        point(from.x + px(control), from.y),
                        point(to.x - px(control), to.y),
                    );
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, edge_color);
                    }
                }
            },
        )
        .absolute()
        .size_full();

        let index_by_id = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.as_str(), index))
            .collect::<HashMap<_, _>>();
        let mut outgoing_by_parent = HashMap::<&str, Vec<&JsonGraphEdge>>::new();
        for edge in graph.edges.iter() {
            outgoing_by_parent
                .entry(edge.from.as_str())
                .or_default()
                .push(edge);
        }
        let node_elements = layout.nodes.iter().map(|position| {
            let node = &graph.nodes[position.index];
            let id = node.id.clone();
            let source = node.source.range.clone();
            let node_kind = node.kind;
            let node_label = node.label.clone();
            let node_edit_range = node.source.range.clone();
            let collapsible = node.child_count > 0;
            let collapsed = collapsed.contains(node.id.as_str());
            let selected = selected_id == Some(node.id.as_str());
            let matches_query = !query.is_empty() && json_graph_node_matches_query(node, &query);
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
            let row_elements = graph_card_rows(
                node,
                outgoing_by_parent
                    .get(node.id.as_str())
                    .into_iter()
                    .flatten()
                    .copied(),
            )
            .into_iter()
            .map(|row| match row {
                GraphCardRow::Field(field) => {
                    let edit_target = field_edit_target(installed_snapshot, field);
                    let field_id = field.id.clone();
                    let field_source = field.source.range.clone();
                    let row_selected = selected_id == Some(field.id.as_str());
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
                        .border_color(colors.dialog_border.opacity(0.7))
                        .bg(if row_selected {
                            colors.dialog_secondary_button_hover
                        } else {
                            colors.dialog_surface
                        })
                        .text_size(px((11.0 * zoom).clamp(8.5, 16.0)))
                        .cursor_pointer()
                        .child(
                            div()
                                .max_w(relative(0.46))
                                .overflow_hidden()
                                .truncate()
                                .text_color(colors.text_link)
                                .child(field.label.to_string()),
                        )
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .overflow_hidden()
                                .truncate()
                                .text_color(colors.dialog_muted)
                                .child(field.display_value.to_string()),
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
                    let row_selector =
                        format!("json-graph-child-row-{}", edge.parent_port.as_str());
                    let port_selector = format!("json-graph-port-{}", edge.parent_port.as_str());
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
                        .border_color(colors.dialog_border.opacity(0.7))
                        .bg(if row_selected {
                            colors.dialog_secondary_button_hover
                        } else {
                            colors.dialog_surface
                        })
                        .text_size(px((11.0 * zoom).clamp(8.5, 16.0)))
                        .cursor_pointer()
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .overflow_hidden()
                                .truncate()
                                .text_color(colors.text_link)
                                .child(edge.label.to_string()),
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
                                .border_color(colors.dialog_border)
                                .bg(colors.dialog_surface),
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
                .rounded(px(8.0 * zoom.max(0.75)))
                .border(px(if selected || matches_query { 2.0 } else { 1.0 }))
                .border_color(if selected || matches_query {
                    colors.text_link
                } else {
                    colors.dialog_border
                })
                .bg(colors.dialog_surface)
                .shadow_sm()
                .cursor_pointer()
                .child(
                    div()
                        .h(px(header_height))
                        .px(px(10.0 * zoom))
                        .flex()
                        .items_center()
                        .justify_between()
                        .bg(colors.dialog_secondary_button_bg)
                        .text_size(px((12.0 * zoom).clamp(9.0, 18.0)))
                        .text_color(colors.text_default)
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
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
                                        &mut this.pending_view_state,
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
                                        state.collapsed_items.push(Arc::from(toggle_id.as_str()));
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

        let control_button =
            |id: &'static str, icon: &'static str, glyph_offset_y: f32, tooltip: SharedString| {
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
                            .size(px(14.0))
                            .relative()
                            .top(px(glyph_offset_y))
                            .text_color(colors.dialog_body),
                    )
            };
        let zoom_out = control_button(
            "json-graph-zoom-out",
            "icon/ui/minus.svg",
            0.0,
            strings.json_graph_zoom_out.clone().into(),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            let state = document_view_state_mut(&mut this.document, &mut this.pending_view_state)
                .derived
                .entry(DocumentViewId::json_graph())
                .or_default();
            state.zoom = (state.zoom - 0.1).clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM);
            cx.notify();
        }));
        let zoom_in = control_button(
            "json-graph-zoom-in",
            "icon/ui/plus.svg",
            0.0,
            strings.json_graph_zoom_in.clone().into(),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            let state = document_view_state_mut(&mut this.document, &mut this.pending_view_state)
                .derived
                .entry(DocumentViewId::json_graph())
                .or_default();
            state.zoom = (state.zoom + 0.1).clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM);
            cx.notify();
        }));
        let fit_layout = layout.clone();
        let fit_bounds = graph_bounds.clone();
        let fit = control_button(
            "json-graph-fit",
            "icon/ui/refresh.svg",
            -1.0,
            strings.json_graph_fit.clone().into(),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            let (actual_width, actual_height) = fit_bounds
                .lock()
                .ok()
                .and_then(|bounds| *bounds)
                .map(|bounds| (f32::from(bounds.size.width), f32::from(bounds.size.height)))
                .unwrap_or((viewport_width, viewport_height));
            let (x, y, zoom) = fit_camera(&fit_layout, actual_width, actual_height);
            let state = document_view_state_mut(&mut this.document, &mut this.pending_view_state)
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
                0.0,
                strings.json_graph_search_previous.clone().into(),
            )
            .on_click(cx.listener(|this, _, _, cx| this.navigate_json_graph_search(-1, cx)))
        });
        let search_next = (!query.is_empty()).then(|| {
            control_button(
                "json-graph-search-next",
                "icon/ui/chevron-down.svg",
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
            .right(px(10.0))
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .justify_between()
            .occlude()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .items_center()
                    .gap(px(5.0))
                    .child(search)
                    .children(search_count)
                    .children(search_previous)
                    .children(search_next),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(5.0))
                    .children(reset_root)
                    .children(focus_subtree)
                    .child(fit)
                    .child(zoom_out)
                    .child(zoom_in),
            );
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
                .bottom(px(10.0))
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
            let panel_width = viewport_width.min(440.0).max(280.0);
            let panel_left = ((viewport_width - panel_width) * 0.5).max(12.0);
            let panel_top = ((viewport_height - 360.0) * 0.42).max(72.0);
            div()
                .id("json-graph-node-details")
                .debug_selector(|| "json-graph-node-details".to_owned())
                .absolute()
                .left(px(panel_left))
                .top(px(panel_top))
                .w(px(panel_width))
                .max_h(px((viewport_height - 96.0).max(240.0)))
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
                                            this.graph_selected_item = None;
                                            if let Some(state) = document_view_state_mut(
                                                &mut this.document,
                                                &mut this.pending_view_state,
                                            )
                                            .derived
                                            .get_mut(&DocumentViewId::json_graph())
                                            {
                                                state.selected_item = None;
                                            }
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
                                .p(px(9.0))
                                .overflow_hidden()
                                .truncate()
                                .rounded(px(6.0))
                                .bg(colors.editor_background)
                                .font_family(source_monospace_font_family())
                                .text_size(px(11.0))
                                .text_color(colors.text_link)
                                .child(json_path.to_string()),
                        ),
                )
        });
        let context_menu = self.graph_context_menu.as_ref().and_then(|menu| {
            let node = graph.nodes.iter().find(|node| node.id == menu.node)?;
            let source = node.source.range.clone();
            let json_path = node.json_path.to_string();
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
                                            &mut this.pending_view_state,
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

        let graph_bounds_for_scroll = graph_bounds.clone();
        let split_canvas = self.view_mode == DocumentHostViewMode::Split;
        div()
            .id("json-graph-canvas")
            .debug_selector(|| "json-graph-canvas".to_owned())
            .size_full()
            .relative()
            .overflow_hidden()
            .border(px(if split_canvas { 0.0 } else { 1.0 }))
            .border_color(hsla(0.0, 0.0, 0.0, 0.0))
            .bg(colors.editor_background)
            .tab_index(0)
            .track_focus(&self.graph_focus_handle)
            .focus(move |canvas| {
                canvas.border_color(if split_canvas {
                    hsla(0.0, 0.0, 0.0, 0.0)
                } else {
                    colors.text_link
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.graph_focus_handle.focus(window);
                    this.graph_context_menu = None;
                    let state =
                        document_view_state_mut(&mut this.document, &mut this.pending_view_state)
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
                let state =
                    document_view_state_mut(&mut this.document, &mut this.pending_view_state)
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
                let state =
                    document_view_state_mut(&mut this.document, &mut this.pending_view_state)
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
            .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key != "enter" {
                    return;
                }
                let Some(range) = selected_source_range.clone() else {
                    return;
                };
                cx.stop_propagation();
                let preserve_split = this.view_mode == DocumentHostViewMode::Split;
                this.select_json_source_range(range, preserve_split, cx);
                if !preserve_split {
                    cx.emit(DocumentHostEvent::ViewModeChanged(DocumentHostMode::Source));
                }
            }))
            .child(edges)
            .children(node_elements)
            .child(toolbar)
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
            self.sync_session_active_view();
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
        let first = graph_layout(&graph, &HashSet::<&str>::new());
        let second = graph_layout(&graph, &HashSet::<&str>::new());
        assert_eq!(first, second);
        let a = first.nodes.iter().find(|node| node.index == 1).unwrap();
        let b = first.nodes.iter().find(|node| node.index == 2).unwrap();
        assert!(a.y + a.height + GRAPH_ROW_GAP <= b.y || b.y + b.height + GRAPH_ROW_GAP <= a.y);
        assert_eq!(first.edges.len(), 2);
        let root = first.nodes.iter().find(|node| node.index == 0).unwrap();
        let edge = &first.edges[0];
        assert_eq!(f32::from(edge.from.x), root.x + root.width);
        assert_eq!(
            f32::from(edge.from.y),
            root.y + GRAPH_CARD_HEADER_HEIGHT + 1.5 * GRAPH_CARD_ROW_HEIGHT
        );
        assert_eq!(f32::from(edge.to.x), a.x);
        assert_eq!(f32::from(edge.to.y), a.y + GRAPH_CARD_HEADER_HEIGHT * 0.5);
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
        let collapsed = HashSet::from(["root"]);
        let layout = graph_layout(&graph, &collapsed);
        assert_eq!(layout.nodes.len(), 1);
        assert!(layout.edges.is_empty());
        let (_, _, zoom) = fit_camera(&layout, 320.0, 200.0);
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
}
