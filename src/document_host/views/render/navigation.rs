// @author kongweiguang

//! Go-to-line / byte overlay for the document host.

use super::*;

impl DocumentHost {
    pub(super) fn render_navigation_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.navigation_visible.then(|| {
            let theme = cx.global::<ThemeManager>().current_arc();
            let strings = cx.global::<I18nManager>().strings_arc();
            let colors = &theme.colors;
            div()
                .id("document-host-navigation-panel")
                .debug_selector(|| "document-host-navigation-panel".to_owned())
                .absolute()
                .top(px(8.0))
                .right(px(12.0))
                .w(px(330.0))
                .max_w(relative(0.94))
                .h(px(46.0))
                .p(px(6.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .occlude()
                .bg(colors.dialog_surface)
                .border(px(1.0))
                .border_color(colors.dialog_border)
                .rounded(px(7.0))
                .shadow_md()
                .child(
                    div()
                        .id("document-host-navigation-kind")
                        .w(px(54.0))
                        .h(px(30.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.0))
                        .cursor_pointer()
                        .bg(colors.dialog_secondary_button_bg)
                        .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                        .text_size(px(12.0))
                        .text_color(colors.dialog_secondary_button_text)
                        .child(
                            strings
                                .large_document_text(if self.navigation_is_byte {
                                    "byte"
                                } else {
                                    "line"
                                })
                                .to_owned(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.navigation_is_byte = !this.navigation_is_byte;
                            let placeholder = cx
                                .global::<I18nManager>()
                                .strings()
                                .large_document_text(if this.navigation_is_byte {
                                    "go_to_byte"
                                } else {
                                    "go_to_line"
                                })
                                .to_owned();
                            this.navigation_input
                                .update(cx, |input, _cx| input.set_input_placeholder(placeholder));
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id("document-host-navigation-input")
                        .debug_selector(|| "document-host-navigation-input".to_owned())
                        .flex_1()
                        .min_w(px(0.0))
                        .h(px(30.0))
                        .px(px(7.0))
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .rounded(px(5.0))
                        .border(px(1.0))
                        .border_color(colors.dialog_border)
                        .child(self.navigation_input.clone()),
                )
                .child(
                    div()
                        .id("document-host-navigation-close")
                        .debug_selector(|| "document-host-navigation-close".to_owned())
                        .size(px(26.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|button| button.bg(colors.dialog_secondary_button_hover))
                        .child(
                            svg()
                                .path(CLOSE_ICON)
                                .size(px(15.0))
                                .text_color(colors.dialog_body),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.navigation_visible = false;
                            this.focus_handle.focus(window);
                            cx.notify();
                        })),
                )
        })
    }
}
