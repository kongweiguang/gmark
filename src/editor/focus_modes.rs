// @author kongweiguang

//! Distraction-free presentation modes that never mutate document state.

use gpui::*;

use super::Editor;

pub(super) const TYPEWRITER_VIEWPORT_RATIO: f32 = 0.45;

impl Editor {
    pub(crate) fn on_toggle_focus_mode_action(
        &mut self,
        _: &crate::components::ToggleFocusMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_contextual_overlays(cx);
        self.focus_mode = !self.focus_mode;
        cx.notify();
    }

    pub(crate) fn on_toggle_typewriter_mode_action(
        &mut self,
        _: &crate::components::ToggleTypewriterMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.typewriter_mode = !self.typewriter_mode;
        if self.typewriter_mode {
            self.request_active_block_scroll_into_view(cx);
        }
        cx.notify();
    }
}

pub(super) fn focus_row_opacity(focus_mode: bool, active: bool, heading: bool) -> f32 {
    if !focus_mode || active {
        1.0
    } else if heading {
        0.68
    } else {
        0.48
    }
}

pub(super) fn typewriter_target_y(viewport_top: f32, viewport_height: f32) -> f32 {
    viewport_top + viewport_height.max(0.0) * TYPEWRITER_VIEWPORT_RATIO
}

#[cfg(test)]
#[path = "../../tests/unit/editor/focus_modes.rs"]
mod tests;
