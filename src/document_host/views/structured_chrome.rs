// @author kongweiguang

//! Table switchers, operations, and context-menu controls.

use super::*;
use crate::theme::ThemeColors;

impl DocumentHost {
    pub(super) fn render_markdown_table_switcher(
        &mut self,
        width: f32,
        colors: &ThemeColors,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let structured_width = width;

        match self.structured_index.as_ref() {
            Some(StructuredIndex::MarkdownTables { tables, selected }) if tables.len() > 1 => {
                let selected = *selected;
                let table_count = tables.len();
                let previous = selected.saturating_sub(1);
                let next = (selected + 1).min(table_count - 1);
                Some(
                    div()
                        .id("document-host-markdown-table-switcher")
                        .debug_selector(|| "document-host-markdown-table-switcher".to_owned())
                        .h(px(34.0))
                        .w(px(structured_width))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .border_b(px(1.0))
                        .border_color(colors.dialog_border)
                        .text_size(px(12.0))
                        .text_color(colors.dialog_muted)
                        .child(
                            div()
                                .id("document-host-markdown-table-previous")
                                .size(px(24.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .text_color(if selected == 0 {
                                    colors.text_placeholder
                                } else {
                                    colors.text_default
                                })
                                .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                                .child("‹")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.select_markdown_table(previous, cx);
                                })),
                        )
                        .child(
                            div()
                                .id("document-host-markdown-table-position")
                                .min_w(px(92.0))
                                .text_align(gpui::TextAlign::Center)
                                .child(
                                    strings
                                        .large_document_text("table_position_template")
                                        .replace("{current}", &(selected + 1).to_string())
                                        .replace("{total}", &table_count.to_string()),
                                ),
                        )
                        .child(
                            div()
                                .id("document-host-markdown-table-next")
                                .debug_selector(|| "document-host-markdown-table-next".to_owned())
                                .size(px(24.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .text_color(if selected + 1 == table_count {
                                    colors.text_placeholder
                                } else {
                                    colors.text_default
                                })
                                .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                                .child("›")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.select_markdown_table(next, cx);
                                })),
                        ),
                )
            }
            _ => None,
        }
    }

    pub(super) fn render_structured_operation_bar(
        &mut self,
        colors: &ThemeColors,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let column_progress = self
            .structured_column_progress
            .as_ref()
            .map(|(processed, total)| (processed.load(Ordering::Relaxed), *total));
        (column_progress.is_some() || !self.hidden_structured_columns.is_empty()).then(|| {
            div()
                .id("document-host-structured-operation-bar")
                .debug_selector(|| "document-host-structured-operation-bar".to_owned())
                .h(px(34.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .border_b(px(1.0))
                .border_color(colors.dialog_border)
                .when_some(column_progress, |bar, (processed, total)| {
                    bar.child(
                        strings
                            .large_document_text("updating_columns_progress_template")
                            .replace("{processed}", &processed.to_string())
                            .replace("{total}", &total.to_string()),
                    )
                    .child(
                        div()
                            .id("document-host-cancel-column-update")
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .bg(colors.dialog_secondary_button_bg)
                            .child(strings.large_document_text("cancel").to_owned())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_delimited_column_transform(cx)
                            })),
                    )
                })
                .when(!self.hidden_structured_columns.is_empty(), |bar| {
                    bar.child(
                        div()
                            .id("document-host-show-all-columns")
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .bg(colors.dialog_secondary_button_bg)
                            .child(strings.large_document_text("show_all_columns").to_owned())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.hidden_structured_columns.clear();
                                cx.notify();
                            })),
                    )
                })
        })
    }

    pub(super) fn render_structured_add_row(
        &mut self,
        structured_live: bool,
        colors: &ThemeColors,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        structured_live.then(|| {
            let row_count = self
                .structured_index
                .as_ref()
                .map_or(0, StructuredIndex::row_count);
            div()
                .id("document-host-structured-add-row")
                .debug_selector(|| "document-host-structured-add-row".to_owned())
                .h(px(30.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .border_t(px(1.0))
                .border_color(colors.dialog_border)
                .cursor_pointer()
                .text_color(colors.text_link)
                .child(strings.large_document_text("add_row").to_owned())
                .on_click(
                    cx.listener(move |this, _, _, cx| this.insert_delimited_row(row_count, cx)),
                )
        })
    }

    pub(super) fn render_structured_context_menu(
        &mut self,
        colors: &ThemeColors,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        self.structured_context_target.map(|target| {
            let row_count = self
                .structured_index
                .as_ref()
                .map_or(0, StructuredIndex::row_count);
            let menu_item = |key: &'static str| {
                div()
                    .id(key)
                    .debug_selector(move || format!("document-host-structured-menu-{key}"))
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .hover(|item| item.bg(colors.dialog_secondary_button_hover))
                    .child(strings.large_document_text(key).to_owned())
            };
            let menu = div()
                .id("document-host-structured-context-menu")
                .debug_selector(|| "document-host-structured-context-menu".to_owned())
                .absolute()
                .top(px(62.0))
                .left(px(82.0))
                .w(px(178.0))
                .p(px(4.0))
                .rounded(px(6.0))
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .bg(colors.dialog_surface)
                .shadow_md();
            match target {
                StructuredMenuTarget::Row(row) => menu
                    .child(menu_item("insert_row_before").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.insert_delimited_row(row, cx);
                        },
                    )))
                    .child(menu_item("insert_row_after").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.insert_delimited_row((row + 1).min(row_count), cx);
                        },
                    )))
                    .child(
                        menu_item("delete_row").on_click(cx.listener(move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.delete_delimited_row(row, cx);
                        })),
                    ),
                StructuredMenuTarget::Column(column) => menu
                    .child(menu_item("insert_column_before").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.transform_delimited_column(
                                DelimitedEdit::InsertColumn {
                                    before: column,
                                    header: cx
                                        .global::<I18nManager>()
                                        .strings()
                                        .large_document_text("default_column_template")
                                        .replace("{number}", &(column + 1).to_string()),
                                },
                                cx,
                            );
                        },
                    )))
                    .child(menu_item("insert_column_after").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.transform_delimited_column(
                                DelimitedEdit::InsertColumn {
                                    before: column + 1,
                                    header: cx
                                        .global::<I18nManager>()
                                        .strings()
                                        .large_document_text("default_column_template")
                                        .replace("{number}", &(column + 2).to_string()),
                                },
                                cx,
                            );
                        },
                    )))
                    .child(menu_item("delete_column").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.transform_delimited_column(
                                DelimitedEdit::DeleteColumn { column },
                                cx,
                            );
                        },
                    )))
                    .child(menu_item("hide_column").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.structured_context_target = None;
                            this.hidden_structured_columns.insert(column);
                            cx.notify();
                        },
                    ))),
            }
        })
    }
}
