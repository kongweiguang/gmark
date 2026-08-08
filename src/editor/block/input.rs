// @author kongweiguang

//! GPUI [`EntityInputHandler`] implementation for Block.
//!
//! Bridges between GPUI's UTF-16-based IME subsystem and the block's
//! internal UTF-8 representation.  All range arguments from GPUI arrive
//! as UTF-16 offsets and are converted through `range_from_utf16` before
//! operating on the block's title.

use std::ops::Range;

use gmark_math_edit::{
    MathCursor2D, MathEditCommand, MathEditError, MathSelection, MathSlot, MathVisualProjection,
};
use gpui::*;

use super::Block;
use super::element;
use crate::components::{BlockEvent, UndoCaptureKind};
use crate::editor::math_edit::MathEditSession;

impl Block {
    fn math_source_range_from_utf16(&self, text: &str, range_utf16: &Range<usize>) -> Range<usize> {
        let range = Self::utf16_range_to_utf8_in(text, range_utf16);
        let start = range.start.min(text.len());
        let end = range.end.min(text.len());
        start.min(end)..start.max(end)
    }

    fn math_source_bounds_for_range(
        &self,
        range_utf16: &Range<usize>,
        bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        let text = self.math_source_text();
        let range = self.math_source_range_from_utf16(&text, range_utf16);
        let line = self.math_source_last_layout.as_ref()?;
        let layout_bounds = self.math_source_last_bounds.unwrap_or(bounds);
        let left = layout_bounds.left() + line.x_for_index(range.start);
        let right = layout_bounds.left() + line.x_for_index(range.end);
        Some(Bounds::from_corners(
            point(left, bounds.top()),
            point(right.max(left + px(1.0)), bounds.bottom()),
        ))
    }

    /// Return the slot currently owned by the structured formula editor.
    ///
    /// The semantic editor keeps the cursor and selection in slot-local UTF-8
    /// coordinates.  EntityInputHandler must never fall back to the block's
    /// Markdown-visible projection while this focus target is active: that
    /// projection includes delimiters and can belong to a different inline
    /// fragment entirely.
    fn math_input_context(&self) -> Option<(MathSlot, String)> {
        let session = self.math_edit_session.as_ref()?;
        let slot = session.editor().cursor().slot().clone();
        let text = Self::math_slot_text(session, &slot)?;
        Some((slot, text))
    }

    /// Resolve a slot's source without making the domain model expose its
    /// internal `slot_source` helper.  Probing with `usize::MAX` is safe: the
    /// domain cursor validates the offset and reports the slot length in the
    /// typed error.  This works for regular AST nodes and environment cells.
    fn math_slot_text(session: &MathEditSession, slot: &MathSlot) -> Option<String> {
        let document = session.document();
        let len = match MathCursor2D::at(document, slot.clone(), usize::MAX) {
            Ok(cursor) => cursor.offset(),
            Err(MathEditError::InvalidCursorOffset { len, .. }) => len,
            Err(_) => return None,
        };
        let start = MathCursor2D::at(document, slot.clone(), 0).ok()?;
        let end = MathCursor2D::at(document, slot.clone(), len).ok()?;
        MathSelection::new(start, end).selected_text(document)
    }

    fn math_selection_range(
        session: &MathEditSession,
        slot: &MathSlot,
        text_len: usize,
    ) -> (Range<usize>, bool) {
        let selection = session.editor().selection();
        if selection.is_structural() {
            return (0..text_len, false);
        }
        if selection.slot().is_some_and(|selected| selected == slot) {
            let range = selection.range().unwrap_or_else(|| {
                session.editor().cursor().offset()..session.editor().cursor().offset()
            });
            let start = range.start.min(text_len);
            let end = range.end.min(text_len);
            let reversed = selection.anchor().offset() > selection.focus().offset();
            return (start.min(end)..start.max(end), reversed);
        }
        let offset = session.editor().cursor().offset().min(text_len);
        (offset..offset, false)
    }

    fn math_clamped_range(text: &str, range: Range<usize>) -> Range<usize> {
        let start = range.start.min(text.len());
        let end = range.end.min(text.len());
        if start <= end { start..end } else { end..end }
    }

    fn set_math_selection(&mut self, slot: MathSlot, range: Range<usize>) -> bool {
        let Some(session) = self.math_edit_session.as_mut() else {
            return false;
        };
        let document = session.document().clone();
        let Ok(anchor) = MathCursor2D::at(&document, slot.clone(), range.start) else {
            return false;
        };
        let Ok(focus) = MathCursor2D::at(&document, slot, range.end) else {
            return false;
        };
        session
            .editor_mut()
            .set_selection(MathSelection::new(anchor, focus))
            .is_ok()
    }

    fn math_input_text(new_text: &str) -> String {
        // A formula slot is single-line.  IMEs and clipboard providers may
        // still send CRLF or raw line breaks; preserving them would make the
        // domain cursor and the visual slot disagree about their source.
        new_text.replace("\r\n", " ").replace(['\r', '\n'], " ")
    }

    fn math_bounds_for_range(
        &self,
        range_utf16: &Range<usize>,
        bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        let session = self.math_edit_session.as_ref()?;
        let (slot, text) = self.math_input_context()?;
        let range =
            Self::math_clamped_range(&text, Self::utf16_range_to_utf8_in(&text, range_utf16));
        let document = session.document();
        let anchor = MathCursor2D::at(document, slot.clone(), range.start).ok()?;
        let focus = MathCursor2D::at(document, slot, range.end).ok()?;
        let selection = MathSelection::new(anchor, focus);
        let projection = MathVisualProjection::from_document(document);
        let rect = projection
            .selection_rect(&selection)
            .or_else(|| projection.caret_rect(selection.focus()))?;
        let formula = projection.geometry().bounds();
        let formula_width = formula.width().max(f64::EPSILON);
        let formula_height = formula.height().max(f64::EPSILON);
        let scale_x = f64::from(bounds.size.width).max(0.0) / formula_width;
        let scale_y = f64::from(bounds.size.height).max(0.0) / formula_height;
        let left = bounds.left() + px((rect.x * scale_x) as f32);
        let top = bounds.top() + px((rect.y * scale_y) as f32);
        let right = left + px((rect.w * scale_x).max(0.0) as f32);
        let bottom = top + px((rect.h * scale_y).max(1.0) as f32);
        Some(Bounds::from_corners(point(left, top), point(right, bottom)))
    }
}

impl EntityInputHandler for Block {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        if self.math_source_focus_handle.is_focused(_window) {
            let text = self.math_source_text();
            let range = self.math_source_range_from_utf16(&text, &range_utf16);
            actual_range.replace(Self::utf8_range_to_utf16_in(&text, &range));
            return text.get(range).map(ToOwned::to_owned);
        }

        if self.math_structure_focus_handle.is_focused(_window) && self.math_edit_session.is_some()
        {
            let (_, text) = self.math_input_context()?;
            let range =
                Self::math_clamped_range(&text, Self::utf16_range_to_utf8_in(&text, &range_utf16));
            actual_range.replace(Self::utf8_range_to_utf16_in(&text, &range));
            return text.get(range).map(ToOwned::to_owned);
        }

        if self.code_language_focus_handle.is_focused(_window) {
            let range = self.code_language_range_from_utf16(&range_utf16);
            actual_range.replace(self.code_language_range_to_utf16(&range));
            return Some(self.code_language_text()[range].to_string());
        }

        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.display_text()[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.math_source_focus_handle.is_focused(_window) {
            let text = self.math_source_text();
            let (range, reversed) = self.math_source_selection();
            return Some(UTF16Selection {
                range: Self::utf8_range_to_utf16_in(&text, &range),
                reversed,
            });
        }

        if self.math_structure_focus_handle.is_focused(_window) && self.math_edit_session.is_some()
        {
            let (_, text) = self.math_input_context()?;
            let session = self.math_edit_session.as_ref()?;
            let (range, reversed) = Self::math_selection_range(
                session,
                &session.editor().cursor().slot().clone(),
                text.len(),
            );
            return Some(UTF16Selection {
                range: Self::utf8_range_to_utf16_in(&text, &range),
                reversed,
            });
        }

        if self.code_language_focus_handle.is_focused(_window) {
            return Some(UTF16Selection {
                range: self.code_language_range_to_utf16(&self.code_language_selected_range),
                reversed: self.code_language_selection_reversed,
            });
        }

        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        if self.math_source_focus_handle.is_focused(window) {
            let text = self.math_source_text();
            return self.math_source_marked_range.as_ref().map(|range| {
                let start = range.start.min(text.len());
                let end = range.end.min(text.len());
                Self::utf8_range_to_utf16_in(&text, &(start.min(end)..start.max(end)))
            });
        }

        if self.math_structure_focus_handle.is_focused(window) && self.math_edit_session.is_some() {
            let (_, text) = self.math_input_context()?;
            return self.math_marked_range.as_ref().map(|range| {
                let range = Self::math_clamped_range(&text, range.clone());
                Self::utf8_range_to_utf16_in(&text, &range)
            });
        }

        if self.code_language_focus_handle.is_focused(window) {
            return self
                .code_language_marked_range
                .as_ref()
                .map(|range| self.code_language_range_to_utf16(range));
        }

        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        if self.math_source_focus_handle.is_focused(window) {
            self.math_source_marked_range = None;
            return;
        }

        if self.math_structure_focus_handle.is_focused(window) {
            self.math_marked_range = None;
            if self.math_edit_session.is_some() {
                return;
            }
        }

        if self.code_language_focus_handle.is_focused(window) {
            self.code_language_marked_range = None;
            return;
        }

        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_read_only() {
            return;
        }
        if self.math_source_focus_handle.is_focused(_window) {
            let text = self.math_source_text();
            let visible_range = range_utf16
                .as_ref()
                .map(|range| self.math_source_range_from_utf16(&text, range))
                .or_else(|| self.math_source_marked_range.clone())
                .unwrap_or_else(|| self.math_source_selection().0);
            let was_marked = self.math_source_marked_range.is_some();
            let changed = self.replace_math_source_text_in_range(
                visible_range,
                new_text,
                None,
                false,
                if was_marked {
                    UndoCaptureKind::ImeCompositionCommit
                } else {
                    UndoCaptureKind::CoalescibleText
                },
                cx,
            );
            if changed || was_marked {
                self.math_source_marked_range = None;
            }
            return;
        }

        if self.math_structure_focus_handle.is_focused(_window) && self.math_edit_session.is_some()
        {
            let Some((slot, text)) = self.math_input_context() else {
                return;
            };
            let visible_range = range_utf16
                .as_ref()
                .map(|range| Self::utf16_range_to_utf8_in(&text, range))
                .or_else(|| self.math_marked_range.clone())
                .or_else(|| {
                    self.math_edit_session
                        .as_ref()
                        .map(|session| Self::math_selection_range(session, &slot, text.len()).0)
                })
                .unwrap_or(0..0);
            let visible_range = Self::math_clamped_range(&text, visible_range);
            let was_marked = self.math_marked_range.is_some();
            let _ = self.set_math_selection(slot, visible_range);
            let sanitized = Self::math_input_text(new_text);
            let changed = self.execute_math_command_live(
                MathEditCommand::InsertText(sanitized),
                if was_marked {
                    UndoCaptureKind::ImeCompositionCommit
                } else {
                    UndoCaptureKind::CoalescibleText
                },
                cx,
            );
            if !changed && was_marked {
                // `execute_math_command_live` only captures after a changed
                // command.  A provider may still send an identical final
                // commit; emit the sealing category so the open composition
                // is not left coalescing with subsequent typing.
                self.prepare_undo_capture(UndoCaptureKind::ImeCompositionCommit, cx);
            }
            // A commit seals the composition even when the provider sends an
            // empty final update.  The live command owns document publication;
            // this field only tracks GPUI's marked-text slice.
            if changed || was_marked {
                self.math_marked_range = None;
            }
            return;
        }

        let committing_composition = self.marked_range.is_some();
        if self.code_language_focus_handle.is_focused(_window) {
            let undo_kind = if self.code_language_marked_range.is_some() {
                UndoCaptureKind::ImeCompositionCommit
            } else {
                UndoCaptureKind::CoalescibleText
            };
            let visible_range = range_utf16
                .as_ref()
                .map(|range| self.code_language_range_from_utf16(range))
                .or(self.code_language_marked_range.clone())
                .unwrap_or(self.code_language_selected_range.clone());
            self.prepare_undo_capture(undo_kind, cx);
            self.replace_code_language_text_in_range(visible_range, new_text, None, false, cx);
            return;
        }

        if self.editor_selection_range.is_some() {
            cx.emit(BlockEvent::RequestReplaceCrossBlockSelection {
                text: new_text.to_string(),
                selected_range_relative: None,
                mark_inserted_text: false,
                undo_kind: if committing_composition {
                    UndoCaptureKind::ImeCompositionCommit
                } else {
                    UndoCaptureKind::CoalescibleText
                },
            });
            return;
        }

        let visible_range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        if self.try_apply_auto_pair_input(visible_range.clone(), new_text, cx) {
            return;
        }
        self.prepare_undo_capture(
            if committing_composition {
                UndoCaptureKind::ImeCompositionCommit
            } else {
                UndoCaptureKind::CoalescibleText
            },
            cx,
        );
        self.replace_text_in_visible_range(visible_range, new_text, None, false, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_read_only() {
            return;
        }
        if self.math_source_focus_handle.is_focused(_window) {
            let text = self.math_source_text();
            let visible_range = range_utf16
                .as_ref()
                .map(|range| self.math_source_range_from_utf16(&text, range))
                .or_else(|| self.math_source_marked_range.clone())
                .unwrap_or_else(|| self.math_source_selection().0);
            let selected_range_relative = new_selected_range_utf16
                .as_ref()
                .map(|range| Self::utf16_range_to_utf8_in(new_text, range));
            let changed = self.replace_math_source_text_in_range(
                visible_range,
                new_text,
                selected_range_relative,
                !new_text.is_empty(),
                UndoCaptureKind::ImeComposition,
                cx,
            );
            if !changed {
                self.math_source_marked_range = None;
            }
            return;
        }

        if self.math_structure_focus_handle.is_focused(_window) && self.math_edit_session.is_some()
        {
            let Some((slot, text)) = self.math_input_context() else {
                return;
            };
            let visible_range = range_utf16
                .as_ref()
                .map(|range| Self::utf16_range_to_utf8_in(&text, range))
                .or_else(|| self.math_marked_range.clone())
                .unwrap_or_else(|| {
                    self.math_edit_session
                        .as_ref()
                        .map(|session| Self::math_selection_range(session, &slot, text.len()).0)
                        .unwrap_or(0..0)
                });
            let visible_range = Self::math_clamped_range(&text, visible_range);
            let _ = self.set_math_selection(slot, visible_range.clone());
            let sanitized = Self::math_input_text(new_text);
            let selected_range_relative = new_selected_range_utf16
                .as_ref()
                .map(|range| Self::utf16_range_to_utf8_in(&sanitized, range))
                .map(|range| Self::math_clamped_range(&sanitized, range));
            let inserted_end = visible_range
                .start
                .saturating_add(sanitized.len())
                .min(text.len().saturating_add(sanitized.len()));
            let changed = self.execute_math_command_live(
                MathEditCommand::InsertText(sanitized.clone()),
                UndoCaptureKind::ImeComposition,
                cx,
            );

            if !changed {
                self.math_marked_range = None;
                return;
            }

            // `InsertText` leaves the domain cursor after the inserted text.
            // IME providers may ask for a different selected subrange; restore
            // it in the same slot after publication so the next composition
            // update continues from the provider's UTF-16 selection.
            if let Some(relative) = selected_range_relative {
                if let Some((next_slot, _)) = self.math_input_context() {
                    let absolute = visible_range.start.saturating_add(relative.start)
                        ..visible_range.start.saturating_add(relative.end);
                    let _ = self.set_math_selection(next_slot, absolute);
                }
            }
            self.math_marked_range =
                (!sanitized.is_empty()).then_some(visible_range.start..inserted_end);
            return;
        }

        if self.code_language_focus_handle.is_focused(_window) {
            let visible_range = range_utf16
                .as_ref()
                .map(|range| self.code_language_range_from_utf16(range))
                .or(self.code_language_marked_range.clone())
                .unwrap_or(self.code_language_selected_range.clone());
            let sanitized_new_text = new_text.replace("\r\n", " ").replace(['\r', '\n'], " ");
            let selected_range_relative = new_selected_range_utf16
                .as_ref()
                .map(|range_utf16| Self::utf16_range_to_utf8_in(&sanitized_new_text, range_utf16))
                .map(|relative| relative.start..relative.end);

            self.prepare_undo_capture(UndoCaptureKind::ImeComposition, cx);
            self.replace_code_language_text_in_range(
                visible_range,
                &sanitized_new_text,
                selected_range_relative,
                !sanitized_new_text.is_empty(),
                cx,
            );
            return;
        }

        if self.editor_selection_range.is_some() {
            let selected_range_relative = new_selected_range_utf16
                .as_ref()
                .map(|range_utf16| Self::utf16_range_to_utf8_in(new_text, range_utf16))
                .map(|relative| relative.start..relative.end);
            cx.emit(BlockEvent::RequestReplaceCrossBlockSelection {
                text: new_text.to_string(),
                selected_range_relative,
                mark_inserted_text: !new_text.is_empty(),
                undo_kind: UndoCaptureKind::ImeComposition,
            });
            return;
        }

        self.prepare_undo_capture(UndoCaptureKind::ImeComposition, cx);
        let visible_range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let selected_range_relative = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| Self::utf16_range_to_utf8_in(new_text, range_utf16))
            .map(|relative| relative.start..relative.end);

        self.replace_text_in_visible_range(
            visible_range,
            new_text,
            selected_range_relative,
            !new_text.is_empty(),
            cx,
        );
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.math_source_focus_handle.is_focused(_window) {
            return self.math_source_bounds_for_range(&range_utf16, bounds);
        }

        if self.math_structure_focus_handle.is_focused(_window) && self.math_edit_session.is_some()
        {
            return self.math_bounds_for_range(&range_utf16, bounds);
        }

        if self.code_language_focus_handle.is_focused(_window) {
            let line = self.code_language_last_layout.as_ref()?;
            let range = self.code_language_range_from_utf16(&range_utf16);
            let start_x = line.x_for_index(range.start);
            let end_x = line.x_for_index(range.end);
            return Some(Bounds::from_corners(
                point(bounds.left() + start_x, bounds.top()),
                point(bounds.left() + end_x, bounds.bottom()),
            ));
        }

        let lines = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let line_height = self.last_line_height;
        let text = self.display_text();
        element::range_bounds(lines, bounds, line_height, text, range, self.text_align())
    }

    fn character_index_for_point(
        &mut self,
        pt: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        if self.math_source_focus_handle.is_focused(_window) {
            let index = self.math_source_index_for_point(pt);
            return Some(Self::utf8_to_utf16_in(&self.math_source_text(), index));
        }

        if self.code_language_focus_handle.is_focused(_window) {
            let index = self.code_language_index_for_mouse_position(pt);
            return Some(Self::utf8_to_utf16_in(self.code_language_text(), index));
        }

        let bounds = self.last_bounds?;
        let lines = self.last_layout.as_ref()?;
        let text = self.display_text();
        let ranges = element::hard_line_ranges(text);
        let relative = Point {
            x: pt.x - bounds.left(),
            y: pt.y - bounds.top(),
        };
        let (line_idx, y_in_line) =
            element::wrapped_line_for_y(lines, self.last_line_height, relative.y)?;
        let layout = &lines[line_idx];
        let origin_x = element::aligned_line_left(layout, bounds, self.text_align());
        let utf8_offset_in_line = match layout
            .closest_index_for_position(point(pt.x - origin_x, y_in_line), self.last_line_height)
        {
            Ok(idx) | Err(idx) => idx,
        };
        let utf8_index = ranges[line_idx].start + utf8_offset_in_line;
        Some(Self::utf8_to_utf16_in(self.display_text(), utf8_index))
    }
}
