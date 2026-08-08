// @author kongweiguang

//! Transaction boundary for interactive Markdown formula editing.
//!
//! GPUI views may render a two-dimensional editor around this model, but all
//! source-preservation and stale-write checks live here so a focus change or
//! background projection can never overwrite newer user input.

use std::ops::Range;

use gmark_document::Revision;
use gmark_math_edit::{
    MathDocument, MathEditCommand, MathEditError, MathEditResult, MathEditor, MathSelection,
    MathSupportLevel,
};

/// Delimiter family surrounding a formula body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MathDelimiter {
    Dollar,
    Parenthesized,
    DisplayDollar,
}

/// Original formula span inside a Markdown block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MathSourceSpan {
    pub(crate) raw: String,
    pub(crate) body_range: Range<usize>,
    pub(crate) delimiter: MathDelimiter,
}

impl MathSourceSpan {
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let trimmed_start = raw.len() - raw.trim_start_matches(char::is_whitespace).len();
        let trimmed = raw.trim();
        let (open, close, delimiter) = if trimmed.starts_with("$$") && trimmed.ends_with("$$") {
            ("$$", "$$", MathDelimiter::DisplayDollar)
        } else if trimmed.starts_with("\\(") && trimmed.ends_with("\\)") {
            ("\\(", "\\)", MathDelimiter::Parenthesized)
        } else if trimmed.starts_with('$') && trimmed.ends_with('$') && trimmed.len() >= 2 {
            ("$", "$", MathDelimiter::Dollar)
        } else {
            return None;
        };
        let open_at = trimmed.find(open)?;
        let close_at = trimmed.len().checked_sub(close.len())?;
        if close_at < open_at + open.len() {
            return None;
        }
        let body_start = trimmed_start + open_at + open.len();
        let body_end = trimmed_start + close_at;
        Some(Self {
            raw: raw.to_owned(),
            body_range: body_start..body_end,
            delimiter,
        })
    }

    pub(crate) fn body<'a>(&self, raw: &'a str) -> Option<&'a str> {
        raw.get(self.body_range.clone())
    }
}

/// Revision-checked live formula session.
///
/// The block publishes every model mutation immediately. `current_raw` is the
/// exact formula slice acknowledged by the document owner; a session can never
/// derive a write from a different revision or silently replace external input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MathEditSession {
    base_revision: Revision,
    current_raw: String,
    span: MathSourceSpan,
    editor: MathEditor,
}

/// One live source edit derived from the session's acknowledged formula slice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MathSourceEdit {
    pub(crate) range: Range<usize>,
    pub(crate) replacement: String,
    pub(crate) next_raw: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MathEditSessionError {
    InvalidSource,
    StaleSource,
    InvalidBody,
    UnsupportedStructure,
}

impl MathEditSession {
    pub(crate) fn begin(raw: &str, revision: Revision) -> Result<Self, MathEditSessionError> {
        let span = MathSourceSpan::parse(raw).ok_or(MathEditSessionError::InvalidSource)?;
        let body = span.body(raw).ok_or(MathEditSessionError::InvalidBody)?;
        let editable_body = body.trim();
        let editor = MathEditor::new(MathDocument::parse(editable_body.to_owned()));
        if editor.document().support_level() != MathSupportLevel::Structured {
            return Err(MathEditSessionError::UnsupportedStructure);
        }
        Ok(Self {
            base_revision: revision,
            current_raw: raw.to_owned(),
            span,
            editor,
        })
    }

    pub(crate) fn document(&self) -> &MathDocument {
        self.editor.document()
    }

    #[cfg(test)]
    pub(crate) fn document_mut(&mut self) -> &mut MathDocument {
        self.editor.document_mut()
    }

    /// Applies one command to the sole formula model. The owning block must
    /// immediately publish [`Self::source_edit`] before accepting more input.
    pub(crate) fn execute(
        &mut self,
        command: MathEditCommand,
    ) -> Result<MathEditResult, MathEditError> {
        self.editor.execute(command)
    }

    pub(crate) fn editor(&self) -> &MathEditor {
        &self.editor
    }

    pub(crate) fn editor_mut(&mut self) -> &mut MathEditor {
        &mut self.editor
    }

    pub(crate) fn move_cursor_horizontal_with_selection(
        &mut self,
        direction: i32,
        extend_selection: bool,
    ) -> Result<bool, MathEditError> {
        let anchor = self.editor.selection().anchor().clone();
        let mut cursor = self.editor.cursor().clone();
        let changed = cursor.move_horizontal(self.editor.document(), direction)?;
        if changed {
            if extend_selection {
                self.editor
                    .set_selection(MathSelection::new(anchor, cursor))?;
            } else {
                self.editor.set_cursor(cursor)?;
            }
        }
        Ok(changed)
    }

    pub(crate) fn move_cursor_vertical_with_selection(
        &mut self,
        direction: i32,
        extend_selection: bool,
    ) -> Result<bool, MathEditError> {
        let anchor = self.editor.selection().anchor().clone();
        let mut cursor = self.editor.cursor().clone();
        let changed = cursor.move_vertical(self.editor.document(), direction)?;
        if changed {
            if extend_selection {
                self.editor
                    .set_selection(MathSelection::new(anchor, cursor))?;
            } else {
                self.editor.set_cursor(cursor)?;
            }
        }
        Ok(changed)
    }

    pub(crate) fn move_cursor_environment_slot_with_selection(
        &mut self,
        direction: i32,
        extend_selection: bool,
    ) -> Result<bool, MathEditError> {
        let anchor = self.editor.selection().anchor().clone();
        let mut cursor = self.editor.cursor().clone();
        let changed = cursor.move_slot(self.editor.document(), direction)?;
        if changed {
            if extend_selection {
                self.editor
                    .set_selection(MathSelection::new(anchor, cursor))?;
            } else {
                self.editor.set_cursor(cursor)?;
            }
        }
        Ok(changed)
    }

    /// Reconstructs the formula with its original delimiters and surrounding
    /// bytes so a preview never has to invent a second source serializer.
    #[cfg(test)]
    pub(crate) fn preview_raw(&self) -> String {
        let body = self.span.body(&self.current_raw).unwrap_or_default();
        let leading = body.len() - body.trim_start().len();
        let trailing = body.len() - body.trim_end().len();
        let mut raw = self.current_raw.clone();
        let start = self.span.body_range.start + leading;
        let end = self.span.body_range.end.saturating_sub(trailing);
        if start <= end && end <= raw.len() {
            raw.replace_range(start..end, &self.editor.document().to_latex());
        }
        raw
    }

    /// Render-only formula source with empty semantic slots projected as
    /// `\square`. The placeholder bytes never enter [`Self::source_edit`].
    pub(crate) fn visual_preview_raw(&self) -> String {
        let body = self.span.body(&self.current_raw).unwrap_or_default();
        let leading = body.len() - body.trim_start().len();
        let trailing = body.len() - body.trim_end().len();
        let projection = gmark_math_edit::MathVisualProjection::from_document(self.document());
        let mut raw = self.current_raw.clone();
        let start = self.span.body_range.start + leading;
        let end = self.span.body_range.end.saturating_sub(trailing);
        if start <= end && end <= raw.len() {
            raw.replace_range(start..end, projection.render_latex());
        }
        raw
    }

    /// Acknowledges a source transaction emitted by this session while keeping
    /// the current semantic cursor and selection intact.
    pub(crate) fn acknowledge_local_publish(
        &mut self,
        revision: Revision,
        current_raw: &str,
    ) -> Result<(), MathEditSessionError> {
        let span = MathSourceSpan::parse(current_raw).ok_or(MathEditSessionError::InvalidSource)?;
        let body = span
            .body(current_raw)
            .ok_or(MathEditSessionError::InvalidBody)?;
        if body.trim() != self.editor.document().to_latex() {
            return Err(MathEditSessionError::StaleSource);
        }
        self.base_revision = revision;
        self.current_raw = current_raw.to_owned();
        self.span = span;
        Ok(())
    }

    /// Produces the next formula-slice replacement against the exact source
    /// revision last acknowledged by the document owner.
    pub(crate) fn source_edit(
        &self,
        current_revision: Revision,
        current_raw: &str,
    ) -> Result<MathSourceEdit, MathEditSessionError> {
        if current_revision != self.base_revision || current_raw != self.current_raw {
            return Err(MathEditSessionError::StaleSource);
        }
        let source_body = self
            .span
            .body(current_raw)
            .ok_or(MathEditSessionError::InvalidBody)?;
        let trimmed = source_body.trim();
        let range = if trimmed.is_empty() {
            // Empty display-math bodies often contain line breaks used only for
            // layout. Keep those bytes intact and insert a newly authored
            // formula at the end of the body instead of constructing an
            // inverted `start..end` range when leading/trailing whitespace
            // overlap.
            self.span.body_range.end..self.span.body_range.end
        } else {
            let leading = source_body.len() - source_body.trim_start().len();
            self.span.body_range.start + leading
                ..self.span.body_range.start + leading + trimmed.len()
        };
        let replacement = self.editor.document().to_latex();
        let mut next_raw = current_raw.to_owned();
        next_raw.replace_range(range.clone(), &replacement);
        Ok(MathSourceEdit {
            range,
            replacement,
            next_raw,
        })
    }

    /// Rehydrates after a controlled document-history restore. External source
    /// changes use a new session instead of mutating a stale one in place.
    pub(crate) fn reload_authoritative(
        &mut self,
        raw: &str,
        revision: Revision,
    ) -> Result<(), MathEditSessionError> {
        let next = Self::begin(raw, revision)?;
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/editor/math_edit.rs"]
mod tests;
