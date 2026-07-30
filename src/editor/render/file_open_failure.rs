// @author kongweiguang

use super::*;

impl Editor {
    fn run_file_open_failure_action(&mut self, reveal: bool, cx: &mut Context<Self>) {
        let Some(path) = self
            .file_open_failure
            .as_ref()
            .map(|failure| failure.path.clone())
        else {
            return;
        };
        let result = if reveal {
            crate::editor::system_file::reveal_in_file_manager(&path)
        } else {
            crate::editor::system_file::open_with_system(&path)
        };
        if let Some(failure) = self.file_open_failure.as_mut() {
            failure.action_error = result.err().map(|error| error.to_string());
        }
        cx.notify();
    }

    pub(super) fn render_file_open_failure(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(failure) = self.file_open_failure.as_ref() else {
            return div().into_any_element();
        };
        let c = &theme.colors;
        let t = &theme.typography;
        let file_name = failure
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| failure.path.to_string_lossy().into_owned());
        let open_editor = cx.entity().downgrade();
        let reveal_editor = open_editor.clone();
        let open_key_editor = open_editor.clone();
        let reveal_key_editor = reveal_editor.clone();
        let open_focus = self.file_open_failure_focus_handles[0].clone();
        let open_pointer_focus = open_focus.clone();
        let reveal_focus = self.file_open_failure_focus_handles[1].clone();
        let reveal_pointer_focus = reveal_focus.clone();

        let open_button = div()
            .id("file-open-failure-open-system")
            .debug_selector(|| "file-open-failure-open-system".to_owned())
            .h(px(32.0))
            .tab_index(0)
            .track_focus(&open_focus)
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(c.dialog_border)
            .bg(c.dialog_secondary_button_bg)
            .hover(|this| this.bg(c.chrome_hover))
            .focus(|this| this.border_color(c.text_link))
            .cursor_pointer()
            .text_size(px(t.text_size * 0.9))
            .text_color(c.text_default)
            .child(
                svg()
                    .path("icon/ui/file.svg")
                    .size(px(15.0))
                    .text_color(c.text_default),
            )
            .child(strings.file_open_with_system.clone())
            .on_click(move |_event, window, cx| {
                open_pointer_focus.focus(window);
                let _ = open_editor.update(cx, |editor, cx| {
                    editor.run_file_open_failure_action(false, cx)
                });
                cx.stop_propagation();
            })
            .on_key_down(move |event, _window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    let _ = open_key_editor.update(cx, |editor, cx| {
                        editor.run_file_open_failure_action(false, cx)
                    });
                    cx.stop_propagation();
                }
            });
        let reveal_button = div()
            .id("file-open-failure-reveal")
            .debug_selector(|| "file-open-failure-reveal".to_owned())
            .h(px(32.0))
            .tab_index(0)
            .track_focus(&reveal_focus)
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(hsla(0.0, 0.0, 0.0, 0.0))
            .hover(|this| this.bg(c.chrome_hover))
            .focus(|this| this.border_color(c.text_link))
            .cursor_pointer()
            .text_size(px(t.text_size * 0.9))
            .text_color(c.dialog_muted)
            .child(
                svg()
                    .path("icon/workspace/folder.svg")
                    .size(px(15.0))
                    .text_color(c.dialog_muted),
            )
            .child(strings.file_reveal_in_manager.clone())
            .on_click(move |_event, window, cx| {
                reveal_pointer_focus.focus(window);
                let _ = reveal_editor.update(cx, |editor, cx| {
                    editor.run_file_open_failure_action(true, cx)
                });
                cx.stop_propagation();
            })
            .on_key_down(move |event, _window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    let _ = reveal_key_editor.update(cx, |editor, cx| {
                        editor.run_file_open_failure_action(true, cx)
                    });
                    cx.stop_propagation();
                }
            });

        div()
            .id("file-open-failure")
            .debug_selector(|| "file-open-failure".to_owned())
            .size_full()
            .px(px(28.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .text_align(TextAlign::Center)
            .child(
                svg()
                    .path("icon/ui/file.svg")
                    .size(px(36.0))
                    .text_color(c.dialog_muted),
            )
            .child(
                div()
                    .w(px(560.0))
                    .max_w(relative(1.0))
                    .text_size(px(t.text_size * 1.08))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.text_default)
                    .child(strings.file_open_failed_title.clone()),
            )
            .child(
                div()
                    .id("file-open-failure-name")
                    .debug_selector(|| "file-open-failure-name".to_owned())
                    .w(px(560.0))
                    .max_w(relative(1.0))
                    .overflow_hidden()
                    .truncate()
                    .text_size(px(t.text_size * 0.92))
                    .text_color(c.dialog_muted)
                    .child(file_name),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_center()
                    .gap(px(6.0))
                    .child(open_button)
                    .child(reveal_button),
            )
            .children(failure.action_error.as_ref().map(|error| {
                div()
                    .max_w(px(560.0))
                    .text_size(px(t.text_size * 0.8))
                    .text_color(c.dialog_danger_button_bg)
                    .child(error.clone())
            }))
            .into_any_element()
    }
}
