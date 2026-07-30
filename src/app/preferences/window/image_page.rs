// @author kongweiguang

//! Image-paste preference controls.

use super::*;

impl PreferencesWindow {
    pub(super) fn image_paste_behavior_label(
        behavior: ImagePasteBehavior,
        strings: &crate::i18n::I18nStrings,
    ) -> String {
        match behavior {
            ImagePasteBehavior::None => strings.preferences_image_paste_none.clone(),
            ImagePasteBehavior::CopyToDocumentFolder => strings
                .preferences_image_paste_copy_to_document_folder
                .clone(),
            ImagePasteBehavior::CopyToAssetsFolder => strings
                .preferences_image_paste_copy_to_assets_folder
                .clone(),
            ImagePasteBehavior::CopyToNamedAssetsFolder => strings
                .preferences_image_paste_copy_to_named_assets_folder
                .clone(),
        }
    }

    pub(super) fn render_image_page(
        &self,
        theme: &Theme,
        strings: &crate::i18n::I18nStrings,
        cx: &mut Context<Self>,
    ) -> Div {
        let options = [
            ImagePasteBehavior::None,
            ImagePasteBehavior::CopyToDocumentFolder,
            ImagePasteBehavior::CopyToAssetsFolder,
            ImagePasteBehavior::CopyToNamedAssetsFolder,
        ];
        let mut dropdown = div()
            .relative()
            .w(px(280.0))
            .h(px(32.0))
            .flex_shrink_0()
            .child(self.dropdown_button(
                "preferences-image-dropdown",
                Self::image_paste_behavior_label(self.image_paste_behavior, strings),
                PreferencesDropdown::Image,
                theme,
                cx,
            ));
        if self.image_dropdown_open {
            let mut list = Self::dropdown_list(theme)
                .left_0()
                .id("preferences-image-dropdown-list")
                .debug_selector(|| "preferences-image-dropdown-list".to_owned());
            for (index, behavior) in options.into_iter().enumerate() {
                let selected = behavior == self.image_paste_behavior;
                let label = Self::image_paste_behavior_label(behavior, strings);
                list = list.child(Self::dropdown_item(
                    ("preferences-image-option", index),
                    label,
                    selected,
                    self.dropdown_selected_indices[PreferencesDropdown::Image.index()] == index,
                    theme,
                    move |this, _, _, cx| {
                        this.commit_dropdown_selection(PreferencesDropdown::Image, index, cx);
                    },
                    cx,
                ));
            }
            dropdown = dropdown.child(list);
        }
        self.labeled_row(&strings.preferences_image_insert_behavior, dropdown, theme)
    }
}
