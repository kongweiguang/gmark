// @author kongweiguang

//! Preferences page navigation and focus behavior.

use super::*;

impl PreferencesWindow {
    pub(super) fn set_nav_file(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.select_nav(PreferencesNav::File, cx);
        self.clear_search(cx);
    }

    pub(super) fn set_nav_theme(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.select_nav(PreferencesNav::Theme, cx);
        self.clear_search(cx);
    }

    pub(super) fn set_nav_editor(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_nav(PreferencesNav::Editor, cx);
        self.clear_search(cx);
    }

    pub(super) fn set_nav_image(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.select_nav(PreferencesNav::Image, cx);
        self.clear_search(cx);
    }

    pub(super) fn set_nav_shortcuts(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_nav(PreferencesNav::Shortcuts, cx);
        self.clear_search(cx);
    }

    pub(super) fn set_nav_status_bar(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_nav(PreferencesNav::StatusBar, cx);
        self.clear_search(cx);
    }

    pub(super) fn on_nav_key_down(
        &mut self,
        nav: PreferencesNav,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = nav.index();
        let target = match event.keystroke.key.as_str() {
            "up" | "left" => {
                Some((current + PreferencesNav::ORDER.len() - 1) % PreferencesNav::ORDER.len())
            }
            "down" | "right" => Some((current + 1) % PreferencesNav::ORDER.len()),
            "home" => Some(0),
            "end" => Some(PreferencesNav::ORDER.len() - 1),
            "enter" | "space" => Some(current),
            _ => None,
        };
        let Some(target) = target else {
            return;
        };
        let nav = PreferencesNav::ORDER[target];
        self.select_nav(nav, cx);
        self.clear_search(cx);
        self.nav_focus_handles[target].focus(window);
        cx.stop_propagation();
    }

    pub(super) fn nav_button(
        &self,
        id: &'static str,
        label: String,
        icon: &'static str,
        nav: PreferencesNav,
        selected: bool,
        theme: &Theme,
        on_click: fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let t = &theme.typography;
        let focus_handle = self.nav_focus_handles[nav.index()].clone();
        let pointer_focus_handle = focus_handle.clone();
        div()
            .h(px(36.0))
            .w_full()
            .tab_index(0)
            .track_focus(&focus_handle)
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .rounded(px(8.0))
            .border(px(1.0))
            .border_color(hsla(0.0, 0.0, 0.0, 0.0))
            .cursor_pointer()
            .text_size(px(t.dialog_body_size))
            .font_weight(t.dialog_button_weight.to_font_weight())
            .text_color(if selected {
                c.text_default
            } else {
                c.dialog_muted
            })
            .bg(if selected {
                c.text_link.opacity(0.14)
            } else {
                hsla(0.0, 0.0, 0.0, 0.0)
            })
            .hover(move |this| {
                this.bg(if selected {
                    c.selection
                } else {
                    c.chrome_hover
                })
            })
            .focus(move |this| this.bg(c.chrome_hover).border_color(c.text_link))
            .id(id)
            .debug_selector(move || id.to_owned())
            .child(
                svg()
                    .path(icon)
                    .size(px(16.0))
                    .flex_shrink_0()
                    .text_color(if selected {
                        c.text_default
                    } else {
                        c.dialog_muted
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .on_click(cx.listener(move |this, event, window, cx| {
                pointer_focus_handle.focus(window);
                on_click(this, event, window, cx);
            }))
            .on_key_down(cx.listener(move |this, event, window, cx| {
                this.on_nav_key_down(nav, event, window, cx);
            }))
    }
}
