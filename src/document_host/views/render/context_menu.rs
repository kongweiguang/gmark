// @author kongweiguang

//! Source-surface context menu rendering.

use super::*;

impl DocumentHost {
    pub(super) fn render_source_context_menu(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.source_context_menu.map(|position| {
            let theme = cx.global::<ThemeManager>().current_arc();
            let strings = cx.global::<I18nManager>().strings_arc();
            let colors = &theme.colors;
            let selected_bytes = self
                .selected_source_byte_range()
                .map(|range| range.end.saturating_sub(range.start));
            let has_selection = selected_bytes.is_some_and(|bytes| bytes > 0);
            let cut_enabled = selected_bytes.is_some_and(|bytes| {
                selection_transfer_for_len(bytes) == SelectionTransfer::Clipboard
            });
            let menu_width = 190.0;
            let menu_height = 259.0;
            let left =
                f32::from(position.x).clamp(8.0, (viewport_width - menu_width - 8.0).max(8.0));
            let top =
                f32::from(position.y).clamp(8.0, (viewport_height - menu_height - 8.0).max(8.0));
            let item =
                |id: &'static str, label: String, command: SourceContextCommand, enabled: bool| {
                    div()
                        .id(id)
                        .debug_selector(move || id.to_owned())
                        .h(px(30.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .rounded(px(4.0))
                        .text_color(if enabled {
                            colors.dialog_body
                        } else {
                            colors.text_placeholder
                        })
                        .when(enabled, |row| {
                            row.cursor_pointer()
                                .hover(|row| row.bg(colors.dialog_secondary_button_hover))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.run_source_context_command(command, window, cx);
                                }))
                        })
                        .child(label)
                };
            div()
                .id("document-host-source-context-menu")
                .debug_selector(|| "document-host-source-context-menu".to_owned())
                .key_context(DOCUMENT_HOST_KEY_CONTEXT)
                .tab_index(0)
                .track_focus(&self.source_context_menu_focus_handle)
                .capture_key_down(cx.listener(Self::on_source_surface_key_down))
                .on_action(cx.listener(Self::on_dismiss_transient_ui))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(menu_width))
                .h(px(menu_height))
                .p(px(5.0))
                .flex()
                .flex_col()
                .gap(px(1.0))
                .occlude()
                .rounded(px(6.0))
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .bg(colors.dialog_surface)
                .shadow_lg()
                .child(item(
                    "large-source-context-copy",
                    strings.large_document_text("copy").to_owned(),
                    SourceContextCommand::Copy,
                    has_selection,
                ))
                .child(item(
                    "large-source-context-cut",
                    strings.large_document_text("cut").to_owned(),
                    SourceContextCommand::Cut,
                    cut_enabled,
                ))
                .child(item(
                    "large-source-context-paste",
                    strings.large_document_text("paste").to_owned(),
                    SourceContextCommand::Paste,
                    true,
                ))
                .child(item(
                    "large-source-context-select-all",
                    strings.large_document_text("select_all").to_owned(),
                    SourceContextCommand::SelectAll,
                    true,
                ))
                .child(item(
                    "large-source-context-export",
                    strings.large_document_text("export_selection").to_owned(),
                    SourceContextCommand::ExportSelection,
                    has_selection,
                ))
                .child(item(
                    "large-source-context-export-utf8",
                    strings.large_document_text("export_utf8").to_owned(),
                    SourceContextCommand::ExportSelectionUtf8,
                    has_selection,
                ))
                .child(item(
                    "large-source-context-format-document",
                    "格式化文档".to_owned(),
                    SourceContextCommand::FormatDocument,
                    self.probe.strategy != OpenStrategy::Paged,
                ))
                .child(item(
                    "large-source-context-format-selection",
                    "格式化选区".to_owned(),
                    SourceContextCommand::FormatSelection,
                    has_selection && self.probe.strategy != OpenStrategy::Paged,
                ))
        })
    }
}
