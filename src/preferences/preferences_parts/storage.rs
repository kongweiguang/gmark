// @author kongweiguang

use super::*;

#[derive(Serialize)]
struct PreferencesFile {
    startup: StartupPreferencesFile,
    updates: UpdatesPreferencesFile,
    language: LanguagePreferencesFile,
    theme: ThemePreferencesFile,
    editor: EditorPreferencesFile,
    status_bar: StatusBarPreferencesFile,
    documents: DocumentsPreferencesFile,
    keybindings: BTreeMap<String, Vec<String>>,
}

#[derive(Serialize)]
struct UpdatesPreferencesFile {
    auto_check: bool,
}

#[derive(Serialize)]
struct DocumentsPreferencesFile {
    loading: DocumentLoadingPreferencesFile,
}

#[derive(Serialize)]
struct DocumentLoadingPreferencesFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_resident_mib: Option<u64>,
}

#[derive(Serialize)]
struct StartupPreferencesFile {
    open: String,
}

#[derive(Serialize)]
struct EditorPreferencesFile {
    show_table_headers: bool,
    resource_insert_behavior: String,
    /// Kept for one compatible release so older GMark versions retain image
    /// paste behavior when a user downgrades.
    image_paste_behavior: String,
    auto_save: String,
    spell_check: bool,
    auto_pair_brackets: bool,
    auto_pair_markdown: bool,
    code_folding: bool,
    format_on_save: bool,
    font_size: u8,
    line_height_percent: u16,
    content_width: u16,
    show_tab_bar_actions: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    recent_editing_commands: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    font_family: String,
}

#[derive(Serialize)]
struct LanguagePreferencesFile {
    default_language_id: String,
}

#[derive(Serialize)]
struct ThemePreferencesFile {
    appearance: String,
    palette: String,
}

#[derive(Serialize)]
struct StatusBarPreferencesFile {
    enabled: bool,
    show_word_count: bool,
    show_cursor_position: bool,
    show_sidebar_toggle: bool,
    show_mode_switch: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    custom_buttons: Vec<StatusBarButton>,
}

impl From<&StatusBarPreferences> for StatusBarPreferencesFile {
    fn from(value: &StatusBarPreferences) -> Self {
        Self {
            enabled: value.enabled,
            show_word_count: value.show_word_count,
            show_cursor_position: value.show_cursor_position,
            show_sidebar_toggle: value.show_sidebar_toggle,
            show_mode_switch: value.show_mode_switch,
            custom_buttons: value.custom_buttons.clone(),
        }
    }
}

impl From<&AppPreferences> for PreferencesFile {
    fn from(value: &AppPreferences) -> Self {
        Self {
            startup: StartupPreferencesFile {
                open: value.startup_open.as_str().into(),
            },
            updates: UpdatesPreferencesFile {
                auto_check: value.auto_check_updates,
            },
            language: LanguagePreferencesFile {
                default_language_id: value.default_language_id.clone(),
            },
            theme: ThemePreferencesFile {
                appearance: value.theme_appearance.as_str().into(),
                palette: value.theme_palette.as_str().into(),
            },
            editor: EditorPreferencesFile {
                show_table_headers: value.show_table_headers,
                resource_insert_behavior: value.resource_insert_behavior().as_str().into(),
                image_paste_behavior: value.image_paste_behavior.as_str().into(),
                auto_save: value.auto_save.as_str().into(),
                spell_check: value.spell_check,
                auto_pair_brackets: value.auto_pair_brackets,
                auto_pair_markdown: value.auto_pair_markdown,
                code_folding: value.code_folding,
                format_on_save: value.format_on_save,
                font_size: value.editor_font_size,
                line_height_percent: value.editor_line_height_percent,
                content_width: value.editor_content_width,
                show_tab_bar_actions: value.show_tab_bar_actions,
                recent_editing_commands: value.recent_editing_commands.clone(),
                font_family: value.editor_font_family.clone(),
            },
            status_bar: StatusBarPreferencesFile::from(&value.status_bar),
            documents: DocumentsPreferencesFile {
                loading: DocumentLoadingPreferencesFile {
                    max_resident_mib: value.document_loading.max_resident_mib,
                },
            },
            keybindings: normalize_shortcut_config(&value.keybindings),
        }
    }
}

pub(crate) fn read_app_preferences() -> anyhow::Result<AppPreferences> {
    read_app_preferences_with_dirs(&GmarkConfigDirs::from_system()?)
}

pub(crate) fn read_app_preferences_with_dirs(
    dirs: &GmarkConfigDirs,
) -> anyhow::Result<AppPreferences> {
    let path = dirs.app_config_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AppPreferences::default());
        }
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read '{}'", path.display()));
        }
    };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else {
        return Ok(AppPreferences::default());
    };

    Ok(app_preferences_from_toml_value(&value, DEFAULT_LANGUAGE_ID))
}

pub(crate) fn load_or_create_app_preferences() -> anyhow::Result<AppPreferences> {
    let dirs = GmarkConfigDirs::from_system()?;
    load_or_create_app_preferences_with_dirs_and_locales(&dirs, sys_locale::get_locales())
}

fn app_preferences_from_toml_value(
    value: &toml::Value,
    fallback_language_id: &str,
) -> AppPreferences {
    let startup_open = value
        .get("startup")
        .and_then(|startup| startup.get("open"))
        .and_then(|open| open.as_str())
        .map(StartupOpenPreference::from_str)
        .unwrap_or(StartupOpenPreference::NewFile);
    let default_language_id = value
        .get("language")
        .and_then(|language| language.get("default_language_id"))
        .and_then(|id| id.as_str())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(fallback_language_id)
        .to_string();
    let auto_check_updates = value
        .get("updates")
        .and_then(|updates| updates.get("auto_check"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    // The old `default_theme_id` field is intentionally ignored. A partial,
    // invalid, or legacy theme block falls back to the complete new default
    // pair instead of mixing an old dark/light choice with a new palette.
    let (theme_appearance, theme_palette) = match (
        value
            .get("theme")
            .and_then(|theme| theme.get("appearance"))
            .and_then(|value| value.as_str())
            .and_then(ThemeAppearance::parse),
        value
            .get("theme")
            .and_then(|theme| theme.get("palette"))
            .and_then(|value| value.as_str())
            .and_then(ThemePalette::parse),
    ) {
        (Some(appearance), Some(palette)) => (appearance, palette),
        _ => (ThemeAppearance::System, ThemePalette::Xcode),
    };
    let keybindings = value
        .get("keybindings")
        .and_then(|keybindings| keybindings.as_table())
        .map(|table| {
            table
                .iter()
                .filter_map(|(key, value)| {
                    let keys = value
                        .as_array()?
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_string))
                        .collect::<Vec<_>>();
                    Some((key.clone(), keys))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .map(|keybindings| normalize_shortcut_config(&keybindings))
        .unwrap_or_default();

    let show_table_headers = value
        .get("editor")
        .and_then(|editor| editor.get("show_table_headers"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let legacy_image_paste_behavior = value
        .get("editor")
        .and_then(|editor| editor.get("image_paste_behavior"))
        .and_then(|value| value.as_str())
        .map(ImagePasteBehavior::from_str)
        .unwrap_or(ImagePasteBehavior::None);
    let image_paste_behavior = value
        .get("editor")
        .and_then(|editor| editor.get("resource_insert_behavior"))
        .and_then(|value| value.as_str())
        .map(ImagePasteBehavior::from_str)
        .unwrap_or(legacy_image_paste_behavior);
    let auto_save = value
        .get("editor")
        .and_then(|editor| editor.get("auto_save"))
        .and_then(|value| value.as_str())
        .map(AutoSavePreference::from_str)
        .unwrap_or_default();
    let spell_check = value
        .get("editor")
        .and_then(|editor| editor.get("spell_check"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let auto_pair_brackets = value
        .get("editor")
        .and_then(|editor| editor.get("auto_pair_brackets"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let auto_pair_markdown = value
        .get("editor")
        .and_then(|editor| editor.get("auto_pair_markdown"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let code_folding = value
        .get("editor")
        .and_then(|editor| editor.get("code_folding"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let format_on_save = value
        .get("editor")
        .and_then(|editor| editor.get("format_on_save"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let editor_font_size = value
        .get("editor")
        .and_then(|editor| editor.get("font_size"))
        .and_then(|value| value.as_integer())
        .and_then(|value| u8::try_from(value).ok())
        .unwrap_or(DEFAULT_EDITOR_FONT_SIZE)
        .clamp(MIN_EDITOR_FONT_SIZE, MAX_EDITOR_FONT_SIZE);
    let editor_line_height_percent = normalize_editor_line_height_percent(
        value
            .get("editor")
            .and_then(|editor| editor.get("line_height_percent"))
            .and_then(|value| value.as_integer())
            .and_then(|value| u16::try_from(value).ok())
            .unwrap_or(DEFAULT_EDITOR_LINE_HEIGHT_PERCENT),
    );
    let editor_content_width = normalize_editor_content_width(
        value
            .get("editor")
            .and_then(|editor| editor.get("content_width"))
            .and_then(|value| value.as_integer())
            .and_then(|value| u16::try_from(value).ok())
            .unwrap_or(DEFAULT_EDITOR_CONTENT_WIDTH),
    );
    let editor_font_family = value
        .get("editor")
        .and_then(|editor| editor.get("font_family"))
        .and_then(|value| value.as_str())
        .map(normalize_editor_font_family)
        .unwrap_or_default();
    let show_tab_bar_actions = value
        .get("editor")
        .and_then(|editor| editor.get("show_tab_bar_actions"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let recent_editing_commands = value
        .get("editor")
        .and_then(|editor| editor.get("recent_editing_commands"))
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .take(5)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let loading = value
        .get("documents")
        .and_then(|documents| documents.get("loading"));
    let document_loading = DocumentLoadingPreferences {
        max_resident_mib: loading
            .and_then(|loading| loading.get("max_resident_mib"))
            .and_then(|value| value.as_integer())
            .and_then(|value| u64::try_from(value).ok()),
    };

    let status_bar = value
        .get("status_bar")
        .map(|sb| {
            let enabled = sb.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            let show_word_count = sb
                .get("show_word_count")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let show_cursor_position = sb
                .get("show_cursor_position")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let show_sidebar_toggle = sb
                .get("show_sidebar_toggle")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let show_mode_switch = sb
                .get("show_mode_switch")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let custom_buttons = sb
                .get("custom_buttons")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            let id = item.get("id")?.as_str()?.to_string();
                            let label = item.get("label")?.as_str()?.to_string();
                            Some(StatusBarButton {
                                id,
                                label,
                                action_id: item
                                    .get("action_id")
                                    .and_then(|a| a.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            StatusBarPreferences {
                enabled,
                show_word_count,
                show_cursor_position,
                show_sidebar_toggle,
                show_mode_switch,
                custom_buttons,
            }
        })
        .unwrap_or_default();

    AppPreferences {
        startup_open,
        auto_check_updates,
        default_language_id,
        theme_appearance,
        theme_palette,
        show_table_headers,
        image_paste_behavior,
        auto_save,
        spell_check,
        auto_pair_brackets,
        auto_pair_markdown,
        code_folding,
        format_on_save,
        editor_font_size,
        editor_line_height_percent,
        editor_content_width,
        editor_font_family,
        show_tab_bar_actions,
        recent_editing_commands,
        keybindings,
        status_bar,
        document_loading,
    }
}

fn detected_language_id_from_locales<I, S>(locales: I) -> &'static str
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    language_id_for_locale_preferences(locales)
}

pub(super) fn load_or_create_app_preferences_with_dirs_and_locales<I, S>(
    dirs: &GmarkConfigDirs,
    locales: I,
) -> anyhow::Result<AppPreferences>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let detected_language_id = detected_language_id_from_locales(locales);
    let path = dirs.app_config_file();
    let preferences = match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str::<toml::Value>(&text)
            .map(|value| app_preferences_from_toml_value(&value, detected_language_id))
            .unwrap_or_else(|_| AppPreferences {
                default_language_id: detected_language_id.into(),
                ..AppPreferences::default()
            }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => AppPreferences {
            default_language_id: detected_language_id.into(),
            ..AppPreferences::default()
        },
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read '{}'", path.display()));
        }
    };
    save_app_preferences_with_dirs(&preferences, dirs)?;
    Ok(preferences)
}

pub(crate) fn save_app_preferences(preferences: &AppPreferences) -> anyhow::Result<()> {
    save_app_preferences_with_dirs(preferences, &GmarkConfigDirs::from_system()?)
}

pub(crate) fn save_app_preferences_with_dirs(
    preferences: &AppPreferences,
    dirs: &GmarkConfigDirs,
) -> anyhow::Result<()> {
    let path = dirs.app_config_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }
    let text = toml::to_string_pretty(&PreferencesFile::from(preferences))?;
    std::fs::write(&path, text).with_context(|| format!("failed to write '{}'", path.display()))
}

pub(crate) fn first_existing_recent_markdown_file() -> Option<PathBuf> {
    let recent_files = read_recent_files().ok()?;
    recent_files.into_iter().find(|path| path.is_file())
}

pub(crate) fn apply_configured_language(cx: &mut App, language_id: &str) -> anyhow::Result<bool> {
    let mut applied = false;
    let changed = cx.update_global::<I18nManager, _>(|i18n_manager, _cx| {
        let changed = i18n_manager.set_language_by_id(language_id);
        applied = changed || i18n_manager.current_language_id() == language_id;
        changed
    });
    if !applied {
        return Ok(false);
    }
    update_app_preferences(|preferences| {
        preferences.default_language_id = language_id.into();
    })?;
    Ok(changed)
}

pub(crate) fn import_language_config_and_select(
    cx: &mut App,
    path: impl AsRef<std::path::Path>,
) -> anyhow::Result<String> {
    let imported_id = cx.update_global::<I18nManager, _>(|i18n_manager, _cx| {
        i18n_manager.import_language_config(path)
    })?;
    update_app_preferences(|preferences| {
        preferences.default_language_id = imported_id.clone();
    })?;
    Ok(imported_id)
}

pub(crate) fn save_preferences_from_window(
    startup_open: StartupOpenPreference,
    auto_check_updates: bool,
    auto_save: AutoSavePreference,
    spell_check: bool,
    auto_pair_brackets: bool,
    auto_pair_markdown: bool,
    code_folding: bool,
    format_on_save: bool,
    editor_font_size: u8,
    editor_line_height_percent: u16,
    editor_content_width: u16,
    editor_font_family: &str,
    show_tab_bar_actions: bool,
    theme_appearance: ThemeAppearance,
    theme_palette: ThemePalette,
    default_language_id: &str,
    image_paste_behavior: ImagePasteBehavior,
    keybindings: BTreeMap<String, Vec<String>>,
    document_loading: &DocumentLoadingPreferences,
    status_bar: &StatusBarPreferences,
) -> anyhow::Result<AppPreferences> {
    let dirs = GmarkConfigDirs::from_system()?;
    save_preferences_from_window_with_dirs(
        startup_open,
        auto_check_updates,
        auto_save,
        spell_check,
        auto_pair_brackets,
        auto_pair_markdown,
        code_folding,
        format_on_save,
        editor_font_size,
        editor_line_height_percent,
        editor_content_width,
        editor_font_family,
        show_tab_bar_actions,
        theme_appearance,
        theme_palette,
        default_language_id,
        image_paste_behavior,
        keybindings,
        document_loading,
        status_bar,
        &dirs,
    )
}

pub(super) fn save_preferences_from_window_with_dirs(
    startup_open: StartupOpenPreference,
    auto_check_updates: bool,
    auto_save: AutoSavePreference,
    spell_check: bool,
    auto_pair_brackets: bool,
    auto_pair_markdown: bool,
    code_folding: bool,
    format_on_save: bool,
    editor_font_size: u8,
    editor_line_height_percent: u16,
    editor_content_width: u16,
    editor_font_family: &str,
    show_tab_bar_actions: bool,
    theme_appearance: ThemeAppearance,
    theme_palette: ThemePalette,
    default_language_id: &str,
    image_paste_behavior: ImagePasteBehavior,
    keybindings: BTreeMap<String, Vec<String>>,
    document_loading: &DocumentLoadingPreferences,
    status_bar: &StatusBarPreferences,
    dirs: &GmarkConfigDirs,
) -> anyhow::Result<AppPreferences> {
    let mut preferences =
        load_or_create_app_preferences_with_dirs_and_locales(dirs, sys_locale::get_locales())?;
    preferences.startup_open = startup_open;
    preferences.auto_check_updates = auto_check_updates;
    preferences.auto_save = auto_save;
    preferences.spell_check = spell_check;
    preferences.auto_pair_brackets = auto_pair_brackets;
    preferences.auto_pair_markdown = auto_pair_markdown;
    preferences.code_folding = code_folding;
    preferences.format_on_save = format_on_save;
    preferences.editor_font_size =
        editor_font_size.clamp(MIN_EDITOR_FONT_SIZE, MAX_EDITOR_FONT_SIZE);
    preferences.editor_line_height_percent =
        normalize_editor_line_height_percent(editor_line_height_percent);
    preferences.editor_content_width = normalize_editor_content_width(editor_content_width);
    preferences.editor_font_family = normalize_editor_font_family(editor_font_family);
    preferences.show_tab_bar_actions = show_tab_bar_actions;
    preferences.theme_appearance = theme_appearance;
    preferences.theme_palette = theme_palette;
    preferences.default_language_id = default_language_id.into();
    preferences.image_paste_behavior = image_paste_behavior;
    preferences.keybindings = normalize_shortcut_config(&keybindings);
    preferences.document_loading = document_loading.clone();
    preferences.status_bar = status_bar.clone();
    save_app_preferences_with_dirs(&preferences, dirs)?;
    Ok(preferences)
}

fn update_app_preferences(
    update: impl FnOnce(&mut AppPreferences),
) -> anyhow::Result<AppPreferences> {
    let mut preferences = load_or_create_app_preferences()?;
    update(&mut preferences);
    save_app_preferences(&preferences)?;
    Ok(preferences)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PreferencesNav {
    File,
    Editor,
    Theme,
    Image,
    Shortcuts,
    StatusBar,
}
