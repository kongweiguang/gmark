// @author kongweiguang

//! Narrow platform adapter for visual accessibility settings.

use gmark_config::SystemVisualPreferences;

/// Reads a point-in-time platform snapshot. The runtime refreshes this during
/// startup and preference/window lifecycle events instead of polling per frame.
#[must_use]
pub(crate) fn read_system_visual_preferences() -> SystemVisualPreferences {
    platform_snapshot()
}

#[cfg(target_os = "windows")]
fn platform_snapshot() -> SystemVisualPreferences {
    use windows::UI::ViewManagement::UISettings;

    let ui_settings = UISettings::new().ok();
    let reduced_motion = ui_settings
        .as_ref()
        .and_then(|settings| settings.AnimationsEnabled().ok())
        .map(|enabled| !enabled)
        .unwrap_or_else(reduced_motion_from_system_parameters);
    let reduced_transparency = ui_settings
        .as_ref()
        .and_then(|settings| settings.AdvancedEffectsEnabled().ok())
        .map(|enabled| !enabled)
        .unwrap_or(false);

    SystemVisualPreferences {
        reduced_motion,
        reduced_transparency,
        high_contrast: high_contrast_from_system_parameters(),
    }
}

#[cfg(target_os = "windows")]
// Reason: Windows requires this native preference API; remove when a safe wrapper is available.
#[allow(unsafe_code)]
fn reduced_motion_from_system_parameters() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };

    let mut enabled = 1i32;
    // SAFETY: `enabled` is a valid writable BOOL-sized value for the documented
    // SPI_GETCLIENTAREAANIMATION request and remains alive for the complete call.
    let result = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut enabled).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    result.is_ok() && enabled == 0
}

#[cfg(target_os = "windows")]
// Reason: Windows requires this native high-contrast API; remove when a safe wrapper is available.
#[allow(unsafe_code)]
fn high_contrast_from_system_parameters() -> bool {
    use std::mem::size_of;
    use windows::Win32::UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        WindowsAndMessaging::{
            SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
        },
    };

    let mut state = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..HIGHCONTRASTW::default()
    };
    // SAFETY: `state` is correctly sized and writable for SPI_GETHIGHCONTRAST
    // and remains alive for the complete call.
    let result = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            state.cbSize,
            Some((&raw mut state).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    result.is_ok() && state.dwFlags.contains(HCF_HIGHCONTRASTON)
}

#[cfg(not(target_os = "windows"))]
fn platform_snapshot() -> SystemVisualPreferences {
    // Explicit overrides remain fully functional. Native macOS/Linux signal
    // wiring requires target-platform validation before it can be claimed.
    SystemVisualPreferences::default()
}
