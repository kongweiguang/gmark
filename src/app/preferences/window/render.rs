// @author kongweiguang

//! Preferences window frame rendering.

use super::*;

impl Render for PreferencesWindow {
    // The localized window title already identifies this surface, so matching
    // the main window's text-only chrome avoids a competing brand mark here.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<ThemeManager>().current().clone();
        let strings = cx.global::<I18nManager>().strings().clone();
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let resolved_visual_preferences = cx
            .try_global::<crate::ui::visual_preferences::VisualPreferencesManager>()
            .map(crate::ui::visual_preferences::VisualPreferencesManager::current)
            .unwrap_or_default();
        let workbench = &theme.colors.workbench;
        let app_surface = workbench.material(
            crate::theme::workbench::SurfaceKind::App,
            resolved_visual_preferences,
        );
        let navigation_surface = workbench.material(
            crate::theme::workbench::SurfaceKind::Navigation,
            resolved_visual_preferences,
        );
        let solid_surface = workbench.material(
            crate::theme::workbench::SurfaceKind::Solid,
            resolved_visual_preferences,
        );
        let compact = f32::from(window.viewport_size().width) < 720.0;
        let can_save = self.has_unsaved_changes() && !self.has_invalid_numeric_input(cx);
        let search_query = self.search_query(cx);
        let clear_search_tooltip: SharedString = strings.ui_clear_search.clone().into();
        let search_results = self.preference_search_results(&strings, cx);
        let window_title =
            SharedString::from(format!("Gmark - {}", strings.preferences_window_title));
        window.set_window_title(window_title.as_ref());
        let titlebar_height = custom_titlebar_height(window, d);
        let cancel_focus_handle = self.action_focus_handles[0].clone();
        let save_focus_handle = self.action_focus_handles[1].clone();

        let content = div()
            .id("preferences-content")
            .debug_selector(|| "preferences-content".to_owned())
            .size_full()
            .pt(px(titlebar_height))
            .flex()
            .when(compact, |this| this.flex_col())
            .key_context("Preferences")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::capture_shortcut_key))
            .bg(app_surface.background)
            .text_color(workbench.text_primary)
            .child(
                div()
                    .id("preferences-navigation")
                    .debug_selector(|| "preferences-navigation".to_owned())
                    .w(px(PREFERENCES_NAV_WIDTH))
                    .h_full()
                    .when(compact, |this| {
                        this.w_full()
                            .h(px(284.0))
                            .border_r(px(0.0))
                            .border_b(px(d.dialog_border_width))
                    })
                    .px(px(12.0))
                    .pt(px(16.0))
                    .flex_shrink_0()
                    .flex()
                    .items_start()
                    .bg(navigation_surface.background)
                    .border_r(px(d.dialog_border_width))
                    .border_color(navigation_surface.border)
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .id("preferences-search-input")
                                    .debug_selector(|| "preferences-search-input".to_owned())
                                    .w_full()
                                    .h(px(34.0))
                                    .px(px(8.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(7.0))
                                    .rounded(px(6.0))
                                    .border(px(d.dialog_border_width))
                                    .border_color(solid_surface.border)
                                    .bg(solid_surface.background)
                                    .child(
                                        div()
                                            .id("preferences-search-icon")
                                            .debug_selector(|| "preferences-search-icon".to_owned())
                                            .size(px(16.0))
                                            .flex_shrink_0()
                                            .text_color(c.dialog_muted)
                                            .child(
                                                svg()
                                                    .path(SEARCH_ICON)
                                                    .size(px(16.0))
                                                    .text_color(c.dialog_muted),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .child(self.search_input.clone()),
                                    )
                                    .child(
                                        div()
                                            .w(px(24.0))
                                            .h(px(24.0))
                                            .flex_shrink_0()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .children((!search_query.is_empty()).then(|| {
                                                div()
                                                    .id("preferences-search-clear")
                                                    .debug_selector(|| {
                                                        "preferences-search-clear".to_owned()
                                                    })
                                                    .size(px(24.0))
                                                    .tab_index(0)
                                                    .border(px(1.0))
                                                    .border_color(hsla(0.0, 0.0, 0.0, 0.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded(px(4.0))
                                                    .text_color(c.dialog_muted)
                                                    .cursor_pointer()
                                                    .hover(|this| this.bg(c.chrome_hover))
                                                    .focus(|style| {
                                                        style
                                                            .bg(c.chrome_hover)
                                                            .border_color(c.text_link)
                                                    })
                                                    .tooltip(move |_window, cx| {
                                                        crate::ui::ui_tooltip(
                                                            clear_search_tooltip.clone(),
                                                            cx,
                                                        )
                                                    })
                                                    .child(
                                                        svg()
                                                            .path(CLOSE_ICON)
                                                            .size(px(14.0))
                                                            .text_color(c.dialog_muted),
                                                    )
                                                    .on_click(
                                                        cx.listener(Self::clear_search_from_button),
                                                    )
                                                    .on_key_down(
                                                        cx.listener(Self::clear_search_from_key),
                                                    )
                                            })),
                                    ),
                            )
                            .child(self.nav_button(
                                "preferences-nav-file",
                                strings.preferences_nav_file.clone(),
                                "icon/ui/files.svg",
                                PreferencesNav::File,
                                self.nav == PreferencesNav::File,
                                &theme,
                                Self::set_nav_file,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-editor",
                                strings.preferences_nav_editor.clone(),
                                "icon/ui/type.svg",
                                PreferencesNav::Editor,
                                self.nav == PreferencesNav::Editor,
                                &theme,
                                Self::set_nav_editor,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-theme",
                                strings.preferences_nav_theme.clone(),
                                "icon/ui/palette.svg",
                                PreferencesNav::Theme,
                                self.nav == PreferencesNav::Theme,
                                &theme,
                                Self::set_nav_theme,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-image",
                                strings.preferences_nav_image.clone(),
                                "icon/ui/image.svg",
                                PreferencesNav::Image,
                                self.nav == PreferencesNav::Image,
                                &theme,
                                Self::set_nav_image,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-shortcuts",
                                strings.preferences_nav_shortcuts.clone(),
                                "icon/ui/keyboard.svg",
                                PreferencesNav::Shortcuts,
                                self.nav == PreferencesNav::Shortcuts,
                                &theme,
                                Self::set_nav_shortcuts,
                                cx,
                            ))
                            .child(self.nav_button(
                                "preferences-nav-status-bar",
                                strings.preferences_nav_status_bar.clone(),
                                "icon/ui/panel-bottom.svg",
                                PreferencesNav::StatusBar,
                                self.nav == PreferencesNav::StatusBar,
                                &theme,
                                Self::set_nav_status_bar,
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .id("preferences-main")
                    .debug_selector(|| "preferences-main".to_owned())
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .when(compact, |this| this.w_full())
                    .p(px(d.dialog_padding))
                    .flex()
                    .flex_col()
                    .gap(px(d.dialog_gap))
                    .child(
                        div()
                            .id("preferences-page-area")
                            .debug_selector(|| "preferences-page-area".to_owned())
                            .w_full()
                            .flex_1()
                            .min_h(px(0.0))
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(d.dialog_gap))
                            .child(
                                div()
                                    .id("preferences-page-title")
                                    .debug_selector(|| "preferences-page-title".to_owned())
                                    .w_full()
                                    .max_w(px(PREFERENCES_FORM_WIDTH))
                                    .flex_shrink_0()
                                    .text_size(px(t.dialog_title_size))
                                    .font_weight(t.dialog_title_weight.to_font_weight())
                                    .text_color(workbench.text_primary)
                                    .child(if search_query.is_empty() {
                                        match self.nav {
                                            PreferencesNav::File => {
                                                strings.preferences_nav_file.clone()
                                            }
                                            PreferencesNav::Editor => {
                                                strings.preferences_nav_editor.clone()
                                            }
                                            PreferencesNav::Theme => {
                                                strings.preferences_nav_theme.clone()
                                            }
                                            PreferencesNav::Image => {
                                                strings.preferences_nav_image.clone()
                                            }
                                            PreferencesNav::Shortcuts => {
                                                strings.preferences_nav_shortcuts.clone()
                                            }
                                            PreferencesNav::StatusBar => {
                                                strings.preferences_nav_status_bar.clone()
                                            }
                                        }
                                    } else {
                                        strings.preferences_search_results.clone()
                                    }),
                            )
                            .child(
                                div()
                                    .id("preferences-page-scroll")
                                    .debug_selector(|| "preferences-page-scroll".to_owned())
                                    .w_full()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .overflow_y_scroll()
                                    .flex()
                                    .flex_col()
                                    .items_start()
                                    .child(if !search_query.is_empty() {
                                        div()
                                            .w_full()
                                            .pt(px(8.0))
                                            .child(self.render_search_results(
                                                &search_results,
                                                &theme,
                                                &strings,
                                                cx,
                                            ))
                                            .into_any_element()
                                    } else {
                                        match self.nav {
                                            PreferencesNav::File => div()
                                                .w_full()
                                                .flex_1()
                                                .min_h(px(0.0))
                                                .flex()
                                                .items_start()
                                                .pt(px(12.0))
                                                .child(
                                                    self.render_startup_page(&theme, &strings, cx),
                                                )
                                                .into_any_element(),
                                            PreferencesNav::Editor => div()
                                                .w_full()
                                                .flex_1()
                                                .min_h(px(0.0))
                                                .flex()
                                                .items_start()
                                                .pt(px(12.0))
                                                .child(
                                                    self.render_editor_page(&theme, &strings, cx),
                                                )
                                                .into_any_element(),
                                            PreferencesNav::Theme => div()
                                                .w_full()
                                                .flex_1()
                                                .min_h(px(0.0))
                                                .flex()
                                                .items_start()
                                                .pt(px(12.0))
                                                .child(self.render_theme_page(&theme, &strings, cx))
                                                .into_any_element(),
                                            PreferencesNav::Image => div()
                                                .w_full()
                                                .flex_1()
                                                .min_h(px(0.0))
                                                .flex()
                                                .items_start()
                                                .pt(px(12.0))
                                                .child(self.render_image_page(&theme, &strings, cx))
                                                .into_any_element(),
                                            PreferencesNav::Shortcuts => {
                                                div()
                                                    .w_full()
                                                    .flex_1()
                                                    .min_h(px(0.0))
                                                    .child(self.render_shortcuts_page(
                                                        &theme, &strings, cx,
                                                    ))
                                                    .into_any_element()
                                            }
                                            PreferencesNav::StatusBar => {
                                                div()
                                                    .w_full()
                                                    .flex_1()
                                                    .min_h(px(0.0))
                                                    .flex()
                                                    .items_start()
                                                    .pt(px(12.0))
                                                    .child(self.render_status_bar_page(
                                                        &theme, &strings, cx,
                                                    ))
                                                    .into_any_element()
                                            }
                                        }
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id("preferences-actions")
                            .debug_selector(|| "preferences-actions".to_owned())
                            .w_full()
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .gap(px(d.dialog_button_gap))
                            .pt(px(12.0))
                            .border_t(px(d.dialog_border_width))
                            .border_color(solid_surface.border)
                            .child(
                                div()
                                    .id("preferences-cancel")
                                    .debug_selector(|| "preferences-cancel".to_owned())
                                    .h(px(d.dialog_button_height))
                                    .px(px(d.dialog_button_padding_x))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .tab_index(0)
                                    .track_focus(&cancel_focus_handle)
                                    .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                    .border(px(d.dialog_border_width))
                                    .border_color(solid_surface.border)
                                    .bg(workbench.control_surface)
                                    .hover(|this| this.bg(workbench.control_hover))
                                    .focus(|this| this.border_color(workbench.focus_ring))
                                    .cursor_pointer()
                                    .text_size(px(t.dialog_button_size))
                                    .font_weight(t.dialog_button_weight.to_font_weight())
                                    .text_color(workbench.text_primary)
                                    .child(strings.preferences_cancel.clone())
                                    .on_click(cx.listener(Self::cancel))
                                    .on_key_down(cx.listener(Self::cancel_from_key)),
                            )
                            .child(
                                div()
                                    .id("preferences-save")
                                    .debug_selector(|| "preferences-save".to_owned())
                                    .h(px(d.dialog_button_height))
                                    .px(px(d.dialog_button_padding_x))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .tab_index(0)
                                    .track_focus(&save_focus_handle)
                                    .rounded(px((d.dialog_radius - 4.0).max(0.0)))
                                    .border(px(if can_save { 0.0 } else { d.dialog_border_width }))
                                    .border_color(solid_surface.border)
                                    .bg(if can_save {
                                        workbench.accent
                                    } else {
                                        workbench.control_surface
                                    })
                                    .hover(move |this| {
                                        if can_save {
                                            this.bg(workbench.accent_hover)
                                        } else {
                                            this.bg(workbench.control_surface)
                                        }
                                    })
                                    .focus(|this| this.border_color(workbench.focus_ring))
                                    .when(can_save, |this| this.cursor_pointer())
                                    .text_size(px(t.dialog_button_size))
                                    .font_weight(t.dialog_button_weight.to_font_weight())
                                    .text_color(if can_save {
                                        workbench.text_inverse
                                    } else {
                                        workbench.text_secondary
                                    })
                                    .child(strings.preferences_save.clone())
                                    .on_click(cx.listener(Self::save))
                                    .on_key_down(cx.listener(Self::save_from_key)),
                            ),
                    ),
            );

        let root = div()
            .size_full()
            .relative()
            .bg(app_surface.background)
            .child(content);

        if let Some(titlebar) = render_custom_titlebar(
            "preferences-titlebar",
            Some(window_title),
            None,
            &theme,
            window,
            cx,
            Self::on_titlebar_close,
        ) {
            root.child(titlebar)
        } else {
            root
        }
    }
}
