// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn render_quick_open_overlay(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.workspace.quick_open.as_ref()?;
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let editor = cx.entity().downgrade();
        let dismiss_editor = editor.clone();
        let close_editor = editor.clone();
        let close_tooltip: SharedString = strings.ui_close.clone().into();
        let query_empty = state.input.read(cx).display_text().trim().is_empty();
        let status = if state.running && state.results.is_empty() {
            Some(strings.quick_open_scanning.clone())
        } else if state.results.is_empty() {
            Some(if query_empty {
                strings.quick_open_prompt.clone()
            } else {
                strings.quick_open_no_results.clone()
            })
        } else {
            None
        };

        Some(
            div()
                .id("quick-open-overlay")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .occlude()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(82.0))
                .bg(c.dialog_backdrop)
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    let _ = dismiss_editor.update(cx, |editor, cx| {
                        editor.workspace.quick_open = None;
                        cx.notify();
                    });
                })
                .child(
                    div()
                        .id("quick-open-dialog")
                        .debug_selector(|| "quick-open-dialog".to_owned())
                        .w(px(560.0))
                        .max_w(relative(0.92))
                        .max_h(relative(0.74))
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .bg(c.dialog_surface)
                        .border(px(d.dialog_border_width))
                        .border_color(c.dialog_border)
                        .rounded(px(d.dialog_radius.min(8.0)))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .child(
                            div()
                                .h(px(38.0))
                                .px(px(14.0))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .truncate()
                                        .text_size(px(t.dialog_title_size))
                                        .font_weight(t.dialog_title_weight.to_font_weight())
                                        .text_color(c.dialog_title)
                                        .child(strings.quick_open_title.clone()),
                                )
                                .child(
                                    div()
                                        .id("quick-open-close")
                                        .debug_selector(|| "quick-open-close".to_owned())
                                        .size(px(28.0))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(5.0))
                                        .cursor_pointer()
                                        .hover(|this| this.bg(c.dialog_secondary_button_hover))
                                        .tooltip(move |_window, cx| {
                                            crate::ui::ui_tooltip(close_tooltip.clone(), cx)
                                        })
                                        .child(
                                            svg()
                                                .path(CLOSE_ICON)
                                                .size(px(15.0))
                                                .text_color(c.dialog_muted)
                                                .debug_selector(|| {
                                                    "quick-open-close-icon".to_owned()
                                                }),
                                        )
                                        .on_click(move |_event, _window, cx| {
                                            let _ = close_editor.update(cx, |editor, cx| {
                                                editor.workspace.quick_open = None;
                                                cx.notify();
                                            });
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .id("quick-open-input")
                                .debug_selector(|| "quick-open-input".to_owned())
                                .mx(px(12.0))
                                .mb(px(10.0))
                                .min_h(px(40.0))
                                .px(px(10.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .rounded(px(6.0))
                                .border(px(d.dialog_border_width))
                                .border_color(c.dialog_border)
                                .child(
                                    div()
                                        .id("quick-open-search-icon")
                                        .debug_selector(|| "quick-open-search-icon".to_owned())
                                        .size(px(16.0))
                                        .flex_shrink_0()
                                        .text_color(c.dialog_muted)
                                        .child(
                                            svg()
                                                .path(SEARCH_TAB_ICON)
                                                .size(px(16.0))
                                                .text_color(c.dialog_muted)
                                                .debug_selector(|| {
                                                    "quick-open-search-icon-svg".to_owned()
                                                }),
                                        ),
                                )
                                .child(div().flex_1().min_w(px(0.0)).child(state.input.clone())),
                        )
                        .child(
                            div()
                                .id("quick-open-results")
                                .debug_selector(|| "quick-open-results".to_owned())
                                .flex_1()
                                .min_h(px(52.0))
                                .overflow_y_scroll()
                                .px(px(8.0))
                                .pb(px(8.0))
                                .children(status.map(|message| {
                                    div()
                                        .px(px(10.0))
                                        .py(px(14.0))
                                        .text_size(px(t.dialog_body_size))
                                        .text_color(c.dialog_muted)
                                        .child(message)
                                }))
                                .children(state.results.iter().enumerate().map(
                                    |(index, result)| {
                                        let editor = editor.clone();
                                        let path = result.path.clone();
                                        div()
                                            .id(("quick-open-result", index))
                                            .debug_selector(move || {
                                                format!("quick-open-result-{index}")
                                            })
                                            .h(px(34.0))
                                            .w_full()
                                            .px(px(10.0))
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .overflow_hidden()
                                            .rounded(px(5.0))
                                            .bg(if index == state.selected {
                                                c.selection
                                            } else {
                                                hsla(0.0, 0.0, 0.0, 0.0)
                                            })
                                            .hover(|this| this.bg(c.dialog_secondary_button_hover))
                                            .cursor_pointer()
                                            .text_size(px(t.dialog_body_size))
                                            .text_color(c.text_default)
                                            .child(
                                                svg()
                                                    .path(MARKDOWN_ICON)
                                                    .size(px(16.0))
                                                    .flex_shrink_0()
                                                    .text_color(c.dialog_muted)
                                                    .debug_selector(move || {
                                                        format!("quick-open-result-icon-{index}")
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .min_w(px(0.0))
                                                    .overflow_hidden()
                                                    .truncate()
                                                    .child(middle_ellipsis(
                                                        &result.relative_path,
                                                        56,
                                                    )),
                                            )
                                            .on_click(move |_event, window, cx| {
                                                let path = path.clone();
                                                let _ = editor.update(cx, |editor, cx| {
                                                    editor.workspace.quick_open = None;
                                                    editor.open_workspace_file(path, window, cx);
                                                });
                                            })
                                    },
                                )),
                        ),
                )
                .into_any_element(),
        )
    }
}
