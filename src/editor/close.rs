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
    pub(in crate::editor) fn discard_current_document_changes(&mut self, cx: &mut Context<Self>) {
        self.checkpoint_recovery_journal();
        if let Some(document_host) = self.document_host.clone() {
            document_host.update(cx, |host, cx| host.discard_unsaved_changes(cx));
        } else {
            // Resident 文档以 session dirty 为真值；只清 UI 缓存会让关闭序列立即重新弹框。
            self.source_document.mark_persisted();
        }
        self.document_dirty = false;
    }

    /// 窗口级“放弃并关闭”承诺丢弃整个窗口的编辑，不能逐标签重复同一弹窗。
    pub(in crate::editor) fn discard_all_document_changes_for_window_close(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let tab_count = self.window_tab_count();
        for _ in 0..tab_count {
            self.discard_current_document_changes(cx);
            if !self.activate_dirty_tab_for_window_close(cx) {
                return true;
            }
        }

        debug_assert!(
            false,
            "discarding every tab must clear the window dirty state"
        );
        false
    }

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
        crate::updater::UpdateCoordinator::cancel_pending_install(cx);
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
        self.pending_window_edited = false;
        if self.discard_all_document_changes_for_window_close(cx) {
            self.remove_workspace_session_for_explicit_close(cx);
            window.remove_window();
        } else {
            self.show_unsaved_changes_dialog = true;
            self.close_dialog_restore_focus = None;
            window.blur();
            cx.notify();
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
