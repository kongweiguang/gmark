// @author kongweiguang

//! Transient document status and selection notices.

use super::*;

impl DocumentHost {
    pub(super) fn render_structure_banner(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.structure_error.as_ref().map(|message| {
            let theme = cx.global::<ThemeManager>().current_arc();
            let strings = cx.global::<I18nManager>().strings_arc();
            let colors = &theme.colors;
            let byte_offset = self.structure_error_byte;
            div()
                .id("document-host-structure-notice")
                .debug_selector(|| "document-host-structure-notice".to_owned())
                .h(px(36.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .border_b(px(1.0))
                .border_color(colors.callout_warning_border)
                .bg(colors.callout_warning_bg)
                .text_color(colors.text_default)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .child(message.clone()),
                )
                .children(byte_offset.map(|offset| {
                    div()
                        .id("document-host-structure-error-jump")
                        .debug_selector(|| "document-host-structure-error-jump".to_owned())
                        .px(px(9.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .bg(colors.dialog_secondary_button_bg)
                        .text_color(colors.dialog_secondary_button_text)
                        .child(
                            strings
                                .large_document_text("go_to_byte_template")
                                .replace("{offset}", &offset.to_string()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.jump_byte_offset_to_source(offset, cx);
                        }))
                }))
        })
    }

    pub(super) fn render_oversized_selection_banner(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.selected_source_byte_range()
            .filter(|range| {
                selection_transfer_for_len(range.end.saturating_sub(range.start))
                    == SelectionTransfer::ExportFile
            })
            .map(|range| {
                let theme = cx.global::<ThemeManager>().current_arc();
                let strings = cx.global::<I18nManager>().strings_arc();
                let colors = &theme.colors;
                let selected_mib = (range.end - range.start) as f64 / (1024.0 * 1024.0);
                div()
                    .id("document-host-selection-export-notice")
                    .h(px(36.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .border_b(px(1.0))
                    .border_color(colors.callout_warning_border)
                    .bg(colors.callout_warning_bg)
                    .text_color(colors.text_default)
                    .child(
                        div().flex_1().min_w(px(0.0)).truncate().child(
                            strings
                                .large_document_text("selected_clipboard_template")
                                .replace("{mib}", &format!("{selected_mib:.1}")),
                        ),
                    )
                    .child(
                        div()
                            .id("document-host-export-selection")
                            .px(px(9.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .bg(colors.dialog_primary_button_bg)
                            .text_color(colors.dialog_primary_button_text)
                            .child(strings.large_document_text("export_selection").to_owned())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_export_selection(&ExportSelection, window, cx);
                            })),
                    )
            })
    }

    pub(super) fn render_external_change_banner(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.coordinator.pending_external_change.as_ref().map(|_| {
            let theme = cx.global::<ThemeManager>().current_arc();
            let strings = cx.global::<I18nManager>().strings_arc();
            let colors = &theme.colors;
            div()
                .id("document-host-external-change-banner")
                .debug_selector(|| "document-host-external-change-banner".to_owned())
                .h(px(36.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .border_b(px(1.0))
                .border_color(colors.callout_warning_border)
                .bg(colors.callout_warning_bg)
                .text_color(colors.text_default)
                .child(
                    div().flex_1().min_w(px(0.0)).truncate().child(
                        self.coordinator.external_status.clone().unwrap_or_else(|| {
                            strings.large_document_text("file_changed_disk").into()
                        }),
                    ),
                )
                .child(
                    div()
                        .id("document-host-external-reload")
                        .px(px(9.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(colors.dialog_primary_button_bg)
                        .text_color(colors.dialog_primary_button_text)
                        .child(strings.large_document_text("reload").to_owned())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.reload_from_disk(window, cx);
                        })),
                )
                .child(
                    div()
                        .id("document-host-external-keep-local")
                        .px(px(9.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(colors.dialog_secondary_button_bg)
                        .text_color(colors.dialog_secondary_button_text)
                        .child(strings.large_document_text("keep_local").to_owned())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.keep_local_after_external_change(cx);
                        })),
                )
        })
    }
}
