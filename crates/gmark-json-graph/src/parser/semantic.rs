// @author kongweiguang

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::lexer::{escape_pointer_segment, token_offset};
use super::model::{
    CandidateKey, ContainerKind, ContainerState, Frame, NodeBuild, ParentContext, ProjectedItem,
    Token,
};
use super::*;
use crate::{
    JsonGraphEdge, JsonGraphEdgeKind, JsonGraphError, JsonGraphField, JsonGraphItemId,
    JsonGraphNode, JsonGraphProjection, JsonValueKind, SourceLocator,
};

impl<'a> GraphParser<'a> {
    pub(super) fn new(
        document: &'a dyn DocumentSnapshot,
        range: std::ops::Range<u64>,
        item_limit: usize,
        cancellation: &'a dyn CancellationSignal,
        root_path: String,
        root_label: String,
    ) -> Result<Self, JsonGraphError> {
        Ok(Self {
            cursor: SnapshotCursor::new(document, range, cancellation),
            item_limit,
            next_sequence: 0,
            truncated: false,
            items: BTreeMap::new(),
            item_keys: HashMap::new(),
            cancellation,
            root_path,
            root_label,
        })
    }

    pub(super) fn parse(mut self) -> Result<JsonGraphProjection, JsonGraphError> {
        let first = self.next_token()?;
        let mut frames = Vec::new();
        let mut root_complete = false;
        let root_path = self.root_path.clone();
        let root_label = self.root_label.clone();
        self.consume_value(first, None, root_path, root_label, &mut frames)?;
        if frames.is_empty() {
            root_complete = true;
        }

        while !root_complete {
            if self.cancellation.is_cancelled() {
                return Err(JsonGraphError::Cancelled);
            }
            let token = self.next_token()?;
            let Some(frame) = frames.last_mut() else {
                return Err(
                    self.invalid(self.cursor.position(), "unexpected token after root value")
                );
            };
            match (&mut frame.state, token) {
                (ContainerState::ObjectKeyOrEnd { allow_end }, Token::ObjectEnd(end))
                    if *allow_end =>
                {
                    self.finish_frame(&mut frames, end)?;
                }
                (ContainerState::ObjectKeyOrEnd { .. }, Token::String(key)) => {
                    frame.pending_key = Some(key);
                    frame.state = ContainerState::ObjectColon;
                }
                (ContainerState::ObjectColon, Token::Colon(_)) => {
                    frame.state = ContainerState::ObjectValue;
                }
                (ContainerState::ObjectValue, value) => {
                    let Some(key) = frame.pending_key.take() else {
                        return Err(JsonGraphError::InvalidJson {
                            offset: token_offset(&value),
                            message: "object value has no key".to_owned(),
                        });
                    };
                    let ordinal = frame.next_ordinal;
                    let label = key.display.clone();
                    let child_path = format!(
                        "{}/{}#{}",
                        frame.path,
                        escape_pointer_segment(&key.display),
                        ordinal
                    );
                    let parent = ParentContext {
                        id: frame.node_id.clone(),
                        depth: frame.depth,
                        kind: frame.kind,
                    };
                    frame.next_ordinal += 1;
                    frame.state = ContainerState::ObjectCommaOrEnd;
                    self.consume_value(value, Some(parent), child_path, label, &mut frames)?;
                }
                (ContainerState::ObjectCommaOrEnd, Token::Comma(_)) => {
                    frame.state = ContainerState::ObjectKeyOrEnd { allow_end: false };
                }
                (ContainerState::ObjectCommaOrEnd, Token::ObjectEnd(end)) => {
                    self.finish_frame(&mut frames, end)?;
                }
                (ContainerState::ArrayValueOrEnd { allow_end }, Token::ArrayEnd(end))
                    if *allow_end =>
                {
                    self.finish_frame(&mut frames, end)?;
                }
                (ContainerState::ArrayValueOrEnd { .. }, value) => {
                    let ordinal = frame.next_ordinal;
                    let parent = ParentContext {
                        id: frame.node_id.clone(),
                        depth: frame.depth,
                        kind: frame.kind,
                    };
                    let child_path = format!("{}/{}", frame.path, ordinal);
                    frame.next_ordinal += 1;
                    frame.state = ContainerState::ArrayCommaOrEnd;
                    self.consume_value(
                        value,
                        Some(parent),
                        child_path,
                        format!("[{ordinal}]"),
                        &mut frames,
                    )?;
                }
                (ContainerState::ArrayCommaOrEnd, Token::Comma(_)) => {
                    frame.state = ContainerState::ArrayValueOrEnd { allow_end: false };
                }
                (ContainerState::ArrayCommaOrEnd, Token::ArrayEnd(end)) => {
                    self.finish_frame(&mut frames, end)?;
                }
                (_, Token::Eof(offset)) => {
                    return Err(self.invalid(offset, "unexpected end of JSON"));
                }
                (_, token) => {
                    return Err(self.invalid(token_offset(&token), "unexpected JSON token"));
                }
            }
            root_complete = frames.is_empty();
        }
        match self.next_token()? {
            Token::Eof(_) => {}
            token => return Err(self.invalid(token_offset(&token), "trailing content after JSON")),
        }

        let mut fields = HashMap::<JsonGraphItemId, Vec<JsonGraphField>>::new();
        for item in self.items.values() {
            if let ProjectedItem::Field { parent, field } = item {
                fields
                    .entry(parent.clone())
                    .or_default()
                    .push(field.clone());
            }
        }
        let selected_nodes = self
            .items
            .values()
            .filter_map(|item| match item {
                ProjectedItem::Node(node) => Some(node),
                ProjectedItem::Field { .. } => None,
            })
            .collect::<Vec<_>>();
        let selected_ids = selected_nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        let edges = selected_nodes
            .iter()
            .filter_map(|node| {
                let parent = node.parent.as_ref()?;
                let kind = node.edge_kind?;
                selected_ids.contains(parent).then(|| JsonGraphEdge {
                    id: JsonGraphItemId::new(format!(
                        "edge:{}->{}",
                        parent.as_str(),
                        node.id.as_str()
                    )),
                    from: parent.clone(),
                    to: node.id.clone(),
                    parent_port: JsonGraphItemId::new(format!("port:{}", node.id.as_str())),
                    source: SourceLocator::new(node.source.clone()),
                    kind,
                    label: node.edge_label.clone(),
                })
            })
            .collect::<Vec<_>>();
        let nodes = selected_nodes
            .into_iter()
            .map(|node| JsonGraphNode {
                id: node.id.clone(),
                json_path: node.json_path.clone(),
                source: SourceLocator::new(node.source.clone()),
                kind: node.kind,
                label: node.label.clone(),
                fields: node
                    .root_field
                    .iter()
                    .cloned()
                    .chain(fields.remove(&node.id).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .into(),
                child_count: node.child_count,
            })
            .collect::<Vec<_>>();
        Ok(JsonGraphProjection {
            nodes: nodes.into(),
            edges: edges.into(),
            truncated: self.truncated,
        })
    }

    fn consume_value(
        &mut self,
        token: Token,
        parent: Option<ParentContext>,
        path: String,
        label: String,
        frames: &mut Vec<Frame>,
    ) -> Result<(), JsonGraphError> {
        match token {
            Token::ObjectStart(start) => {
                self.start_container(ContainerKind::Object, start, parent, path, label, frames)
            }
            Token::ArrayStart(start) => {
                self.start_container(ContainerKind::Array, start, parent, path, label, frames)
            }
            Token::String(value) => self.add_scalar(
                parent,
                path,
                label,
                value.start..value.end,
                value.display,
                JsonValueKind::String,
            ),
            Token::Scalar {
                start,
                end,
                display,
                kind,
            } => self.add_scalar(parent, path, label, start..end, display, kind),
            token => Err(self.invalid(token_offset(&token), "expected a JSON value")),
        }
    }

    fn start_container(
        &mut self,
        kind: ContainerKind,
        start: u64,
        parent: Option<ParentContext>,
        path: String,
        label: String,
        frames: &mut Vec<Frame>,
    ) -> Result<(), JsonGraphError> {
        if let Some(parent) = &parent {
            self.increment_child_count(&parent.id);
        }
        let depth = parent.as_ref().map_or(0, |parent| parent.depth + 1);
        let node_id = JsonGraphItemId::new(format!("node:{path}"));
        let edge_kind = parent.as_ref().map(|parent| match parent.kind {
            ContainerKind::Object => JsonGraphEdgeKind::ObjectMember,
            ContainerKind::Array => JsonGraphEdgeKind::ArrayItem,
        });
        self.consider_item(
            depth,
            ProjectedItem::Node(NodeBuild {
                id: node_id.clone(),
                json_path: Arc::from(path.clone()),
                source: start..start.saturating_add(1),
                kind: match kind {
                    ContainerKind::Object => JsonValueKind::Object,
                    ContainerKind::Array => JsonValueKind::Array,
                },
                label: Arc::from(label.clone()),
                child_count: 0,
                root_field: None,
                parent: parent.as_ref().map(|parent| parent.id.clone()),
                edge_kind,
                edge_label: Arc::from(label),
            }),
        );
        frames.push(Frame {
            kind,
            state: match kind {
                ContainerKind::Object => ContainerState::ObjectKeyOrEnd { allow_end: true },
                ContainerKind::Array => ContainerState::ArrayValueOrEnd { allow_end: true },
            },
            node_id,
            depth,
            path,
            next_ordinal: 0,
            pending_key: None,
        });
        Ok(())
    }

    fn add_scalar(
        &mut self,
        parent: Option<ParentContext>,
        path: String,
        label: String,
        source: std::ops::Range<u64>,
        display: String,
        kind: JsonValueKind,
    ) -> Result<(), JsonGraphError> {
        if let Some(parent) = parent {
            self.consider_item(
                parent.depth + 1,
                ProjectedItem::Field {
                    parent: parent.id,
                    field: JsonGraphField {
                        id: JsonGraphItemId::new(format!("field:{path}")),
                        json_path: Arc::from(path),
                        label: Arc::from(label),
                        display_value: Arc::from(display),
                        source: SourceLocator::new(source),
                        kind,
                    },
                },
            );
            return Ok(());
        }
        let root_field = JsonGraphField {
            id: JsonGraphItemId::new("field:$"),
            json_path: Arc::from("$"),
            label: Arc::from("value"),
            display_value: Arc::from(display),
            source: SourceLocator::new(source.clone()),
            kind,
        };
        self.consider_item(
            0,
            ProjectedItem::Node(NodeBuild {
                id: JsonGraphItemId::new(format!("node:{path}")),
                json_path: Arc::from(path),
                source,
                kind,
                label: Arc::from("$"),
                child_count: 0,
                root_field: Some(root_field),
                parent: None,
                edge_kind: None,
                edge_label: Arc::from("$"),
            }),
        );
        Ok(())
    }

    fn finish_frame(&mut self, frames: &mut Vec<Frame>, end: u64) -> Result<(), JsonGraphError> {
        let Some(frame) = frames.pop() else {
            return Err(self.invalid(end.saturating_sub(1), "unexpected container terminator"));
        };
        if let Some(key) = self.item_keys.get(&frame.node_id).copied()
            && let Some(ProjectedItem::Node(node)) = self.items.get_mut(&key)
        {
            node.source.end = end;
        }
        Ok(())
    }

    fn increment_child_count(&mut self, parent: &JsonGraphItemId) {
        if let Some(key) = self.item_keys.get(parent).copied()
            && let Some(ProjectedItem::Node(node)) = self.items.get_mut(&key)
        {
            node.child_count = node.child_count.saturating_add(1);
        }
    }

    /// 始终只保留预算内最浅、同层最先出现的项目；源码仍完整扫描以验证语法。
    fn consider_item(&mut self, depth: usize, item: ProjectedItem) {
        // 同层优先保留容器卡片，确保被截断图仍有可选择、可聚焦的结构入口；
        // 标量行随后按源码顺序填充剩余预算。
        let kind_rank = match &item {
            ProjectedItem::Node(_) => 0,
            ProjectedItem::Field { .. } => 1,
        };
        let key = CandidateKey {
            depth,
            kind_rank,
            sequence: self.next_sequence,
        };
        self.next_sequence = self.next_sequence.wrapping_add(1);
        if self.items.len() >= self.item_limit {
            self.truncated = true;
            let Some((&worst_key, _)) = self.items.last_key_value() else {
                return;
            };
            if key >= worst_key {
                return;
            }
            if let Some(evicted) = self.items.remove(&worst_key) {
                self.item_keys.remove(evicted.id());
            }
        }
        self.item_keys.insert(item.id().clone(), key);
        self.items.insert(key, item);
    }
}
