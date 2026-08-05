// @author kongweiguang

//! Runtime bridge from persisted accessibility overrides to resolved values.

use gmark_config::{
    ResolvedVisualPreferences, SystemVisualPreferences, VisualAccessibilityPreferences,
};
use gpui::{App, Global};

/// Single global source of truth for material and motion accessibility.
pub(crate) struct VisualPreferencesManager {
    preferences: VisualAccessibilityPreferences,
    system: SystemVisualPreferences,
    resolved: ResolvedVisualPreferences,
}

impl Global for VisualPreferencesManager {}

impl VisualPreferencesManager {
    pub(crate) fn init(cx: &mut App, preferences: VisualAccessibilityPreferences) {
        let system = crate::platform::visual_preferences::read_system_visual_preferences();
        let resolved = preferences.resolve(system);
        cx.set_global(Self {
            preferences,
            system,
            resolved,
        });
    }

    #[must_use]
    pub(crate) const fn current(&self) -> ResolvedVisualPreferences {
        self.resolved
    }

    // Reason: TASK-008 consumes this setter; remove after its System/On/Off controls land.
    #[allow(dead_code)]
    pub(crate) fn set_preferences(&mut self, preferences: VisualAccessibilityPreferences) -> bool {
        self.preferences = preferences;
        self.recompute()
    }

    pub(crate) fn refresh_system(&mut self) -> bool {
        self.system = crate::platform::visual_preferences::read_system_visual_preferences();
        self.recompute()
    }

    fn recompute(&mut self) -> bool {
        let resolved = self.preferences.resolve(self.system);
        if resolved == self.resolved {
            return false;
        }
        self.resolved = resolved;
        true
    }
}
