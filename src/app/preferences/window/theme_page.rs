// @author kongweiguang

//! Theme and language preference controls.

use super::*;

impl PreferencesWindow {
    pub(super) fn render_theme_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let appearance_label = strings.preferences_theme_appearance.clone();
        let palette_label = strings.preferences_palette.clone();
        let dark_label = strings.preferences_theme_dark.clone();
        let light_label = strings.preferences_theme_light.clone();
        let system_label = strings.preferences_follow_system_theme.clone();
        let xcode_label = "Xcode";
        let fleet_label = "Fleet";
        let obsidian_label = "Obsidian";
        let claude_label = "Claude";
        let appearance_control = div().w(px(280.0)).flex().gap(px(4.0));
        let appearance_dark = self.theme_appearance_option(
            "preferences-theme-appearance-dark",
            0,
            dark_label.into(),
            ThemeAppearance::Dark,
            self.theme_appearance == ThemeAppearance::Dark,
            theme,
            cx,
        );
        let appearance_light = self.theme_appearance_option(
            "preferences-theme-appearance-light",
            1,
            light_label.into(),
            ThemeAppearance::Light,
            self.theme_appearance == ThemeAppearance::Light,
            theme,
            cx,
        );
        let appearance_system = self.theme_appearance_option(
            "preferences-theme-appearance-system",
            2,
            system_label.into(),
            ThemeAppearance::System,
            self.theme_appearance == ThemeAppearance::System,
            theme,
            cx,
        );
        let appearance_control =
            appearance_control.children([appearance_dark, appearance_light, appearance_system]);
        let palette_control = div().w(px(360.0)).flex().gap(px(4.0));
        let palette_xcode = self.theme_palette_option(
            "preferences-theme-palette-xcode",
            0,
            xcode_label.into(),
            ThemePalette::Xcode,
            self.theme_palette == ThemePalette::Xcode,
            theme,
            cx,
        );
        let palette_fleet = self.theme_palette_option(
            "preferences-theme-palette-fleet",
            1,
            fleet_label.into(),
            ThemePalette::Fleet,
            self.theme_palette == ThemePalette::Fleet,
            theme,
            cx,
        );
        let palette_obsidian = self.theme_palette_option(
            "preferences-theme-palette-obsidian",
            2,
            obsidian_label.into(),
            ThemePalette::Obsidian,
            self.theme_palette == ThemePalette::Obsidian,
            theme,
            cx,
        );
        let palette_claude = self.theme_palette_option(
            "preferences-theme-palette-claude",
            3,
            claude_label.into(),
            ThemePalette::Claude,
            self.theme_palette == ThemePalette::Claude,
            theme,
            cx,
        );
        let palette_control = palette_control.children([
            palette_xcode,
            palette_fleet,
            palette_obsidian,
            palette_claude,
        ]);
        let language_dropdown = div()
            .relative()
            .w(px(280.0))
            .h(px(32.0))
            .flex_shrink_0()
            .child(
                self.dropdown_button(
                    "preferences-language-dropdown",
                    self.language_options
                        .iter()
                        .find(|entry| entry.id == self.selected_language_id)
                        .map(|entry| entry.name.clone())
                        .unwrap_or_else(|| self.selected_language_id.clone()),
                    PreferencesDropdown::Language,
                    theme,
                    cx,
                ),
            );
        let language_list = if self.language_dropdown_open {
            // 前两行控件各高 34px、行间距 12px；语言菜单必须从第三行按钮底部再留 4px。
            let mut list = Self::dropdown_list(theme)
                .top(px(128.0))
                .right_0()
                .id("preferences-language-dropdown-list")
                .debug_selector(|| "preferences-language-dropdown-list".to_owned())
                .max_h(px(240.0))
                .overflow_y_scroll();
            for (index, entry) in self.language_options.clone().into_iter().enumerate() {
                let selected = entry.id == self.selected_language_id;
                let highlighted =
                    self.dropdown_selected_indices[PreferencesDropdown::Language.index()] == index;
                let language_id = entry.id.clone();
                list = list.child(Self::dropdown_item(
                    ("preferences-language-option", index),
                    entry.name,
                    selected,
                    highlighted,
                    theme,
                    move |this, _, _, cx| {
                        this.selected_language_id = language_id.clone();
                        this.close_all_dropdowns();
                        cx.notify();
                    },
                    cx,
                ));
            }
            Some(list)
        } else {
            None
        };

        let mut accessibility_control =
            |control: PreferencesAccessibilityControl,
             selected: gmark_config::AccessibilityOverride| {
                let labels = [
                    strings.preferences_accessibility_system.clone(),
                    strings.preferences_accessibility_enabled.clone(),
                    strings.preferences_accessibility_disabled.clone(),
                ];
                let options = [
                    gmark_config::AccessibilityOverride::System,
                    gmark_config::AccessibilityOverride::Enabled,
                    gmark_config::AccessibilityOverride::Disabled,
                ];
                div()
                    .w_full()
                    .flex()
                    .gap(px(4.0))
                    .children(options.into_iter().zip(labels).map(|(option, label)| {
                        self.accessibility_option(
                            control,
                            option,
                            selected == option,
                            label.into(),
                            theme,
                            cx,
                        )
                    }))
            };

        let accessibility_heading = div()
            .w_full()
            .max_w(px(PREFERENCES_FORM_WIDTH))
            .pt(px(12.0))
            .border_t(px(theme.dimensions.dialog_border_width))
            .border_color(theme.colors.workbench.border_subtle)
            .text_size(px(theme.typography.dialog_title_size))
            .font_weight(theme.typography.dialog_title_weight.to_font_weight())
            .text_color(theme.colors.workbench.text_primary)
            .child(strings.preferences_accessibility_title.clone());

        div()
            .relative()
            .w_full()
            .max_w(px(PREFERENCES_FORM_WIDTH))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.labeled_row(&appearance_label, appearance_control, theme))
            .child(self.labeled_row(&palette_label, palette_control, theme))
            .child(self.labeled_row(&strings.menu_language, language_dropdown, theme))
            .child(accessibility_heading)
            .child(self.accessibility_row(
                strings.preferences_reduced_motion.clone(),
                strings.preferences_reduced_motion_hint.clone(),
                accessibility_control(
                    PreferencesAccessibilityControl::ReducedMotion,
                    self.visual_accessibility.reduced_motion,
                ),
                theme,
            ))
            .child(self.accessibility_row(
                strings.preferences_reduced_transparency.clone(),
                strings.preferences_reduced_transparency_hint.clone(),
                accessibility_control(
                    PreferencesAccessibilityControl::ReducedTransparency,
                    self.visual_accessibility.reduced_transparency,
                ),
                theme,
            ))
            .child(self.accessibility_row(
                strings.preferences_high_contrast.clone(),
                strings.preferences_high_contrast_hint.clone(),
                accessibility_control(
                    PreferencesAccessibilityControl::HighContrast,
                    self.visual_accessibility.high_contrast,
                ),
                theme,
            ))
            .children(language_list)
    }
}
