// @author kongweiguang

//! Find overlay for the document host.

use super::*;

impl DocumentHost {
    pub(super) fn render_search_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.search_visible.then(|| {
            let theme = cx.global::<ThemeManager>().current_arc();
            let strings = cx.global::<I18nManager>().strings_arc();
            let colors = &theme.colors;
            let count: SharedString = if let Some(error) = &self.search_error {
                error.clone()
            } else if self.search_running {
                strings.large_document_text("searching").into()
            } else if self.search_results.is_empty() {
                strings.large_document_text("no_results").into()
            } else {
                format!(
                    "{} / {}{}",
                    self.search_selected + 1,
                    self.search_results.len(),
                    if self.search_results.len() == self.search_options.result_limit {
                        "+"
                    } else {
                        ""
                    }
                )
                .into()
            };
            let option_button = |id: &'static str, icon: &'static str, active: bool| {
                div()
                    .id(id)
                    .debug_selector(move || id.to_owned())
                    .debug_selector(move || id.to_owned())
                    .size(px(26.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .border(px(1.0))
                    .border_color(if active {
                        colors.text_link
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .bg(if active {
                        colors.dialog_secondary_button_hover
                    } else {
                        colors.dialog_surface
                    })
                    .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .child(
                        svg()
                            .path(icon)
                            .size(px(15.0))
                            .text_color(colors.dialog_body),
                    )
            };
            div()
                .id("document-host-find-panel")
                .debug_selector(|| "document-host-find-panel".to_owned())
                .absolute()
                .top(px(8.0))
                .right(px(12.0))
                .w(px(540.0))
                .max_w(relative(0.94))
                .h(px(46.0))
                .p(px(6.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .occlude()
                .bg(colors.dialog_surface)
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .rounded(px(7.0))
                .shadow_md()
                .child(
                    div()
                        .id("document-host-search-input")
                        .debug_selector(|| "document-host-search-input".to_owned())
                        .w(px(210.0))
                        .h(px(30.0))
                        .px(px(7.0))
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .rounded(px(5.0))
                        .border(px(1.0))
                        .border_color(colors.dialog_border)
                        .child(self.search_input.clone()),
                )
                .child(
                    div()
                        .id("document-host-search-count")
                        .w(px(74.0))
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .text_color(colors.dialog_muted)
                        .child(count),
                )
                .child(
                    option_button(
                        "document-host-search-case",
                        FIND_CASE_ICON,
                        self.search_options.case_sensitive,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_search_option(
                            |options| options.case_sensitive = !options.case_sensitive,
                            cx,
                        );
                    })),
                )
                .child(
                    option_button(
                        "document-host-search-word",
                        FIND_WORD_ICON,
                        self.search_options.whole_word,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_search_option(
                            |options| options.whole_word = !options.whole_word,
                            cx,
                        );
                    })),
                )
                .child(
                    option_button(
                        "document-host-search-regex",
                        FIND_REGEX_ICON,
                        self.search_options.regex,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_search_option(|options| options.regex = !options.regex, cx);
                    })),
                )
                .child(
                    option_button("document-host-search-previous", CHEVRON_UP_ICON, false)
                        .on_click(cx.listener(|this, _, _, cx| this.navigate_search(-1, cx))),
                )
                .child(
                    option_button("document-host-search-next", CHEVRON_DOWN_ICON, false)
                        .on_click(cx.listener(|this, _, _, cx| this.navigate_search(1, cx))),
                )
                .child(
                    option_button("document-host-search-close", CLOSE_ICON, false).on_click(
                        cx.listener(|this, _, window, cx| {
                            this.search_visible = false;
                            this.focus_handle.focus(window);
                            cx.notify();
                        }),
                    ),
                )
        })
    }
}
