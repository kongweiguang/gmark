// @author kongweiguang

//! Semantic colors for the standalone update feedback process.

use gpui::{Hsla, WindowAppearance, hsla};

/// Minimal palette kept independent from the main application theme runtime.
/// The agent can start while Gmark is fully stopped, so it cannot rely on a
/// `ThemeManager` global or user theme files being available.
#[derive(Clone, Copy, Debug)]
pub(crate) struct UpdateAgentPalette {
    pub(crate) background: Hsla,
    pub(crate) primary_text: Hsla,
    pub(crate) secondary_text: Hsla,
    waiting: Hsla,
    success: Hsla,
    danger: Hsla,
}

impl UpdateAgentPalette {
    #[must_use]
    pub(crate) fn for_appearance(appearance: WindowAppearance) -> Self {
        let dark = matches!(
            appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        Self {
            background: if dark {
                hsla(0.0, 0.0, 0.10, 1.0)
            } else {
                hsla(0.0, 0.0, 0.98, 1.0)
            },
            primary_text: if dark {
                hsla(0.0, 0.0, 0.93, 1.0)
            } else {
                hsla(0.0, 0.0, 0.12, 1.0)
            },
            secondary_text: if dark {
                hsla(0.0, 0.0, 0.70, 1.0)
            } else {
                hsla(0.0, 0.0, 0.28, 1.0)
            },
            waiting: hsla(0.58, 0.72, 0.52, 1.0),
            success: hsla(0.42, 0.55, 0.38, 1.0),
            danger: hsla(0.0, 0.72, 0.52, 1.0),
        }
    }

    #[must_use]
    pub(crate) fn status_accent(self, failure: bool, success: bool) -> Hsla {
        if failure {
            self.danger
        } else if success {
            self.success
        } else {
            self.waiting
        }
    }
}
