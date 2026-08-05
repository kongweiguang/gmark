// @author kongweiguang

//! Preferences window construction and public entry points.

use super::*;

fn try_open_preferences_window_with_state(
    cx: &mut App,
    preferences: AppPreferences,
    title: String,
) -> anyhow::Result<WindowHandle<PreferencesWindow>> {
    let bounds = Bounds::centered(None, size(px(860.0), px(620.0)), cx);
    let window_title = SharedString::from(format!("gmark - {title}"));
    let handle = cx
        .open_window(
            gmark_window_options(window_title, bounds),
            move |_window, cx| cx.new(move |cx| PreferencesWindow::new(preferences, cx)),
        )
        .map_err(|error| anyhow::anyhow!("failed to open preferences window: {error}"))?;

    handle
        .update(cx, |view, window, cx| {
            let preferences = cx.entity().downgrade();
            window.on_window_should_close(cx, move |_window, cx| {
                let _ = preferences.update(cx, |view, cx| {
                    view.restore_saved_visual_accessibility(cx);
                    view.restore_saved_theme(cx);
                });
                true
            });
            window.activate_window();
            view.focus_handle.focus(window);
        })
        .map_err(|error| anyhow::anyhow!("failed to initialize preferences window: {error}"))?;

    Ok(handle)
}

#[cfg(test)]
pub(super) fn open_preferences_window_with_state(
    cx: &mut App,
    preferences: AppPreferences,
    title: String,
) -> WindowHandle<PreferencesWindow> {
    try_open_preferences_window_with_state(cx, preferences, title)
        .expect("test preferences window should open")
}

pub(crate) fn open_preferences_window(
    cx: &mut App,
) -> anyhow::Result<WindowHandle<PreferencesWindow>> {
    let preferences = match read_app_preferences() {
        Ok(preferences) => preferences,
        Err(err) => {
            eprintln!("failed to read app preferences: {err}");
            AppPreferences::default()
        }
    };
    let title = cx
        .global::<I18nManager>()
        .strings()
        .preferences_window_title
        .clone();
    try_open_preferences_window_with_state(cx, preferences, title)
}

pub(crate) fn localized_shortcut_command_label(
    command: ShortcutCommand,
    strings: &crate::i18n::I18nStrings,
) -> String {
    PreferencesWindow::shortcut_command_label(command, strings)
}
