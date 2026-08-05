// @author kongweiguang

//! Semantic workbench colors and the single material-resolution entry point.

use gpui::{Hsla, rgba};
use serde::{Deserialize, Serialize};

pub use gmark_config::ResolvedVisualPreferences;

/// Stable visual roles shared by chrome, navigation, controls and transient UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkbenchThemeTokens {
    pub app_background: Hsla,
    pub editor_surface: Hsla,
    pub solid_surface: Hsla,
    pub navigation_surface: Hsla,
    pub elevated_surface: Hsla,
    pub glass_surface: Hsla,
    pub glass_strong_surface: Hsla,
    pub overlay_scrim: Hsla,
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub text_tertiary: Hsla,
    pub text_inverse: Hsla,
    pub icon: Hsla,
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub accent_pressed: Hsla,
    pub accent_soft: Hsla,
    pub focus_ring: Hsla,
    pub selection: Hsla,
    pub caret: Hsla,
    pub control_surface: Hsla,
    pub control_hover: Hsla,
    pub control_pressed: Hsla,
    pub input_surface: Hsla,
    pub border_subtle: Hsla,
    pub border_strong: Hsla,
    pub danger: Hsla,
    pub warning: Hsla,
    pub success: Hsla,
    pub info: Hsla,
    pub shadow: Hsla,
}

/// Surface intent; dense content deliberately has no glass variant.
// Reason: later UI slices need all surfaces; remove after TASK-003 through TASK-009 consume them.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceKind {
    App,
    Editor,
    Navigation,
    Solid,
    Elevated,
    Glass,
    GlassStrong,
}

/// Fully resolved material colors for one render pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialStyle {
    pub background: Hsla,
    pub border: Hsla,
    pub shadow: Hsla,
    pub is_translucent: bool,
}

impl WorkbenchThemeTokens {
    /// Resolves transparency and contrast centrally so call sites cannot invent
    /// their own glass fallback.
    #[must_use]
    pub fn material(
        &self,
        kind: SurfaceKind,
        preferences: ResolvedVisualPreferences,
    ) -> MaterialStyle {
        let wants_glass = matches!(kind, SurfaceKind::Glass | SurfaceKind::GlassStrong);
        let force_solid = preferences.reduced_transparency || preferences.high_contrast;
        let background = match kind {
            SurfaceKind::App => self.app_background,
            SurfaceKind::Editor => self.editor_surface,
            SurfaceKind::Navigation => self.navigation_surface,
            SurfaceKind::Solid => self.solid_surface,
            SurfaceKind::Elevated => self.elevated_surface,
            SurfaceKind::Glass if !force_solid => self.glass_surface,
            SurfaceKind::GlassStrong if !force_solid => self.glass_strong_surface,
            SurfaceKind::Glass | SurfaceKind::GlassStrong => self.elevated_surface,
        };
        MaterialStyle {
            background,
            border: if preferences.high_contrast {
                self.border_strong
            } else {
                self.border_subtle
            },
            shadow: if preferences.high_contrast {
                transparent()
            } else {
                self.shadow
            },
            is_translucent: wants_glass && !force_solid,
        }
    }

    pub(super) fn xcode_dark() -> Self {
        dark_palette(
            0x1c1d2aff, 0x292a30ff, 0x21222eff, 0x1c1d2aff, 0x303139ff, 0xffffffff, 0xd1d1d6ff,
            0x8f8f98ff, 0x0a84ffff, 0x409cffff, 0x0077e6ff, 0x64d2ffff, 0x48495099, 0x6d6e78ff,
        )
    }

    pub(super) fn xcode_light() -> Self {
        light_palette(
            0xf5f5f5ff, 0xffffffff, 0xffffffff, 0xf8fafaff, 0xffffffff, 0x000000ff, 0x3a3a3cff,
            0x6e6e73ff, 0x007affff, 0x1988ffff, 0x006ee6ff, 0x007affff, 0xd1d1d699, 0x8e8e93ff,
        )
    }

    pub(super) fn fleet_dark() -> Self {
        dark_palette(
            0x090909ff, 0x18191bff, 0x202123ff, 0x18191bff, 0x28292bff, 0xe0e1e4ff, 0xc7c8cbff,
            0x898e94ff, 0x726cf9ff, 0x827cffff, 0x625ce9ff, 0x82d2ceff, 0x36373999, 0x6e747bff,
        )
    }

    pub(super) fn fleet_light() -> Self {
        light_palette(
            0xf2f2f2ff, 0xffffffff, 0xffffffff, 0xf7f7f7ff, 0xffffffff, 0x181818ff, 0x4f4f4fff,
            0x767676ff, 0x726cf9ff, 0x827cffff, 0x625ce9ff, 0x087f8cff, 0xe2e2e299, 0x999999ff,
        )
    }

    pub(super) fn obsidian_dark() -> Self {
        dark_palette(
            0x1e1e1eff, 0x1e1e1eff, 0x242424ff, 0x262626ff, 0x303030ff, 0xdadadaff, 0xb3b3b3ff,
            0x999999ff, 0x8a5cf5ff, 0xa68af9ff, 0x7046d9ff, 0x53dfddff, 0x36363699, 0x666666ff,
        )
    }

    pub(super) fn obsidian_light() -> Self {
        light_palette(
            0xf6f6f6ff, 0xffffffff, 0xffffffff, 0xf4f4f4ff, 0xffffffff, 0x222222ff, 0x5c5c5cff,
            0x707070ff, 0x9873f7ff, 0xa68af9ff, 0x8158e6ff, 0x087f8cff, 0xe0e0e099, 0x9a9a9aff,
        )
    }

    pub(super) fn claude_dark() -> Self {
        dark_palette(
            0x141413ff, 0x141413ff, 0x1a1918ff, 0x1a1918ff, 0x262624ff, 0xfaf9f5ff, 0xb0aea5ff,
            0x87867fff, 0xc6613fff, 0xd97757ff, 0xa84f34ff, 0x6a9bccff, 0x3d3d3a99, 0x6f6e68ff,
        )
    }

    pub(super) fn claude_light() -> Self {
        light_palette(
            0xf5f4edff, 0xfaf9f5ff, 0xfaf9f5ff, 0xf5f4edff, 0xffffffff, 0x141413ff, 0x30302eff,
            0x5e5d59ff, 0xd97757ff, 0xe18667ff, 0xc6613fff, 0x2f6f9fff, 0xd1cfc599, 0x87867fff,
        )
    }
}

fn dark_palette(
    app: u32,
    editor: u32,
    solid: u32,
    navigation: u32,
    elevated: u32,
    text_primary: u32,
    text_secondary: u32,
    text_tertiary: u32,
    accent: u32,
    accent_hover: u32,
    accent_pressed: u32,
    info: u32,
    border_subtle: u32,
    border_strong: u32,
) -> WorkbenchThemeTokens {
    palette(
        app,
        editor,
        solid,
        navigation,
        elevated,
        text_primary,
        text_secondary,
        text_tertiary,
        0x000000ff,
        accent,
        accent_hover,
        accent_pressed,
        info,
        border_subtle,
        border_strong,
        0xff6961ff,
        0xffb340ff,
        0x30d158ff,
        0x00000070,
    )
}

fn light_palette(
    app: u32,
    editor: u32,
    solid: u32,
    navigation: u32,
    elevated: u32,
    text_primary: u32,
    text_secondary: u32,
    text_tertiary: u32,
    accent: u32,
    accent_hover: u32,
    accent_pressed: u32,
    info: u32,
    border_subtle: u32,
    border_strong: u32,
) -> WorkbenchThemeTokens {
    palette(
        app,
        editor,
        solid,
        navigation,
        elevated,
        text_primary,
        text_secondary,
        text_tertiary,
        0xffffffff,
        accent,
        accent_hover,
        accent_pressed,
        info,
        border_subtle,
        border_strong,
        0xd70015ff,
        0xb25000ff,
        0x248a3dff,
        0x0000002e,
    )
}

fn palette(
    app: u32,
    editor: u32,
    solid: u32,
    navigation: u32,
    elevated: u32,
    text_primary: u32,
    text_secondary: u32,
    text_tertiary: u32,
    text_inverse: u32,
    accent: u32,
    accent_hover: u32,
    accent_pressed: u32,
    info: u32,
    border_subtle: u32,
    border_strong: u32,
    danger: u32,
    warning: u32,
    success: u32,
    shadow: u32,
) -> WorkbenchThemeTokens {
    let elevated = color(elevated);
    let accent = color(accent);
    WorkbenchThemeTokens {
        app_background: color(app),
        editor_surface: color(editor),
        solid_surface: color(solid),
        navigation_surface: color(navigation),
        elevated_surface: elevated,
        glass_surface: alpha(elevated, 0.86),
        glass_strong_surface: alpha(elevated, 0.95),
        overlay_scrim: color(if text_inverse == 0x000000ff {
            0x0000008f
        } else {
            0x1d1d1f52
        }),
        text_primary: color(text_primary),
        text_secondary: color(text_secondary),
        text_tertiary: color(text_tertiary),
        text_inverse: color(text_inverse),
        icon: color(text_secondary),
        accent,
        accent_hover: color(accent_hover),
        accent_pressed: color(accent_pressed),
        accent_soft: alpha(accent, 0.18),
        focus_ring: alpha(accent, 0.78),
        selection: alpha(accent, 0.22),
        caret: color(text_primary),
        control_surface: color(solid),
        control_hover: alpha(color(text_secondary), 0.16),
        control_pressed: alpha(color(text_secondary), 0.24),
        input_surface: color(editor),
        border_subtle: color(border_subtle),
        border_strong: color(border_strong),
        danger: color(danger),
        warning: color(warning),
        success: color(success),
        info: color(info),
        shadow: color(shadow),
    }
}

fn color(value: u32) -> Hsla {
    Hsla::from(rgba(value))
}

fn alpha(mut color: Hsla, value: f32) -> Hsla {
    color.a = value;
    color
}

fn transparent() -> Hsla {
    Hsla::from(rgba(0x00000000))
}
