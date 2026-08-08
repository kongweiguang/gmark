// @author kongweiguang

use super::*;
use crate::components::{
    Delete, DeleteBack, ExitCodeBlock, MoveLeft, MoveRight, SelectLeft, SelectRight,
};

#[derive(Clone)]
pub(crate) struct MathPaletteDrag;

pub(crate) struct MathPaletteDragPreview;

impl Render for MathPaletteDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size(px(1.0))
    }
}

#[path = "math_palette.rs"]
mod math_palette;

impl Block {
    /// The LaTeX body shown in the compact source control.
    ///
    /// Markdown delimiters and layout whitespace stay outside the editable range,
    /// so the field behaves like an ordinary input while document writes still
    /// preserve the original `$...$`, `\(...\)` or `$$...$$` spelling.
    pub(crate) fn math_source_text(&self) -> String {
        let raw = self.current_math_raw();
        raw.get(Self::math_source_editable_range_in(&raw))
            .unwrap_or(raw.as_str())
            .to_owned()
    }

    fn math_source_global_range(&self, range: Range<usize>) -> Option<Range<usize>> {
        let raw = self.current_math_raw();
        let editable = Self::math_source_editable_range_in(&raw);
        let editable_len = editable.end.saturating_sub(editable.start);
        let start = range.start.min(editable_len);
        let end = range.end.min(editable_len).max(start);
        let local = editable.start.saturating_add(start)..editable.start.saturating_add(end);
        if let Some(inline) = self.math_edit_inline_range.as_ref() {
            return Some(
                inline.start.saturating_add(local.start)..inline.start.saturating_add(local.end),
            );
        }
        Some(local)
    }

    fn math_source_editable_range_in(raw: &str) -> Range<usize> {
        let Some(span) = crate::editor::math_edit::MathSourceSpan::parse(raw) else {
            return 0..raw.len();
        };
        let Some(body) = span.body(raw) else {
            return 0..raw.len();
        };
        let trimmed = body.trim();
        if trimmed.is_empty() {
            // Keep a block formula's closing line break in place when the body is
            // empty. Typing into `$$\n\n$$` therefore produces `$$\n…\n$$`.
            let insertion = if body.ends_with("\r\n") {
                span.body_range.end.saturating_sub(2)
            } else if body.ends_with('\r') || body.ends_with('\n') {
                span.body_range.end.saturating_sub(1)
            } else {
                span.body_range.end
            };
            return insertion..insertion;
        }
        let leading = body.len().saturating_sub(body.trim_start().len());
        let start = span.body_range.start.saturating_add(leading);
        start..start.saturating_add(trimmed.len())
    }

    pub(crate) fn math_source_selection(&self) -> (Range<usize>, bool) {
        let text_len = self.math_source_text().len();
        let range = self.math_source_selected_range.clone();
        let start = range.start.min(text_len);
        let end = range.end.min(text_len);
        (
            start.min(end)..start.max(end),
            self.math_source_selection_reversed,
        )
    }

    pub(crate) fn set_math_source_selection(&mut self, range: Range<usize>, reversed: bool) {
        let len = self.math_source_text().len();
        let start = range.start.min(len);
        let end = range.end.min(len);
        self.math_source_selected_range = start.min(end)..start.max(end);
        self.math_source_selection_reversed = reversed;
        self.math_source_marked_range = None;
        self.cursor_blink_epoch = std::time::Instant::now();
    }

    pub(crate) fn replace_math_source_text_in_range(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        selected_range_relative: Option<Range<usize>>,
        mark_inserted_text: bool,
        undo_kind: UndoCaptureKind,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.is_read_only() {
            return false;
        }
        let current = self.math_source_text();
        let range = {
            let start = range.start.min(current.len());
            let end = range.end.min(current.len());
            start.min(end)..start.max(end)
        };
        let sanitized = new_text.replace("\r\n", " ").replace(['\r', '\n'], " ");
        let Some(global_range) = self.math_source_global_range(range.clone()) else {
            return false;
        };
        let inline_range_before = self.math_edit_inline_range.clone();
        self.prepare_undo_capture(undo_kind, cx);
        self.replace_text_in_visible_range(
            global_range,
            &sanitized,
            selected_range_relative.clone(),
            mark_inserted_text && !sanitized.is_empty(),
            cx,
        );

        if inline_range_before.is_some()
            && let Some(active) = self.math_edit_inline_range.as_mut()
        {
            let removed = range.end.saturating_sub(range.start);
            let inserted = sanitized.len();
            let next_len = active
                .end
                .saturating_sub(active.start)
                .saturating_sub(removed)
                .saturating_add(inserted);
            active.end = active.start.saturating_add(next_len);
        }

        let next = self.math_source_text();
        let selected = selected_range_relative
            .map(|relative| {
                let start = range
                    .start
                    .saturating_add(relative.start.min(sanitized.len()));
                let end = range
                    .start
                    .saturating_add(relative.end.min(sanitized.len()));
                start.min(end)..start.max(end)
            })
            .unwrap_or_else(|| {
                let offset = range.start.saturating_add(sanitized.len());
                offset..offset
            });
        self.math_source_selected_range = {
            let len = next.len();
            selected.start.min(len)..selected.end.min(len)
        };
        self.math_source_selection_reversed = false;
        self.math_source_marked_range = mark_inserted_text
            .then_some(range.start..range.start.saturating_add(sanitized.len()))
            .filter(|range| !range.is_empty());
        true
    }

    pub(crate) fn on_math_source_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.math_source_focus_handle.focus(window);
        let offset = self.math_source_index_for_point(event.position);
        if event.modifiers.shift {
            self.select_math_source_to(offset);
        } else {
            self.set_math_source_selection(offset..offset, false);
        }
        self.math_source_is_selecting = true;
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn on_math_source_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.math_source_is_selecting = false;
        cx.stop_propagation();
    }

    pub(crate) fn on_math_source_mouse_up_out(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // GPUI dispatches mouse_up_out during capture. Leave propagation intact
        // so the control under the pointer can still synthesize its click.
        if self.math_source_is_selecting {
            self.math_source_is_selecting = false;
            cx.notify();
        }
    }

    pub(crate) fn on_math_source_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.math_source_is_selecting {
            return;
        }
        // A missed mouse-up must not leave later pointer movement extending the
        // selection. Only an active left-button drag changes the input range.
        if !event.dragging() {
            self.math_source_is_selecting = false;
            cx.notify();
            return;
        }
        cx.stop_propagation();
        self.select_math_source_to(self.math_source_index_for_point(event.position));
        cx.notify();
    }

    fn select_math_source_to(&mut self, offset: usize) {
        let (selection, reversed) = self.math_source_selection();
        let anchor = if reversed {
            selection.end
        } else {
            selection.start
        };
        self.set_math_source_selection(anchor.min(offset)..anchor.max(offset), offset < anchor);
    }

    pub(crate) fn on_math_source_delete_back(
        &mut self,
        _action: &DeleteBack,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.math_source_focus_handle.is_focused(window) {
            return;
        }
        let text = self.math_source_text();
        let (selection, _) = self.math_source_selection();
        let range = if selection.is_empty() {
            let offset = selection.start;
            text[..offset]
                .char_indices()
                .next_back()
                .map(|(start, _)| start..offset)
                .unwrap_or(offset..offset)
        } else {
            selection
        };
        self.replace_math_source_text_in_range(
            range,
            "",
            None,
            false,
            UndoCaptureKind::CoalescibleText,
            cx,
        );
        cx.stop_propagation();
    }

    pub(crate) fn on_math_source_delete(
        &mut self,
        _action: &Delete,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.math_source_focus_handle.is_focused(window) {
            return;
        }
        let text = self.math_source_text();
        let (selection, _) = self.math_source_selection();
        let range = if selection.is_empty() {
            let offset = selection.start;
            let end = text[offset..]
                .chars()
                .next()
                .map(|ch| offset + ch.len_utf8())
                .unwrap_or(offset);
            offset..end
        } else {
            selection
        };
        self.replace_math_source_text_in_range(
            range,
            "",
            None,
            false,
            UndoCaptureKind::CoalescibleText,
            cx,
        );
        cx.stop_propagation();
    }

    pub(crate) fn on_math_source_move_left(
        &mut self,
        _action: &MoveLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_math_source_cursor(-1, false, window, cx);
    }

    pub(crate) fn on_math_source_move_right(
        &mut self,
        _action: &MoveRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_math_source_cursor(1, false, window, cx);
    }

    pub(crate) fn on_math_source_select_left(
        &mut self,
        _action: &SelectLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_math_source_cursor(-1, true, window, cx);
    }

    pub(crate) fn on_math_source_select_right(
        &mut self,
        _action: &SelectRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_math_source_cursor(1, true, window, cx);
    }

    fn move_math_source_cursor(
        &mut self,
        direction: i32,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.math_source_focus_handle.is_focused(window) {
            return;
        }
        let text = self.math_source_text();
        let (selection, reversed) = self.math_source_selection();
        let anchor = if reversed {
            selection.end
        } else {
            selection.start
        };
        let offset = if !extend && !selection.is_empty() {
            if direction < 0 {
                selection.start
            } else {
                selection.end
            }
        } else {
            let caret = if reversed {
                selection.start
            } else {
                selection.end
            };
            if direction < 0 {
                text[..caret]
                    .char_indices()
                    .next_back()
                    .map(|(start, _)| start)
                    .unwrap_or(0)
            } else {
                text[caret..]
                    .chars()
                    .next()
                    .map(|ch| caret + ch.len_utf8())
                    .unwrap_or(text.len())
            }
        };
        if extend {
            self.set_math_source_selection(anchor.min(offset)..anchor.max(offset), offset < anchor);
        } else {
            self.set_math_source_selection(offset..offset, false);
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn begin_inline_math_edit(
        &mut self,
        source: &str,
        range: Range<usize>,
        window: &mut Window,
    ) {
        if self.is_read_only() || self.math_edit_session.is_some() {
            return;
        }
        if self
            .display_text()
            .get(range.clone())
            .is_none_or(|current| current != source)
        {
            return;
        }
        self.math_edit_inline_range = Some(range);
        self.math_palette_anchor_y = Some(window.mouse_position().y);
        match crate::editor::math_edit::MathEditSession::begin(source, self.document_revision) {
            Ok(session) => {
                self.math_edit_session = Some(session);
                self.math_structure_focus_handle.focus(window);
            }
            Err(_) => {
                // Unsupported or malformed inline LaTeX still gets a source
                // surface. It is never promoted to a structure session that
                // could rewrite unknown commands.
                self.math_edit_session = None;
                self.math_source_focus_handle.focus(window);
            }
        }
    }

    /// Starts or ends the visual editor around focus changes. Source mutations
    /// are already published by each command, so losing focus never restores an
    /// older formula snapshot.
    pub(crate) fn sync_math_edit_focus(
        &mut self,
        focused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let editing_math =
            self.kind() == BlockKind::MathBlock || self.math_edit_inline_range.is_some();
        if !editing_math {
            self.math_edit_session = None;
            self.math_edit_inline_range = None;
            return false;
        }
        if !focused {
            self.finish_math_edit(cx);
            return false;
        }
        if self.math_edit_session.is_none() {
            let raw = self.current_math_raw();
            match crate::editor::math_edit::MathEditSession::begin(&raw, self.document_revision) {
                Ok(session) => {
                    self.math_edit_session = Some(session);
                    if self.math_palette_anchor_y.is_none() {
                        self.math_palette_anchor_y = Some(window.mouse_position().y);
                    }
                    if self.kind() == BlockKind::MathBlock
                        && !self.math_source_focus_handle.is_focused(window)
                    {
                        self.math_structure_focus_handle.focus(window);
                    }
                }
                Err(crate::editor::math_edit::MathEditSessionError::UnsupportedStructure)
                | Err(crate::editor::math_edit::MathEditSessionError::InvalidSource)
                | Err(crate::editor::math_edit::MathEditSessionError::InvalidBody) => {
                    // Unsupported or malformed LaTeX remains statically rendered and
                    // source-editable; never expose structure controls that could rewrite it.
                    self.math_edit_session = None;
                }
                Err(crate::editor::math_edit::MathEditSessionError::StaleSource) => {
                    self.math_edit_session = None;
                }
            }
        }
        false
    }

    pub(crate) fn finish_math_edit(&mut self, cx: &mut Context<Self>) {
        self.math_edit_session = None;
        self.math_edit_inline_range = None;
        self.math_marked_range = None;
        self.math_source_marked_range = None;
        self.math_source_selected_range = 0..0;
        self.math_source_selection_reversed = false;
        self.math_source_is_selecting = false;
        self.math_palette_anchor_y = None;
        cx.notify();
    }

    fn current_math_raw(&self) -> String {
        if let Some(range) = self.math_edit_inline_range.as_ref() {
            return self
                .display_text()
                .get(range.clone())
                .unwrap_or_default()
                .to_owned();
        }
        self.record
            .raw_fallback
            .as_deref()
            .unwrap_or_else(|| self.display_text())
            .to_owned()
    }

    /// Aligns the structured session after a local block transaction has been
    /// published to the document Rope. Calls from other owners intentionally
    /// skip this method, so the next commit/cancel observes a stale revision.
    pub(crate) fn rebase_math_edit_after_local_revision(
        &mut self,
        revision: gmark_document::Revision,
    ) {
        let current = self.current_math_raw();
        if let Some(session) = self.math_edit_session.as_mut() {
            let recent_cursor = session.editor().cursor().clone();
            if session
                .acknowledge_local_publish(revision, &current)
                .is_err()
            {
                // Undo/redo and external revisions are authoritative. Reparse
                // them instead of letting a stale session overwrite the slice,
                // then restore the last slot only when it still exists.
                if session.reload_authoritative(&current, revision).is_ok() {
                    let _ = session.editor_mut().set_cursor(recent_cursor);
                } else {
                    self.math_edit_session = None;
                    self.math_edit_inline_range = None;
                }
            }
        }
        self.document_revision = revision;
    }

    pub(crate) fn execute_math_command_live(
        &mut self,
        command: gmark_math_edit::MathEditCommand,
        undo_kind: UndoCaptureKind,
        cx: &mut Context<Self>,
    ) -> bool {
        let current = self.current_math_raw();
        let edit = {
            let Some(session) = self.math_edit_session.as_mut() else {
                return false;
            };
            let changed = session
                .execute(command)
                .map(|result| result.changed)
                .unwrap_or(false);
            if !changed {
                return false;
            }
            match session.source_edit(self.document_revision, &current) {
                Ok(edit) => edit,
                Err(_) => {
                    self.math_edit_session = None;
                    self.math_edit_inline_range = None;
                    cx.notify();
                    return false;
                }
            }
        };

        if edit.next_raw == current {
            return false;
        }

        let inline_title = self.math_edit_inline_range.as_ref().map(|range| {
            let mut title = self.record.title.clone();
            let valid = title.replace_inline_math_source(range.clone(), &edit.next_raw);
            (valid, title)
        });
        if inline_title.as_ref().is_some_and(|(valid, _)| !valid) {
            self.math_edit_session = None;
            self.math_edit_inline_range = None;
            cx.notify();
            return false;
        }

        self.prepare_undo_capture(undo_kind, cx);
        if let Some((_, title)) = inline_title {
            if let Some(range) = self.math_edit_inline_range.as_mut() {
                range.end = range.start.saturating_add(edit.next_raw.len());
            }
            self.record.set_title(title);
        } else {
            self.record
                .set_title(InlineTextTree::plain(edit.next_raw.clone()));
        }
        self.sync_render_cache();
        self.math_preview_key = None;
        self.last_successful_math_render = None;
        self.math_render_error = None;
        self.mark_changed(cx);
        true
    }

    pub(crate) fn execute_math_palette_command(
        &mut self,
        command: gmark_math_edit::MathEditCommand,
        cx: &mut Context<Self>,
    ) -> bool {
        self.execute_math_command_live(command, UndoCaptureKind::NonCoalescible, cx)
    }
}
