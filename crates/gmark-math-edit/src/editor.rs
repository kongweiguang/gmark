// @author kongweiguang

use super::*;

#[path = "editor_support.rs"]
mod support;
use support::{
    EditorSnapshot, SpecialCursor, command_latex, find_path_for_special, global_cursor_offset,
    slot_global_range,
};
pub use support::{MathCommand, MathEditCommand, MathEditResult, MathEnvironmentKind};

/// Stateful command runner with snapshot-based undo/redo.  Snapshots are
/// intentionally source-oriented: restoring one can never drop opaque LaTeX
/// that the parser did not understand.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MathEditor {
    document: MathDocument,
    cursor: MathCursor2D,
    selection: MathSelection,
    undo: Vec<EditorSnapshot>,
    redo: Vec<EditorSnapshot>,
}

impl MathEditor {
    #[must_use]
    pub fn new(document: MathDocument) -> Self {
        let cursor = MathCursor2D::start(&document);
        let selection = MathSelection::collapsed(cursor.clone());
        Self {
            document,
            cursor,
            selection,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_state(
        document: MathDocument,
        cursor: MathCursor2D,
        selection: MathSelection,
    ) -> Self {
        Self {
            document,
            cursor,
            selection,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    #[must_use]
    pub fn from_latex(latex: impl Into<String>) -> Self {
        Self::new(MathDocument::parse(latex))
    }

    #[must_use]
    pub fn document(&self) -> &MathDocument {
        &self.document
    }

    pub fn document_mut(&mut self) -> &mut MathDocument {
        &mut self.document
    }

    #[must_use]
    pub fn cursor(&self) -> &MathCursor2D {
        &self.cursor
    }

    #[must_use]
    pub fn selection(&self) -> &MathSelection {
        &self.selection
    }

    pub fn set_cursor(&mut self, cursor: MathCursor2D) -> Result<(), MathEditError> {
        let source = slot_source(&self.document, cursor.slot())?;
        validate_cursor_offset(&source, cursor.offset())?;
        self.cursor = cursor;
        self.selection = MathSelection::collapsed(self.cursor.clone());
        Ok(())
    }

    pub fn set_selection(&mut self, selection: MathSelection) -> Result<(), MathEditError> {
        slot_source(&self.document, selection.anchor.slot())?;
        slot_source(&self.document, selection.focus.slot())?;
        self.selection = selection;
        self.cursor = self.selection.focus.clone();
        Ok(())
    }

    pub fn execute(&mut self, command: MathEditCommand) -> Result<MathEditResult, MathEditError> {
        let before_snapshot = EditorSnapshot {
            document: self.document.clone(),
            cursor: self.cursor.clone(),
            selection: self.selection.clone(),
        };
        let before = self.document.to_latex();
        let mut changed = match command {
            MathEditCommand::ReplaceNode { path, replacement } => {
                let ast = self
                    .document
                    .ast_mut()
                    .ok_or(MathEditError::OpaqueDocument)?;
                ast.replace(&path, replacement)?;
                self.cursor = MathCursor2D::start(&self.document);
                true
            }
            MathEditCommand::RemoveNode(path) => {
                let ast = self
                    .document
                    .ast_mut()
                    .ok_or(MathEditError::OpaqueDocument)?;
                let _ = ast.remove(&path)?;
                self.cursor = MathCursor2D::start(&self.document);
                true
            }
            MathEditCommand::InsertBefore { path, node } => {
                let ast = self
                    .document
                    .ast_mut()
                    .ok_or(MathEditError::OpaqueDocument)?;
                ast.insert_before(&path, node)?;
                true
            }
            MathEditCommand::InsertAfter { path, node } => {
                let ast = self
                    .document
                    .ast_mut()
                    .ok_or(MathEditError::OpaqueDocument)?;
                ast.insert_after(&path, node)?;
                true
            }
            other => self.execute_source_command(other)?,
        };

        let after = self.document.to_latex();
        if changed && before != after {
            self.undo.push(before_snapshot);
            self.redo.clear();
        } else {
            changed = false;
        }
        self.selection = MathSelection::collapsed(self.cursor.clone());
        Ok(MathEditResult {
            before,
            after,
            cursor: self.cursor.clone(),
            selection: self.selection.clone(),
            changed,
        })
    }

    pub fn apply(&mut self, command: MathEditCommand) -> Result<MathEditResult, MathEditError> {
        self.execute(command)
    }

    pub fn undo(&mut self) -> Result<bool, MathEditError> {
        let Some(snapshot) = self.undo.pop() else {
            return Ok(false);
        };
        self.redo.push(EditorSnapshot {
            document: self.document.clone(),
            cursor: self.cursor.clone(),
            selection: self.selection.clone(),
        });
        self.document = snapshot.document;
        self.cursor = snapshot.cursor;
        self.selection = snapshot.selection;
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, MathEditError> {
        let Some(snapshot) = self.redo.pop() else {
            return Ok(false);
        };
        self.undo.push(EditorSnapshot {
            document: self.document.clone(),
            cursor: self.cursor.clone(),
            selection: self.selection.clone(),
        });
        self.document = snapshot.document;
        self.cursor = snapshot.cursor;
        self.selection = snapshot.selection;
        Ok(true)
    }

    #[must_use]
    pub fn into_document(self) -> MathDocument {
        self.document
    }

    fn execute_source_command(&mut self, command: MathEditCommand) -> Result<bool, MathEditError> {
        let special_cursor = match &command {
            MathEditCommand::InsertNthRoot => Some(SpecialCursor::NthRoot),
            MathEditCommand::InsertDelimiter(_pair) => Some(SpecialCursor::Delimiter {
                inside_empty: self
                    .selection_range()
                    .is_ok_and(|target| target.0.is_empty()),
            }),
            MathEditCommand::InsertOperatorWithLimits(_) => Some(SpecialCursor::OperatorLimits),
            _ => None,
        };
        let target = self.selection_range()?;
        let target_start = target.0.start;
        if target.0.is_empty()
            && matches!(
                &command,
                MathEditCommand::DeleteBackward | MathEditCommand::DeleteForward
            )
            && let Some(changed) = self.merge_environment_boundary(&command, &target.1)?
            && changed
        {
            return Ok(true);
        }
        if matches!(
            &command,
            MathEditCommand::DeleteBackward | MathEditCommand::DeleteForward
        ) {
            if !self.selection.is_structural()
                && self.selection.anchor.slot != self.selection.focus.slot
            {
                return self.delete_cross_slot_selection(&target);
            }
            if target.0.is_empty()
                && let Some(changed) = self.remove_empty_structure()?
            {
                return Ok(changed);
            }
        }
        // Cross-slot insertion/wrapping has no unambiguous local target.  A
        // no-op is preferable to indexing the full source with slot-local
        // offsets (which could otherwise panic or damage structural braces).
        if !self.selection.is_structural()
            && self.selection.anchor.slot != self.selection.focus.slot
        {
            return Ok(false);
        }
        let original_slot = if self.selection.is_structural() {
            MathSlot::root()
        } else {
            self.cursor.slot.clone()
        };
        let local_anchor = self
            .selection
            .anchor
            .offset
            .min(self.selection.focus.offset);
        let slot_base = if original_slot.path.is_root() && !original_slot.is_environment_cell() {
            0
        } else {
            target.0.start.saturating_sub(local_anchor)
        };
        let target_local = if self.selection.is_structural() {
            0..target.0.len()
        } else {
            local_anchor
                ..local_anchor
                    + (self
                        .selection
                        .focus
                        .offset
                        .max(self.selection.anchor.offset)
                        - local_anchor)
        };
        let source_target = (target_local.clone(), target.1.clone());
        let (range, replacement, cursor_offset) = match command {
            MathEditCommand::InsertText(text) => {
                let end = target_local.start + text.len();
                (target_local, text, end)
            }
            MathEditCommand::DeleteBackward => {
                if !target_local.is_empty() {
                    (
                        target_local.start..target_local.end,
                        String::new(),
                        target_local.start,
                    )
                } else {
                    let source = &target.1;
                    let Some((start, _)) = source[..self.cursor.offset].char_indices().next_back()
                    else {
                        return Ok(false);
                    };
                    (start..self.cursor.offset, String::new(), start)
                }
            }
            MathEditCommand::DeleteForward => {
                if !target_local.is_empty() {
                    (
                        target_local.start..target_local.end,
                        String::new(),
                        target_local.start,
                    )
                } else {
                    let source = &target.1;
                    let Some(character) = source[self.cursor.offset..].chars().next() else {
                        return Ok(false);
                    };
                    (
                        self.cursor.offset..self.cursor.offset + character.len_utf8(),
                        String::new(),
                        self.cursor.offset,
                    )
                }
            }
            MathEditCommand::ReplaceSelection(node) => {
                let replacement = node.to_latex();
                let cursor = target_local.start + replacement.len();
                (target_local, replacement, cursor)
            }
            MathEditCommand::InsertFraction => wrap_template(&source_target, "\\frac", "{}", "{}"),
            MathEditCommand::InsertRoot
            | MathEditCommand::InsertRadical
            | MathEditCommand::InsertSquareRoot => {
                wrap_template(&source_target, "\\sqrt", "{}", "")
            }
            MathEditCommand::InsertRootWithIndex(index) => {
                let selected = &target.1[target_local.clone()];
                let replacement = format!("\\sqrt[{index}]{{{selected}}}");
                let cursor = target_local.start + replacement.len();
                (target_local, replacement, cursor)
            }
            MathEditCommand::InsertNthRoot => {
                let selected = &target.1[target_local.clone()];
                let replacement = format!(r"\sqrt[]{{{selected}}}");
                let cursor = target_local.start + replacement.len();
                (target_local, replacement, cursor)
            }
            MathEditCommand::InsertDelimiter(pair) => {
                let selected = &target.1[target_local.clone()];
                let mut replacement = pair.wrap_body(selected);
                let next_is_alphabetic = target_local.end < target.1.len()
                    && target.1[target_local.end..]
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_ascii_alphabetic());
                if pair
                    .close()
                    .chars()
                    .last()
                    .is_some_and(|character| character.is_ascii_alphabetic())
                    && next_is_alphabetic
                {
                    replacement.push(' ');
                }
                let cursor = target_local.start + replacement.len();
                (target_local, replacement, cursor)
            }
            MathEditCommand::InsertOperatorWithLimits(name) => {
                let operator = command_latex(&name);
                let replacement = format!(r"{operator}_{{}}^{{}}");
                let cursor = target_local.start + replacement.len();
                (target_local, replacement, cursor)
            }
            MathEditCommand::InsertSuperscript => wrap_suffix(&source_target, "^{}"),
            MathEditCommand::InsertSubscript => wrap_suffix(&source_target, "_{}"),
            MathEditCommand::InsertTextMode | MathEditCommand::InsertTextCommand => {
                wrap_template(&source_target, "\\text", "{}", "")
            }
            MathEditCommand::InsertAccent(name) => {
                wrap_template(&source_target, &format!("\\{name}"), "{}", "")
            }
            MathEditCommand::InsertBigOperator(name) => {
                let value = format!("\\{name}");
                let offset = target_local.start + value.len();
                (target_local, value, offset)
            }
            MathEditCommand::InsertSymbol(name) => {
                let value = format!("\\{name}");
                let offset = target_local.start + value.len();
                (target_local, value, offset)
            }
            MathEditCommand::InsertMatrix { rows, columns } => {
                wrap_environment(&source_target, "matrix", rows, columns)
            }
            MathEditCommand::InsertCases { rows } => {
                wrap_environment(&source_target, "cases", rows, 2)
            }
            MathEditCommand::InsertAligned { rows, columns } => {
                wrap_environment(&source_target, "aligned", rows, columns)
            }
            MathEditCommand::InsertEnvironment {
                name,
                rows,
                columns,
            } => wrap_environment(&source_target, &name, rows, columns),
            MathEditCommand::ReplaceNode { .. }
            | MathEditCommand::RemoveNode(_)
            | MathEditCommand::InsertBefore { .. }
            | MathEditCommand::InsertAfter { .. } => return Ok(false),
        };
        let global_range = range.start + slot_base..range.end + slot_base;
        self.replace_slot_range(global_range, &replacement)?;
        let local_offset = cursor_offset;
        self.cursor = if let Some(special) = special_cursor {
            self.cursor_for_special_command(special, target_start, replacement.len())?
                .unwrap_or(MathCursor2D::at(
                    &self.document,
                    original_slot,
                    local_offset,
                )?)
        } else {
            MathCursor2D::at(&self.document, original_slot, local_offset)?
        };
        Ok(true)
    }

    fn cursor_for_special_command(
        &self,
        special: SpecialCursor,
        range_start: usize,
        replacement_len: usize,
    ) -> Result<Option<MathCursor2D>, MathEditError> {
        let Some(ast) = self.document.ast() else {
            return Ok(None);
        };
        let range = range_start..range_start + replacement_len;
        let Some(path) = find_path_for_special(ast, &range, |node| match special {
            SpecialCursor::NthRoot => matches!(node, MathNode::SquareRoot { index: Some(_), .. }),
            SpecialCursor::Delimiter { inside_empty: true } => {
                matches!(node, MathNode::Delimited { .. })
            }
            SpecialCursor::Delimiter {
                inside_empty: false,
            } => false,
            SpecialCursor::OperatorLimits => matches!(node, MathNode::BigOperator { .. }),
        }) else {
            return Ok(None);
        };
        let target = match special {
            SpecialCursor::NthRoot => path.child(0),
            SpecialCursor::Delimiter { inside_empty: true } => path.child(0),
            SpecialCursor::Delimiter {
                inside_empty: false,
            } => return Ok(None),
            SpecialCursor::OperatorLimits => {
                let Some(parent) = path.parent() else {
                    return Ok(None);
                };
                let Some(operator_index) = path.last() else {
                    return Ok(None);
                };
                let Some(MathNode::Sequence(children)) = ast.node(&parent) else {
                    return Ok(None);
                };
                let Some(script_index) = (operator_index + 1..children.len())
                    .find(|index| matches!(children[*index], MathNode::Subscript(_)))
                else {
                    return Ok(None);
                };
                MathPath::from_indices(parent.indices().iter().copied().chain([script_index, 0, 0]))
            }
        };
        Ok(MathCursor2D::at(&self.document, MathSlot::node(target), 0).ok())
    }

    /// Delete a range whose endpoints belong to different editable slots.
    ///
    /// A raw source-range deletion would also remove the braces/separators
    /// between fraction slots (or the `&`/row break between matrix cells),
    /// leaving an opaque, malformed formula.  Clear only the selected content
    /// in each affected slot when the endpoints share a compound structure or
    /// environment; retain a source-range fallback for unrelated structures.
    fn delete_cross_slot_selection(
        &mut self,
        target: &(Range<usize>, String),
    ) -> Result<bool, MathEditError> {
        let source = &target.1;
        let anchor_global = global_cursor_offset(&self.document, &self.selection.anchor)?;
        let focus_global = global_cursor_offset(&self.document, &self.selection.focus)?;
        if anchor_global == focus_global {
            return Ok(false);
        }
        let (start_global, end_global, start_cursor, end_cursor) = if anchor_global < focus_global {
            (
                anchor_global,
                focus_global,
                self.selection.anchor.clone(),
                self.selection.focus.clone(),
            )
        } else {
            (
                focus_global,
                anchor_global,
                self.selection.focus.clone(),
                self.selection.anchor.clone(),
            )
        };

        let mut replacements = Vec::new();
        if start_cursor.slot.is_environment_cell()
            && end_cursor.slot.is_environment_cell()
            && start_cursor.slot.path == end_cursor.slot.path
        {
            let Some(ast) = self.document.ast() else {
                return Ok(false);
            };
            let slots = ast.environment_slots(start_cursor.slot.path());
            let Some(start_index) = slots.iter().position(|slot| slot == &start_cursor.slot) else {
                return Ok(false);
            };
            let Some(end_index) = slots.iter().position(|slot| slot == &end_cursor.slot) else {
                return Ok(false);
            };
            let (first, last, first_offset, last_offset) = if start_index <= end_index {
                (
                    start_index,
                    end_index,
                    start_cursor.offset,
                    end_cursor.offset,
                )
            } else {
                (
                    end_index,
                    start_index,
                    end_cursor.offset,
                    start_cursor.offset,
                )
            };
            for (index, slot) in slots.iter().enumerate().take(last + 1).skip(first) {
                let range = environment_cell_range(source, slot)?;
                let clear = if index == first {
                    range.start + first_offset.min(range.len())..range.end
                } else if index == last {
                    range.start..range.start + last_offset.min(range.len())
                } else {
                    range
                };
                if !clear.is_empty() {
                    replacements.push((clear, String::new()));
                }
            }
        } else if start_cursor.slot.path.parent() == end_cursor.slot.path.parent()
            && !start_cursor.slot.is_environment_cell()
            && !end_cursor.slot.is_environment_cell()
            && self.shared_compound_parent(start_cursor.slot.path.parent().as_ref())
        {
            let start_range = slot_global_range(&self.document, &start_cursor.slot)?;
            let end_range = slot_global_range(&self.document, &end_cursor.slot)?;
            let (first_range, second_range, first_offset, second_offset) =
                if start_range.start <= end_range.start {
                    (
                        start_range,
                        end_range,
                        start_cursor.offset,
                        end_cursor.offset,
                    )
                } else {
                    (
                        end_range,
                        start_range,
                        end_cursor.offset,
                        start_cursor.offset,
                    )
                };
            let first = first_range.start + first_offset.min(first_range.len())..first_range.end;
            let second =
                second_range.start..second_range.start + second_offset.min(second_range.len());
            if !first.is_empty() {
                replacements.push((first, String::new()));
            }
            if !second.is_empty() {
                replacements.push((second, String::new()));
            }
        }

        if replacements.is_empty() {
            replacements.push((start_global..end_global, String::new()));
        }
        // All ranges refer to the original source.  Apply them from right to
        // left so earlier byte offsets remain stable.
        replacements.sort_by_key(|replacement| std::cmp::Reverse(replacement.0.start));
        for (range, replacement) in replacements {
            self.document.replace_latex_range(range, &replacement)?;
        }

        let cursor = if start_cursor.slot.is_environment_cell()
            && end_cursor.slot.is_environment_cell()
            && start_cursor.slot.path == end_cursor.slot.path
            || self.shared_compound_parent(start_cursor.slot.path.parent().as_ref())
        {
            let offset = start_cursor
                .offset
                .min(slot_source(&self.document, &start_cursor.slot)?.len());
            MathCursor2D::at(&self.document, start_cursor.slot, offset)?
        } else {
            MathCursor2D::at(&self.document, MathSlot::root(), start_global)?
        };
        self.cursor = cursor;
        Ok(true)
    }

    fn shared_compound_parent(&self, path: Option<&MathPath>) -> bool {
        let Some(path) = path else {
            return false;
        };
        self.document
            .ast()
            .and_then(|ast| ast.node(path))
            .is_some_and(|node| {
                matches!(
                    node,
                    MathNode::Fraction { .. } | MathNode::SquareRoot { .. }
                )
            })
    }

    /// Remove an empty structural node when a deletion key is pressed inside
    /// its only editable slot.  This keeps an empty fraction/root/script or
    /// delimiter from becoming a permanent, non-deletable visual object while
    /// leaving ordinary empty root text as a harmless no-op.
    fn remove_empty_structure(&mut self) -> Result<Option<bool>, MathEditError> {
        if self.selection.is_structural() || !self.selection.is_collapsed() {
            return Ok(None);
        }
        let slot = &self.cursor.slot;
        if !slot_source(&self.document, slot)?.trim().is_empty() {
            return Ok(None);
        }
        let Some(ast) = self.document.ast() else {
            return Ok(None);
        };
        if slot.is_environment_cell() {
            // A multi-cell environment still has a meaningful empty cell at
            // an outer edge; only the genuinely empty one-cell environment is
            // eligible for whole-node removal.
            if ast.environment_slots(slot.path()).len() != 1 {
                return Ok(None);
            }
        }
        let mut candidate = slot.path().clone();
        loop {
            let Some(parent) = candidate.parent() else {
                if candidate.is_root()
                    && ast
                        .node(&candidate)
                        .is_some_and(|node| matches!(node, MathNode::Environment { .. }))
                {
                    break;
                }
                return Ok(None);
            };
            if ast
                .node(&parent)
                .is_some_and(|node| matches!(node, MathNode::Sequence(_)))
            {
                break;
            }
            candidate = parent;
        }
        let range = ast
            .source_range(&candidate)
            .ok_or_else(|| MathEditError::UnknownPath(candidate.clone()))?;
        self.document.replace_latex_range(range.clone(), "")?;
        self.cursor = MathCursor2D::at(&self.document, MathSlot::root(), range.start)?;
        Ok(Some(true))
    }

    /// Merges an adjacent matrix-like slot when deletion reaches its edge.
    /// The separator (`&` or a row break) is removed as one source edit, then
    /// the cursor is placed at the join in the surviving cell. Empty cells do
    /// not gain synthetic whitespace; non-empty cells keep a readable single
    /// space between their original contents.
    fn merge_environment_boundary(
        &mut self,
        command: &MathEditCommand,
        current_source: &str,
    ) -> Result<Option<bool>, MathEditError> {
        let slot = &self.cursor.slot;
        let MathSlotRole::EnvironmentCell { row, column } = slot.role else {
            return Ok(None);
        };
        let Some(ast) = self.document.ast() else {
            return Ok(None);
        };
        let slots = ast.environment_slots(slot.path());
        let Some(index) = slots.iter().position(|candidate| candidate == slot) else {
            return Ok(None);
        };
        let neighbor_index = match command {
            MathEditCommand::DeleteBackward
                if current_source[..self.cursor.offset].trim().is_empty() =>
            {
                index.checked_sub(1)
            }
            MathEditCommand::DeleteForward
                if current_source[self.cursor.offset..].trim().is_empty() =>
            {
                index.checked_add(1)
            }
            _ => return Ok(None),
        };
        let Some(neighbor) = neighbor_index.and_then(|candidate| slots.get(candidate)) else {
            return Ok(Some(false));
        };
        let source = self.document.to_latex();
        let current_range = environment_cell_range(&source, slot)?;
        let neighbor_range = environment_cell_range(&source, neighbor)?;
        let (left_range, right_range, left_slot, cursor_at_left) =
            if neighbor_range.end <= current_range.start {
                (neighbor_range, current_range, neighbor.clone(), true)
            } else {
                (current_range, neighbor_range, slot.clone(), false)
            };
        let left_text = source[left_range.clone()].trim_end().to_owned();
        let right_text = source[right_range.clone()].trim_start().to_owned();
        let join = if left_text.is_empty() || right_text.is_empty() {
            String::new()
        } else {
            " ".to_owned()
        };
        let mut replacement = left_text.clone();
        replacement.push_str(&join);
        replacement.push_str(&right_text);
        self.document
            .replace_latex_range(left_range.start..right_range.end, &replacement)?;
        let merged_len = left_text.len() + join.len();
        let cursor_slot = if cursor_at_left {
            left_slot
        } else {
            // A forward merge keeps the current cell's row/column address.
            MathSlot::environment_cell(slot.path.clone(), row, column)
        };
        self.cursor = MathCursor2D::at(&self.document, cursor_slot, merged_len)?;
        Ok(Some(true))
    }

    fn selection_range(&self) -> Result<(Range<usize>, String), MathEditError> {
        let source = self.document.to_latex();
        if let Some(range) = &self.selection.structural_range {
            return Ok((range.clone(), source));
        }
        let slot = &self.selection.anchor.slot;
        let slot_text = slot_source(&self.document, slot)?;
        validate_cursor_offset(&slot_text, self.selection.anchor.offset)?;
        if self.selection.anchor.slot != self.selection.focus.slot {
            let focus_text = slot_source(&self.document, &self.selection.focus.slot)?;
            validate_cursor_offset(&focus_text, self.selection.focus.offset)?;
            let anchor = global_cursor_offset(&self.document, &self.selection.anchor)?;
            let focus = global_cursor_offset(&self.document, &self.selection.focus)?;
            return Ok((anchor.min(focus)..anchor.max(focus), source));
        }
        validate_cursor_offset(&slot_text, self.selection.focus.offset)?;
        let start = self
            .selection
            .anchor
            .offset
            .min(self.selection.focus.offset);
        let end = self
            .selection
            .anchor
            .offset
            .max(self.selection.focus.offset);
        if slot.is_environment_cell() {
            let range = environment_cell_range(&source, slot)?;
            let cell_source = source[range.clone()].to_owned();
            let local = start.min(cell_source.len())..end.min(cell_source.len());
            return Ok((
                range.start + local.start..range.start + local.end,
                cell_source,
            ));
        }
        if slot.path.is_root() {
            return Ok((start..end, source));
        }
        let ast = self.document.ast().ok_or(MathEditError::OpaqueDocument)?;
        let outer = ast
            .source_range(slot.path())
            .ok_or_else(|| MathEditError::UnknownPath(slot.path().clone()))?;
        let node_source = source[outer.clone()].to_owned();
        Ok((
            outer.start + start.min(node_source.len())..outer.start + end.min(node_source.len()),
            node_source,
        ))
    }

    fn replace_slot_range(
        &mut self,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<(), MathEditError> {
        let source = self.document.to_latex();
        validate_range(&source, &range)?;
        self.document.replace_latex_range(range, replacement)
    }
}
