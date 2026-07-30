// @author kongweiguang

//! Pure JSON graph view helpers shared by rendering, selection, and editing.

use super::model::ROW_LIMIT_STEP;
use super::*;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub(super) enum GraphCardRow<'a> {
    Field(&'a JsonGraphField),
    Child(&'a JsonGraphEdge),
}

impl GraphCardRow<'_> {
    pub(super) fn source_start(self) -> u64 {
        match self {
            Self::Field(field) => field.source.range.start,
            Self::Child(edge) => edge.source.range.start,
        }
    }
}

pub(super) fn graph_card_rows<'a>(
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

pub(super) fn search_reveal_row_limit(
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

pub(in crate::document_host::implementation) fn json_graph_node_matches_query(
    node: &JsonGraphNode,
    query: &str,
) -> bool {
    node.label.to_lowercase().contains(query)
        || node.json_path.to_lowercase().contains(query)
        || node.fields.iter().any(|field| {
            field.json_path.to_lowercase().contains(query)
                || field.label.to_lowercase().contains(query)
                || field.display_value.to_lowercase().contains(query)
        })
}

pub(super) fn zoom_camera_around(
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

pub(super) fn expand_ancestors(
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

pub(super) fn bounded_node_content(
    document: Option<&DocumentSession>,
    node: &JsonGraphNode,
) -> SharedString {
    bounded_graph_content(document, node.source.range.clone(), &node.label)
}

pub(super) fn bounded_graph_content(
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
pub(super) fn jsonpath_for_display(internal_path: &str) -> String {
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

pub(super) fn node_edit_target_for_identity(
    document_epoch: u64,
    base_revision: u64,
    node: &JsonGraphNode,
) -> JsonGraphEditTarget {
    JsonGraphEditTarget {
        item_id: node.id.clone(),
        range: node.source.range.clone(),
        document_epoch,
        base_revision,
        label: node.label.clone(),
        kind: node.kind,
    }
}

pub(super) fn field_edit_target_for_identity(
    document_epoch: u64,
    base_revision: u64,
    field: &JsonGraphField,
) -> JsonGraphEditTarget {
    JsonGraphEditTarget {
        item_id: field.id.clone(),
        range: field.source.range.clone(),
        document_epoch,
        base_revision,
        label: field.label.clone(),
        kind: field.kind,
    }
}

pub(super) fn node_edit_target(
    snapshot: &JsonGraphSnapshot,
    node: &JsonGraphNode,
) -> JsonGraphEditTarget {
    node_edit_target_for_identity(snapshot.document_epoch(), snapshot.revision(), node)
}

pub(super) fn field_edit_target(
    snapshot: &JsonGraphSnapshot,
    field: &JsonGraphField,
) -> JsonGraphEditTarget {
    field_edit_target_for_identity(snapshot.document_epoch(), snapshot.revision(), field)
}
