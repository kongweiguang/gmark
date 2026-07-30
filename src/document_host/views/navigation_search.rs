// @author kongweiguang

//! Search and source paging navigation.

use super::*;

impl DocumentHost {
    pub(super) fn jump_to_search_result(&mut self, cx: &mut Context<Self>) {
        let Some(found_start) = self
            .search_results
            .get(self.search_selected)
            .map(|found| found.range.start)
        else {
            return;
        };
        let line = if let Some(document) = self.document.as_ref() {
            let Some(line) = document
                .line_for_offset(found_start)
                .and_then(|line| usize::try_from(line).ok())
            else {
                return;
            };
            self.anchor_source_window_for_byte(line as u64, found_start);
            line
        } else {
            let estimated = self.probe.estimated_lines.max(1);
            let line = ((found_start as u128 * estimated as u128) / self.probe.len.max(1) as u128)
                .min(usize::MAX as u128) as usize;
            self.source_window_start = 0;
            self.invalidate_source_rows();
            line.min(self.line_count().saturating_sub(1))
        };
        // CSV/TSV 的全文搜索仍以 Source 字节坐标为真值，但命中不能夺走用户当前的
        // 表格工作区；Source 选择留作随后切换或 Split 左栏同步使用。
        let keep_delimited_table = self.is_delimited_document()
            && matches!(
                self.view_mode,
                DocumentHostViewMode::Live
                    | DocumentHostViewMode::Structure
                    | DocumentHostViewMode::Split
            );
        if !keep_delimited_table {
            self.view_mode = DocumentHostViewMode::Source;
            self.sync_tab_active_view();
        }
        self.select_source_lines(line..line.saturating_add(1), false);
        self.scroll_source_line(line, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn navigate_search(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.search_results.is_empty() {
            return;
        }
        let count = self.search_results.len() as i64;
        self.search_selected =
            (self.search_selected as i64 + i64::from(delta)).rem_euclid(count) as usize;
        self.jump_to_search_result(cx);
    }

    pub(super) fn toggle_search_option(
        &mut self,
        update: impl FnOnce(&mut SearchOptions),
        cx: &mut Context<Self>,
    ) {
        update(&mut self.search_options);
        self.schedule_search(cx);
    }

    pub(crate) fn on_find_in_document(
        &mut self,
        _: &FindInDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation_visible = false;
        self.search_visible = true;
        let host = cx.entity().downgrade();
        self.search_input.update(cx, move |input, _cx| {
            input.set_host_action_handler(move |action, window, cx| {
                let _ = host.update(cx, |view, cx| {
                    view.on_search_host_action(action, window, cx)
                });
            });
            input.focus_handle.focus(window);
        });
        cx.notify();
    }

    pub(crate) fn on_go_to_line(
        &mut self,
        _: &GoToLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_visible = false;
        self.navigation_visible = true;
        let host = cx.entity().downgrade();
        self.navigation_input.update(cx, move |input, _cx| {
            input.set_host_action_handler(move |action, window, cx| {
                let _ = host.update(cx, |view, cx| {
                    view.on_navigation_host_action(action, window, cx)
                });
            });
            let len = input.display_text().len();
            input.selected_range = 0..len;
            input.focus_handle.focus(window);
        });
        cx.notify();
    }

    pub(crate) fn on_find_next(&mut self, _: &FindNext, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate_search(1, cx);
    }

    pub(crate) fn on_find_previous(
        &mut self,
        _: &FindPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_search(-1, cx);
    }

    pub(crate) fn on_dismiss_transient_ui(
        &mut self,
        _: &DismissTransientUi,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.search_visible || self.navigation_visible || self.source_context_menu.is_some() {
            self.search_visible = false;
            self.navigation_visible = false;
            self.source_context_menu = None;
            self.focus_handle.focus(window);
            cx.notify();
        }
    }

    pub(super) fn scroll_page(&mut self, toward_end: bool, cx: &mut Context<Self>) {
        let handle = self.scroll_handle.0.borrow().base_handle.clone();
        let row_height = self.source_row_height.max(1.0);
        let local_top = (-f32::from(handle.offset().y) / row_height)
            .max(0.0)
            .floor() as usize;
        let top = self.source_list_origin.saturating_add(local_top);
        let page_rows = (f32::from(handle.bounds().size.height) / row_height)
            .floor()
            .max(1.0) as usize;
        let target = if toward_end {
            top.saturating_add(page_rows)
                .min(self.line_count().saturating_sub(1))
        } else {
            top.saturating_sub(page_rows)
        };
        // UniformList 的 logical_scroll_top/bottom 只描述当前挂载子树，虚拟列表中会同时
        // 返回 0；必须把稳定行高的像素 offset 映射回全局行，PageUp/Down 才能闭环。
        self.scroll_source_line_strict(target, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn on_page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_page(false, cx);
    }

    pub(super) fn on_page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_page(true, cx);
    }

    pub(super) fn on_jump_to_top(&mut self, _: &JumpToTop, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_source_line_strict(0, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn on_jump_to_bottom(
        &mut self,
        _: &JumpToBottom,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(last) = self.line_count().checked_sub(1) {
            self.scroll_source_line_strict(last, ScrollStrategy::Bottom);
            cx.notify();
        }
    }
}
