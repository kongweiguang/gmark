// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn finish_workspace_move(
        &mut self,
        plan: &super::workspace_file_ops::WorkspaceMovePlan,
        active_path: Option<&Path>,
        selection: &UndoSelectionSnapshot,
        view_mode: super::ViewMode,
        cx: &mut Context<Self>,
    ) {
        if let Some(active_path) = active_path {
            let next_path = super::workspace_file_ops::map_moved_path(
                active_path,
                &plan.source,
                &plan.destination,
            );
            if let Some(rewrite) = plan
                .rewrites
                .iter()
                .find(|rewrite| rewrite.before_path == active_path)
            {
                if let Ok(opened) = crate::document_io::decode_markdown_bytes(&rewrite.after) {
                    self.replace_document_from_markdown(opened.text, Some(next_path.clone()), cx);
                    self.source_encoding = opened.encoding;
                    self.set_view_mode(view_mode, cx);
                    self.apply_selection_snapshot_in_current_mode(selection, cx);
                }
            } else if next_path != active_path {
                self.document_kind = DocumentKind::from_path(&next_path);
                self.file_path = Some(next_path.clone());
                self.saved_file_fingerprint = crate::recovery::fingerprint_file(&next_path).ok();
                self.pending_window_title_refresh = true;
                self.restart_file_watcher(cx);
                self.checkpoint_recovery_journal();
                self.sync_workspace_after_document_path_change(cx);
            }
            if self.file_path.as_ref() == Some(&next_path) {
                crate::app_menu::record_recent_file_from_editor(&next_path, cx);
            }
        }
        self.workspace.operation_dialog = None;
        self.workspace.pinned_empty_directories = self
            .workspace
            .pinned_empty_directories
            .iter()
            .map(|path| {
                super::workspace_file_ops::map_moved_path(path, &plan.source, &plan.destination)
            })
            .collect();
        self.workspace.undo_file_operation = Some(WorkspaceUndoOperation::Move(plan.reversed()));
        self.workspace.operation_error = None;
        self.invalidate_workspace_file_tree();
        self.sync_workspace_after_document_path_change(cx);
    }
}
