// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn render_new_tab_menu_overlay(
        &self,
        theme: &crate::theme::Theme,
        strings: &crate::i18n::I18nStrings,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let position = self.tabs.new_tab_menu.as_ref()?.position;
        let c = &theme.colors;
        let d = &theme.dimensions;
        let panel_width = d.context_menu_submenu_width.max(210.0);
        let panel_origin = clamped_floating_panel_origin(
            position,
            panel_width,
            compact_menu_panel_height(4, 0, d),
            window.viewport_size(),
        );
        let editor = cx.entity().downgrade();
        let dismiss_editor = editor.clone();
        let markdown_editor = editor.clone();
        let untyped_editor = editor.clone();
        let json_editor = editor.clone();
        let csv_editor = editor;
        let item = |id: &'static str, label: String, icon: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .h(px(d.menu_item_height))
                .px(px(d.menu_item_padding_x))
                .flex()
                .items_center()
                .gap(px(7.0))
                .rounded(px(d.menu_item_radius))
                .text_size(px(d.menu_text_size))
                .text_color(c.dialog_secondary_button_text)
                .hover(|item| item.bg(c.dialog_secondary_button_hover))
                .cursor_pointer()
                .child(menu_icon_slot(Some(icon), c.dialog_muted))
                .child(label)
        };
        Some(
            div()
                .id("new-tab-type-menu-overlay")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .occlude()
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    let _ = dismiss_editor.update(cx, |editor, cx| {
                        editor.tabs.new_tab_menu = None;
                        cx.notify();
                    });
                })
                .child(
                    div()
                        .id("new-tab-type-menu")
                        .debug_selector(|| "new-tab-type-menu".to_owned())
                        .absolute()
                        .left(panel_origin.x)
                        .top(panel_origin.y)
                        .w(px(panel_width))
                        .p(px(d.menu_panel_padding))
                        .flex()
                        .flex_col()
                        .gap(px(d.menu_panel_gap))
                        .bg(c.dialog_surface)
                        .border(px(d.dialog_border_width))
                        .border_color(c.dialog_border)
                        .rounded(px(d.menu_panel_radius))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation()
                        })
                        .child(
                            item(
                                "new-tab-untyped",
                                strings.new_document_untyped.clone(),
                                "icon/ui/file.svg",
                            )
                            .on_click(move |_event, _window, cx| {
                                let _ = untyped_editor.update(cx, |editor, cx| {
                                    editor.tabs.new_tab_menu = None;
                                    editor.new_untyped_tab(cx);
                                });
                            }),
                        )
                        .child(
                            item(
                                "new-tab-markdown",
                                strings.new_document_markdown.clone(),
                                TAB_DOCUMENT_ICON,
                            )
                            .on_click(move |_event, _window, cx| {
                                let _ = markdown_editor.update(cx, |editor, cx| {
                                    editor.tabs.new_tab_menu = None;
                                    editor.new_untitled_tab(cx);
                                });
                            }),
                        )
                        .child(
                            item(
                                "new-tab-json",
                                strings.new_document_json.clone(),
                                "icon/ui/code.svg",
                            )
                            .on_click(move |_event, _window, cx| {
                                let _ = json_editor.update(cx, |editor, cx| {
                                    editor.tabs.new_tab_menu = None;
                                    editor.new_document_tab(DocumentKind::Json, cx);
                                });
                            }),
                        )
                        .child(
                            item(
                                "new-tab-csv",
                                strings.new_document_csv.clone(),
                                "icon/ui/table.svg",
                            )
                            .on_click(move |_event, _window, cx| {
                                let _ = csv_editor.update(cx, |editor, cx| {
                                    editor.tabs.new_tab_menu = None;
                                    editor.new_document_tab(DocumentKind::Csv, cx);
                                });
                            }),
                        ),
                )
                .into_any_element(),
        )
    }

    #[cfg(test)]
    pub(super) fn inactive_tab_count(&self) -> usize {
        self.tabs
            .records
            .iter()
            .filter(|record| record.snapshot.is_some())
            .count()
    }
}
