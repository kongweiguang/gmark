// @author kongweiguang

//! Undo and redo coordination.

use super::*;

impl DocumentHost {
    pub(crate) fn on_undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving || self.reloading {
            return;
        }
        if self
            .document
            .as_mut()
            .is_some_and(|document| document.undo())
        {
            let restored_selection = self
                .document
                .as_ref()
                .map(DocumentSession::source_selection);
            if let (Some(journal), Some(document)) = (
                self.coordinator.recovery_journal.as_mut(),
                self.document.as_ref(),
            ) && let Err(error) = journal.record_after_change(
                document,
                &RecoveryRecord {
                    action: RecoveryAction::Undo,
                    selection: restored_selection,
                    view_id: DocumentViewId::source(),
                },
            ) {
                self.coordinator.recovery_error = Some(error.to_string().into());
            }
            self.active_edit = None;
            if let Some(selection) = restored_selection {
                self.set_source_selection(selection, cx);
            }
            self.focus_handle.focus(window);
            self.invalidate_source_rows();
            let dirty = self
                .document
                .as_ref()
                .is_some_and(|document| !document.is_pristine());
            set_document_dirty_state(&mut self.document, &mut self.pending_dirty, dirty);
            self.schedule_search(cx);
            let preserve_live_table = self.is_delimited_document()
                && matches!(
                    self.view_mode,
                    DocumentHostViewMode::Live | DocumentHostViewMode::Split
                )
                && self.structured_index.is_some();
            if preserve_live_table {
                self.structured_pending = None;
                self.structured_cell_overrides.clear();
                self.structured_cell_source_edits.clear();
                self.schedule_delimited_snapshot_rebuild(cx);
                self.clear_structure_error();
            } else if dirty {
                self.structured_index = None;
                self.invalidate_structured_runtime();
            } else {
                self.rebuild_clean_structured_index(cx);
            }
            self.schedule_json_graph_projection(cx);
            if dirty && !preserve_live_table {
                self.schedule_delimited_snapshot_rebuild(cx);
            }
            cx.emit(DocumentHostEvent::StateChanged);
            cx.notify();
        }
    }

    pub(crate) fn on_redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving || self.reloading {
            return;
        }
        if self
            .document
            .as_mut()
            .is_some_and(|document| document.redo())
        {
            let restored_selection = self
                .document
                .as_ref()
                .map(DocumentSession::source_selection);
            if let (Some(journal), Some(document)) = (
                self.coordinator.recovery_journal.as_mut(),
                self.document.as_ref(),
            ) && let Err(error) = journal.record_after_change(
                document,
                &RecoveryRecord {
                    action: RecoveryAction::Redo,
                    selection: restored_selection,
                    view_id: DocumentViewId::source(),
                },
            ) {
                self.coordinator.recovery_error = Some(error.to_string().into());
            }
            self.active_edit = None;
            if let Some(selection) = restored_selection {
                self.set_source_selection(selection, cx);
            }
            self.focus_handle.focus(window);
            self.invalidate_source_rows();
            let dirty = self
                .document
                .as_ref()
                .is_some_and(|document| !document.is_pristine());
            set_document_dirty_state(&mut self.document, &mut self.pending_dirty, dirty);
            let preserve_live_table = self.is_delimited_document()
                && matches!(
                    self.view_mode,
                    DocumentHostViewMode::Live | DocumentHostViewMode::Split
                )
                && self.structured_index.is_some();
            if preserve_live_table {
                self.structured_pending = None;
                self.structured_cell_overrides.clear();
                self.structured_cell_source_edits.clear();
            } else {
                self.structured_index = None;
                self.invalidate_structured_runtime();
            }
            self.clear_structure_error();
            self.schedule_search(cx);
            self.schedule_json_graph_projection(cx);
            self.schedule_delimited_snapshot_rebuild(cx);
            if preserve_live_table {
                self.clear_structure_error();
            }
            cx.emit(DocumentHostEvent::StateChanged);
            cx.notify();
        }
    }
}
