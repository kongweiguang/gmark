// @author kongweiguang

use super::*;

/// Metadata for a selectable theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeCatalogEntry {
    pub id: String,
    pub name: String,
}

const BUILTIN_THEME_GMARK_ID: &str = "gmark";
const BUILTIN_THEME_GMARK_NAME: &str = "gmark";
const BUILTIN_THEME_GMARK_LIGHT_ID: &str = "gmark-light";
pub(super) const BUILTIN_THEME_GMARK_LIGHT_NAME: &str = "gmark Light";
pub(crate) const SYSTEM_THEME_ID: &str = "system";

/// 系统主题模式只保存用户意图；渲染与导出始终使用解析后的具体主题。
pub(crate) fn resolved_system_theme_id(appearance: WindowAppearance) -> &'static str {
    match appearance {
        WindowAppearance::Light | WindowAppearance::VibrantLight => BUILTIN_THEME_GMARK_LIGHT_ID,
        WindowAppearance::Dark | WindowAppearance::VibrantDark => BUILTIN_THEME_GMARK_ID,
    }
}

fn builtin_theme_catalog() -> Vec<ThemeCatalogEntry> {
    vec![
        ThemeCatalogEntry {
            id: BUILTIN_THEME_GMARK_ID.into(),
            name: BUILTIN_THEME_GMARK_NAME.into(),
        },
        ThemeCatalogEntry {
            id: BUILTIN_THEME_GMARK_LIGHT_ID.into(),
            name: BUILTIN_THEME_GMARK_LIGHT_NAME.into(),
        },
    ]
}

#[derive(Debug, Clone)]
pub(super) struct CustomThemeEntry {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) creator: String,
    pub(super) base_theme_id: String,
    pub(super) theme: Theme,
}

/// Global singleton that holds the current [`Theme`].
///
/// Registered via [`Global`] so every component can access it through
/// `cx.global::<ThemeManager>().current()` without passing props.
pub struct ThemeManager {
    current: Arc<Theme>,
    current_theme_id: String,
    selected_theme_id: String,
    custom_themes: Vec<CustomThemeEntry>,
    theme_catalog: Vec<ThemeCatalogEntry>,
    editor_typography_override: Option<(u8, u16)>,
    editor_content_width_override: Option<u16>,
}

impl Global for ThemeManager {}

impl Default for ThemeManager {
    fn default() -> Self {
        Self {
            current: Arc::new(Theme::default_theme()),
            current_theme_id: BUILTIN_THEME_GMARK_ID.into(),
            selected_theme_id: BUILTIN_THEME_GMARK_ID.into(),
            custom_themes: Vec::new(),
            theme_catalog: builtin_theme_catalog(),
            editor_typography_override: None,
            editor_content_width_override: None,
        }
    }
}

impl ThemeManager {
    /// Installs the configured theme into GPUI's global state.
    #[cfg(test)]
    pub fn init(cx: &mut App) {
        let theme_id = crate::config::read_app_preferences()
            .map(|preferences| preferences.default_theme_id)
            .unwrap_or_else(|_| BUILTIN_THEME_GMARK_ID.into());
        Self::init_with_theme_id(cx, &theme_id);
    }

    /// Installs a specific theme into GPUI's global state.
    pub fn init_with_theme_id(cx: &mut App, theme_id: &str) {
        let mut manager = Self::default();
        if let Ok(dirs) = GmarkConfigDirs::from_system()
            && let Err(err) = manager.load_custom_themes_from_dirs(&dirs)
        {
            eprintln!("failed to load custom themes: {err}");
        }
        let _ = manager.set_theme_preference(theme_id, cx.window_appearance());
        cx.set_global(manager);
    }

    /// Returns the currently active theme.
    pub fn current(&self) -> &Theme {
        &self.current
    }

    /// Returns an `Arc` clone of the currently active theme — O(1), no
    /// per-field copy. Use this in hot render paths instead of cloning the
    /// whole `Theme` struct (which has ~200 fields and a `String` name).
    pub fn current_arc(&self) -> Arc<Theme> {
        self.current.clone()
    }

    /// Returns the identifier of the currently active theme.
    #[cfg(test)]
    pub fn current_theme_id(&self) -> &str {
        &self.current_theme_id
    }

    /// Returns the persisted selection, which may be `system` while the active theme is concrete.
    pub fn selected_theme_id(&self) -> &str {
        &self.selected_theme_id
    }

    /// Returns all built-in and imported themes exposed in the native menu.
    pub fn available_themes(&self) -> &[ThemeCatalogEntry] {
        &self.theme_catalog
    }

    /// Applies editor typography as a proportional user override without changing chrome text.
    pub fn set_editor_typography(&mut self, font_size: u8, line_height_percent: u16) {
        self.editor_typography_override = Some((font_size, line_height_percent));
        let mut theme = (*self.current).clone();
        let font_size = f32::from(font_size.clamp(12, 24));
        let line_height = f32::from(line_height_percent.clamp(120, 200)) / 100.0;
        let scale = font_size / theme.typography.text_size.max(1.0);
        theme.typography.text_size = font_size;
        theme.typography.text_line_height = line_height;
        theme.typography.h1_size *= scale;
        theme.typography.h2_size *= scale;
        theme.typography.h3_size *= scale;
        theme.typography.h4_size *= scale;
        theme.typography.h5_size *= scale;
        theme.typography.h6_size *= scale;
        theme.typography.code_size *= scale;
        self.current = Arc::new(theme);
    }

    /// Applies the user reading-width override shared by prose, tables, images and Split preview.
    pub fn set_editor_content_width(&mut self, content_width: u16) {
        self.editor_content_width_override = Some(content_width);
        let mut theme = (*self.current).clone();
        theme.dimensions.centered_max_width = f32::from(content_width.clamp(680, 1200));
        self.current = Arc::new(theme);
    }

    fn apply_editor_overrides(&mut self) {
        if let Some((font_size, line_height_percent)) = self.editor_typography_override {
            self.set_editor_typography(font_size, line_height_percent);
        }
        if let Some(content_width) = self.editor_content_width_override {
            self.set_editor_content_width(content_width);
        }
    }

    /// Activates a theme by identifier.
    pub fn set_theme_by_id(&mut self, theme_id: &str) -> bool {
        let changed = match theme_id {
            id if id == BUILTIN_THEME_GMARK_ID => {
                self.current = Arc::new(Theme::default_theme());
                self.current_theme_id = BUILTIN_THEME_GMARK_ID.into();
                true
            }
            id if id == BUILTIN_THEME_GMARK_LIGHT_ID => {
                self.current = Arc::new(Theme::light_theme());
                self.current_theme_id = BUILTIN_THEME_GMARK_LIGHT_ID.into();
                true
            }
            id => {
                let Some(entry) = self.custom_themes.iter().find(|entry| entry.id == id) else {
                    return false;
                };
                self.current = Arc::new(entry.theme.clone());
                self.current_theme_id = entry.id.clone();
                true
            }
        };
        if changed {
            self.selected_theme_id = theme_id.into();
            self.apply_editor_overrides();
        }
        changed
    }

    /// Applies either a fixed theme or the theme resolved from the current platform appearance.
    pub fn set_theme_preference(&mut self, theme_id: &str, appearance: WindowAppearance) -> bool {
        if theme_id != SYSTEM_THEME_ID {
            return self.set_theme_by_id(theme_id);
        }
        let resolved = resolved_system_theme_id(appearance);
        let changed =
            self.current_theme_id != resolved || self.selected_theme_id != SYSTEM_THEME_ID;
        if !self.set_theme_by_id(resolved) {
            return false;
        }
        self.selected_theme_id = SYSTEM_THEME_ID.into();
        changed
    }

    /// Refreshes a selected system theme and leaves fixed/custom themes untouched.
    pub fn update_system_appearance(&mut self, appearance: WindowAppearance) -> bool {
        if self.selected_theme_id != SYSTEM_THEME_ID {
            return false;
        }
        let resolved = resolved_system_theme_id(appearance);
        if self.current_theme_id == resolved {
            return false;
        }
        let changed = self.set_theme_by_id(resolved);
        if changed {
            self.selected_theme_id = SYSTEM_THEME_ID.into();
        }
        changed
    }

    /// Imports a user theme pack, persists a normalized copy, and activates it.
    pub fn import_theme_config(&mut self, path: impl AsRef<Path>) -> anyhow::Result<String> {
        let dirs = GmarkConfigDirs::from_system()?;
        self.import_theme_config_with_dirs(path, &dirs)
    }

    pub(super) fn import_theme_config_with_dirs(
        &mut self,
        path: impl AsRef<Path>,
        dirs: &GmarkConfigDirs,
    ) -> anyhow::Result<String> {
        let raw = read_json_or_jsonc(path.as_ref())?;
        let default_base_theme_id = self.theme_import_base_theme_id();
        let (entry, normalized) =
            custom_theme_from_value_with_default_base(raw, default_base_theme_id.as_str())?;
        let file_name = format!(
            "{}_{}.json",
            sanitize_config_file_stem(&entry.name),
            sanitize_config_file_stem(&entry.creator)
        );
        let themes_dir = dirs.themes_dir();
        std::fs::create_dir_all(&themes_dir)?;
        std::fs::write(
            themes_dir.join(file_name),
            serde_json::to_string_pretty(&normalized)?,
        )?;
        let imported_id = entry.id.clone();
        self.upsert_custom_theme(entry);
        self.set_theme_by_id(&imported_id);
        Ok(imported_id)
    }

    pub(super) fn load_custom_themes_from_dirs(
        &mut self,
        dirs: &GmarkConfigDirs,
    ) -> anyhow::Result<()> {
        let themes_dir = dirs.themes_dir();
        if !themes_dir.exists() {
            return Ok(());
        }

        let mut loaded = Vec::new();
        for entry in std::fs::read_dir(&themes_dir)? {
            let path = entry?.path();
            if path.is_file() {
                match read_json_or_jsonc(&path)
                    .and_then(|value| custom_theme_from_value(value).map(|(entry, _)| entry))
                {
                    Ok(entry) => loaded.push(entry),
                    Err(err) => {
                        eprintln!("skipping custom theme config '{}': {err}", path.display())
                    }
                }
            }
        }
        loaded.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then(left.creator.cmp(&right.creator))
        });
        for entry in loaded {
            self.upsert_custom_theme(entry);
        }
        Ok(())
    }

    fn upsert_custom_theme(&mut self, entry: CustomThemeEntry) {
        if let Some(existing) = self
            .custom_themes
            .iter_mut()
            .find(|existing| existing.id == entry.id)
        {
            *existing = entry;
        } else {
            self.custom_themes.push(entry);
        }
        self.rebuild_theme_catalog();
    }

    fn rebuild_theme_catalog(&mut self) {
        let mut catalog = builtin_theme_catalog();
        catalog.extend(self.custom_themes.iter().map(|entry| ThemeCatalogEntry {
            id: entry.id.clone(),
            name: format!("{} - {}", entry.name, entry.creator),
        }));
        self.theme_catalog = catalog;
    }

    fn theme_import_base_theme_id(&self) -> String {
        match self.current_theme_id.as_str() {
            BUILTIN_THEME_GMARK_LIGHT_ID => BUILTIN_THEME_GMARK_LIGHT_ID.into(),
            BUILTIN_THEME_GMARK_ID => BUILTIN_THEME_GMARK_ID.into(),
            id => self
                .custom_themes
                .iter()
                .find(|entry| entry.id == id)
                .map(|entry| entry.base_theme_id.clone())
                .unwrap_or_else(|| BUILTIN_THEME_GMARK_ID.into()),
        }
    }
}

pub(super) fn custom_theme_from_value(value: Value) -> anyhow::Result<(CustomThemeEntry, Value)> {
    custom_theme_from_value_with_default_base(value, BUILTIN_THEME_GMARK_ID)
}

fn custom_theme_from_value_with_default_base(
    mut value: Value,
    default_base_theme_id: &str,
) -> anyhow::Result<(CustomThemeEntry, Value)> {
    prune_empty_json_values(&mut value);
    let Value::Object(mut object) = value else {
        bail!("theme config must be a JSON object");
    };
    let object = object_without_empty_values(std::mem::take(&mut object));
    let name = required_string(&object, "name")?;
    let creator = required_string(&object, "creator")?;
    let base_theme_id = resolved_custom_theme_base_id(&object, default_base_theme_id);
    let raw_theme_patch = object
        .get("theme")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    if !raw_theme_patch.is_object() {
        bail!("field 'theme' must be a JSON object when present");
    }

    let base_theme = custom_theme_base_theme(base_theme_id);
    let mut merged = serde_json::to_value(base_theme)?;
    let mut theme_patch = filter_json_by_schema(&raw_theme_patch, &merged);
    if let Value::Object(theme_patch_object) = &mut theme_patch {
        theme_patch_object.remove("name");
    }
    merge_non_empty_json_values(&mut merged, &theme_patch);
    if let Value::Object(merged_object) = &mut merged {
        merged_object.insert("name".into(), Value::String(name.clone()));
    }
    let theme: Theme = serde_json::from_value(merged)
        .with_context(|| format!("failed to construct custom theme '{name}'"))?;
    let id = format!(
        "custom:{}_{}",
        sanitize_config_file_stem(&name),
        sanitize_config_file_stem(&creator)
    );
    let mut normalized_object = Map::new();
    normalized_object.insert("name".into(), Value::String(name.clone()));
    normalized_object.insert("creator".into(), Value::String(creator.clone()));
    normalized_object.insert(
        "base_theme_id".into(),
        Value::String(base_theme_id.to_string()),
    );
    for key in ["description", "version", "homepage", "license"] {
        if let Some(value) = object.get(key) {
            normalized_object.insert(key.into(), value.clone());
        }
    }
    if !theme_patch
        .as_object()
        .map(|object| object.is_empty())
        .unwrap_or(false)
    {
        normalized_object.insert("theme".into(), theme_patch);
    }
    let normalized = Value::Object(normalized_object);

    Ok((
        CustomThemeEntry {
            id,
            name,
            creator,
            base_theme_id: base_theme_id.to_string(),
            theme,
        },
        normalized,
    ))
}

fn resolved_custom_theme_base_id<'a>(
    object: &'a Map<String, Value>,
    default_base_theme_id: &'a str,
) -> &'a str {
    object
        .get("base_theme_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| is_builtin_theme_id(value))
        .unwrap_or_else(|| {
            if is_builtin_theme_id(default_base_theme_id) {
                default_base_theme_id
            } else {
                BUILTIN_THEME_GMARK_ID
            }
        })
}

fn is_builtin_theme_id(theme_id: &str) -> bool {
    theme_id == BUILTIN_THEME_GMARK_ID || theme_id == BUILTIN_THEME_GMARK_LIGHT_ID
}

fn custom_theme_base_theme(theme_id: &str) -> Theme {
    if theme_id == BUILTIN_THEME_GMARK_LIGHT_ID {
        Theme::light_theme()
    } else {
        Theme::default_theme()
    }
}

fn filter_json_by_schema(value: &Value, schema: &Value) -> Value {
    match (value, schema) {
        (Value::Object(value_object), Value::Object(schema_object)) => {
            let mut filtered = Map::new();
            for (key, value) in value_object {
                if let Some(schema_value) = schema_object.get(key) {
                    filtered.insert(key.clone(), filter_json_by_schema(value, schema_value));
                }
            }
            Value::Object(filtered)
        }
        (value, _) => value.clone(),
    }
}

fn required_string(object: &Map<String, Value>, key: &str) -> anyhow::Result<String> {
    let Some(value) = object.get(key) else {
        bail!("missing required field '{key}'");
    };
    let Some(text) = value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    else {
        bail!("field '{key}' must be a non-empty string");
    };
    Ok(text.to_string())
}
