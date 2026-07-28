// @author kongweiguang

//! Read-only enlarged Mermaid diagram overlay.

use super::*;
use crate::components::DismissTransientUi;
use crate::i18n::I18nStrings;
use crate::theme::Theme;

const DIAGRAM_OVERLAY_MIN_SCALE: f32 = 0.01;
const DIAGRAM_OVERLAY_MAX_SCALE: f32 = 100.0;
const DIAGRAM_OVERLAY_WHEEL_ZOOM_SENSITIVITY: f32 = 700.0;

impl Editor {
    pub(super) fn open_diagram_overlay(
        &mut self,
        block_id: EntityId,
        preview_key: u64,
        rendered: crate::components::MermaidSvgRender,
        cx: &mut Context<Self>,
    ) {
        self.diagram_overlay = Some(DiagramOverlayState {
            block_id,
            preview_key,
            rendered,
            manual_scale: None,
            close_focus_handle: cx.focus_handle(),
            focus_close_on_render: true,
        });
        cx.notify();
    }

    pub(super) fn close_diagram_overlay(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.diagram_overlay.take() {
            self.focus_block(state.block_id);
            cx.notify();
        }
    }

    fn toggle_diagram_overlay_scale(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.diagram_overlay.as_mut() {
            state.manual_scale = if state.manual_scale.is_some() {
                None
            } else {
                Some(
                    (1.0 / state.rendered.display_scale.max(f32::EPSILON))
                        .clamp(DIAGRAM_OVERLAY_MIN_SCALE, DIAGRAM_OVERLAY_MAX_SCALE),
                )
            };
            cx.notify();
        }
    }

    fn zoom_diagram_overlay(
        &mut self,
        wheel_delta_y: Pixels,
        fit_scale: f32,
        cx: &mut Context<Self>,
    ) {
        let wheel_delta_y = f32::from(wheel_delta_y);
        if !wheel_delta_y.is_finite() || wheel_delta_y.abs() < f32::EPSILON {
            return;
        }

        let Some(state) = self.diagram_overlay.as_mut() else {
            return;
        };
        let current_scale = state.manual_scale.unwrap_or(fit_scale);
        // GPUI 向上滚轮为正值；以乘法缩放使触控板和普通滚轮在不同级别下手感一致。
        let factor = (wheel_delta_y / DIAGRAM_OVERLAY_WHEEL_ZOOM_SENSITIVITY).exp();
        let next_scale =
            (current_scale * factor).clamp(DIAGRAM_OVERLAY_MIN_SCALE, DIAGRAM_OVERLAY_MAX_SCALE);
        if (next_scale - current_scale).abs() < f32::EPSILON {
            return;
        }

        state.manual_scale = Some(next_scale);
        cx.notify();
    }

    pub(super) fn render_diagram_overlay(
        &mut self,
        theme: &Theme,
        strings: &I18nStrings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let block_id = self.diagram_overlay.as_ref()?.block_id;
        let Some(block) = self.document.block_entity_by_id(block_id) else {
            self.diagram_overlay = None;
            return None;
        };
        let block = block.read(cx);
        if block.kind() != BlockKind::MermaidBlock {
            self.diagram_overlay = None;
            return None;
        }
        // 主题或视口变化会生成新缓存身份。后台任务完成前继续展示旧图；完成后原位
        // 切换到同一块的新 SVG，不关闭覆盖层，也不触碰文档事务。
        if block.mermaid_preview_task.is_none()
            && let (Some(preview_key), Some(rendered)) = (
                block.mermaid_preview_key,
                block.last_successful_mermaid_render.clone(),
            )
            && self
                .diagram_overlay
                .as_ref()
                .is_some_and(|state| state.preview_key != preview_key)
            && let Some(state) = self.diagram_overlay.as_mut()
        {
            state.preview_key = preview_key;
            state.rendered = rendered;
        }
        let state = self.diagram_overlay.as_ref()?.clone();
        if self
            .diagram_overlay
            .as_mut()
            .is_some_and(|state| std::mem::take(&mut state.focus_close_on_render))
        {
            let close_focus = state.close_focus_handle.clone();
            window.defer(cx, move |window, _cx| close_focus.focus(window));
        }

        let viewport = window.viewport_size();
        let max_width = f32::from(viewport.width) * 0.9;
        let max_height = f32::from(viewport.height) * 0.9;
        let fit_scale = (max_width / state.rendered.display_width.max(1.0))
            .min(max_height / state.rendered.display_height.max(1.0))
            .min(1.0);
        let scale = state.manual_scale.unwrap_or(fit_scale);
        let width = state.rendered.display_width * scale;
        let height = state.rendered.display_height * scale;
        let close_editor = cx.entity().downgrade();
        let dismiss_editor = close_editor.clone();
        let key_editor = close_editor.clone();
        let backdrop_editor = close_editor.clone();
        let scale_editor = close_editor.clone();
        let zoom_editor = close_editor.clone();
        let scale_label = if state.manual_scale.is_some() {
            strings.large_document_text("diagram_fit_window")
        } else {
            strings.large_document_text("diagram_actual_size")
        };

        Some(
            div()
                .id("diagram-overlay")
                .debug_selector(|| "diagram-overlay".to_owned())
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::black().opacity(0.58))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    let _ =
                        backdrop_editor.update(cx, |editor, cx| editor.close_diagram_overlay(cx));
                })
                // 覆盖层是模态阅读界面；工具栏和留白上的滚轮同样不能穿透到正文。
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("diagram-overlay-panel")
                        .debug_selector(|| "diagram-overlay-panel".to_owned())
                        .w(px(max_width))
                        .h(px(max_height))
                        .p(px(12.0))
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .rounded(px(theme.dimensions.dialog_radius))
                        .bg(theme.colors.dialog_surface)
                        .border(px(theme.dimensions.dialog_border_width))
                        .border_color(theme.colors.dialog_border)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .justify_end()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .id("diagram-overlay-scale")
                                        .debug_selector(|| "diagram-overlay-scale".to_owned())
                                        .px(px(10.0))
                                        .h(px(30.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.0))
                                        .cursor_pointer()
                                        .text_color(theme.colors.dialog_body)
                                        .hover(|this| this.bg(theme.colors.chrome_hover))
                                        .child(scale_label.to_owned())
                                        .on_click(move |_, _, cx| {
                                            let _ = scale_editor.update(cx, |editor, cx| {
                                                editor.toggle_diagram_overlay_scale(cx)
                                            });
                                        }),
                                )
                                .child(
                                    div()
                                        .id("diagram-overlay-close")
                                        .debug_selector(|| "diagram-overlay-close".to_owned())
                                        .tab_index(0)
                                        .track_focus(&state.close_focus_handle)
                                        .px(px(10.0))
                                        .h(px(30.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.0))
                                        .cursor_pointer()
                                        .text_color(theme.colors.dialog_body)
                                        .hover(|this| this.bg(theme.colors.chrome_hover))
                                        .child(strings.ui_close.clone())
                                        .on_action(move |_: &DismissTransientUi, _window, cx| {
                                            let _ = dismiss_editor.update(cx, |editor, cx| {
                                                editor.close_diagram_overlay(cx)
                                            });
                                        })
                                        .on_key_down(move |event: &KeyDownEvent, _window, cx| {
                                            if event.keystroke.key == "escape" {
                                                let _ = key_editor.update(cx, |editor, cx| {
                                                    editor.close_diagram_overlay(cx)
                                                });
                                                cx.stop_propagation();
                                            }
                                        })
                                        .on_click(move |_, _, cx| {
                                            let _ = close_editor.update(cx, |editor, cx| {
                                                editor.close_diagram_overlay(cx)
                                            });
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .id("diagram-overlay-scroll")
                                .flex_1()
                                .min_h(px(0.0))
                                .overflow_y_scroll()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .id("diagram-overlay-canvas")
                                        .debug_selector(|| "diagram-overlay-canvas".to_owned())
                                        .w(px(width.max(1.0)))
                                        .h(px(height.max(1.0)))
                                        .on_scroll_wheel(
                                            move |event: &ScrollWheelEvent, _window, cx| {
                                                let delta = event.delta.pixel_delta(px(28.0));
                                                let _ = zoom_editor.update(cx, |editor, cx| {
                                                    editor.zoom_diagram_overlay(
                                                        delta.y, fit_scale, cx,
                                                    );
                                                });
                                                // Canvas 在滚动容器内部，先于其默认滚动处理；在此
                                                // 截断可同时避免预览和编辑器正文跟随滚动。
                                                cx.stop_propagation();
                                            },
                                        )
                                        .child(
                                            img(state.rendered.path)
                                                .w(px(width.max(1.0)))
                                                .h(px(height.max(1.0)))
                                                .object_fit(ObjectFit::Contain),
                                        ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
