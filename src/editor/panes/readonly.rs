// @author kongweiguang

//! Read-only pane surfaces for image previews and file-open failures.
//!
//! These surfaces deliberately do not construct an [`Editor`], a
//! `DocumentHost`, or any Controller lease.  They retain only the path/error
//! payload and local presentation state while inactive.

use std::path::PathBuf;

use gpui::{
    Context, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Render,
    ScrollHandle, ScrollWheelEvent, StatefulInteractiveElement, Styled, StyledImage, Window, div,
    img, px, relative,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaneReadOnlyKind {
    Image { path: PathBuf },
    Error { path: PathBuf, message: String },
}

pub struct PaneReadOnlyCanvas {
    kind: PaneReadOnlyKind,
    focus_handle: FocusHandle,
    action_focus_handles: [FocusHandle; 2],
    scroll_handle: ScrollHandle,
    zoom: f32,
    action_error: Option<String>,
}

impl PaneReadOnlyCanvas {
    pub(crate) fn new(kind: PaneReadOnlyKind, cx: &mut Context<Self>) -> Self {
        Self {
            kind,
            focus_handle: cx.focus_handle(),
            action_focus_handles: std::array::from_fn(|_| cx.focus_handle()),
            scroll_handle: ScrollHandle::new(),
            zoom: 1.0,
            action_error: None,
        }
    }

    pub(crate) fn kind(&self) -> &PaneReadOnlyKind {
        &self.kind
    }

    pub(crate) fn set_zoom(&mut self, zoom: f32, cx: &mut Context<Self>) {
        self.zoom = zoom.clamp(0.1, 8.0);
        cx.notify();
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !(event.modifiers.control || event.modifiers.platform) {
            return;
        }
        let delta = event.delta.pixel_delta(px(28.0));
        self.zoom = (self.zoom - f32::from(delta.y) / 700.0).clamp(0.25, 8.0);
        cx.notify();
        cx.stop_propagation();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let next = match event.keystroke.key.as_str() {
            "+" | "=" => Some(self.zoom * 1.1),
            "-" => Some(self.zoom / 1.1),
            "0" => Some(1.0),
            _ => None,
        };
        if let Some(next) = next {
            self.zoom = next.clamp(0.25, 8.0);
            cx.notify();
            cx.stop_propagation();
        }
    }

    fn path(&self) -> &std::path::Path {
        match &self.kind {
            PaneReadOnlyKind::Image { path } | PaneReadOnlyKind::Error { path, .. } => path,
        }
    }

    fn run_system_action(&mut self, reveal: bool, cx: &mut Context<Self>) {
        let result = if reveal {
            crate::editor::system_file::reveal_in_file_manager(self.path())
        } else {
            crate::editor::system_file::open_with_system(self.path())
        };
        self.action_error = result.err().map(|error| error.to_string());
        cx.notify();
    }
}

impl Render for PaneReadOnlyCanvas {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<crate::theme::ThemeManager>().current_arc();
        let strings = cx
            .global::<crate::ui::i18n::I18nManager>()
            .strings()
            .clone();
        let workbench = &theme.colors.workbench;
        let focused = self.focus_handle.is_focused(window);
        let focus_ring = if focused {
            workbench.focus_ring
        } else {
            workbench.border_subtle
        };
        let on_scroll = cx.listener(Self::on_scroll_wheel);
        let on_key_down = cx.listener(Self::on_key_down);
        let surface = match &self.kind {
            PaneReadOnlyKind::Image { path } => div()
                .id("pane-readonly-image-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_scroll()
                .track_scroll(&self.scroll_handle)
                .on_scroll_wheel(on_scroll)
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(relative(self.zoom.clamp(0.25, 8.0)))
                        .h(relative(self.zoom.clamp(0.25, 8.0)))
                        .child(
                            img(path.clone())
                                .size_full()
                                .object_fit(gpui::ObjectFit::Contain),
                        ),
                )
                .into_any_element(),
            PaneReadOnlyKind::Error { path, message } => {
                let open_focus = self.action_focus_handles[0].clone();
                let reveal_focus = self.action_focus_handles[1].clone();
                let open_focus_key = open_focus.clone();
                let reveal_focus_key = reveal_focus.clone();
                let open_editor = cx.entity().downgrade();
                let reveal_editor = open_editor.clone();
                let open_key_editor = open_editor.clone();
                let reveal_key_editor = reveal_editor.clone();
                let open_button = div()
                    .id("pane-readonly-open-system")
                    .tab_index(0)
                    .track_focus(&open_focus)
                    .px(px(12.0))
                    .py(px(6.0))
                    .border(px(1.0))
                    .border_color(workbench.border_subtle)
                    .bg(workbench.control_surface)
                    .hover(|this| this.bg(workbench.control_hover))
                    .focus(|this| this.border_color(workbench.focus_ring))
                    .cursor_pointer()
                    .child(strings.file_open_with_system.clone())
                    .on_click(move |_event, window, cx| {
                        open_focus.focus(window);
                        let _ = open_editor
                            .update(cx, |canvas, cx| canvas.run_system_action(false, cx));
                        cx.stop_propagation();
                    })
                    .on_key_down(move |event, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            open_focus_key.focus(window);
                            let _ = open_key_editor
                                .update(cx, |canvas, cx| canvas.run_system_action(false, cx));
                            cx.stop_propagation();
                        }
                    });
                let reveal_button = div()
                    .id("pane-readonly-reveal")
                    .tab_index(0)
                    .track_focus(&reveal_focus)
                    .px(px(12.0))
                    .py(px(6.0))
                    .border(px(1.0))
                    .border_color(workbench.border_subtle)
                    .bg(workbench.control_surface)
                    .hover(|this| this.bg(workbench.control_hover))
                    .focus(|this| this.border_color(workbench.focus_ring))
                    .cursor_pointer()
                    .child(strings.file_reveal_in_manager.clone())
                    .on_click(move |_event, window, cx| {
                        reveal_focus.focus(window);
                        let _ = reveal_editor
                            .update(cx, |canvas, cx| canvas.run_system_action(true, cx));
                        cx.stop_propagation();
                    })
                    .on_key_down(move |event, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            reveal_focus_key.focus(window);
                            let _ = reveal_key_editor
                                .update(cx, |canvas, cx| canvas.run_system_action(true, cx));
                            cx.stop_propagation();
                        }
                    });
                let mut body = div()
                    .flex_1()
                    .min_h(px(0.0))
                    .p(px(24.0))
                    .gap(px(8.0))
                    .items_center()
                    .justify_center()
                    .text_color(workbench.text_primary)
                    .child(strings.file_open_failed_title.clone())
                    .child(path.to_string_lossy().to_string())
                    .child(strings.file_open_failed_message.clone())
                    .child(message.clone())
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(open_button)
                            .child(reveal_button),
                    );
                if let Some(action_error) = self.action_error.clone() {
                    body = body.child(
                        div()
                            .text_color(workbench.text_secondary)
                            .child(action_error),
                    );
                }
                body.into_any_element()
            }
        };
        div()
            .id("pane-readonly-canvas")
            .size_full()
            .flex()
            .flex_col()
            .bg(workbench.editor_surface)
            .border(px(1.0))
            .border_color(focus_ring)
            .tab_index(0)
            .track_focus(&self.focus_handle)
            .on_key_down(on_key_down)
            .child(surface)
    }
}
