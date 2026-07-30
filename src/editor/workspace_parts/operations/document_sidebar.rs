// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn render_document_sidebar(
        &mut self,
        theme: &Theme,
        strings: &I18nStrings,
        panel_width: f32,
        resizable: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.focus_mode || !self.workspace.document_sidebar_open {
            return None;
        }

        if self.document_host.is_none() && self.document_kind == super::DocumentKind::Markdown {
            self.sync_workspace_outline(cx);
        }
        let focus_handle = self.ensure_document_sidebar_focus_handle(cx);
        let resize_focus_handle =
            resizable.then(|| self.ensure_document_sidebar_resize_focus_handle(cx));
        let editor = cx.entity().downgrade();
        let resize_editor = editor.clone();
        let c = &theme.colors;
        let d = &theme.dimensions;
        let (title, icon, body) = if let Some(host) = self.document_host.clone() {
            let (format, paged) = {
                let host = host.read(cx);
                (
                    host.document_sidebar_snapshot().format,
                    host.is_paged_document(),
                )
            };
            let (title, icon) = if paged {
                (strings.document_sidebar_info.clone(), "icon/ui/info.svg")
            } else {
                match format {
                    DocumentMenuFormat::Json | DocumentMenuFormat::JsonLines => (
                        strings.document_sidebar_structure.clone(),
                        "icon/ui/code.svg",
                    ),
                    DocumentMenuFormat::Csv | DocumentMenuFormat::Tsv => (
                        strings.document_sidebar_columns.clone(),
                        "icon/ui/table.svg",
                    ),
                    DocumentMenuFormat::Text => {
                        (strings.document_sidebar_info.clone(), "icon/ui/info.svg")
                    }
                    DocumentMenuFormat::Markdown => {
                        (strings.workspace_tab_outline.clone(), OUTLINE_TAB_ICON)
                    }
                }
            };
            let body = if format == DocumentMenuFormat::Markdown && !paged {
                self.sync_workspace_outline(cx);
                div()
                    .id("document-sidebar-markdown-outline")
                    .debug_selector(|| "document-sidebar-markdown-outline".to_owned())
                    .child(self.render_workspace_outline_tree(theme, strings, &editor))
                    .into_any_element()
            } else {
                host.update(cx, |host, cx| {
                    host.render_document_sidebar(theme, strings, &editor, cx)
                })
            };
            (title, icon, body)
        } else {
            let format = match self.document_kind {
                super::DocumentKind::Markdown => DocumentMenuFormat::Markdown,
                super::DocumentKind::Json => DocumentMenuFormat::Json,
                super::DocumentKind::Csv => DocumentMenuFormat::Csv,
                super::DocumentKind::Unspecified => DocumentMenuFormat::Text,
            };
            if format == DocumentMenuFormat::Markdown {
                (
                    strings.workspace_tab_outline.clone(),
                    OUTLINE_TAB_ICON,
                    div()
                        .id("document-sidebar-markdown-outline")
                        .debug_selector(|| "document-sidebar-markdown-outline".to_owned())
                        .child(self.render_workspace_outline_tree(theme, strings, &editor))
                        .into_any_element(),
                )
            } else {
                (
                    strings.document_sidebar_info.clone(),
                    "icon/ui/info.svg",
                    self.render_document_sidebar_info_fallback(theme, strings),
                )
            }
        };

        Some(
            div()
                .id("document-sidebar-panel")
                .debug_selector(|| "document-sidebar-panel".to_owned())
                .track_focus(&focus_handle)
                .relative()
                .h_full()
                .w(px(panel_width))
                .flex()
                .flex_col()
                .flex_shrink_0()
                .bg(c.sidebar_background)
                .border_l(px(d.dialog_border_width))
                .border_color(c.dialog_border)
                .child(
                    div()
                        .id("document-sidebar-header")
                        .debug_selector(|| "document-sidebar-header".to_owned())
                        .h(px(44.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(theme.typography.text_size))
                        .text_color(c.text_default)
                        .child(
                            svg()
                                .path(icon)
                                .size(px(16.0))
                                .text_color(c.dialog_muted)
                                .debug_selector(|| "document-sidebar-header-icon".to_owned()),
                        )
                        .child(div().flex_1().min_w(px(0.0)).truncate().child(title)),
                )
                .child(
                    div()
                        .id("document-sidebar-scroll")
                        .debug_selector(|| "document-sidebar-scroll".to_owned())
                        .track_scroll(&self.workspace.document_sidebar_scroll)
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_y_scroll()
                        .px(px(8.0))
                        .py(px(10.0))
                        .child(body),
                )
                .children(resize_focus_handle.clone().map(|focus_handle| {
                    div()
                        .id("document-sidebar-resize-handle")
                        .debug_selector(|| "document-sidebar-resize-handle".to_owned())
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(-WORKSPACE_RESIZE_HIT_WIDTH * 0.5))
                        .w(px(WORKSPACE_RESIZE_HIT_WIDTH))
                        .tab_index(0)
                        .track_focus(&focus_handle)
                        .cursor_col_resize()
                        .hover(|this| this.bg(c.text_link.opacity(0.08)))
                        .focus(|this| this.bg(c.text_link.opacity(0.08)))
                        .child(
                            div()
                                .id("document-sidebar-resize-line")
                                .debug_selector(|| "document-sidebar-resize-line".to_owned())
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(px((WORKSPACE_RESIZE_HIT_WIDTH - 1.0) * 0.5))
                                .w(px(1.0))
                                .bg(c.dialog_border),
                        )
                        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                            focus_handle.focus(window);
                            let _ = resize_editor.update(cx, |editor, cx| {
                                editor.start_document_sidebar_resize(
                                    event.position.x,
                                    panel_width,
                                    cx,
                                );
                            });
                            cx.stop_propagation();
                        })
                        .on_key_down(cx.listener(move |editor, event, _window, cx| {
                            editor.on_document_sidebar_resize_key_down(event, panel_width, cx);
                        }))
                }))
                .into_any_element(),
        )
    }

    fn render_document_sidebar_info_fallback(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
    ) -> AnyElement {
        let colors = &theme.colors;
        let source = self.source_document.text();
        let summary = self.source_document.source_format_summary();
        let line_endings = match summary.line_endings {
            gmark_document::LineEndingStatus::None => match summary.dominant {
                gmark_document::LineEnding::Lf => "LF",
                gmark_document::LineEnding::CrLf => "CRLF",
                gmark_document::LineEnding::Cr => "CR",
            },
            gmark_document::LineEndingStatus::Uniform(ending) => match ending {
                gmark_document::LineEnding::Lf => "LF",
                gmark_document::LineEnding::CrLf => "CRLF",
                gmark_document::LineEnding::Cr => "CR",
            },
            gmark_document::LineEndingStatus::Mixed => "Mixed",
        };
        let format = match self.document_kind {
            super::DocumentKind::Json => "JSON",
            super::DocumentKind::Csv => "CSV",
            _ => strings.document_sidebar_text.as_str(),
        };
        let rows = [
            (strings.document_sidebar_format.clone(), format.to_owned()),
            (
                strings.document_sidebar_size.clone(),
                format_sidebar_byte_count(source.len() as u64),
            ),
            (
                strings.document_sidebar_lines.clone(),
                source.lines().count().max(1).to_string(),
            ),
            (
                strings.document_sidebar_encoding.clone(),
                self.source_encoding.label().to_owned(),
            ),
            (
                strings.document_sidebar_line_endings.clone(),
                line_endings.to_owned(),
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
}
