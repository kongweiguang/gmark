// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn on_tab_strip_key_down(
        &mut self,
        index: usize,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.tabs.records.len();
        if index >= count || count == 0 {
            return;
        }
        let target = match event.keystroke.key.as_str() {
            "left" => Some((index + count - 1) % count),
            "right" => Some((index + 1) % count),
            "home" => Some(0),
            "end" => Some(count - 1),
            "enter" | "space" => Some(index),
            "delete" => {
                self.request_close_tab_index(index, cx);
                if !self.tabs.show_close_dialog {
                    self.focus_tab_index(self.tabs.active, window, cx);
                }
                cx.stop_propagation();
                return;
            }
            _ => None,
        };
        if let Some(target) = target {
            self.switch_to_tab_index(target, cx);
            self.focus_tab_index(target, window, cx);
            cx.stop_propagation();
        }
    }

    pub(super) fn on_new_tab_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" && self.tabs.dismiss_new_or_split_menu() {
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            self.new_untitled_tab(cx);
            cx.stop_propagation();
        }
    }
}
