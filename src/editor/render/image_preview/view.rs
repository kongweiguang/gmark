// @author kongweiguang

use super::*;

impl Editor {
    fn apply_image_preview_zoom_action(
        &mut self,
        action: ImagePreviewZoomAction,
        fit_scale: f32,
        cx: &mut Context<Self>,
    ) {
        self.image_preview_zoom =
            image_preview_zoom_for_action(self.image_preview_zoom, fit_scale, action);
        if action == ImagePreviewZoomAction::FitWidth {
            let offset = self.scroll_handle.offset();
            self.scroll_handle.set_offset(point(px(0.0), offset.y));
        }
        cx.notify();
    }

    pub(in crate::editor) fn on_image_preview_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !(event.modifiers.control || event.modifiers.platform) {
            return;
        }
        let delta = event.delta.pixel_delta(px(28.0));
        let zoom = image_preview_zoom_after_wheel(self.image_preview_zoom, delta.y);
        if (zoom - self.image_preview_zoom).abs() > f32::EPSILON {
            let viewport = self.scroll_handle.bounds();
            if viewport.size.width > px(1.0)
                && let Some(path) = self.image_preview_path.clone()
                && let Some(Ok(asset)) = window.use_asset::<ImagePreviewAssetLoader>(&path, cx)
            {
                let viewport_width = f32::from(viewport.size.width);
                let fitted_width = (viewport_width - IMAGE_PREVIEW_PADDING * 2.0)
                    .max(1.0)
                    .min(asset.width as f32);
                let old_canvas_width = (fitted_width * self.image_preview_zoom).max(1.0);
                let new_canvas_width = (fitted_width * zoom).max(1.0);
                let offset = image_preview_offset_after_anchored_zoom(
                    self.scroll_handle.offset(),
                    event.position,
                    viewport,
                    size(asset.width as f32, asset.height as f32),
                    old_canvas_width,
                    old_canvas_width / asset.width as f32,
                    new_canvas_width,
                    new_canvas_width / asset.width as f32,
                );
                self.scroll_handle.set_offset(offset);
            }
            self.image_preview_zoom = zoom;
            cx.notify();
        }
        cx.stop_propagation();
    }

    pub(in crate::editor) fn render_image_preview(
        &mut self,
        theme: &Theme,
        strings: &I18nStrings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(path) = self.image_preview_path.clone() else {
            return div().into_any_element();
        };
        let asset = window.use_asset::<ImagePreviewAssetLoader>(&path, cx);
        let Some(asset) = asset else {
            return image_preview_message(
                "image-preview-loading",
                strings.image_loading_without_alt.clone(),
                theme,
            );
        };
        let asset = match asset {
            Ok(asset) => asset,
            Err(error) => {
                return image_preview_message(
                    "image-preview-error",
                    format!("{}: {error}", strings.file_open_failed_title),
                    theme,
                );
            }
        };

        let zoom = self
            .image_preview_zoom
            .clamp(IMAGE_PREVIEW_MIN_ZOOM, IMAGE_PREVIEW_MAX_ZOOM);
        let viewport = self.scroll_handle.bounds().size;
        let fallback_viewport = window.viewport_size();
        let viewport_width = if viewport.width > px(1.0) {
            f32::from(viewport.width)
        } else {
            f32::from(fallback_viewport.width)
        };
        let viewport_height = if viewport.height > px(1.0) {
            f32::from(viewport.height)
        } else {
            f32::from(fallback_viewport.height)
        };
        let fitted_width = (viewport_width - IMAGE_PREVIEW_PADDING * 2.0)
            .max(1.0)
            .min(asset.width as f32);
        let fit_scale = fitted_width / asset.width as f32;
        let canvas_width = (fitted_width * zoom).max(1.0);
        let center_canvas = canvas_width <= (viewport_width - IMAGE_PREVIEW_PADDING * 2.0).max(1.0);
        let scale = canvas_width / asset.width as f32;
        let scroll_y = (-f32::from(self.scroll_handle.offset().y)).max(0.0);
        let canvas = image_preview_canvas(
            &path,
            &asset,
            canvas_width,
            scale,
            scroll_y,
            viewport_height,
            cx,
        );
        let zoom_toolbar =
            self.render_image_preview_zoom_toolbar(theme, strings, fit_scale, scale, cx);
        div()
            .id("image-preview")
            .debug_selector(|| "image-preview".to_owned())
            .size_full()
            .min_w(px(0.0))
            .relative()
            .overflow_hidden()
            .bg(theme.colors.editor_background)
            .child(
                div()
                    .id("image-preview-scroll")
                    .debug_selector(|| "image-preview-scroll".to_owned())
                    .size_full()
                    .overflow_scroll()
                    .track_scroll(&self.scroll_handle)
                    .scrollbar_width(px(theme.dimensions.scrollbar_width))
                    .on_scroll_wheel(cx.listener(Self::on_image_preview_scroll_wheel))
                    .child(
                        div()
                            .id("image-preview-viewport")
                            .debug_selector(|| "image-preview-viewport".to_owned())
                            .w_full()
                            .min_h_full()
                            .p(px(IMAGE_PREVIEW_PADDING))
                            .flex()
                            .items_start()
                            .when(center_canvas, |this| this.justify_center())
                            .child(canvas),
                    ),
            )
            .child(zoom_toolbar)
            .into_any_element()
    }

    fn render_image_preview_zoom_toolbar(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        fit_scale: f32,
        actual_scale: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let wb = &c.workbench;
        let editor = cx.entity().downgrade();
        let control = |id: &'static str,
                       icon: &'static str,
                       index: usize,
                       action: ImagePreviewZoomAction,
                       tooltip: SharedString| {
            let focus = self.image_preview_focus_handles[index].clone();
            let pointer_focus = focus.clone();
            let click_editor = editor.clone();
            let key_editor = editor.clone();
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .size(px(30.0))
                .tab_index(0)
                .track_focus(&focus)
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.0))
                .border(px(1.0))
                .border_color(wb.control_surface.opacity(0.0))
                .hover(|button| button.bg(wb.control_hover))
                .focus(|button| button.border_color(wb.focus_ring))
                .cursor_pointer()
                .tooltip(move |_window, cx| crate::ui::ui_tooltip(tooltip.clone(), cx))
                .child(svg().path(icon).size(px(14.0)).text_color(wb.icon))
                .on_click(move |_event, window, cx| {
                    pointer_focus.focus(window);
                    let _ = click_editor.update(cx, |editor, cx| {
                        editor.apply_image_preview_zoom_action(action, fit_scale, cx)
                    });
                    cx.stop_propagation();
                })
                .on_key_down(move |event, _window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        let _ = key_editor.update(cx, |editor, cx| {
                            editor.apply_image_preview_zoom_action(action, fit_scale, cx)
                        });
                        cx.stop_propagation();
                    }
                })
                .into_any_element()
        };
        let actual_focus = self.image_preview_focus_handles[1].clone();
        let actual_pointer_focus = actual_focus.clone();
        let actual_click_editor = editor.clone();
        let actual_key_editor = editor.clone();
        let actual_label = format!("{}%", (actual_scale * 100.0).round() as i32);
        let actual_tooltip: SharedString = "100%".into();
        let actual_size = div()
            .id("image-preview-actual-size")
            .debug_selector(|| "image-preview-actual-size".to_owned())
            .h(px(30.0))
            .min_w(px(52.0))
            .px(px(8.0))
            .tab_index(0)
            .track_focus(&actual_focus)
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .border(px(1.0))
            .border_color(wb.control_surface.opacity(0.0))
            .hover(|button| button.bg(wb.control_hover))
            .focus(|button| button.border_color(wb.focus_ring))
            .cursor_pointer()
            .text_size(px(12.0))
            .text_color(wb.text_primary)
            .tooltip(move |_window, cx| crate::ui::ui_tooltip(actual_tooltip.clone(), cx))
            .child(actual_label)
            .on_click(move |_event, window, cx| {
                actual_pointer_focus.focus(window);
                let _ = actual_click_editor.update(cx, |editor, cx| {
                    editor.apply_image_preview_zoom_action(
                        ImagePreviewZoomAction::ActualSize,
                        fit_scale,
                        cx,
                    )
                });
                cx.stop_propagation();
            })
            .on_key_down(move |event, _window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    let _ = actual_key_editor.update(cx, |editor, cx| {
                        editor.apply_image_preview_zoom_action(
                            ImagePreviewZoomAction::ActualSize,
                            fit_scale,
                            cx,
                        )
                    });
                    cx.stop_propagation();
                }
            });
        div()
            .id("image-preview-zoom-toolbar")
            .debug_selector(|| "image-preview-zoom-toolbar".to_owned())
            .absolute()
            .top(px(12.0))
            .right(px(16.0))
            .h(px(38.0))
            .px(px(4.0))
            .flex()
            .items_center()
            .gap(px(2.0))
            .rounded(px(10.0))
            .border(px(1.0))
            .border_color(wb.border_subtle)
            .bg(wb.glass_strong_surface)
            .shadow_md()
            .occlude()
            .child(control(
                "image-preview-zoom-out",
                "icon/ui/minus.svg",
                0,
                ImagePreviewZoomAction::ZoomOut,
                strings.json_graph_zoom_out.clone().into(),
            ))
            .child(actual_size)
            .child(control(
                "image-preview-zoom-in",
                "icon/ui/plus.svg",
                2,
                ImagePreviewZoomAction::ZoomIn,
                strings.json_graph_zoom_in.clone().into(),
            ))
            .child(control(
                "image-preview-fit-width",
                "icon/ui/expand.svg",
                3,
                ImagePreviewZoomAction::FitWidth,
                strings.json_graph_fit.clone().into(),
            ))
            .into_any_element()
    }
}
