// @author kongweiguang

use super::*;

impl PreferencesWindow {
    pub(super) fn dropdown_button(
        &self,
        id: &'static str,
        label: String,
        dropdown: PreferencesDropdown,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let focus_handle = self.dropdown_focus_handles[dropdown.index()].clone();
        let pointer_focus_handle = focus_handle.clone();
        div()
            .w(px(280.0))
            .h(px(32.0))
            .tab_index(0)
            .track_focus(&focus_handle)
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(d.menu_item_radius))
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .bg(c.dialog_surface)
            .hover(|this| this.bg(c.chrome_hover))
            .focus(move |this| this.border_color(c.text_link))
            .cursor_pointer()
            .text_size(px(t.dialog_body_size))
            .text_color(c.dialog_body)
            .id(id)
            .debug_selector(move || id.to_owned())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .child(
                svg()
                    .path(CHEVRON_DOWN_ICON)
                    .size(px(14.0))
                    .text_color(c.dialog_body),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                pointer_focus_handle.focus(window);
                this.on_dropdown_click(dropdown, window, cx);
            }))
            .on_key_down(cx.listener(move |this, event, window, cx| {
                this.on_dropdown_key_down(dropdown, event, window, cx);
            }))
    }

    /// 下拉列表是独立浮层，不能参与设置行布局，否则左侧标签会随列表高度跳动。
    pub(super) fn dropdown_list(theme: &Theme) -> Div {
        let c = &theme.colors;
        let d = &theme.dimensions;
        div()
            .absolute()
            .occlude()
            .top(px(36.0))
            .w(px(280.0))
            .p(px(4.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .rounded(px(10.0))
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .bg(c.dialog_surface)
            .shadow_lg()
    }

    pub(super) fn dropdown_item(
        id: impl Into<ElementId>,
        label: String,
        selected: bool,
        highlighted: bool,
        theme: &Theme,
        on_click: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        div()
            .w_full()
            .min_h(px(30.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(d.menu_item_radius))
            .cursor_pointer()
            .bg(if highlighted {
                c.text_link.opacity(0.14)
            } else {
                hsla(0.0, 0.0, 0.0, 0.0)
            })
            .hover(|this| this.bg(c.dialog_secondary_button_hover))
            .text_size(px(t.dialog_body_size))
            .text_color(c.dialog_body)
            .id(id)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .child(
                div()
                    .size(px(16.0))
                    .flex_shrink_0()
                    .children(selected.then(|| {
                        svg()
                            .path(CHECK_ICON)
                            .size(px(14.0))
                            .text_color(c.dialog_body)
                    })),
            )
            .on_click(cx.listener(on_click))
    }

    pub(super) fn labeled_row(&self, label: &str, control: impl IntoElement, theme: &Theme) -> Div {
        let c = &theme.colors;
        let t = &theme.typography;
        div()
            .w_full()
            .max_w(px(PREFERENCES_FORM_WIDTH))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(20.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(t.dialog_body_size))
                    .font_weight(t.dialog_button_weight.to_font_weight())
                    .text_color(c.dialog_title)
                    .child(SharedString::from(label.to_string())),
            )
            .child(control)
    }

    pub(super) fn accessibility_row(
        &self,
        title: String,
        hint: String,
        control: impl IntoElement,
        theme: &Theme,
    ) -> Div {
        let workbench = &theme.colors.workbench;
        div()
            .w_full()
            .max_w(px(PREFERENCES_FORM_WIDTH))
            .flex()
            .items_start()
            .justify_between()
            .gap(px(20.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(180.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(theme.typography.dialog_body_size))
                            .font_weight(theme.typography.dialog_button_weight.to_font_weight())
                            .text_color(workbench.text_primary)
                            .child(SharedString::from(title)),
                    )
                    .child(
                        div()
                            .text_size(px(theme.typography.dialog_body_size - 1.0))
                            .text_color(workbench.text_secondary)
                            .child(SharedString::from(hint)),
                    ),
            )
            .child(div().w(px(280.0)).flex_shrink_0().child(control))
    }

    pub(super) fn theme_appearance_option(
        &self,
        id: &'static str,
        index: usize,
        label: SharedString,
        option: ThemeAppearance,
        selected: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let focus_handle = self.theme_appearance_focus_handles[index].clone();
        let pointer_focus_handle = focus_handle.clone();
        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .flex_1()
            .min_w(px(0.0))
            .h(px(34.0))
            .tab_index(0)
            .track_focus(&focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .px(px(8.0))
            .rounded(px(d.menu_item_radius))
            .border(px(d.dialog_border_width))
            .border_color(if selected {
                c.text_link
            } else {
                c.dialog_border
            })
            .bg(if selected {
                c.text_link.opacity(0.16)
            } else {
                c.dialog_surface
            })
            .hover(|this| this.bg(c.chrome_hover))
            .focus(|this| this.border_color(c.text_link))
            .cursor_pointer()
            .text_size(px(t.dialog_body_size))
            .text_color(if selected { c.text_link } else { c.dialog_body })
            .child(
                div()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                pointer_focus_handle.focus(window);
                this.preview_theme_appearance(option, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    focus_handle.focus(window);
                    this.preview_theme_appearance(option, cx);
                    cx.stop_propagation();
                }
            }))
    }

    pub(super) fn theme_palette_option(
        &self,
        id: &'static str,
        index: usize,
        label: SharedString,
        option: ThemePalette,
        selected: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let focus_handle = self.theme_palette_focus_handles[index].clone();
        let pointer_focus_handle = focus_handle.clone();
        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .flex_1()
            .min_w(px(0.0))
            .h(px(34.0))
            .tab_index(0)
            .track_focus(&focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .px(px(8.0))
            .rounded(px(d.menu_item_radius))
            .border(px(d.dialog_border_width))
            .border_color(if selected {
                c.text_link
            } else {
                c.dialog_border
            })
            .bg(if selected {
                c.text_link.opacity(0.16)
            } else {
                c.dialog_surface
            })
            .hover(|this| this.bg(c.chrome_hover))
            .focus(|this| this.border_color(c.text_link))
            .cursor_pointer()
            .text_size(px(t.dialog_body_size))
            .text_color(if selected { c.text_link } else { c.dialog_body })
            .child(
                div()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                pointer_focus_handle.focus(window);
                this.preview_theme_palette(option, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    focus_handle.focus(window);
                    this.preview_theme_palette(option, cx);
                    cx.stop_propagation();
                }
            }))
    }

    pub(super) fn accessibility_option(
        &self,
        control: PreferencesAccessibilityControl,
        option: gmark_config::AccessibilityOverride,
        selected: bool,
        label: SharedString,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let workbench = &theme.colors.workbench;
        let visual_preferences = cx
            .try_global::<crate::ui::visual_preferences::VisualPreferencesManager>()
            .map(crate::ui::visual_preferences::VisualPreferencesManager::current)
            .unwrap_or_default();
        let material = workbench.material(
            crate::theme::workbench::SurfaceKind::Solid,
            visual_preferences,
        );
        let option_index = match option {
            gmark_config::AccessibilityOverride::System => 0,
            gmark_config::AccessibilityOverride::Enabled => 1,
            gmark_config::AccessibilityOverride::Disabled => 2,
        };
        let focus_handle = self.accessibility_focus_handles[control.index()][option_index].clone();
        let pointer_focus_handle = focus_handle.clone();
        let id = control.id();
        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .flex_1()
            .min_w(px(0.0))
            .h(px(36.0))
            .tab_index(0)
            .track_focus(&focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .px(px(8.0))
            .rounded(px(theme.dimensions.menu_item_radius))
            .border(px(theme.dimensions.dialog_border_width))
            .border_color(if selected {
                workbench.accent
            } else {
                material.border
            })
            .bg(if selected {
                workbench.accent_soft
            } else {
                material.background
            })
            .hover(|this| this.bg(workbench.control_hover))
            .focus(|this| this.border_color(workbench.focus_ring))
            .cursor_pointer()
            .text_size(px(theme.typography.dialog_body_size))
            .text_color(if selected {
                workbench.accent
            } else {
                workbench.text_secondary
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .truncate()
                    .child(label),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                pointer_focus_handle.focus(window);
                this.set_accessibility_override(control, option, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                if matches!(key, "enter" | "space") {
                    focus_handle.focus(window);
                    this.set_accessibility_override(control, option, cx);
                    cx.stop_propagation();
                }
            }))
    }

    pub(super) fn numeric_stepper(
        &self,
        id: &'static str,
        input: PreferencesNumericInput,
        decrease: PreferencesStepperControl,
        increase: PreferencesStepperControl,
        unit: &'static str,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let input_id = input.input_id();
        let input_is_valid = self.numeric_input_is_valid(input, cx);
        let input_focus_handle = self.numeric_inputs[input.index()]
            .read(cx)
            .focus_handle
            .clone();
        let button =
            |control: PreferencesStepperControl, icon: &'static str, cx: &mut Context<Self>| {
                let focus_handle = self.stepper_focus_handles[control.index()].clone();
                let pointer_focus_handle = focus_handle.clone();
                div()
                    .id(control.id())
                    .debug_selector(move || control.id().to_owned())
                    .size(px(32.0))
                    .tab_index(0)
                    .track_focus(&focus_handle)
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(d.menu_item_radius))
                    .border(px(d.dialog_border_width))
                    .border_color(c.dialog_border)
                    .bg(c.dialog_secondary_button_bg)
                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                    .focus(move |this| this.border_color(c.text_link))
                    .cursor_pointer()
                    .text_color(c.dialog_secondary_button_text)
                    .child(
                        svg()
                            .path(icon)
                            .size(px(14.0))
                            .text_color(c.dialog_secondary_button_text),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        pointer_focus_handle.focus(window);
                        this.activate_stepper(control, cx);
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.activate_stepper(control, cx);
                            cx.stop_propagation();
                        }
                    }))
            };

        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .w(px(160.0))
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(button(decrease, MINUS_ICON, cx))
            .child(
                div()
                    .id(input_id)
                    .debug_selector(move || input_id.to_owned())
                    .flex_1()
                    .h_full()
                    .min_w(px(0.0))
                    .relative()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .rounded(px(d.menu_item_radius))
                    .border(px(d.dialog_border_width))
                    .border_color(if input_is_valid {
                        c.dialog_border
                    } else {
                        c.dialog_danger_button_bg
                    })
                    .bg(c.dialog_surface)
                    .px(px(7.0))
                    .cursor(CursorStyle::IBeam)
                    .text_size(px(t.dialog_body_size))
                    .text_color(c.dialog_title)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .child(self.numeric_inputs[input.index()].clone()),
                    )
                    .children((!unit.is_empty()).then(|| {
                        div()
                            .flex_shrink_0()
                            .pl(px(3.0))
                            .text_color(c.dialog_muted)
                            .child(unit)
                    }))
                    .on_click(move |_, window, _| input_focus_handle.focus(window)),
            )
            .child(button(increase, PLUS_ICON, cx))
    }
}
