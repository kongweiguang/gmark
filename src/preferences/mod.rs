// @author kongweiguang

//! Persistent app preferences and the preferences window.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Context as _;
use gpui::prelude::FluentBuilder;
use gpui::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    Block, BlockEvent, BlockRecord, ShortcutCategory, ShortcutCommand, ShortcutDefinition,
    install_keybindings, normalize_shortcut_config, normalize_shortcut_keys,
    resolved_shortcut_keys, shortcut_conflict_for, shortcut_definitions, switch::Switch,
};
use crate::config::{GmarkConfigDirs, read_recent_files};
use crate::i18n::{I18nManager, LanguageCatalogEntry, language_id_for_locale_preferences};
use crate::theme::{SYSTEM_THEME_ID, Theme, ThemeCatalogEntry, ThemeManager};
use crate::window_chrome::{custom_titlebar_height, gmark_window_options, render_custom_titlebar};

const DEFAULT_THEME_ID: &str = "gmark";
const DEFAULT_LANGUAGE_ID: &str = "en-US";
const DEFAULT_EDITOR_FONT_SIZE: u8 = 16;
const DEFAULT_EDITOR_LINE_HEIGHT_PERCENT: u16 = 160;
const MIN_EDITOR_FONT_SIZE: u8 = 12;
const MAX_EDITOR_FONT_SIZE: u8 = 24;
const MIN_EDITOR_LINE_HEIGHT_PERCENT: u16 = 120;
const MAX_EDITOR_LINE_HEIGHT_PERCENT: u16 = 200;
const EDITOR_LINE_HEIGHT_STEP: u16 = 5;
const DEFAULT_EDITOR_CONTENT_WIDTH: u16 = 1200;
const MIN_EDITOR_CONTENT_WIDTH: u16 = 680;
const MAX_EDITOR_CONTENT_WIDTH: u16 = 1600;
const EDITOR_CONTENT_WIDTH_STEP: u16 = 40;
const MAX_EDITOR_FONT_FAMILY_CHARS: usize = 80;

fn normalize_editor_line_height_percent(value: u16) -> u16 {
    let clamped = value.clamp(
        MIN_EDITOR_LINE_HEIGHT_PERCENT,
        MAX_EDITOR_LINE_HEIGHT_PERCENT,
    );
    ((clamped + EDITOR_LINE_HEIGHT_STEP / 2) / EDITOR_LINE_HEIGHT_STEP * EDITOR_LINE_HEIGHT_STEP)
        .clamp(
            MIN_EDITOR_LINE_HEIGHT_PERCENT,
            MAX_EDITOR_LINE_HEIGHT_PERCENT,
        )
}

fn normalize_editor_content_width(value: u16) -> u16 {
    let clamped = value.clamp(MIN_EDITOR_CONTENT_WIDTH, MAX_EDITOR_CONTENT_WIDTH);
    ((clamped + EDITOR_CONTENT_WIDTH_STEP / 2) / EDITOR_CONTENT_WIDTH_STEP
        * EDITOR_CONTENT_WIDTH_STEP)
        .clamp(MIN_EDITOR_CONTENT_WIDTH, MAX_EDITOR_CONTENT_WIDTH)
}

fn normalize_editor_font_family(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(MAX_EDITOR_FONT_FAMILY_CHARS)
        .collect()
}
const PREFERENCES_NAV_WIDTH: f32 = 200.0;
const PREFERENCES_FORM_WIDTH: f32 = 560.0;
const CHEVRON_DOWN_ICON: &str = "icon/ui/chevron-down.svg";
const CHECK_ICON: &str = "icon/ui/check.svg";
const SEARCH_ICON: &str = "icon/ui/search.svg";
const CLOSE_ICON: &str = "icon/ui/close.svg";
const MINUS_ICON: &str = "icon/ui/minus.svg";
const PLUS_ICON: &str = "icon/ui/plus.svg";
const SUN_ICON: &str = "icon/ui/sun.svg";
const MOON_ICON: &str = "icon/ui/moon.svg";
const MONITOR_ICON: &str = "icon/ui/monitor.svg";
const PALETTE_ICON: &str = "icon/ui/palette.svg";

fn theme_option_icon(theme_id: &str) -> &'static str {
    match theme_id {
        SYSTEM_THEME_ID => MONITOR_ICON,
        "gmark" => MOON_ICON,
        "gmark-light" => SUN_ICON,
        _ => PALETTE_ICON,
    }
}

/// A user-configurable button shown in the status bar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StatusBarButton {
    pub id: String,
    pub label: String,
    pub action_id: String,
}

/// Status bar visibility and component toggles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatusBarPreferences {
    pub enabled: bool,
    pub show_word_count: bool,
    pub show_cursor_position: bool,
    pub show_sidebar_toggle: bool,
    pub show_mode_switch: bool,
    pub custom_buttons: Vec<StatusBarButton>,
}

impl Default for StatusBarPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            show_word_count: true,
            show_cursor_position: true,
            show_sidebar_toggle: true,
            show_mode_switch: true,
            custom_buttons: Vec::new(),
        }
    }
}

/// Startup document selection stored in `config.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartupOpenPreference {
    NewFile,
    LastOpenedFile,
}

impl StartupOpenPreference {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NewFile => "new_file",
            Self::LastOpenedFile => "last_opened_file",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "last_opened_file" => Self::LastOpenedFile,
            _ => Self::NewFile,
        }
    }
}

/// Where pasted clipboard images should be stored before inserting Markdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImagePasteBehavior {
    None,
    CopyToDocumentFolder,
    CopyToAssetsFolder,
    CopyToNamedAssetsFolder,
}

/// Automatic file-save behavior. Crash recovery remains independent and always journals dirty work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AutoSavePreference {
    /// Never write the Markdown file without an explicit Save command.
    #[default]
    Off,
    /// Save an existing, conflict-free file after one second without edits.
    AfterDelay,
}

impl AutoSavePreference {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::AfterDelay => "after_delay",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "after_delay" => Self::AfterDelay,
            _ => Self::Off,
        }
    }
}

/// Docking edge for the optional workspace panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WorkspaceSidebarPosition {
    #[default]
    Left,
    Right,
}

impl WorkspaceSidebarPosition {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "right" => Self::Right,
            _ => Self::Left,
        }
    }
}

impl ImagePasteBehavior {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CopyToDocumentFolder => "copy_to_document_folder",
            Self::CopyToAssetsFolder => "copy_to_assets_folder",
            Self::CopyToNamedAssetsFolder => "copy_to_named_assets_folder",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "copy_to_document_folder" => Self::CopyToDocumentFolder,
            "copy_to_assets_folder" => Self::CopyToAssetsFolder,
            "copy_to_named_assets_folder" => Self::CopyToNamedAssetsFolder,
            _ => Self::None,
        }
    }
}

/// User preferences persisted under the app config directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AppPreferences {
    pub(crate) startup_open: StartupOpenPreference,
    pub(crate) default_language_id: String,
    pub(crate) default_theme_id: String,
    pub(crate) show_table_headers: bool,
    pub(crate) image_paste_behavior: ImagePasteBehavior,
    pub(crate) auto_save: AutoSavePreference,
    pub(crate) spell_check: bool,
    pub(crate) auto_pair_brackets: bool,
    pub(crate) auto_pair_markdown: bool,
    pub(crate) editor_font_size: u8,
    pub(crate) editor_line_height_percent: u16,
    pub(crate) editor_content_width: u16,
    pub(crate) editor_font_family: String,
    pub(crate) workspace_sidebar_position: WorkspaceSidebarPosition,
    pub(crate) show_tab_bar_actions: bool,
    pub(crate) recent_editing_commands: Vec<String>,
    pub(crate) keybindings: BTreeMap<String, Vec<String>>,
    pub(crate) status_bar: StatusBarPreferences,
    pub(crate) document_loading: DocumentLoadingPreferences,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            startup_open: StartupOpenPreference::NewFile,
            default_language_id: DEFAULT_LANGUAGE_ID.into(),
            default_theme_id: DEFAULT_THEME_ID.into(),
            show_table_headers: true,
            image_paste_behavior: ImagePasteBehavior::None,
            auto_save: AutoSavePreference::Off,
            spell_check: true,
            auto_pair_brackets: true,
            auto_pair_markdown: true,
            editor_font_size: DEFAULT_EDITOR_FONT_SIZE,
            editor_line_height_percent: DEFAULT_EDITOR_LINE_HEIGHT_PERCENT,
            editor_content_width: DEFAULT_EDITOR_CONTENT_WIDTH,
            editor_font_family: String::new(),
            workspace_sidebar_position: WorkspaceSidebarPosition::Left,
            show_tab_bar_actions: false,
            recent_editing_commands: Vec::new(),
            keybindings: BTreeMap::new(),
            status_bar: StatusBarPreferences::default(),
            document_loading: DocumentLoadingPreferences::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DocumentLoadingPreset {
    #[default]
    Balanced,
    LowMemory,
    HighPerformance,
}

impl DocumentLoadingPreset {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::LowMemory => "low_memory",
            Self::HighPerformance => "high_performance",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "low_memory" => Self::LowMemory,
            "high_performance" => Self::HighPerformance,
            _ => Self::Balanced,
        }
    }

    fn core(self) -> gmark_document_core::LoadingPreset {
        match self {
            Self::Balanced => gmark_document_core::LoadingPreset::Balanced,
            Self::LowMemory => gmark_document_core::LoadingPreset::LowMemory,
            Self::HighPerformance => gmark_document_core::LoadingPreset::HighPerformance,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DocumentLoadingPreferences {
    pub(crate) preset: DocumentLoadingPreset,
    pub(crate) max_resident_mib: Option<u64>,
    pub(crate) max_resident_lines: Option<u64>,
    pub(crate) max_structural_units: Option<u64>,
}

impl DocumentLoadingPreferences {
    const MIB_RANGE: std::ops::RangeInclusive<u64> = 1..=1_024;
    const LINE_RANGE: std::ops::RangeInclusive<u64> = 1_000..=10_000_000;
    const STRUCTURE_RANGE: std::ops::RangeInclusive<u64> = 10_000..=50_000_000;

    pub(crate) fn policy(&self) -> gmark_document_core::LoadingPolicy {
        let valid = |value: Option<u64>, range: &std::ops::RangeInclusive<u64>| {
            value.filter(|value| range.contains(value))
        };
        gmark_document_core::LoadingPolicy {
            preset: self.preset.core(),
            max_resident_bytes: valid(self.max_resident_mib, &Self::MIB_RANGE)
                .and_then(|mib| mib.checked_mul(1024 * 1024)),
            max_resident_lines: valid(self.max_resident_lines, &Self::LINE_RANGE),
            max_structural_units: valid(self.max_structural_units, &Self::STRUCTURE_RANGE),
            force_safe_source: false,
        }
    }

    pub(crate) fn effective_max_resident_mib(&self) -> u64 {
        self.max_resident_mib
            .filter(|value| Self::MIB_RANGE.contains(value))
            .unwrap_or(self.preset.core().limits().max_resident_bytes / (1024 * 1024))
    }

    pub(crate) fn effective_max_resident_lines(&self) -> u64 {
        self.max_resident_lines
            .filter(|value| Self::LINE_RANGE.contains(value))
            .unwrap_or(self.preset.core().limits().max_resident_lines)
    }

    pub(crate) fn effective_max_structural_units(&self) -> u64 {
        self.max_structural_units
            .filter(|value| Self::STRUCTURE_RANGE.contains(value))
            .unwrap_or(self.preset.core().limits().max_structural_units)
    }

    /// 非法覆盖值仍保留在配置中供用户修正，但打开策略会逐字段回退到预设。
    pub(crate) fn has_invalid_override(&self) -> bool {
        self.max_resident_mib
            .is_some_and(|value| !Self::MIB_RANGE.contains(&value))
            || self
                .max_resident_lines
                .is_some_and(|value| !Self::LINE_RANGE.contains(&value))
            || self
                .max_structural_units
                .is_some_and(|value| !Self::STRUCTURE_RANGE.contains(&value))
    }
}

/// Status Bar Settings
struct StatusBarSettings {
    status_bar_enabled: bool,
    status_bar_show_word_count: bool,
    status_bar_show_cursor_position: bool,
    status_bar_show_sidebar_toggle: bool,
    status_bar_show_mode_switch: bool,
    custom_buttons: Vec<StatusBarButton>,
}

/// Runtime-accessible editor settings mirrored from [`AppPreferences`] so the
/// render path can read them without touching disk. Toggling persists the new
/// value back to the preferences file.
pub struct EditorSettings {
    show_table_headers: bool,
    auto_save: AutoSavePreference,
    spell_check: bool,
    auto_pair_brackets: bool,
    auto_pair_markdown: bool,
    editor_font_family: String,
    workspace_sidebar_position: WorkspaceSidebarPosition,
    show_tab_bar_actions: bool,
    status_bar_settings: StatusBarSettings,
}

impl Global for EditorSettings {}

impl EditorSettings {
    #[cfg(test)]
    pub fn init(
        cx: &mut App,
        show_table_headers: bool,
        auto_save: AutoSavePreference,
        spell_check: bool,
    ) {
        Self::init_with_typography(
            cx,
            show_table_headers,
            auto_save,
            spell_check,
            DEFAULT_EDITOR_FONT_SIZE,
            DEFAULT_EDITOR_LINE_HEIGHT_PERCENT,
            DEFAULT_EDITOR_CONTENT_WIDTH,
            "",
            WorkspaceSidebarPosition::Left,
            false,
        );
    }

    pub fn init_with_typography(
        cx: &mut App,
        show_table_headers: bool,
        auto_save: AutoSavePreference,
        spell_check: bool,
        editor_font_size: u8,
        editor_line_height_percent: u16,
        editor_content_width: u16,
        editor_font_family: &str,
        workspace_sidebar_position: WorkspaceSidebarPosition,
        show_tab_bar_actions: bool,
    ) {
        let loaded_preferences = read_app_preferences().ok();
        let status_bar = loaded_preferences
            .as_ref()
            .map(|preferences| preferences.status_bar.clone())
            .unwrap_or_default();
        let auto_pair_brackets = loaded_preferences
            .as_ref()
            .map(|preferences| preferences.auto_pair_brackets)
            .unwrap_or(true);
        let auto_pair_markdown = loaded_preferences
            .as_ref()
            .map(|preferences| preferences.auto_pair_markdown)
            .unwrap_or(true);
        Self::set_global(
            cx,
            show_table_headers,
            auto_save,
            spell_check,
            auto_pair_brackets,
            auto_pair_markdown,
            editor_font_family,
            workspace_sidebar_position,
            show_tab_bar_actions,
            &status_bar,
        );
        cx.update_global::<ThemeManager, _>(|theme_manager, _cx| {
            theme_manager.set_editor_typography(editor_font_size, editor_line_height_percent);
            theme_manager.set_editor_content_width(editor_content_width);
        });
    }

    fn set_global(
        cx: &mut App,
        show_table_headers: bool,
        auto_save: AutoSavePreference,
        spell_check: bool,
        auto_pair_brackets: bool,
        auto_pair_markdown: bool,
        editor_font_family: &str,
        workspace_sidebar_position: WorkspaceSidebarPosition,
        show_tab_bar_actions: bool,
        status_bar: &StatusBarPreferences,
    ) {
        cx.set_global(Self {
            show_table_headers,
            auto_save,
            spell_check,
            auto_pair_brackets,
            auto_pair_markdown,
            editor_font_family: normalize_editor_font_family(editor_font_family),
            workspace_sidebar_position,
            show_tab_bar_actions,
            status_bar_settings: StatusBarSettings {
                status_bar_enabled: status_bar.enabled,
                status_bar_show_word_count: status_bar.show_word_count,
                status_bar_show_cursor_position: status_bar.show_cursor_position,
                status_bar_show_sidebar_toggle: status_bar.show_sidebar_toggle,
                status_bar_show_mode_switch: status_bar.show_mode_switch,
                custom_buttons: status_bar.custom_buttons.clone(),
            },
        });
    }

    /// Whether table top rows are styled as headers. Defaults to `true` when
    /// the global has not been installed (e.g. in unit tests).
    pub fn show_table_headers(cx: &App) -> bool {
        cx.try_global::<Self>()
            .map(|settings| settings.show_table_headers)
            .unwrap_or(true)
    }

    pub fn set_show_table_headers(cx: &mut App, show_table_headers: bool) {
        let status_bar = cx
            .try_global::<Self>()
            .map(|s| StatusBarPreferences {
                enabled: s.status_bar_settings.status_bar_enabled,
                show_word_count: s.status_bar_settings.status_bar_show_word_count,
                show_cursor_position: s.status_bar_settings.status_bar_show_cursor_position,
                show_sidebar_toggle: s.status_bar_settings.status_bar_show_sidebar_toggle,
                show_mode_switch: s.status_bar_settings.status_bar_show_mode_switch,
                custom_buttons: s.status_bar_settings.custom_buttons.clone(),
            })
            .unwrap_or_default();
        let auto_save = cx
            .try_global::<Self>()
            .map(|settings| settings.auto_save)
            .unwrap_or_default();
        let spell_check = cx
            .try_global::<Self>()
            .map(|settings| settings.spell_check)
            .unwrap_or(true);
        let auto_pair_brackets = cx
            .try_global::<Self>()
            .map(|settings| settings.auto_pair_brackets)
            .unwrap_or(true);
        let auto_pair_markdown = cx
            .try_global::<Self>()
            .map(|settings| settings.auto_pair_markdown)
            .unwrap_or(true);
        let editor_font_family = cx
            .try_global::<Self>()
            .map(|settings| settings.editor_font_family.clone())
            .unwrap_or_default();
        let workspace_sidebar_position = cx
            .try_global::<Self>()
            .map(|settings| settings.workspace_sidebar_position)
            .unwrap_or_default();
        let show_tab_bar_actions = cx
            .try_global::<Self>()
            .map(|settings| settings.show_tab_bar_actions)
            .unwrap_or(false);
        Self::set_global(
            cx,
            show_table_headers,
            auto_save,
            spell_check,
            auto_pair_brackets,
            auto_pair_markdown,
            &editor_font_family,
            workspace_sidebar_position,
            show_tab_bar_actions,
            &status_bar,
        );
        match read_app_preferences() {
            Ok(mut preferences) => {
                preferences.show_table_headers = show_table_headers;
                if let Err(err) = save_app_preferences(&preferences) {
                    eprintln!("failed to save table header preference: {err}");
                }
            }
            Err(err) => eprintln!("failed to read table header preference: {err}"),
        }
    }

    pub fn status_bar_preferences(cx: &App) -> StatusBarPreferences {
        cx.try_global::<Self>()
            .map(|s| StatusBarPreferences {
                enabled: s.status_bar_settings.status_bar_enabled,
                show_word_count: s.status_bar_settings.status_bar_show_word_count,
                show_cursor_position: s.status_bar_settings.status_bar_show_cursor_position,
                show_sidebar_toggle: s.status_bar_settings.status_bar_show_sidebar_toggle,
                show_mode_switch: s.status_bar_settings.status_bar_show_mode_switch,
                custom_buttons: s.status_bar_settings.custom_buttons.clone(),
            })
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn set_status_bar_preferences_for_test(
        cx: &mut App,
        preferences: StatusBarPreferences,
    ) {
        cx.update_global::<Self, _>(|settings, _cx| {
            settings.status_bar_settings = StatusBarSettings {
                status_bar_enabled: preferences.enabled,
                status_bar_show_word_count: preferences.show_word_count,
                status_bar_show_cursor_position: preferences.show_cursor_position,
                status_bar_show_sidebar_toggle: preferences.show_sidebar_toggle,
                status_bar_show_mode_switch: preferences.show_mode_switch,
                custom_buttons: preferences.custom_buttons,
            };
        });
    }

    pub(crate) fn auto_save(cx: &App) -> AutoSavePreference {
        cx.try_global::<Self>()
            .map(|settings| settings.auto_save)
            .unwrap_or_default()
    }

    pub(crate) fn spell_check(cx: &App) -> bool {
        cx.try_global::<Self>()
            .map(|settings| settings.spell_check)
            .unwrap_or(false)
    }

    pub(crate) fn auto_pair_brackets(cx: &App) -> bool {
        cx.try_global::<Self>()
            .map(|settings| settings.auto_pair_brackets)
            .unwrap_or(true)
    }

    pub(crate) fn auto_pair_markdown(cx: &App) -> bool {
        cx.try_global::<Self>()
            .map(|settings| settings.auto_pair_markdown)
            .unwrap_or(true)
    }

    pub(crate) fn editor_font_family(cx: &App) -> String {
        cx.try_global::<Self>()
            .map(|settings| settings.editor_font_family.clone())
            .unwrap_or_default()
    }

    pub(crate) fn workspace_sidebar_position(cx: &App) -> WorkspaceSidebarPosition {
        cx.try_global::<Self>()
            .map(|settings| settings.workspace_sidebar_position)
            .unwrap_or_default()
    }

    pub(crate) fn show_tab_bar_actions(cx: &App) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|settings| settings.show_tab_bar_actions)
    }

    #[cfg(test)]
    pub(crate) fn set_workspace_sidebar_position_for_test(
        cx: &mut App,
        position: WorkspaceSidebarPosition,
    ) {
        cx.update_global::<Self, _>(|settings, _cx| {
            settings.workspace_sidebar_position = position;
        });
    }

    #[cfg(test)]
    pub(crate) fn set_show_tab_bar_actions_for_test(cx: &mut App, show: bool) {
        cx.update_global::<Self, _>(|settings, _cx| {
            settings.show_tab_bar_actions = show;
        });
    }
}
#[path = "preferences_parts/storage.rs"]
mod storage;
use storage::PreferencesNav;
#[cfg(test)]
use storage::*;
pub(crate) use storage::{
    apply_configured_language, apply_configured_theme, first_existing_recent_markdown_file,
    import_language_config_and_select, import_theme_config_and_select,
    load_or_create_app_preferences, read_app_preferences, save_app_preferences,
    save_preferences_from_window,
};

#[path = "preferences_parts/window.rs"]
mod window;
pub(crate) use window::{localized_shortcut_command_label, open_preferences_window};
