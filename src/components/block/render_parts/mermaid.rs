// @author kongweiguang

//! Stable, block-local Mermaid workbench chrome and view-mode composition.

use super::super::MermaidViewMode;
use super::*;

const MERMAID_FRAME_RADIUS: f32 = 14.0;
const MERMAID_HEADER_HEIGHT: f32 = 44.0;
const MERMAID_CONTROL_HEIGHT: f32 = 28.0;
const MERMAID_TITLE_BREAKPOINT: f32 = 420.0;
const MERMAID_SOURCE_ICON: &str = "icon/ui/code.svg";
const MERMAID_PREVIEW_ICON: &str = "icon/ui/preview.svg";
const MERMAID_SPLIT_ICON: &str = "icon/ui/split.svg";
const MERMAID_EXPORT_ICON: &str = "icon/ui/save.svg";

impl Block {
    pub(super) fn render_mermaid_workbench(
        &mut self,
        block_id: ElementId,
        depth_padding: f32,
        content_inset: f32,
        is_placeholder: bool,
        theme: &Theme,
        strings: &I18nStrings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let viewport = window.viewport_size();
        let viewport_width = f32::from(viewport.width.max(px(1.0)));
        let viewport_height = f32::from(viewport.height.max(px(1.0)));
        let narrow = viewport_width < MERMAID_NARROW_BREAKPOINT;
        let preview_scroll_handle = self.mermaid_preview_scroll_handle.clone();
        let read_only = self.is_read_only();
        let mode = if read_only {
            MermaidViewMode::Preview
        } else {
            self.mermaid_view_mode()
        };
        let source_line_count = self.display_text().split('\n').count().max(1);
        let heights = mermaid_workbench_body_height(
            mode,
            viewport_width,
            viewport_height,
            self.last_successful_mermaid_render
                .as_ref()
                .map(|rendered| rendered.display_height),
            source_line_count,
            (t.code_size * t.text_line_height).max(1.0),
        );
        let source_icon = MERMAID_SOURCE_ICON;
        let preview_icon = MERMAID_PREVIEW_ICON;
        let split_icon = MERMAID_SPLIT_ICON;
        let mode_button =
            |id: &'static str, icon: &'static str, target: MermaidViewMode, label: String| {
                let active = mode == target;
                let disabled = read_only && target != MermaidViewMode::Preview;
                let tooltip: SharedString = if disabled {
                    strings.mermaid_read_only.clone().into()
                } else {
                    label.into()
                };
                div()
                    .id(id)
                    .debug_selector(move || id.to_owned())
                    .size(px(MERMAID_CONTROL_HEIGHT))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.0))
                    .bg(if active {
                        c.dialog_secondary_button_hover
                    } else {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    })
                    .text_color(if active { c.text_link } else { c.dialog_muted })
                    .opacity(if disabled { 0.45 } else { 1.0 })
                    .tooltip(move |_window, cx| crate::ui::ui_tooltip(tooltip.clone(), cx))
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .when(!disabled, |button| {
                        button
                            .cursor_pointer()
                            .hover(|button| button.bg(c.dialog_secondary_button_hover))
                            .active(|button| button.opacity(0.82))
                            .on_click(cx.listener(move |block, _event, window, cx| {
                                block.set_mermaid_view_mode(target, window, cx);
                            }))
                    })
                    .child(svg().path(icon).size(px(15.0)).text_color(if active {
                        c.text_link
                    } else {
                        c.dialog_muted
                    }))
            };
        let mode_switch = div()
            .id("mermaid-view-mode-switch")
            .debug_selector(|| "mermaid-view-mode-switch".to_owned())
            .h(px(MERMAID_CONTROL_HEIGHT + 2.0))
            .p(px(1.0))
            .flex()
            .items_center()
            .gap(px(1.0))
            .rounded(px(10.0))
            .bg(c.dialog_secondary_button_bg)
            .border(px(1.0))
            .border_color(c.dialog_border)
            .child(mode_button(
                "mermaid-view-source",
                source_icon,
                MermaidViewMode::Source,
                strings.status_bar_mode_source.clone(),
            ))
            .child(mode_button(
                "mermaid-view-preview",
                preview_icon,
                MermaidViewMode::Preview,
                strings.status_bar_mode_preview.clone(),
            ))
            .child(mode_button(
                "mermaid-view-split",
                split_icon,
                MermaidViewMode::Split,
                strings.status_bar_mode_split.clone(),
            ));

        let export_enabled = self.can_export_mermaid_svg();
        let export_tooltip: SharedString = if export_enabled {
            strings.mermaid_export_svg.clone().into()
        } else {
            strings.mermaid_export_unavailable.clone().into()
        };
        let export_button = div()
            .id("mermaid-export-svg")
            .debug_selector(|| "mermaid-export-svg".to_owned())
            .size(px(MERMAID_CONTROL_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .opacity(if export_enabled { 1.0 } else { 0.45 })
            .text_color(c.dialog_muted)
            .tooltip(move |_window, cx| crate::ui::ui_tooltip(export_tooltip.clone(), cx))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .when(export_enabled, |button| {
                button
                    .cursor_pointer()
                    .hover(|button| button.bg(c.dialog_secondary_button_hover))
                    .active(|button| button.opacity(0.82))
                    .on_click(cx.listener(|block, _event, window, cx| {
                        block.request_mermaid_svg_export(window.window_handle(), cx);
                    }))
            })
            .child(
                svg()
                    .path(MERMAID_EXPORT_ICON)
                    .size(px(15.0))
                    .text_color(c.dialog_muted),
            );

        let overlay_render = self.last_successful_mermaid_render.clone();
        let overlay_key = self.mermaid_preview_key;
        let expand_enabled = overlay_render.is_some() && overlay_key.is_some();
        let expand_tooltip: SharedString = strings.mermaid_expand.clone().into();
        let expand_button = div()
            .id("mermaid-open-overlay")
            .debug_selector(|| "mermaid-open-overlay".to_owned())
            .size(px(MERMAID_CONTROL_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .opacity(if expand_enabled { 1.0 } else { 0.45 })
            .text_color(c.dialog_muted)
            .tooltip(move |_window, cx| crate::ui::ui_tooltip(expand_tooltip.clone(), cx))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .when(expand_enabled, |button| {
                match (overlay_render.clone(), overlay_key) {
                    (Some(rendered), Some(preview_key)) => button
                        .cursor_pointer()
                        .hover(|button| button.bg(c.dialog_secondary_button_hover))
                        .active(|button| button.opacity(0.82))
                        .on_click(cx.listener(move |_block, _event, _window, cx| {
                            cx.emit(BlockEvent::RequestOpenMermaidOverlay {
                                preview_key,
                                rendered: rendered.clone(),
                            });
                        })),
                    _ => button,
                }
            })
            .child(
                svg()
                    .path(EXPAND_ICON)
                    .size(px(15.0))
                    .text_color(c.dialog_muted),
            );

        let copy_tooltip: SharedString = if self.mermaid_copy_feedback {
            strings.mermaid_copied.clone()
        } else {
            strings.mermaid_copy_source.clone()
        }
        .into();
        let copy_icon = if self.mermaid_copy_feedback {
            CHECK_ICON
        } else {
            COPY_ICON
        };
        let copy_button = div()
            .id("mermaid-copy-source")
            .debug_selector(|| "mermaid-copy-source".to_owned())
            .size(px(MERMAID_CONTROL_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .cursor_pointer()
            .text_color(if self.mermaid_copy_feedback {
                c.text_link
            } else {
                c.dialog_muted
            })
            .hover(|button| button.bg(c.dialog_secondary_button_hover))
            .active(|button| button.opacity(0.82))
            .tooltip(move |_window, cx| crate::ui::ui_tooltip(copy_tooltip.clone(), cx))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .on_click(cx.listener(|block, _event, _window, cx| {
                block.copy_mermaid_source(cx);
            }))
            .child(svg().path(copy_icon).size(px(15.0)).text_color(
                if self.mermaid_copy_feedback {
                    c.text_link
                } else {
                    c.dialog_muted
                },
            ));

        let actions = div()
            .id("mermaid-toolbar-actions")
            .debug_selector(|| "mermaid-toolbar-actions".to_owned())
            .absolute()
            .right(px(8.0))
            .top(px(8.0))
            .flex()
            .items_center()
            .gap(px(2.0))
            .child(export_button)
            .child(expand_button)
            .child(copy_button);
        let header = div()
            .id("mermaid-workbench-toolbar")
            .debug_selector(|| "mermaid-workbench-toolbar".to_owned())
            .relative()
            .w_full()
            .h(px(MERMAID_HEADER_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(c.dialog_surface)
            .border_b_1()
            .border_color(c.dialog_border)
            .children((viewport_width >= MERMAID_TITLE_BREAKPOINT).then(|| {
                div()
                    .absolute()
                    .left(px(14.0))
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .text_size(px((t.code_size + 1.0).max(12.0)))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.dialog_body)
                    .child("mermaid")
            }))
            .child(mode_switch)
            .child(actions);

        let available_width = effective_image_width(self, viewport_width, d).max(1.0);
        let source_id = ElementId::Name(format!("mermaid-source-{}", self.record.id).into());
        let render_source = |height: f32, cx: &mut Context<Block>| {
            div()
                .id(source_id.clone())
                .debug_selector(|| "mermaid-source-editor".to_owned())
                .w_full()
                .h(px(height))
                .min_h(px(height))
                .px(px(12.0))
                .overflow_y_scroll()
                .bg(c.code_bg)
                .cursor(CursorStyle::IBeam)
                .font_family(crate::document_host::source_monospace_font_family())
                .text_size(px(t.code_size))
                .line_height(rems(t.text_line_height))
                .text_color(c.code_text)
                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                .into_any_element()
        };

        let workbench_bounds = self.mermaid_workbench_bounds.clone();
        let bounds_tracker = canvas(
            move |bounds, _, _| {
                if let Ok(mut current) = workbench_bounds.lock() {
                    *current = Some(bounds);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0();

        let body = match mode {
            MermaidViewMode::Preview => div()
                .id("mermaid-preview-pane")
                .debug_selector(|| "mermaid-preview-pane".to_owned())
                .w_full()
                .h(px(heights.preview_height))
                .min_w(px(0.0))
                .overflow_hidden()
                .overflow_y_scroll()
                .track_scroll(&preview_scroll_handle)
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .bg(c.source_mode_block_bg)
                .child(self.render_mermaid_content(
                    theme,
                    window,
                    available_width,
                    heights.preview_height,
                    cx,
                ))
                .into_any_element(),
            MermaidViewMode::Source => div()
                .id("mermaid-source-pane")
                .debug_selector(|| "mermaid-source-pane".to_owned())
                .w_full()
                .h(px(heights.source_height))
                .min_w(px(0.0))
                .overflow_hidden()
                .bg(c.source_mode_block_bg)
                .child(render_source(heights.source_height, cx))
                .into_any_element(),
            MermaidViewMode::Split if narrow => {
                let source = render_source(heights.source_height, cx);
                let preview = self.render_mermaid_content(
                    theme,
                    window,
                    available_width,
                    heights.preview_height,
                    cx,
                );
                div()
                    .id("mermaid-split-pane")
                    .debug_selector(|| "mermaid-split-pane".to_owned())
                    .w_full()
                    .h(px(heights.body_height))
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .bg(c.source_mode_block_bg)
                    .child(source)
                    .child(
                        div()
                            .w_full()
                            .h(px(1.0))
                            .flex_shrink_0()
                            .bg(c.dialog_border),
                    )
                    .child(
                        div()
                            .id("mermaid-split-preview-narrow")
                            .debug_selector(|| "mermaid-split-preview-narrow".to_owned())
                            .w_full()
                            .h(px(heights.preview_height))
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .overflow_y_scroll()
                            .track_scroll(&preview_scroll_handle)
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                            .child(preview),
                    )
                    .into_any_element()
            }
            MermaidViewMode::Split => {
                let source = render_source(heights.body_height, cx);
                let preview = self.render_mermaid_content(
                    theme,
                    window,
                    ((available_width - 1.0) / 2.0).max(1.0),
                    heights.body_height,
                    cx,
                );
                div()
                    .id("mermaid-split-pane")
                    .debug_selector(|| "mermaid-split-pane".to_owned())
                    .w_full()
                    .h(px(heights.body_height))
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .flex()
                    .bg(c.source_mode_block_bg)
                    .child(
                        div()
                            .id("mermaid-split-preview-wide")
                            .w(Length::Definite(relative(0.5)))
                            .h_full()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .child(source),
                    )
                    .child(
                        div()
                            .h_full()
                            .w(px(1.0))
                            .flex_shrink_0()
                            .bg(c.dialog_border),
                    )
                    .child(
                        div()
                            .id("mermaid-split-preview-content-wide")
                            .debug_selector(|| "mermaid-split-preview-wide".to_owned())
                            .w(Length::Definite(relative(0.5)))
                            .h_full()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .overflow_y_scroll()
                            .track_scroll(&preview_scroll_handle)
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                            .child(preview),
                    )
                    .into_any_element()
            }
        };

        self.render_shell(block_id, true, CursorStyle::PointingHand, 0.0, 0.0, d, cx)
            .debug_selector(|| "mermaid-workbench".to_owned())
            .relative()
            .min_h(px(0.0))
            .py(px(0.0))
            .pl(px(depth_padding))
            .pr(px(content_inset))
            .child(bounds_tracker)
            .child(
                div()
                    .id("mermaid-workbench-frame")
                    .debug_selector(|| "mermaid-workbench-frame".to_owned())
                    .w_full()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .rounded(px(MERMAID_FRAME_RADIUS))
                    .border(px(1.0))
                    .border_color(if self.focus_handle.is_focused(window) {
                        c.text_link.opacity(0.72)
                    } else {
                        c.dialog_border
                    })
                    .bg(c.source_mode_block_bg)
                    .child(header)
                    .child(body),
            )
            .into_any_element()
    }
}
