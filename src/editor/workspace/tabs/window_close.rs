// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn dirty_tab_index_except_active(&self) -> Option<usize> {
        self.tabs
            .records
            .iter()
            .enumerate()
            .find_map(|(index, record)| {
                (index != self.tabs.active
                    && record
                        .snapshot
                        .as_ref()
                        .is_some_and(|snapshot| snapshot.document_dirty))
                .then_some(index)
            })
    }

    pub(in crate::editor) fn activate_dirty_tab_for_window_close(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.is_document_dirty() {
            return true;
        }
        self.dirty_tab_index_except_active()
            .is_some_and(|index| self.switch_to_tab_index(index, cx))
    }

    pub(in crate::editor) fn window_tab_count(&self) -> usize {
        self.tabs.records.len()
    }

    /// 返回 true 表示当前保存完成后可以直接关闭窗口；false 表示仍需逐个处理后台 dirty 标签。
    pub(in crate::editor) fn prepare_window_close_save(&mut self) -> bool {
        let has_more_dirty_tabs = self.dirty_tab_index_except_active().is_some();
        self.tabs.continue_window_close_after_save = has_more_dirty_tabs;
        self.tabs.close_after_save = false;
        !has_more_dirty_tabs
    }

    pub(in crate::editor) fn continue_window_close_after_save(&mut self, cx: &mut Context<Self>) {
        if !self.tabs.continue_window_close_after_save {
            return;
        }
        self.tabs.continue_window_close_after_save = false;
        if self.activate_dirty_tab_for_window_close(cx) {
            self.show_unsaved_changes_dialog = true;
            self.close_dialog_restore_focus = None;
            cx.notify();
        }
    }

    pub(in crate::editor) fn abort_window_close_tab_sequence(&mut self, cx: &mut Context<Self>) {
        self.cancel_explicit_window_close();
        if self.tabs.continue_window_close_after_save {
            self.tabs.continue_window_close_after_save = false;
            cx.notify();
        }
    }

    pub(super) fn ensure_tab_strip_focus_handles(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (Vec<FocusHandle>, FocusHandle) {
        let live_ids: HashSet<_> = self.tabs.records.iter().map(|record| record.id).collect();
        self.tabs
            .focus_handles
            .retain(|id, _| live_ids.contains(id));
        let handles = self
            .tabs
            .records
            .iter()
            .map(|record| {
                self.tabs
                    .focus_handles
                    .entry(record.id)
                    .or_insert_with(|| cx.focus_handle())
                    .clone()
            })
            .collect();
        let new_tab = self
            .tabs
            .new_tab_focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        (handles, new_tab)
    }

    pub(super) fn focus_tab_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(record) = self.tabs.records.get(index) else {
            return;
        };
        // 标签栏局部导航必须留在 tablist；鼠标点击和全局 Next/Previous 仍沿用编辑器焦点恢复。
        self.pending_focus = None;
        self.tabs
            .focus_handles
            .entry(record.id)
            .or_insert_with(|| cx.focus_handle())
            .focus(window);
    }
}
