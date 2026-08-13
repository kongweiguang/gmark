// @author kongweiguang

use super::*;

impl Editor {
    pub(crate) fn pinned_tab_count(&self) -> usize {
        self.tabs
            .records
            .iter()
            .take_while(|record| record.pinned)
            .count()
    }

    pub(in crate::editor) fn toggle_pin_tab(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if index >= self.tabs.records.len() {
            return false;
        }
        let active_id = self.tabs.records[self.tabs.active].id;
        self.tabs.records[index].pinned = !self.tabs.records[index].pinned;
        // 固定标签始终构成稳定前缀；稳定排序保留每个分区内用户定义的视觉顺序。
        self.tabs.records.sort_by_key(|record| !record.pinned);
        self.tabs.active = self
            .tabs
            .records
            .iter()
            .position(|record| record.id == active_id)
            .expect("active tab must survive pin reorder");
        self.schedule_workspace_session_save(cx);
        cx.notify();
        true
    }

    pub(crate) fn reorder_tab(
        &mut self,
        source: usize,
        target: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if source >= self.tabs.records.len()
            || target >= self.tabs.records.len()
            || source == target
        {
            return false;
        }
        let active_id = self.tabs.records[self.tabs.active].id;
        let source_pinned = self.tabs.records[source].pinned;
        let pinned_count = self.pinned_tab_count();
        let allowed = if source_pinned {
            0..pinned_count
        } else {
            pinned_count..self.tabs.records.len()
        };
        let target = target.clamp(allowed.start, allowed.end.saturating_sub(1));
        if source == target {
            return false;
        }
        let record = self.tabs.records.remove(source);
        self.tabs.records.insert(target, record);
        self.tabs.active = self
            .tabs
            .records
            .iter()
            .position(|record| record.id == active_id)
            .expect("active tab must survive drag reorder");
        self.schedule_workspace_session_save(cx);
        cx.notify();
        true
    }

    pub(in crate::editor) fn request_close_other_tabs(
        &mut self,
        keep_index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(keep) = self.tabs.records.get(keep_index).map(|record| record.id) else {
            return;
        };
        self.tabs.close_others_keep = Some(keep);
        self.advance_close_other_tabs(cx);
    }

    pub(super) fn advance_close_other_tabs(&mut self, cx: &mut Context<Self>) {
        let Some(keep) = self.tabs.close_others_keep else {
            return;
        };
        loop {
            let Some(index) = self
                .tabs
                .records
                .iter()
                .position(|record| record.id != keep)
            else {
                self.tabs.close_others_keep = None;
                if let Some(keep_index) = self
                    .tabs
                    .records
                    .iter()
                    .position(|record| record.id == keep)
                {
                    self.switch_to_tab_index(keep_index, cx);
                }
                cx.notify();
                return;
            };
            let before = self.tabs.records.len();
            self.request_close_tab_index(index, cx);
            if self.tabs.show_close_dialog || self.tabs.records.len() == before {
                return;
            }
        }
    }
}
