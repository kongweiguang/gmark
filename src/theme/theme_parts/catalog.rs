// @author kongweiguang

use super::*;
use std::sync::Arc;

use gpui::{App, Global, WindowAppearance};

const XCODE_DARK_ID: &str = "xcode-dark";
const XCODE_LIGHT_ID: &str = "xcode-light";
const JETBRAINS_DARK_ID: &str = "jetbrains-dark";
const JETBRAINS_LIGHT_ID: &str = "jetbrains-light";
const OBSIDIAN_DARK_ID: &str = "obsidian-dark";
const OBSIDIAN_LIGHT_ID: &str = "obsidian-light";

/// The requested relationship between the application and the platform appearance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeAppearance {
    Dark,
    Light,
    #[default]
    System,
}

impl ThemeAppearance {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::System => "system",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            "system" => Some(Self::System),
            _ => None,
        }
    }

    pub(crate) fn resolved(self, platform: WindowAppearance) -> Self {
        match self {
            Self::System => match platform {
                WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
                WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
            },
            fixed => fixed,
        }
    }
}

/// A built-in semantic color vocabulary. Appearance is intentionally kept
/// separate so adding another palette does not change persisted preferences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePalette {
    #[default]
    Xcode,
    JetBrains,
    Obsidian,
}

impl ThemePalette {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Xcode => "xcode",
            Self::JetBrains => "jetbrains",
            Self::Obsidian => "obsidian",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "xcode" => Some(Self::Xcode),
            "jetbrains" => Some(Self::JetBrains),
            "obsidian" => Some(Self::Obsidian),
            _ => None,
        }
    }
}

/// The two persisted theme dimensions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThemePreference {
    pub appearance: ThemeAppearance,
    pub palette: ThemePalette,
}

impl ThemePreference {
    pub(crate) const fn new(appearance: ThemeAppearance, palette: ThemePalette) -> Self {
        Self {
            appearance,
            palette,
        }
    }
}

/// Stable identifier for the concrete theme currently rendered by the app.
#[cfg(test)]
pub(crate) fn resolved_theme_id(
    appearance: ThemeAppearance,
    palette: ThemePalette,
    platform: WindowAppearance,
) -> &'static str {
    let appearance = appearance.resolved(platform);
    match (palette, appearance) {
        (ThemePalette::Xcode, ThemeAppearance::Dark) => XCODE_DARK_ID,
        (ThemePalette::Xcode, ThemeAppearance::Light) => XCODE_LIGHT_ID,
        (ThemePalette::JetBrains, ThemeAppearance::Dark) => JETBRAINS_DARK_ID,
        (ThemePalette::JetBrains, ThemeAppearance::Light) => JETBRAINS_LIGHT_ID,
        (ThemePalette::Obsidian, ThemeAppearance::Dark) => OBSIDIAN_DARK_ID,
        (ThemePalette::Obsidian, ThemeAppearance::Light) => OBSIDIAN_LIGHT_ID,
        (
            ThemePalette::Xcode | ThemePalette::JetBrains | ThemePalette::Obsidian,
            ThemeAppearance::System,
        ) => {
            unreachable!("system appearance must be resolved before selecting a theme")
        }
    }
}

fn build_theme(
    appearance: ThemeAppearance,
    palette: ThemePalette,
    platform: WindowAppearance,
) -> (String, Theme) {
    let resolved_appearance = appearance.resolved(platform);
    let (id, theme) = match (palette, resolved_appearance) {
        (ThemePalette::Xcode, ThemeAppearance::Dark) => (XCODE_DARK_ID, Theme::xcode_dark()),
        (ThemePalette::Xcode, ThemeAppearance::Light) => (XCODE_LIGHT_ID, Theme::xcode_light()),
        (ThemePalette::JetBrains, ThemeAppearance::Dark) => {
            (JETBRAINS_DARK_ID, Theme::jetbrains_dark())
        }
        (ThemePalette::JetBrains, ThemeAppearance::Light) => {
            (JETBRAINS_LIGHT_ID, Theme::jetbrains_light())
        }
        (ThemePalette::Obsidian, ThemeAppearance::Dark) => {
            (OBSIDIAN_DARK_ID, Theme::obsidian_dark())
        }
        (ThemePalette::Obsidian, ThemeAppearance::Light) => {
            (OBSIDIAN_LIGHT_ID, Theme::obsidian_light())
        }
        (
            ThemePalette::Xcode | ThemePalette::JetBrains | ThemePalette::Obsidian,
            ThemeAppearance::System,
        ) => {
            unreachable!("system appearance must be resolved before building a theme")
        }
    };
    (id.into(), theme)
}

/// Global theme state. It stores the user's requested dimensions separately
/// from the concrete theme so system appearance changes never lose the palette.
pub struct ThemeManager {
    current: Arc<Theme>,
    current_theme_id: String,
    selected_preference: ThemePreference,
    editor_typography_override: Option<(u8, u16)>,
    editor_content_width_override: Option<u16>,
}

impl Global for ThemeManager {}

impl Default for ThemeManager {
    fn default() -> Self {
        Self {
            current: Arc::new(Theme::xcode_dark()),
            current_theme_id: XCODE_DARK_ID.into(),
            selected_preference: ThemePreference::default(),
            editor_typography_override: None,
            editor_content_width_override: None,
        }
    }
}

impl ThemeManager {
    /// Installs the configured theme into GPUI's global state.
    #[cfg(test)]
    pub fn init(cx: &mut App) {
        let preferences = crate::config::read_app_preferences().unwrap_or_default();
        Self::init_with_preference(cx, preferences.theme_appearance, preferences.theme_palette);
    }

    /// Installs a selected palette and appearance into GPUI's global state.
    pub fn init_with_preference(cx: &mut App, appearance: ThemeAppearance, palette: ThemePalette) {
        let mut manager = Self::default();
        manager.set_theme_preference(appearance, palette, cx.window_appearance());
        cx.set_global(manager);
    }

    pub fn current(&self) -> &Theme {
        &self.current
    }

    /// Returns an `Arc` clone for hot render paths without copying theme tokens.
    pub fn current_arc(&self) -> Arc<Theme> {
        self.current.clone()
    }

    #[cfg(test)]
    pub fn current_theme_id(&self) -> &str {
        &self.current_theme_id
    }

    #[cfg(test)]
    pub fn selected_appearance(&self) -> ThemeAppearance {
        self.selected_preference.appearance
    }

    #[cfg(test)]
    pub fn selected_palette(&self) -> ThemePalette {
        self.selected_preference.palette
    }

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

    pub fn set_editor_content_width(&mut self, content_width: u16) {
        self.editor_content_width_override = Some(content_width);
        let mut theme = (*self.current).clone();
        theme.dimensions.centered_max_width = f32::from(content_width.clamp(680, 1600));
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

    /// Applies both preference dimensions and resolves `system` immediately.
    pub fn set_theme_preference(
        &mut self,
        appearance: ThemeAppearance,
        palette: ThemePalette,
        platform: WindowAppearance,
    ) -> bool {
        let (theme_id, theme) = build_theme(appearance, palette, platform);
        let preference = ThemePreference::new(appearance, palette);
        let changed = self.current_theme_id != theme_id || self.selected_preference != preference;
        if !changed {
            return false;
        }
        self.current_theme_id = theme_id;
        self.selected_preference = preference;
        self.current = Arc::new(theme);
        self.apply_editor_overrides();
        changed
    }

    /// Refreshes only a `system` appearance selection; fixed modes are stable.
    pub fn update_system_appearance(&mut self, platform: WindowAppearance) -> bool {
        if self.selected_preference.appearance != ThemeAppearance::System {
            return false;
        }
        let (theme_id, theme) = build_theme(
            self.selected_preference.appearance,
            self.selected_preference.palette,
            platform,
        );
        if self.current_theme_id == theme_id {
            return false;
        }
        self.current_theme_id = theme_id;
        self.current = Arc::new(theme);
        self.apply_editor_overrides();
        true
    }
}
