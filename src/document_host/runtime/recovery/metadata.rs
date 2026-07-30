// @author kongweiguang

//! Document metadata and workspace source-state access.

use super::*;

impl DocumentHost {
    /// The probe is the authoritative format source for contextual navigation.
    /// File extensions are insufficient for JSONL/TSV and safe-source fallbacks.
    pub(crate) fn document_menu_format(&self) -> DocumentMenuFormat {
        DocumentMenuFormat::from_document_format(&self.probe.format)
    }

    pub(crate) fn document_length(&self) -> u64 {
        self.probe.len
    }

    pub(crate) fn document_line_count(&self) -> u64 {
        self.line_count() as u64
    }

    pub(crate) fn document_line_ending_label(&self) -> String {
        let Some(summary) = self
            .document
            .as_ref()
            .and_then(DocumentSession::resident_source_document)
            .map(gmark_document::SourceDocument::source_format_summary)
        else {
            // Paged documents intentionally avoid a whole-file scan. The
            // source view still exposes individual endings when requested.
            return "—".to_owned();
        };
        match summary.line_endings {
            gmark_document::LineEndingStatus::None => match summary.dominant {
                gmark_document::LineEnding::Lf => "LF".to_owned(),
                gmark_document::LineEnding::CrLf => "CRLF".to_owned(),
                gmark_document::LineEnding::Cr => "CR".to_owned(),
            },
            gmark_document::LineEndingStatus::Uniform(ending) => match ending {
                gmark_document::LineEnding::Lf => "LF".to_owned(),
                gmark_document::LineEnding::CrLf => "CRLF".to_owned(),
                gmark_document::LineEnding::Cr => "CR".to_owned(),
            },
            gmark_document::LineEndingStatus::Mixed => "Mixed".to_owned(),
        }
    }

    pub(crate) fn is_paged_document(&self) -> bool {
        self.probe.strategy == OpenStrategy::Paged
    }

    pub(crate) fn selection_export_in_progress(&self) -> bool {
        self.selection_export_cancellation.is_some()
    }

    pub(crate) fn has_source_selection(&self) -> bool {
        self.selected_source_byte_range().is_some()
    }

    pub(crate) fn supports_structured_filter(&self) -> bool {
        matches!(
            self.probe.format,
            DocumentFormat::Json | DocumentFormat::JsonLines | DocumentFormat::Delimited { .. }
        )
    }

    pub(crate) fn has_json_graph_selection(&self) -> bool {
        self.probe.format == DocumentFormat::Json && self.graph_selected_item.is_some()
    }

    pub(crate) fn focus_structured_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.supports_structured_filter() {
            return;
        }
        if self.view_mode == DocumentHostViewMode::Source {
            self.show_structure_view(cx);
        }
        let focus_handle = self.structured_filter_input.read(cx).focus_handle.clone();
        focus_handle.focus(window);
        cx.notify();
    }

    pub(crate) fn focus_structured_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_delimited_document() {
            return;
        }
        self.show_live_view(cx);
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub(crate) fn focus_json_inspector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.probe.format != DocumentFormat::Json {
            return;
        }
        self.show_structure_view(cx);
        self.graph_focus_handle.focus(window);
        cx.notify();
    }

    pub(crate) fn restore_workspace_source_state(
        &mut self,
        mut selection: SourceSelection,
        scroll_y: f32,
        cx: &mut Context<Self>,
    ) {
        let len = self.probe.len;
        selection.anchor.byte_offset = selection.anchor.byte_offset.min(len);
        selection.head.byte_offset = selection.head.byte_offset.min(len);
        let state = document_view_state_mut(&mut self.document, &mut self.tab_view_state);
        state.source.selection = selection;
        state.source.top_byte_anchor = selection.head;
        state.source.line_offset_y = scroll_y;
        self.provisional_anchor = Some(selection.head);
        self.scroll_handle
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(0.0), px(scroll_y)));
        cx.notify();
    }

    pub(crate) fn workspace_source_state(&self) -> (SourceSelection, Point<Pixels>) {
        let state = &self.tab_view_state;
        let handle = self.scroll_handle.0.borrow().base_handle.clone();
        let top_line = self.source_list_origin.saturating_add(
            (-f32::from(handle.offset().y) / self.source_row_height.max(1.0))
                .max(0.0)
                .floor() as usize,
        );
        (
            state.source.selection,
            point(
                px(0.0),
                px(-(top_line as f32) * self.source_row_height.max(1.0)),
            ),
        )
    }
}
