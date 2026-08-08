// @author kongweiguang

//! Command contracts, snapshots, and source-coordinate helpers for [`MathEditor`].

use super::*;

/// Commands understood by the GPUI-independent formula editor. Each
/// structural template is represented as one command so the host can record
/// it as a single undo transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MathEditCommand {
    InsertText(String),
    DeleteBackward,
    DeleteForward,
    ReplaceSelection(MathNode),
    ReplaceNode {
        path: MathPath,
        replacement: MathNode,
    },
    RemoveNode(MathPath),
    InsertBefore {
        path: MathPath,
        node: MathNode,
    },
    InsertAfter {
        path: MathPath,
        node: MathNode,
    },
    InsertFraction,
    InsertRoot,
    InsertRadical,
    InsertSquareRoot,
    InsertRootWithIndex(String),
    /// Insert an editable nth-root and place the caret in its degree slot.
    InsertNthRoot,
    /// Insert or wrap a semantic auto-sized delimiter pair.
    InsertDelimiter(MathDelimiterPair),
    /// Insert an operator followed by editable lower and upper limit slots.
    InsertOperatorWithLimits(String),
    InsertSuperscript,
    InsertSubscript,
    InsertMatrix {
        rows: usize,
        columns: usize,
    },
    InsertCases {
        rows: usize,
    },
    InsertAligned {
        rows: usize,
        columns: usize,
    },
    InsertEnvironment {
        name: String,
        rows: usize,
        columns: usize,
    },
    InsertTextMode,
    InsertTextCommand,
    InsertSymbol(String),
    InsertAccent(String),
    InsertBigOperator(String),
}

/// Short alias used by adapters that call these operations simply commands.
pub type MathCommand = MathEditCommand;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SpecialCursor {
    NthRoot,
    Delimiter { inside_empty: bool },
    OperatorLimits,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MathEnvironmentKind {
    Matrix,
    Cases,
    Aligned,
    Named(String),
}

impl MathEnvironmentKind {
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Matrix => "matrix",
            Self::Cases => "cases",
            Self::Aligned => "aligned",
            Self::Named(name) => name,
        }
    }
}

impl MathEditCommand {
    #[must_use]
    pub fn insert_environment(kind: MathEnvironmentKind, rows: usize, columns: usize) -> Self {
        match kind {
            MathEnvironmentKind::Matrix => Self::InsertMatrix { rows, columns },
            MathEnvironmentKind::Cases => Self::InsertCases { rows },
            MathEnvironmentKind::Aligned => Self::InsertAligned { rows, columns },
            MathEnvironmentKind::Named(name) => Self::InsertEnvironment {
                name,
                rows,
                columns,
            },
        }
    }
}

/// The observable result of one command. `before` and `after` make a result
/// independently undoable by a document host without retaining an editor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MathEditResult {
    pub before: String,
    pub after: String,
    pub cursor: MathCursor2D,
    pub selection: MathSelection,
    pub changed: bool,
}

impl MathEditResult {
    #[must_use]
    pub fn source(&self) -> &str {
        &self.after
    }

    #[must_use]
    pub fn is_noop(&self) -> bool {
        !self.changed
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EditorSnapshot {
    pub(super) document: MathDocument,
    pub(super) cursor: MathCursor2D,
    pub(super) selection: MathSelection,
}

pub(super) fn command_latex(name: &str) -> String {
    if name.starts_with('\\') {
        name.to_owned()
    } else {
        format!("\\{name}")
    }
}

pub(super) fn slot_global_range(
    document: &MathDocument,
    slot: &MathSlot,
) -> Result<Range<usize>, MathEditError> {
    if slot.is_environment_cell() {
        return environment_cell_range(&document.to_latex(), slot);
    }
    let Some(ast) = document.ast() else {
        return slot
            .path()
            .is_root()
            .then(|| 0..document.to_latex().len())
            .ok_or(MathEditError::OpaqueDocument);
    };
    ast.source_range(slot.path())
        .ok_or_else(|| MathEditError::InvalidSlot(slot.clone()))
}

pub(super) fn global_cursor_offset(
    document: &MathDocument,
    cursor: &MathCursor2D,
) -> Result<usize, MathEditError> {
    let source = slot_source(document, cursor.slot())?;
    validate_cursor_offset(&source, cursor.offset())?;
    Ok(slot_global_range(document, cursor.slot())?.start + cursor.offset())
}

pub(super) fn find_path_for_special(
    ast: &MathAst,
    range: &Range<usize>,
    predicate: impl Fn(&MathNode) -> bool,
) -> Option<MathPath> {
    ast.paths().into_iter().find(|path| {
        let Some(candidate) = ast.source_range(path) else {
            return false;
        };
        let matches_range =
            candidate == *range || (candidate.start == range.start && candidate.end <= range.end);
        matches_range && ast.node(path).is_some_and(&predicate)
    })
}
