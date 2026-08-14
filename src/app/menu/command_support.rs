// @author kongweiguang

//! Shared menu command helpers and recent-file actions.

use super::*;

use std::time::Duration;

use futures::future::{Either, select};

pub(crate) fn record_recent_file_from_editor(path: &Path, cx: &mut App) {
    record_recent_file_and_refresh(path, cx);
}

pub(super) fn show_window_prompt(
    window: Option<AnyWindowHandle>,
    title: &str,
    detail: &str,
    cx: &mut App,
) {
    if let Some(window) = window {
        let ok = cx.global::<I18nManager>().strings().info_dialog_ok.clone();
        let _ = window.update(cx, |_view, window, cx| {
            let buttons = [ok.as_str()];
            let _ = window.prompt(PromptLevel::Critical, title, Some(detail), &buttons, cx);
        });
    } else {
        eprintln!("{title}: {detail}");
    }
}

pub(super) fn with_active_editor<R>(
    cx: &mut App,
    update: impl FnOnce(&mut Editor, &mut Window, &mut Context<Editor>) -> R,
) -> Option<R> {
    let window = cx.active_window()?.downcast::<Editor>()?;
    window.update(cx, update).ok()
}

pub(super) fn show_info_dialog_on_active_editor(cx: &mut App, kind: InfoDialogKind) {
    let _ = with_active_editor(cx, move |editor, _window, cx| {
        editor.show_info_dialog(kind, cx);
    });
}

pub(super) fn request_update_check_on_active_editor(cx: &mut App) {
    let _ = with_active_editor(cx, |editor, window, cx| {
        editor.request_check_updates(window, cx);
    });
}

pub(super) fn open_crash_reports(cx: &mut App) {
    match crate::crash_report::open_reports_directory() {
        Ok(()) => {}
        Err(error) => {
            let title = cx
                .global::<I18nManager>()
                .strings()
                .open_failed_title
                .clone();
            show_window_prompt(cx.active_window(), &title, &error.to_string(), cx);
        }
    }
}

pub(super) fn open_recent_file(cx: &mut App, path: PathBuf) {
    let error_window = cx.active_window();
    open_recent_file_with_error_window(cx, path, error_window);
}

pub(super) fn open_recent_file_with_error_window(
    cx: &mut App,
    path: PathBuf,
    error_window: Option<AnyWindowHandle>,
) {
    let path_for_probe = path.clone();
    cx.spawn(async move |cx| {
        let exists = match select(
            cx.background_spawn(async move { path_for_probe.is_file() }),
            cx.background_executor().timer(Duration::from_secs(30)),
        )
        .await
        {
            Either::Left((exists, _timer)) => Some(exists),
            Either::Right((_elapsed, _probe)) => {
                eprintln!(
                    "timed out checking recent file '{}'; opening asynchronously",
                    path.display()
                );
                None
            }
        };

        let _ = cx.update(move |cx| {
            if exists == Some(false) {
                remove_recent_file_and_refresh(&path, cx);
                let strings = cx.global::<I18nManager>().strings().clone();
                let detail = strings
                    .recent_file_missing_message_template
                    .replace("{path}", &path.to_string_lossy());
                show_window_prompt(
                    error_window,
                    &strings.recent_file_missing_title,
                    &detail,
                    cx,
                );
                return;
            }

            if let Err(error) = open_file_in_new_window(cx, &path) {
                let title = cx
                    .global::<I18nManager>()
                    .strings()
                    .open_failed_title
                    .clone();
                show_window_prompt(error_window, &title, &error.to_string(), cx);
            }
        });
    })
    .detach();
}
