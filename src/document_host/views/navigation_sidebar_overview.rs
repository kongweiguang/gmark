// @author kongweiguang

//! Sidebar overview, column navigation, and format summary.

use super::*;
use crate::theme::{Theme, ThemeColors};
use gpui::{AnyElement, WeakEntity};

impl DocumentHost {
    /// Resolve a stable navigation target against the current host state.
    /// Stale rows are ignored by the existing structured/source epoch checks.
    pub(crate) fn reveal_document_sidebar_target(
        &mut self,
        target: DocumentSidebarTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            DocumentSidebarTarget::Column { column } => {
                self.reveal_document_sidebar_column(column, window, cx)
            }
            DocumentSidebarTarget::StructuredRow { row, offset, json } => {
                self.reveal_document_sidebar_structure(row, offset, json, cx)
            }
        }
    }

    /// 渲染右侧文档导航的只读投影。条目只读取当前已建立的结构索引，正文与主视图仍由
    /// DocumentHost 独占；后台索引尚未完成时保持稳定的 loading 状态。
    pub(crate) fn render_document_sidebar<SidebarHost>(
        &mut self,
        theme: &Theme,
        strings: &I18nStrings,
        _sidebar_host: &WeakEntity<SidebarHost>,
        cx: &mut Context<Self>,
    ) -> AnyElement
    where
        SidebarHost: 'static,
    {
        let colors = &theme.colors;
        let format = self.document_sidebar_snapshot().format;
        if self.is_paged_document() {
            return self.render_document_sidebar_info(colors, strings, cx);
        }
        match format {
            DocumentMenuFormat::Csv | DocumentMenuFormat::Tsv => {
                self.render_document_sidebar_columns(colors, strings, cx)
            }
            DocumentMenuFormat::Json | DocumentMenuFormat::JsonLines => {
                self.render_document_sidebar_structure(colors, strings, cx)
            }
            DocumentMenuFormat::Markdown | DocumentMenuFormat::Text => {
                self.render_document_sidebar_info(colors, strings, cx)
            }
        }
    }

    pub(super) fn render_document_sidebar_info(
        &self,
        colors: &ThemeColors,
        strings: &I18nStrings,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let format = match self.document_menu_format() {
            DocumentMenuFormat::Text => strings.document_sidebar_text.clone(),
            format => format.label(false).to_owned(),
        };
        let rows = [
            (strings.document_sidebar_format.clone(), format),
            (
                strings.document_sidebar_size.clone(),
                format_byte_count(self.probe.len),
            ),
            (
                strings.document_sidebar_lines.clone(),
                self.document_line_count().to_string(),
            ),
            (
                strings.document_sidebar_encoding.clone(),
                self.encoding_label(),
            ),
            (
                strings.document_sidebar_line_endings.clone(),
                self.document_line_ending_label(),
            ),
        ];
        div()
            .id("document-sidebar-info")
            .debug_selector(|| "document-sidebar-info".to_owned())
            .w_full()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(rows.into_iter().map(|(label, value)| {
                let selector_label = label.clone();
                div()
                    .id(SharedString::from(format!("document-sidebar-info-{label}")))
                    .debug_selector(move || format!("document-sidebar-info-{selector_label}"))
                    .w_full()
                    .min_h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(px(6.0))
                    .text_size(px(12.0))
                    .child(div().text_color(colors.text_placeholder).child(label))
                    .child(
                        div()
                            .max_w(px(150.0))
                            .overflow_hidden()
                            .truncate()
                            .text_color(colors.text_default)
                            .child(value),
                    )
            }))
            .into_any_element()
    }

    fn render_document_sidebar_columns(
        &mut self,
        colors: &ThemeColors,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.is_paged_document() || self.structure_error.is_some() {
            return self.render_document_sidebar_info(colors, strings, cx);
        }
        let Some(StructuredIndex::Delimited(index)) = self.structured_index.as_ref() else {
            return self.render_document_sidebar_loading(colors, strings);
        };
        let headers = index.headers().to_vec();
        if headers.is_empty() {
            return self.render_document_sidebar_empty(colors, strings);
        }
        let row_count = index.record_count();
        let mut body = div()
            .id("document-sidebar-columns")
            .debug_selector(|| "document-sidebar-columns".to_owned())
            .w_full()
            .flex()
            .flex_col()
            .gap(px(2.0));
        for (column, header) in headers.into_iter().enumerate() {
            let label = if header.trim().is_empty() {
                strings
                    .document_sidebar_column_fallback
                    .replace("{count}", &(column + 1).to_string())
            } else {
                header
            };
            body = body.child(
                div()
                    .id(SharedString::from(format!(
                        "document-sidebar-column-{column}"
                    )))
                    .debug_selector(move || format!("document-sidebar-column-{column}"))
                    .w_full()
                    .min_h(px(30.0))
                    .tab_index(0)
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(7.0))
                    .hover(|this| this.bg(colors.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .child(
                        div()
                            .w(px(24.0))
                            .flex_shrink_0()
                            .text_color(colors.text_placeholder)
                            .child(format!("{}", column + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .truncate()
                            .text_color(colors.text_default)
                            .child(label),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.reveal_document_sidebar_target(
                            DocumentSidebarTarget::Column { column },
                            window,
                            cx,
                        );
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.reveal_document_sidebar_target(
                                DocumentSidebarTarget::Column { column },
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }
                    })),
            );
        }
        body = body.child(
            div()
                .id("document-sidebar-column-count")
                .debug_selector(|| "document-sidebar-column-count".to_owned())
                .mt(px(8.0))
                .px(px(8.0))
                .text_size(px(11.0))
                .text_color(colors.text_placeholder)
                .child(
                    strings
                        .document_sidebar_rows_template
                        .replace("{count}", &row_count.to_string()),
                ),
        );
        body.into_any_element()
    }
}

fn format_byte_count(bytes: u64) -> String {
    const KIB: u64 = 1_024;
    const MIB: u64 = KIB * 1_024;
    const GIB: u64 = MIB * 1_024;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}
