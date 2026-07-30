// @author kongweiguang

//! Preferences search result rendering.

use super::*;

impl PreferencesWindow {
    pub(super) fn render_search_results(
        &self,
        results: &[PreferenceSearchItem],
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let t = &theme.typography;
        let mut list = div()
            .id("preferences-search-results")
            .debug_selector(|| "preferences-search-results".to_owned())
            .w_full()
            .max_w(px(PREFERENCES_FORM_WIDTH))
            .flex()
            .flex_col()
            .gap(px(2.0));

        if results.is_empty() {
            return list.child(
                div()
                    .id("preferences-search-no-results")
                    .debug_selector(|| "preferences-search-no-results".to_owned())
                    .w_full()
                    .py(px(18.0))
                    .text_size(px(t.dialog_body_size))
                    .text_color(c.dialog_muted)
                    .child(strings.preferences_search_no_results.clone()),
            );
        }

        for (index, result) in results.iter().enumerate() {
            let nav = result.nav;
            let category = result.category.clone();
            let label = result.label.clone();
            list = list.child(
                div()
                    .id(("preferences-search-result", index))
                    .debug_selector(move || format!("preferences-search-result-{index}"))
                    .w_full()
                    .h(px(40.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(5.0))
                    .bg(if index == self.search_selected {
                        c.selection
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .child(
                        div()
                            .w(px(104.0))
                            .flex_shrink_0()
                            .overflow_hidden()
                            .truncate()
                            .text_size(px((t.dialog_body_size - 1.0).max(10.0)))
                            .text_color(c.dialog_muted)
                            .child(category),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .truncate()
                            .text_size(px(t.dialog_body_size))
                            .text_color(c.dialog_body)
                            .child(label),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_search_result(nav, window, cx);
                    })),
            );
        }
        list
    }
}
