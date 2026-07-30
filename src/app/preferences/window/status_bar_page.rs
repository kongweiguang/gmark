// @author kongweiguang

//! Status bar preference controls.

use super::*;

impl PreferencesWindow {
    pub(super) fn render_status_bar_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = &theme.colors;
        let t = &theme.typography;

        let switch_row =
            |label: &str, preference: PreferencesSwitch, checked: bool, cx: &mut Context<Self>| {
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(t.dialog_body_size))
                            .text_color(c.dialog_body)
                            .child(SharedString::from(label.to_string())),
                    )
                    .child(self.preference_switch(preference, checked, cx))
            };

        let items = div()
            .id("preferences-status-bar-options")
            .debug_selector(|| "preferences-status-bar-options".to_owned())
            .w_full()
            .max_w(px(PREFERENCES_FORM_WIDTH))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(switch_row(
                &strings.preferences_status_bar_enabled,
                PreferencesSwitch::StatusBarEnabled,
                self.status_bar_enabled,
                cx,
            ))
            .child(switch_row(
                &strings.preferences_status_bar_show_word_count,
                PreferencesSwitch::StatusBarWordCount,
                self.status_bar_show_word_count,
                cx,
            ))
            .child(switch_row(
                &strings.preferences_status_bar_show_cursor_position,
                PreferencesSwitch::StatusBarCursorPosition,
                self.status_bar_show_cursor_position,
                cx,
            ))
            .child(switch_row(
                &strings.preferences_status_bar_show_sidebar_toggle,
                PreferencesSwitch::StatusBarSidebarToggle,
                self.status_bar_show_sidebar_toggle,
                cx,
            ))
            .child(switch_row(
                &strings.preferences_status_bar_show_mode_switch,
                PreferencesSwitch::StatusBarModeSwitch,
                self.status_bar_show_mode_switch,
                cx,
            ));

        div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .child(items)
    }
}
