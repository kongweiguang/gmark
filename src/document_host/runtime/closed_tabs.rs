// @author kongweiguang

//! Closed-tab suspension and resumption.

use super::*;

impl DocumentHost {
    /// 关闭的标签会暂存在 reopen 栈中，所以不能依赖 Entity Drop 释放任务。
    /// 所有 worker 在这里显式取消并推进代次；保留的 PieceTree、selection 与 ViewState
    /// 仍是纯内存状态，重新打开时可以安全恢复。
    pub(crate) fn suspend_for_closed_tab(&mut self) {
        if self.closed_suspended {
            return;
        }
        debug_assert!(!self.saving && !self.reloading);
        self.closed_suspended = true;
        self.document_epoch = self.document_epoch.wrapping_add(1);

        self.coordinator.lifetime_cancellation.cancel();
        self.coordinator.external_generation = self.coordinator.external_generation.wrapping_add(1);
        self.coordinator.external_task = Task::ready(());

        self.coordinator.index_generation = self.coordinator.index_generation.wrapping_add(1);
        if let Some(cancellation) = self.coordinator.index_cancellation.take() {
            cancellation.cancel();
        }
        self.coordinator.index_task = Task::ready(());

        self.invalidate_source_rows();
        if let Some(cancellation) = self.fold_cancellation.take() {
            cancellation.cancel();
        }
        self.fold_generation = self.fold_generation.wrapping_add(1);
        self.fold_task = Task::ready(());
        self.cancel_source_formatting();
        self.invalidate_structured_runtime();

        self.coordinator.search_generation = self.coordinator.search_generation.wrapping_add(1);
        if let Some(cancellation) = self.coordinator.search_cancellation.take() {
            cancellation.cancel();
        }
        self.search_running = false;
        self.coordinator.search_task = Task::ready(());

        self.coordinator.save.generation = self.coordinator.save.generation.wrapping_add(1);
        if let Some(cancellation) = self.coordinator.save.cancellation.take() {
            cancellation.cancel();
        }
        self.coordinator.save.task = Task::ready(());

        self.source_drag_anchor = None;
        self.source_drag_autoscroll_direction = 0;
        self.source_drag_autoscroll_task = Task::ready(());
        self.cancel_selection_transfers();
    }

    /// 只恢复关闭标签时被挂起的任务。普通标签切换仍保留既有实体，不会重复启动 monitor。
    pub(crate) fn resume_after_closed_tab(&mut self, cx: &mut Context<Self>) {
        if !self.closed_suspended {
            return;
        }
        self.closed_suspended = false;
        self.coordinator.lifetime_cancellation = SearchCancellation::default();
        if self.document.is_none() {
            self.start_initial_index(cx);
        } else if self.structured_index.is_none()
            && !document_dirty_state(&self.document, &self.pending_dirty)
        {
            self.rebuild_clean_structured_index(cx);
        }
        self.start_external_monitor(cx);
        if !self.search_input.read(cx).display_text().is_empty() {
            self.schedule_search(cx);
        }
        cx.notify();
    }
}
