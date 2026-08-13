// @author kongweiguang

use std::ops::Range;
use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentRevision(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SourceAffinity {
    #[default]
    Before,
    After,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceAnchor {
    pub byte_offset: u64,
    pub affinity: SourceAffinity,
}

impl SourceAnchor {
    pub const fn new(byte_offset: u64, affinity: SourceAffinity) -> Self {
        Self {
            byte_offset,
            affinity,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceSelection {
    pub anchor: SourceAnchor,
    pub head: SourceAnchor,
}

/// 将一个源码 transaction 的旧坐标映射到新坐标。
///
/// 编辑范围始终使用 transaction 基线 revision 的字节坐标；映射表只保存
/// 范围和替换长度，不持有正文，因此可以由所有视图共享而不会产生第二份正文。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentMutationMap {
    edits: Arc<[MutationEdit]>,
    inverse: Option<Arc<[MutationEdit]>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutationEdit {
    pub range: Range<u64>,
    pub replacement_len: u64,
}

impl DocumentMutationMap {
    pub fn empty() -> Self {
        Self {
            edits: Arc::from([]),
            inverse: None,
        }
    }

    pub fn from_transaction(transaction: &Transaction) -> Self {
        Self {
            edits: transaction
                .edits
                .iter()
                .map(|edit| MutationEdit {
                    range: edit.range.clone(),
                    replacement_len: edit.replacement.len() as u64,
                })
                .collect::<Vec<_>>()
                .into(),
            inverse: None,
        }
    }

    pub fn with_inverse(edits: &[SourceEdit], inverse: &[SourceEdit]) -> Self {
        Self {
            edits: edits
                .iter()
                .map(|edit| MutationEdit {
                    range: edit.range.clone(),
                    replacement_len: edit.replacement.len() as u64,
                })
                .collect::<Vec<_>>()
                .into(),
            inverse: Some(
                inverse
                    .iter()
                    .map(|edit| MutationEdit {
                        range: edit.range.clone(),
                        replacement_len: edit.replacement.len() as u64,
                    })
                    .collect::<Vec<_>>()
                    .into(),
            ),
        }
    }

    pub fn inverse(&self) -> Option<Self> {
        self.inverse.as_ref().map(|inverse| Self {
            edits: inverse.clone(),
            inverse: Some(self.edits.clone()),
        })
    }

    pub fn edits(&self) -> &[MutationEdit] {
        &self.edits
    }

    pub fn map_anchor(&self, anchor: SourceAnchor) -> SourceAnchor {
        let original = anchor.byte_offset;
        let mut offset = original;
        let mut delta = 0_i128;
        for edit in self.edits.iter() {
            let start = edit.range.start;
            let end = edit.range.end;
            let replacement_len = edit.replacement_len;
            if start == end {
                if original < start
                    || (original == start && anchor.affinity == SourceAffinity::Before)
                {
                    continue;
                }
                offset = shift_u64(offset, replacement_len as i128);
                delta += replacement_len as i128;
                continue;
            }

            if original < start {
                continue;
            }
            let edit_delta = replacement_len as i128 - (end - start) as i128;
            if original > end {
                offset = shift_u64(offset, edit_delta);
                delta += edit_delta;
                continue;
            }

            // At or inside a replaced range, affinity chooses the corresponding
            // side of the replacement. This keeps a reversed selection's direction.
            offset = if anchor.affinity == SourceAffinity::Before {
                shift_u64(start, delta)
            } else {
                shift_u64(start.saturating_add(replacement_len), delta)
            };
            return SourceAnchor::new(offset, anchor.affinity);
        }
        SourceAnchor::new(offset, anchor.affinity)
    }

    pub fn map_selection(&self, selection: SourceSelection) -> SourceSelection {
        SourceSelection {
            anchor: self.map_anchor(selection.anchor),
            head: self.map_anchor(selection.head),
        }
    }
}

fn shift_u64(value: u64, delta: i128) -> u64 {
    if delta >= 0 {
        value.saturating_add(delta as u64)
    } else {
        value.saturating_sub((-delta) as u64)
    }
}

/// 可持久化的视图实例身份。`DocumentViewId` 表示视图类型，实例 ID 表示
/// 同一文档中打开的具体窗口/标签；selection 与 undo 恢复必须按实例隔离。
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct DocumentViewInstanceId(Uuid);

impl DocumentViewInstanceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn uuid(self) -> Uuid {
        self.0
    }
}

impl Default for DocumentViewInstanceId {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceSelection {
    pub const fn collapsed(byte_offset: u64, affinity: SourceAffinity) -> Self {
        let anchor = SourceAnchor::new(byte_offset, affinity);
        Self {
            anchor,
            head: anchor,
        }
    }

    pub fn from_range(range: Range<u64>, reversed: bool) -> Self {
        if range.is_empty() {
            return Self::collapsed(range.start, SourceAffinity::Before);
        }
        let start = SourceAnchor::new(range.start, SourceAffinity::Before);
        let end = SourceAnchor::new(range.end, SourceAffinity::After);
        if reversed {
            Self {
                anchor: end,
                head: start,
            }
        } else {
            Self {
                anchor: start,
                head: end,
            }
        }
    }

    pub fn range(self) -> Range<u64> {
        self.anchor.byte_offset.min(self.head.byte_offset)
            ..self.anchor.byte_offset.max(self.head.byte_offset)
    }

    pub fn reversed(self) -> bool {
        self.head.byte_offset < self.anchor.byte_offset
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEdit {
    pub range: Range<u64>,
    pub replacement: Arc<str>,
}

impl SourceEdit {
    pub fn new(range: Range<u64>, replacement: impl Into<Arc<str>>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub base_revision: DocumentRevision,
    pub edits: Vec<SourceEdit>,
}

impl Transaction {
    pub fn new(base_revision: DocumentRevision, edits: Vec<SourceEdit>) -> Self {
        Self {
            base_revision,
            edits,
        }
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum EditError {
    #[error("stale document revision: expected {expected:?}, got {actual:?}")]
    StaleRevision {
        expected: DocumentRevision,
        actual: DocumentRevision,
    },
    #[error("invalid source byte range {start}..{end} for document length {len}")]
    InvalidRange { start: u64, end: u64, len: u64 },
    #[error("edit range is not on a UTF-8 boundary")]
    InvalidUtf8Boundary,
    #[error("document revision overflow")]
    RevisionOverflow,
    #[error("source byte offset does not fit this platform")]
    OffsetOverflow,
}
