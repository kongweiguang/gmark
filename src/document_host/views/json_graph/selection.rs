// @author kongweiguang

//! JSON graph selection, search navigation, and Source synchronization.

use super::support::{expand_ancestors, search_reveal_row_limit};
use super::*;

impl DocumentHost {
    pub(in crate::document_host::implementation) fn select_json_graph_item(
        &mut self,
        id: JsonGraphItemId,
        source: Range<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.graph_focus_handle.focus(window);
        self.graph_selected_item = Some(id.clone());
        document_view_state_mut(&mut self.document, &mut self.tab_view_state)
            .derived
            .entry(DocumentViewId::json_graph())
            .or_default()
            .selected_item = Some(Arc::from(id.as_str()));
        if self.view_mode == DocumentHostViewMode::Split {
            self.select_json_source_range(source, true, cx);
        }
        cx.notify();
    }

    pub(in crate::document_host::implementation) fn dismiss_json_graph_details(&mut self) {
        self.graph_selected_item = None;
        if let Some(state) = document_view_state_mut(&mut self.document, &mut self.tab_view_state)
            .derived
            .get_mut(&DocumentViewId::json_graph())
        {
            state.selected_item = None;
        }
    }

    pub(in crate::document_host::implementation) fn navigate_json_graph_search(
        &mut self,
        delta: i32,
        cx: &mut Context<Self>,
    ) {
        if self.graph_search_matches.is_empty() {
            return;
        }
        let len = self.graph_search_matches.len();
        self.graph_search_selected = if delta < 0 {
            (self.graph_search_selected + len - 1) % len
        } else {
            (self.graph_search_selected + 1) % len
        };
        let selected = self.graph_search_matches[self.graph_search_selected].clone();
        self.graph_selected_item = Some(selected.clone());
        self.graph_pending_center = Some(selected.clone());
        self.reveal_graph_item(&selected);
        cx.notify();
    }

    pub(in crate::document_host::implementation) fn reveal_graph_item(
        &mut self,
        selected: &JsonGraphItemId,
    ) {
        let Some(graph) = self
            .derived_projection_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.as_any().downcast_ref::<JsonGraphSnapshot>())
            .map(JsonGraphSnapshot::projection)
        else {
            return;
        };
        // 搜索可以命中高密度卡片中尚未构造的行；先提升该卡片的运行时行预算，
        // 再展开祖先，保证随后布局得到真实端口并能把命中项居中。
        if let Some((parent, required)) = search_reveal_row_limit(graph, selected) {
            let limit = self
                .graph_row_limits
                .entry(parent)
                .or_insert(model::DEFAULT_ROW_LIMIT);
            if required > *limit {
                *limit = required;
                self.graph_layout_cache = None;
            }
        }
        let state = document_view_state_mut(&mut self.document, &mut self.tab_view_state)
            .derived
            .entry(DocumentViewId::json_graph())
            .or_default();
        expand_ancestors(graph, selected, &mut state.collapsed_items);
    }

    pub(in crate::document_host::implementation) fn select_json_source_range(
        &mut self,
        range: Range<u64>,
        preserve_split: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let len = document.len();
        let start = range.start.min(len);
        let end = range.end.min(len).max(start);
        let _ = document.set_selection(start..end, false);
        let line = document
            .line_for_offset(start)
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or_default();
        self.selection_anchor = Some(line);
        self.selected_lines = Some(line..line.saturating_add(1));
        self.anchor_source_window_for_byte(line as u64, start);
        self.scroll_source_line(line, ScrollStrategy::Center);
        if !preserve_split {
            self.view_mode = DocumentHostViewMode::Source;
            self.sync_tab_active_view();
        }
        cx.notify();
    }
}
