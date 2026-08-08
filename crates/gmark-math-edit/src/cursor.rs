// @author kongweiguang

use super::*;

/// 源码中的光标。offset 始终是 UTF-8 字符边界。
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct MathCursor {
    offset: usize,
}

impl MathCursor {
    #[must_use]
    pub const fn start() -> Self {
        Self { offset: 0 }
    }

    pub fn at(document: &MathDocument, offset: usize) -> Result<Self, MathEditError> {
        let source = document.to_latex();
        validate_cursor_offset(&source, offset)?;
        Ok(Self { offset })
    }

    #[must_use]
    pub const fn offset(self) -> usize {
        self.offset
    }

    pub fn in_slot(
        document: &MathDocument,
        slot: impl IntoMathSlot,
        offset: usize,
    ) -> Result<MathCursor2D, MathEditError> {
        MathCursor2D::at(document, slot, offset)
    }

    /// 向前移动一个 Unicode 标量值，不跨越源码边界。
    pub fn move_left(&mut self, document: &MathDocument) {
        let source = document.to_latex();
        self.offset = source[..self.offset.min(source.len())]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index);
    }

    /// 向后移动一个 Unicode 标量值，不跨越源码边界。
    pub fn move_right(&mut self, document: &MathDocument) {
        let source = document.to_latex();
        self.offset = source[self.offset.min(source.len())..]
            .chars()
            .next()
            .map_or(source.len(), |character| self.offset + character.len_utf8());
    }

    /// 在光标处插入文本，并将光标放到新文本之后。
    pub fn insert(&mut self, document: &mut MathDocument, text: &str) -> Result<(), MathEditError> {
        document.replace_latex_range(self.offset..self.offset, text)?;
        self.offset += text.len();
        Ok(())
    }

    /// 删除光标前的一个 Unicode 标量值。
    pub fn delete_backward(&mut self, document: &mut MathDocument) -> Result<bool, MathEditError> {
        let source = document.to_latex();
        validate_cursor_offset(&source, self.offset)?;
        let Some((start, _)) = source[..self.offset].char_indices().next_back() else {
            return Ok(false);
        };
        document.replace_latex_range(start..self.offset, "")?;
        self.offset = start;
        Ok(true)
    }

    /// 删除光标后的一个 Unicode 标量值。
    pub fn delete_forward(&mut self, document: &mut MathDocument) -> Result<bool, MathEditError> {
        let source = document.to_latex();
        validate_cursor_offset(&source, self.offset)?;
        let Some(character) = source[self.offset..].chars().next() else {
            return Ok(false);
        };
        document.replace_latex_range(self.offset..self.offset + character.len_utf8(), "")?;
        Ok(true)
    }
}

/// The semantic role of a two-dimensional editor slot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MathSlotRole {
    /// A regular AST node (including the root sequence).
    Node,
    /// A cell inside a matrix/cases/aligned or an otherwise unknown
    /// `\begin{...}` environment.
    EnvironmentCell { row: usize, column: usize },
}

/// A stable address for a text-bearing region of the formula.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MathSlot {
    pub(super) path: MathPath,
    pub(super) role: MathSlotRole,
}

impl MathSlot {
    #[must_use]
    pub fn root() -> Self {
        Self {
            path: MathPath::root(),
            role: MathSlotRole::Node,
        }
    }

    #[must_use]
    pub fn node(path: MathPath) -> Self {
        Self {
            path,
            role: MathSlotRole::Node,
        }
    }

    #[must_use]
    pub fn from_path(path: MathPath) -> Self {
        Self::node(path)
    }

    #[must_use]
    pub fn environment_cell(path: MathPath, row: usize, column: usize) -> Self {
        Self {
            path,
            role: MathSlotRole::EnvironmentCell { row, column },
        }
    }

    #[must_use]
    pub fn path(&self) -> &MathPath {
        &self.path
    }

    #[must_use]
    pub const fn role(&self) -> MathSlotRole {
        self.role
    }

    #[must_use]
    pub const fn row(&self) -> Option<usize> {
        match self.role {
            MathSlotRole::EnvironmentCell { row, .. } => Some(row),
            MathSlotRole::Node => None,
        }
    }

    #[must_use]
    pub const fn column(&self) -> Option<usize> {
        match self.role {
            MathSlotRole::EnvironmentCell { column, .. } => Some(column),
            MathSlotRole::Node => None,
        }
    }

    #[must_use]
    pub const fn is_environment_cell(&self) -> bool {
        matches!(self.role, MathSlotRole::EnvironmentCell { .. })
    }

    #[must_use]
    pub fn child(&self, index: usize) -> Self {
        Self::node(self.path.child(index))
    }
}

pub trait IntoMathSlot {
    fn into_math_slot(self) -> MathSlot;
}

impl IntoMathSlot for MathSlot {
    fn into_math_slot(self) -> MathSlot {
        self
    }
}

impl IntoMathSlot for &MathSlot {
    fn into_math_slot(self) -> MathSlot {
        self.clone()
    }
}

impl IntoMathSlot for MathPath {
    fn into_math_slot(self) -> MathSlot {
        MathSlot::node(self)
    }
}

impl IntoMathSlot for &MathPath {
    fn into_math_slot(self) -> MathSlot {
        MathSlot::node(self.clone())
    }
}

/// A cursor that can move between AST slots as well as within a slot.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MathCursor2D {
    pub(super) slot: MathSlot,
    pub(super) offset: usize,
    pub(super) preferred_column: usize,
}

/// Alternate spelling retained for integrations that use `2d` in identifiers.
pub type MathCursor2d = MathCursor2D;

impl MathCursor2D {
    #[must_use]
    pub fn new(slot: MathSlot, offset: usize) -> Self {
        Self {
            slot,
            offset,
            preferred_column: offset,
        }
    }

    #[must_use]
    pub fn start(document: &MathDocument) -> Self {
        Self {
            slot: MathSlot::root(),
            offset: 0,
            preferred_column: 0,
        }
        .clamp(document)
    }

    pub fn at<S: IntoMathSlot>(
        document: &MathDocument,
        slot: S,
        offset: usize,
    ) -> Result<Self, MathEditError> {
        let slot = slot.into_math_slot();
        let source = slot_source(document, &slot)?;
        validate_cursor_offset(&source, offset)?;
        Ok(Self {
            slot,
            offset,
            preferred_column: offset,
        })
    }

    #[must_use]
    pub fn slot(&self) -> &MathSlot {
        &self.slot
    }

    #[must_use]
    pub fn path(&self) -> &MathPath {
        self.slot.path()
    }

    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    #[must_use]
    pub const fn preferred_column(&self) -> usize {
        self.preferred_column
    }

    pub fn move_left(&mut self, document: &MathDocument) -> Result<bool, MathEditError> {
        let source = slot_source(document, &self.slot)?;
        validate_cursor_offset(&source, self.offset)?;
        let Some((offset, _)) = source[..self.offset].char_indices().next_back() else {
            return Ok(false);
        };
        self.offset = offset;
        self.preferred_column = offset;
        Ok(true)
    }

    pub fn move_right(&mut self, document: &MathDocument) -> Result<bool, MathEditError> {
        let source = slot_source(document, &self.slot)?;
        validate_cursor_offset(&source, self.offset)?;
        let Some(character) = source[self.offset..].chars().next() else {
            return Ok(false);
        };
        self.offset += character.len_utf8();
        self.preferred_column = self.offset;
        Ok(true)
    }

    pub fn move_horizontal(
        &mut self,
        document: &MathDocument,
        direction: i32,
    ) -> Result<bool, MathEditError> {
        if direction < 0 {
            self.move_left(document)
        } else if direction > 0 {
            self.move_right(document)
        } else {
            Ok(false)
        }
    }

    /// Move between adjacent cells of a matrix-like environment.  This is
    /// deliberately separate from character-wise left/right movement so a
    /// host can map Tab/Shift-Tab without changing ordinary text navigation.
    pub fn move_environment_slot(
        &mut self,
        document: &MathDocument,
        direction: i32,
    ) -> Result<bool, MathEditError> {
        if direction == 0 {
            return Ok(false);
        }
        let MathSlotRole::EnvironmentCell { .. } = self.slot.role else {
            return Ok(false);
        };
        let Some(ast) = document.ast() else {
            return Ok(false);
        };
        let slots = ast.environment_slots(self.slot.path());
        let Some(index) = slots.iter().position(|slot| slot == &self.slot) else {
            return Ok(false);
        };
        let next = if direction < 0 {
            index.checked_sub(1)
        } else {
            index.checked_add(1)
        }
        .and_then(|index| slots.get(index));
        let Some(next) = next else {
            return Ok(false);
        };
        let source = slot_source(document, next)?;
        self.slot = next.clone();
        self.offset = self.preferred_column.min(source.len());
        Ok(true)
    }

    /// Traverse every editable semantic slot in visual source order. Hosts map
    /// this to Tab/Shift-Tab; matrices, fractions, roots, scripts and nested
    /// structures therefore share one predictable navigation path.
    pub fn move_slot(
        &mut self,
        document: &MathDocument,
        direction: i32,
    ) -> Result<bool, MathEditError> {
        if direction == 0 {
            return Ok(false);
        }
        let projection = crate::MathVisualProjection::from_document(document);
        let mut slots = projection
            .geometry()
            .slots()
            .iter()
            .map(|geometry| geometry.slot.clone())
            .collect::<Vec<_>>();
        slots.dedup();
        let Some(index) = slots.iter().position(|slot| slot == &self.slot) else {
            return Ok(false);
        };
        let next = if direction < 0 {
            index.checked_sub(1)
        } else {
            index.checked_add(1)
        }
        .and_then(|index| slots.get(index));
        let Some(next) = next else {
            return Ok(false);
        };
        let source = slot_source(document, next)?;
        self.slot = next.clone();
        self.offset = self.preferred_column.min(source.len());
        Ok(true)
    }

    /// Move to the corresponding vertically adjacent slot.  Fraction and
    /// radical slots use their numerator/denominator or index/radicand; grid
    /// environments use the same column in the adjacent row.
    pub fn move_vertical(
        &mut self,
        document: &MathDocument,
        direction: i32,
    ) -> Result<bool, MathEditError> {
        if direction == 0 {
            return Ok(false);
        }
        if let MathSlotRole::EnvironmentCell { row, column } = self.slot.role {
            let next_row = if direction < 0 {
                row.checked_sub(1)
            } else {
                row.checked_add(1)
            };
            let Some(next_row) = next_row else {
                return Ok(false);
            };
            let candidate = MathSlot::environment_cell(self.slot.path.clone(), next_row, column);
            if slot_source(document, &candidate).is_err() {
                return Ok(false);
            }
            self.slot = candidate;
            self.offset = self
                .preferred_column
                .min(slot_source(document, &self.slot)?.len());
            return Ok(true);
        }
        let Some(parent_path) = self.slot.path.parent() else {
            return Ok(false);
        };
        let Some(index) = self.slot.path.last() else {
            return Ok(false);
        };
        let Some(parent) = document.ast().and_then(|ast| ast.node(&parent_path)) else {
            return Ok(false);
        };
        let Some(target) = (match (parent, direction < 0, index) {
            (MathNode::Fraction { .. }, true, 1) => Some(0),
            (MathNode::Fraction { .. }, false, 0) => Some(1),
            (MathNode::SquareRoot { index: Some(_), .. }, true, 1) => Some(0),
            (MathNode::SquareRoot { index: Some(_), .. }, false, 0) => Some(1),
            _ => None,
        }) else {
            return Ok(false);
        };
        let candidate = MathSlot::node(parent_path.child(target));
        let len = slot_source(document, &candidate)?.len();
        self.slot = candidate;
        self.offset = self.preferred_column.min(len);
        Ok(true)
    }

    pub fn move_up(&mut self, document: &MathDocument) -> Result<bool, MathEditError> {
        self.move_vertical(document, -1)
    }

    pub fn move_down(&mut self, document: &MathDocument) -> Result<bool, MathEditError> {
        self.move_vertical(document, 1)
    }

    pub fn insert(
        &mut self,
        document: &mut MathDocument,
        text: &str,
    ) -> Result<MathEditResult, MathEditError> {
        let selection = MathSelection::collapsed(self.clone());
        let mut editor = MathEditor::with_state(document.clone(), self.clone(), selection);
        let result = editor.execute(MathEditCommand::InsertText(text.to_owned()))?;
        *document = editor.into_document();
        *self = result.cursor.clone();
        Ok(result)
    }

    pub fn delete_backward(
        &mut self,
        document: &mut MathDocument,
    ) -> Result<MathEditResult, MathEditError> {
        let selection = MathSelection::collapsed(self.clone());
        let mut editor = MathEditor::with_state(document.clone(), self.clone(), selection);
        let result = editor.execute(MathEditCommand::DeleteBackward)?;
        *document = editor.into_document();
        *self = result.cursor.clone();
        Ok(result)
    }

    pub fn delete_forward(
        &mut self,
        document: &mut MathDocument,
    ) -> Result<MathEditResult, MathEditError> {
        let selection = MathSelection::collapsed(self.clone());
        let mut editor = MathEditor::with_state(document.clone(), self.clone(), selection);
        let result = editor.execute(MathEditCommand::DeleteForward)?;
        *document = editor.into_document();
        *self = result.cursor.clone();
        Ok(result)
    }

    pub fn select_to(
        &self,
        document: &MathDocument,
        offset: usize,
    ) -> Result<MathSelection, MathEditError> {
        let focus = Self::at(document, self.slot.clone(), offset)?;
        Ok(MathSelection::new(self.clone(), focus))
    }

    fn clamp(mut self, document: &MathDocument) -> Self {
        if let Ok(source) = slot_source(document, &self.slot) {
            self.offset = self.offset.min(source.len());
        } else {
            self.slot = MathSlot::root();
            self.offset = 0;
        }
        self
    }
}

/// Structural or range selection.  `Structural` selections carry a path and
/// source range, while range selections carry two concrete cursor endpoints.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MathSelection {
    pub(super) anchor: MathCursor2D,
    pub(super) focus: MathCursor2D,
    pub(super) structural_path: Option<MathPath>,
    pub(super) structural_range: Option<Range<usize>>,
}

impl MathSelection {
    #[must_use]
    pub fn collapsed(cursor: MathCursor2D) -> Self {
        Self {
            anchor: cursor.clone(),
            focus: cursor,
            structural_path: None,
            structural_range: None,
        }
    }

    #[must_use]
    pub fn new(anchor: MathCursor2D, focus: MathCursor2D) -> Self {
        Self {
            anchor,
            focus,
            structural_path: None,
            structural_range: None,
        }
    }

    #[must_use]
    pub fn structural(path: MathPath, range: Range<usize>) -> Self {
        let cursor = MathCursor2D {
            slot: MathSlot::node(path.clone()),
            offset: range.end,
            preferred_column: range.end,
        };
        Self {
            anchor: cursor.clone(),
            focus: cursor,
            structural_path: Some(path),
            structural_range: Some(range),
        }
    }

    #[must_use]
    pub fn from_range(path: MathPath, range: Range<usize>) -> Self {
        Self::structural(path, range)
    }

    pub fn for_node(ast: &MathAst, path: &MathPath) -> Result<Self, MathEditError> {
        ast.select(path)
    }

    #[must_use]
    pub fn anchor(&self) -> &MathCursor2D {
        &self.anchor
    }

    #[must_use]
    pub fn focus(&self) -> &MathCursor2D {
        &self.focus
    }

    #[must_use]
    pub fn slot(&self) -> Option<&MathSlot> {
        (self.anchor.slot == self.focus.slot).then_some(&self.anchor.slot)
    }

    #[must_use]
    pub fn structural_path(&self) -> Option<&MathPath> {
        self.structural_path.as_ref()
    }

    #[must_use]
    pub fn is_structural(&self) -> bool {
        self.structural_path.is_some()
    }

    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.structural_path.is_none()
            && self.anchor.slot == self.focus.slot
            && self.anchor.offset == self.focus.offset
    }

    #[must_use]
    pub fn range(&self) -> Option<Range<usize>> {
        if let Some(range) = &self.structural_range {
            return Some(range.clone());
        }
        if self.anchor.slot == self.focus.slot {
            Some(
                self.anchor.offset.min(self.focus.offset)
                    ..self.anchor.offset.max(self.focus.offset),
            )
        } else {
            None
        }
    }

    #[must_use]
    pub fn normalized(&self) -> Self {
        if self.anchor.offset <= self.focus.offset {
            return self.clone();
        }
        Self {
            anchor: self.focus.clone(),
            focus: self.anchor.clone(),
            structural_path: self.structural_path.clone(),
            structural_range: self.structural_range.clone(),
        }
    }

    #[must_use]
    pub fn selected_text(&self, document: &MathDocument) -> Option<String> {
        let range = self.range()?;
        let slot = if self.is_structural() {
            MathSlot::root()
        } else {
            self.anchor.slot.clone()
        };
        slot_source(document, &slot)
            .ok()
            .and_then(|source| source.get(range).map(ToOwned::to_owned))
    }
}
