// @author kongweiguang

//! Dropdown state and keyboard selection behavior.

use super::*;

impl PreferencesWindow {
    pub(super) fn close_all_dropdowns(&mut self) {
        self.startup_dropdown_open = false;
        self.auto_save_dropdown_open = false;
        self.language_dropdown_open = false;
        self.image_dropdown_open = false;
        self.font_dropdown_open = false;
    }

    pub(super) fn dropdown_is_open(&self, dropdown: PreferencesDropdown) -> bool {
        match dropdown {
            PreferencesDropdown::Startup => self.startup_dropdown_open,
            PreferencesDropdown::AutoSave => self.auto_save_dropdown_open,
            PreferencesDropdown::Language => self.language_dropdown_open,
            PreferencesDropdown::Image => self.image_dropdown_open,
            PreferencesDropdown::Font => self.font_dropdown_open,
        }
    }

    pub(super) fn set_dropdown_open(&mut self, dropdown: PreferencesDropdown, open: bool) {
        self.close_all_dropdowns();
        if open {
            match dropdown {
                PreferencesDropdown::Startup => self.startup_dropdown_open = true,
                PreferencesDropdown::AutoSave => self.auto_save_dropdown_open = true,
                PreferencesDropdown::Language => self.language_dropdown_open = true,
                PreferencesDropdown::Image => self.image_dropdown_open = true,
                PreferencesDropdown::Font => self.font_dropdown_open = true,
            }
            let selected = self.dropdown_current_index(dropdown);
            self.dropdown_selected_indices[dropdown.index()] = selected;
        }
    }

    pub(super) fn dropdown_option_count(&self, dropdown: PreferencesDropdown) -> usize {
        match dropdown {
            PreferencesDropdown::Startup | PreferencesDropdown::AutoSave => 2,
            PreferencesDropdown::Language => self.language_options.len(),
            PreferencesDropdown::Image => 4,
            PreferencesDropdown::Font => self.font_options.len(),
        }
    }

    pub(super) fn dropdown_current_index(&self, dropdown: PreferencesDropdown) -> usize {
        match dropdown {
            PreferencesDropdown::Startup => match self.startup_open {
                StartupOpenPreference::NewFile => 0,
                StartupOpenPreference::LastOpenedFile => 1,
            },
            PreferencesDropdown::AutoSave => match self.auto_save {
                AutoSavePreference::Off => 0,
                AutoSavePreference::AfterDelay => 1,
            },
            PreferencesDropdown::Language => self
                .language_options
                .iter()
                .position(|entry| entry.id == self.selected_language_id)
                .unwrap_or(0),
            PreferencesDropdown::Image => match self.image_paste_behavior {
                ImagePasteBehavior::None => 0,
                ImagePasteBehavior::CopyToDocumentFolder => 1,
                ImagePasteBehavior::CopyToAssetsFolder => 2,
                ImagePasteBehavior::CopyToNamedAssetsFolder => 3,
            },
            PreferencesDropdown::Font => self
                .font_options
                .iter()
                .position(|font| font == &self.editor_font_family)
                .unwrap_or(0),
        }
    }

    pub(super) fn commit_dropdown_selection(
        &mut self,
        dropdown: PreferencesDropdown,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        match dropdown {
            PreferencesDropdown::Startup => {
                self.startup_open = [
                    StartupOpenPreference::NewFile,
                    StartupOpenPreference::LastOpenedFile,
                ][index.min(1)];
                self.close_all_dropdowns();
                cx.notify();
            }
            PreferencesDropdown::AutoSave => {
                self.auto_save =
                    [AutoSavePreference::Off, AutoSavePreference::AfterDelay][index.min(1)];
                self.close_all_dropdowns();
                cx.notify();
            }
            PreferencesDropdown::Language => {
                if let Some(language_id) = self
                    .language_options
                    .get(index)
                    .map(|entry| entry.id.clone())
                {
                    self.selected_language_id = language_id;
                    self.close_all_dropdowns();
                    cx.notify();
                }
            }
            PreferencesDropdown::Image => {
                self.image_paste_behavior = [
                    ImagePasteBehavior::None,
                    ImagePasteBehavior::CopyToDocumentFolder,
                    ImagePasteBehavior::CopyToAssetsFolder,
                    ImagePasteBehavior::CopyToNamedAssetsFolder,
                ][index.min(3)];
                self.close_all_dropdowns();
                cx.notify();
            }
            PreferencesDropdown::Font => {
                if let Some(font) = self.font_options.get(index).cloned() {
                    self.preview_editor_font_family(font, cx);
                }
            }
        }
    }

    pub(super) fn on_dropdown_click(
        &mut self,
        dropdown: PreferencesDropdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dropdown_focus_handles[dropdown.index()].focus(window);
        let open = !self.dropdown_is_open(dropdown);
        self.set_dropdown_open(dropdown, open);
        cx.notify();
    }

    pub(super) fn on_dropdown_key_down(
        &mut self,
        dropdown: PreferencesDropdown,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let open = self.dropdown_is_open(dropdown);
        if key == "escape" && open {
            self.close_all_dropdowns();
            self.dropdown_focus_handles[dropdown.index()].focus(window);
            cx.notify();
            cx.stop_propagation();
            return;
        }

        if matches!(key, "enter" | "space") {
            if open {
                let selected = self.dropdown_selected_indices[dropdown.index()];
                self.commit_dropdown_selection(dropdown, selected, cx);
            } else {
                self.set_dropdown_open(dropdown, true);
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }

        if !matches!(key, "up" | "down" | "home" | "end") {
            return;
        }
        let count = self.dropdown_option_count(dropdown);
        if count == 0 {
            return;
        }
        if !open {
            self.set_dropdown_open(dropdown, true);
        } else {
            let current = self.dropdown_selected_indices[dropdown.index()].min(count - 1);
            self.dropdown_selected_indices[dropdown.index()] = match key {
                "up" => (current + count - 1) % count,
                "down" => (current + 1) % count,
                "home" => 0,
                "end" => count - 1,
                _ => current,
            };
        }
        cx.notify();
        cx.stop_propagation();
    }
}
