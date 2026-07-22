// @author kongweiguang

//! Unsaved-changes dialog and window-close interception.
//!
//! When the document is dirty, `Editor::on_window_should_close` returns
//! false and shows an overlay offering three choices: save-and-close,
//! discard-and-close, or keep editing.  Focus is restored to the
//! previously active block when the dialog is dismissed without closing.

use gpui::*;

use super::Editor;

impl Editor {
    pub(crate) fn request_close_current_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_menu_bar(cx);
        self.hide_info_dialog(cx);
        self.pending_close_after_save = false;

        if self.on_window_should_close(window, cx) {
            self.close_dialog_restore_focus = None;
            window.remove_window();
        }
    }

    pub(crate) fn restore_focus_after_close_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(focus_id) = self.close_dialog_restore_focus.take() {
            self.pending_focus = Some(focus_id);
            self.pending_scroll_active_block_into_view = true;
            cx.notify();
        }
    }

    pub(crate) fn hide_unsaved_changes_dialog(&mut self, cx: &mut Context<Self>) {
        if self.show_unsaved_changes_dialog {
            self.show_unsaved_changes_dialog = false;
            cx.notify();
        }
    }

    pub(crate) fn abort_pending_close_after_save(&mut self, cx: &mut Context<Self>) {
        let had_pending_close = self.pending_close_after_save;
        self.pending_close_after_save = false;
        self.cancel_explicit_window_close();
        self.abort_window_close_tab_sequence(cx);
        self.close_menu_bar(cx);
        self.hide_unsaved_changes_dialog(cx);
        if had_pending_close {
            self.restore_focus_after_close_dialog(cx);
        } else {
            self.close_dialog_restore_focus = None;
        }
    }

    pub(crate) fn on_window_should_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.mark_explicit_window_close(true);
        let should_close = self.evaluate_window_should_close(window, cx);
        if should_close {
            self.remove_workspace_session_for_explicit_close(cx);
        }
        should_close
    }

    pub(crate) fn on_window_should_close_for_quit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.mark_explicit_window_close(false);
        self.last_selection_snapshot = self.capture_source_selection_snapshot(cx);
        self.persist_workspace_session_before_quit(cx);
        self.evaluate_window_should_close(window, cx)
    }

    fn evaluate_window_should_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.show_external_conflict_dialog {
            return false;
        }
        if !self.activate_dirty_tab_for_window_close(cx) {
            return true;
        }

        self.close_menu_bar(cx);
        self.hide_info_dialog(cx);
        if !self.show_unsaved_changes_dialog {
            self.close_dialog_restore_focus = self.document.focused_block_entity_id(window, cx);
            self.show_unsaved_changes_dialog = true;
            window.blur();
            cx.notify();
        }

        false
    }

    pub(crate) fn on_cancel_close_dialog(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_close_after_save = false;
        self.cancel_explicit_window_close();
        self.abort_window_close_tab_sequence(cx);
        self.close_menu_bar(cx);
        self.hide_unsaved_changes_dialog(cx);
        self.restore_focus_after_close_dialog(cx);
    }

    pub(crate) fn on_discard_and_close(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_close_after_save = false;
        self.close_dialog_restore_focus = None;
        self.close_menu_bar(cx);
        self.hide_unsaved_changes_dialog(cx);
        self.checkpoint_recovery_journal();
        self.document_dirty = false;
        self.pending_window_edited = false;
        if self.activate_dirty_tab_for_window_close(cx) {
            self.show_unsaved_changes_dialog = true;
            self.close_dialog_restore_focus = None;
            window.blur();
            cx.notify();
        } else {
            self.remove_workspace_session_for_explicit_close(cx);
            window.remove_window();
        }
    }

    pub(crate) fn on_save_and_close(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_close_after_save = self.prepare_window_close_save();
        self.close_menu_bar(cx);
        self.hide_unsaved_changes_dialog(cx);
        self.pending_save = true;
        cx.notify();
    }
}
