// @author kongweiguang

//! 从不可变 SourceBacked 快照生成有界 JSON 图投影。
//!
//! 本 crate 只描述 JSON 格式能力，不知道文件大小、PieceTree、磁盘 IO 或 GPUI。
//! 宿主通过窄快照与取消契约接入任意存储引擎。

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::Arc;
use thiserror::Error;

pub use gmark_document_core::{
    DocumentSnapshot, ProjectionCancellation as CancellationSignal, SnapshotError, SourceLocator,
};

const READ_CHUNK_BYTES: u64 = 64 * 1024;
const DISPLAY_TEXT_BYTES: usize = 120;
pub const DEFAULT_JSON_GRAPH_ITEM_LIMIT: usize = 1_500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphRequest {
    pub document_epoch: u64,
    pub revision: u64,
    pub generation: u64,
    pub root: Option<JsonGraphRoot>,
    pub item_limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphRoot {
    pub source: SourceLocator,
    pub json_path: Arc<str>,
    pub label: Arc<str>,
}

impl JsonGraphRoot {
    pub fn new(
        source: SourceLocator,
        json_path: impl Into<Arc<str>>,
        label: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            source,
            json_path: json_path.into(),
            label: label.into(),
        }
    }
}

impl JsonGraphRequest {
    pub fn accepts(&self, snapshot: &JsonGraphSnapshot) -> bool {
        self.document_epoch == snapshot.document_epoch
            && self.revision == snapshot.revision
            && self.generation == snapshot.generation
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum JsonGraphError {
    #[error("operation was cancelled")]
    Cancelled,
    #[error("the immutable source snapshot changed")]
    SourceChanged,
    #[error("invalid byte range {start}..{end} for a {len}-byte source")]
    InvalidRange { start: u64, end: u64, len: u64 },
    #[error("byte range length does not fit this platform")]
    RangeTooLarge,
    #[error("invalid JSON near byte {offset}: {message}")]
    InvalidJson { offset: u64, message: String },
    #[error(transparent)]
    Read(#[from] SnapshotError),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JsonGraphItemId(Arc<str>);

impl JsonGraphItemId {
    pub fn new(value: impl Into<Arc<str>>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonValueKind {
    Object,
    Array,
    String,
    Number,
    Boolean,
    Null,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphNode {
    pub id: JsonGraphItemId,
    pub json_path: Arc<str>,
    pub source: SourceLocator,
    pub kind: JsonValueKind,
    pub label: Arc<str>,
    pub fields: Arc<[JsonGraphField]>,
    pub child_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphField {
    pub id: JsonGraphItemId,
    pub json_path: Arc<str>,
    pub label: Arc<str>,
    pub display_value: Arc<str>,
    pub source: SourceLocator,
    pub kind: JsonValueKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonGraphEdgeKind {
    ObjectMember,
    ArrayItem,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphEdge {
    pub id: JsonGraphItemId,
    pub from: JsonGraphItemId,
    pub to: JsonGraphItemId,
    /// 父卡片中承载该容器字段的稳定端口；UI 用它绑定字段行和连线，不能按边序号猜测。
    pub parent_port: JsonGraphItemId,
    pub source: SourceLocator,
    pub kind: JsonGraphEdgeKind,
    pub label: Arc<str>,
}

/// 投影严格受 item_limit 限制；达到预算后仍完成语法验证，但不再保留新图项目。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphProjection {
    pub nodes: Arc<[JsonGraphNode]>,
    pub edges: Arc<[JsonGraphEdge]>,
    pub truncated: bool,
}

#[derive(Default)]
pub struct JsonGraphProvider;

impl JsonGraphProvider {
    pub fn build(
        &self,
        document: &dyn DocumentSnapshot,
        request: &JsonGraphRequest,
        cancellation: &dyn CancellationSignal,
    ) -> Result<JsonGraphSnapshot, JsonGraphError> {
        if document.revision().0 != request.revision {
            return Err(JsonGraphError::SourceChanged);
        }
        let (range, root_path, root_label) = request.root.as_ref().map_or_else(
            || (0..document.len(), "$".to_owned(), "$".to_owned()),
            |root| {
                (
                    root.source.range.clone(),
                    root.json_path.to_string(),
                    root.label.to_string(),
                )
            },
        );
        if range.start > range.end || range.end > document.len() {
            return Err(JsonGraphError::InvalidRange {
                start: range.start,
                end: range.end,
                len: document.len(),
            });
        }
        let projection = GraphParser::new(
            document,
            range,
            request.item_limit.max(1),
            cancellation,
            root_path,
            root_label,
        )?
        .parse()?;
        let locators = projection
            .nodes
            .iter()
            .map(|node| node.source.clone())
            .collect();
        Ok(JsonGraphSnapshot {
            document_epoch: request.document_epoch,
            revision: request.revision,
            generation: request.generation,
            projection,
            locators,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGraphSnapshot {
    pub document_epoch: u64,
    pub revision: u64,
    pub generation: u64,
    projection: JsonGraphProjection,
    locators: Vec<SourceLocator>,
}

impl JsonGraphSnapshot {
    pub fn projection(&self) -> &JsonGraphProjection {
        &self.projection
    }

    pub fn source_locators(&self) -> &[SourceLocator] {
        &self.locators
    }
}

impl gmark_document_core::DerivedProjectionSnapshot for JsonGraphSnapshot {
    fn document_epoch(&self) -> u64 {
        self.document_epoch
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn status(&self) -> gmark_document_core::DerivedProjectionStatus {
        if self.projection.truncated {
            gmark_document_core::DerivedProjectionStatus::LimitExceeded
        } else {
            gmark_document_core::DerivedProjectionStatus::Ready
        }
    }

    fn source_locators(&self) -> &[SourceLocator] {
        &self.locators
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Clone)]
struct StringToken {
    start: u64,
    end: u64,
    display: String,
}

enum Token {
    ObjectStart(u64),
    ObjectEnd(u64),
    ArrayStart(u64),
    ArrayEnd(u64),
    Colon(u64),
    Comma(u64),
    String(StringToken),
    Scalar {
        start: u64,
        end: u64,
        display: String,
        kind: JsonValueKind,
    },
    Eof(u64),
}

#[derive(Clone, Copy)]
enum ContainerKind {
    Object,
    Array,
}

enum ContainerState {
    ObjectKeyOrEnd { allow_end: bool },
    ObjectColon,
    ObjectValue,
    ObjectCommaOrEnd,
    ArrayValueOrEnd { allow_end: bool },
    ArrayCommaOrEnd,
}

struct Frame {
    kind: ContainerKind,
    state: ContainerState,
    node_id: JsonGraphItemId,
    depth: usize,
    path: String,
    next_ordinal: usize,
    pending_key: Option<StringToken>,
}

#[derive(Clone)]
struct ParentContext {
    id: JsonGraphItemId,
    depth: usize,
    kind: ContainerKind,
}

struct NodeBuild {
    id: JsonGraphItemId,
    json_path: Arc<str>,
    source: Range<u64>,
    kind: JsonValueKind,
    label: Arc<str>,
    child_count: usize,
    root_field: Option<JsonGraphField>,
    parent: Option<JsonGraphItemId>,
    edge_kind: Option<JsonGraphEdgeKind>,
    edge_label: Arc<str>,
}

enum ProjectedItem {
    Node(NodeBuild),
    Field {
        parent: JsonGraphItemId,
        field: JsonGraphField,
    },
}

impl ProjectedItem {
    fn id(&self) -> &JsonGraphItemId {
        match self {
            Self::Node(node) => &node.id,
            Self::Field { field, .. } => &field.id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CandidateKey {
    depth: usize,
    kind_rank: u8,
    sequence: u64,
}

struct GraphParser<'a> {
    cursor: SnapshotCursor<'a>,
    item_limit: usize,
    next_sequence: u64,
    truncated: bool,
    items: BTreeMap<CandidateKey, ProjectedItem>,
    item_keys: HashMap<JsonGraphItemId, CandidateKey>,
    cancellation: &'a dyn CancellationSignal,
    root_path: String,
    root_label: String,
}

impl<'a> GraphParser<'a> {
    fn new(
        document: &'a dyn DocumentSnapshot,
        range: Range<u64>,
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

    fn parse(mut self) -> Result<JsonGraphProjection, JsonGraphError> {
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
            .collect::<std::collections::HashSet<_>>();
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
        source: Range<u64>,
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
                source: source.clone(),
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

    fn next_token(&mut self) -> Result<Token, JsonGraphError> {
        self.cursor.skip_whitespace()?;
        let start = self.cursor.position();
        let Some(byte) = self.cursor.bump()? else {
            return Ok(Token::Eof(start));
        };
        match byte {
            b'{' => Ok(Token::ObjectStart(start)),
            b'}' => Ok(Token::ObjectEnd(self.cursor.position())),
            b'[' => Ok(Token::ArrayStart(start)),
            b']' => Ok(Token::ArrayEnd(self.cursor.position())),
            b':' => Ok(Token::Colon(start)),
            b',' => Ok(Token::Comma(start)),
            b'"' => self.read_string(start).map(Token::String),
            b't' => self.read_literal(start, b"rue", "true", JsonValueKind::Boolean),
            b'f' => self.read_literal(start, b"alse", "false", JsonValueKind::Boolean),
            b'n' => self.read_literal(start, b"ull", "null", JsonValueKind::Null),
            b'-' | b'0'..=b'9' => self.read_number(start, byte),
            _ => Err(self.invalid(start, "invalid JSON token")),
        }
    }

    fn read_literal(
        &mut self,
        start: u64,
        tail: &[u8],
        display: &str,
        kind: JsonValueKind,
    ) -> Result<Token, JsonGraphError> {
        for expected in tail {
            if self.cursor.bump()? != Some(*expected) {
                return Err(self.invalid(self.cursor.position(), "invalid JSON literal"));
            }
        }
        Ok(Token::Scalar {
            start,
            end: self.cursor.position(),
            display: display.to_owned(),
            kind,
        })
    }

    fn read_string(&mut self, start: u64) -> Result<StringToken, JsonGraphError> {
        let mut raw = vec![b'"'];
        let mut escaped = false;
        loop {
            let Some(byte) = self.cursor.bump()? else {
                return Err(self.invalid(start, "unterminated string"));
            };
            if raw.len() <= DISPLAY_TEXT_BYTES * 4 {
                raw.push(byte);
            }
            if escaped {
                if byte == b'u' {
                    for _ in 0..4 {
                        let Some(hex) = self.cursor.bump()? else {
                            return Err(self.invalid(start, "unterminated unicode escape"));
                        };
                        if !hex.is_ascii_hexdigit() {
                            return Err(
                                self.invalid(self.cursor.position() - 1, "invalid unicode escape")
                            );
                        }
                        if raw.len() <= DISPLAY_TEXT_BYTES * 4 {
                            raw.push(hex);
                        }
                    }
                } else if !matches!(byte, b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') {
                    return Err(self.invalid(self.cursor.position() - 1, "invalid string escape"));
                }
                escaped = false;
                continue;
            }
            match byte {
                b'\\' => escaped = true,
                b'"' => break,
                0x00..=0x1f => {
                    return Err(
                        self.invalid(self.cursor.position() - 1, "control character in string")
                    );
                }
                _ => {}
            }
        }
        let display = decode_bounded_string_prefix(raw)
            .ok_or_else(|| self.invalid(start, "invalid Unicode string escape"))?;
        Ok(StringToken {
            start,
            end: self.cursor.position(),
            display: truncate_display(display),
        })
    }

    fn read_number(&mut self, start: u64, first: u8) -> Result<Token, JsonGraphError> {
        let mut bytes = vec![first];
        let mut push = |byte| {
            if bytes.len() < DISPLAY_TEXT_BYTES {
                bytes.push(byte);
            }
        };
        let first_digit = if first == b'-' {
            let Some(digit) = self.cursor.bump()? else {
                return Err(self.invalid(start, "number is missing an integer part"));
            };
            if !digit.is_ascii_digit() {
                return Err(self.invalid(self.cursor.position() - 1, "invalid number"));
            }
            push(digit);
            digit
        } else {
            first
        };
        if first_digit == b'0'
            && self
                .cursor
                .peek()?
                .is_some_and(|byte| byte.is_ascii_digit())
        {
            return Err(self.invalid(self.cursor.position(), "leading zero in number"));
        }
        while self
            .cursor
            .peek()?
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            push(self.bump_required(start, "number ended unexpectedly")?);
        }
        if self.cursor.peek()? == Some(b'.') {
            push(self.bump_required(start, "number ended after decimal point")?);
            if !self
                .cursor
                .peek()?
                .is_some_and(|byte| byte.is_ascii_digit())
            {
                return Err(self.invalid(self.cursor.position(), "fraction is missing digits"));
            }
            while self
                .cursor
                .peek()?
                .is_some_and(|byte| byte.is_ascii_digit())
            {
                push(self.bump_required(start, "fraction ended unexpectedly")?);
            }
        }
        if self
            .cursor
            .peek()?
            .is_some_and(|byte| matches!(byte, b'e' | b'E'))
        {
            push(self.bump_required(start, "exponent ended unexpectedly")?);
            if self
                .cursor
                .peek()?
                .is_some_and(|byte| matches!(byte, b'+' | b'-'))
            {
                push(self.bump_required(start, "exponent sign has no digits")?);
            }
            if !self
                .cursor
                .peek()?
                .is_some_and(|byte| byte.is_ascii_digit())
            {
                return Err(self.invalid(self.cursor.position(), "exponent is missing digits"));
            }
            while self
                .cursor
                .peek()?
                .is_some_and(|byte| byte.is_ascii_digit())
            {
                push(self.bump_required(start, "exponent ended unexpectedly")?);
            }
        }
        let display = String::from_utf8_lossy(&bytes).into_owned();
        Ok(Token::Scalar {
            start,
            end: self.cursor.position(),
            display,
            kind: JsonValueKind::Number,
        })
    }

    fn invalid(&self, offset: u64, message: impl Into<String>) -> JsonGraphError {
        JsonGraphError::InvalidJson {
            offset,
            message: message.into(),
        }
    }

    fn bump_required(&mut self, offset: u64, message: &'static str) -> Result<u8, JsonGraphError> {
        self.cursor
            .bump()?
            .ok_or_else(|| JsonGraphError::InvalidJson {
                offset,
                message: message.to_owned(),
            })
    }
}

fn token_offset(token: &Token) -> u64 {
    match token {
        Token::ObjectStart(offset)
        | Token::ArrayStart(offset)
        | Token::Colon(offset)
        | Token::Comma(offset)
        | Token::Eof(offset) => *offset,
        Token::ObjectEnd(offset) | Token::ArrayEnd(offset) => offset.saturating_sub(1),
        Token::String(value) => value.start,
        Token::Scalar { start, .. } => *start,
    }
}

fn escape_pointer_segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn truncate_display(mut value: String) -> String {
    if value.chars().count() <= DISPLAY_TEXT_BYTES {
        return value;
    }
    value = value.chars().take(DISPLAY_TEXT_BYTES).collect();
    value.push('…');
    value
}

/// 长字符串不物化完整值；给有界 JSON 前缀补上引号，并退到最后一个完整的
/// UTF-8/escape 边界后再解码，避免把半个 `\uXXXX` 或多字节字符显示成乱码。
fn decode_bounded_string_prefix(mut raw: Vec<u8>) -> Option<String> {
    let truncated = raw.last() != Some(&b'"');
    if !truncated {
        return serde_json::from_slice::<String>(&raw)
            .ok()
            .map(truncate_display);
    }
    if truncated {
        raw.push(b'"');
    }
    loop {
        if let Ok(mut decoded) = serde_json::from_slice::<String>(&raw) {
            if truncated {
                decoded.push('…');
            }
            return Some(truncate_display(decoded));
        }
        if raw.len() <= 2 {
            return Some("…".to_owned());
        }
        raw.remove(raw.len() - 2);
    }
}

struct SnapshotCursor<'a> {
    document: &'a dyn DocumentSnapshot,
    range: Range<u64>,
    position: u64,
    chunk_start: u64,
    chunk: Vec<u8>,
    cancellation: &'a dyn CancellationSignal,
}

impl<'a> SnapshotCursor<'a> {
    fn new(
        document: &'a dyn DocumentSnapshot,
        range: Range<u64>,
        cancellation: &'a dyn CancellationSignal,
    ) -> Self {
        Self {
            document,
            position: range.start,
            chunk_start: range.start,
            range,
            chunk: Vec::new(),
            cancellation,
        }
    }

    fn position(&self) -> u64 {
        self.position
    }

    fn peek(&mut self) -> Result<Option<u8>, JsonGraphError> {
        if self.position >= self.range.end {
            return Ok(None);
        }
        self.ensure_chunk()?;
        let index = usize::try_from(self.position.saturating_sub(self.chunk_start))
            .map_err(|_| JsonGraphError::RangeTooLarge)?;
        Ok(self.chunk.get(index).copied())
    }

    fn bump(&mut self) -> Result<Option<u8>, JsonGraphError> {
        let byte = self.peek()?;
        if byte.is_some() {
            self.position += 1;
        }
        Ok(byte)
    }

    fn skip_whitespace(&mut self) -> Result<(), JsonGraphError> {
        while self.peek()?.is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
        Ok(())
    }

    fn ensure_chunk(&mut self) -> Result<(), JsonGraphError> {
        if self.cancellation.is_cancelled() {
            return Err(JsonGraphError::Cancelled);
        }
        let chunk_end = self.chunk_start.saturating_add(self.chunk.len() as u64);
        if self.position >= self.chunk_start && self.position < chunk_end {
            return Ok(());
        }
        self.chunk_start = self.position;
        let end = self
            .position
            .saturating_add(READ_CHUNK_BYTES)
            .min(self.range.end);
        self.chunk = self.document.read_range(self.position..end)?;
        Ok(())
    }
}
