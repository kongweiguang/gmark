// @author kongweiguang

//! JSON graph search, zoom, focus-root, and status controls.

use super::model::{MAX_ZOOM as GRAPH_MAX_ZOOM, MIN_ZOOM as GRAPH_MIN_ZOOM, fit_camera};
use super::panel_state::JsonGraphRenderContext;
use super::*;
use crate::theme::workbench::SurfaceKind;
use crate::ui::visual_preferences::VisualPreferencesManager;
use gpui::AnyElement;

pub(super) struct JsonGraphControls {
    pub(super) toolbar: AnyElement,
    pub(super) zoom_toolbar: AnyElement,
    pub(super) stale_banner: Option<AnyElement>,
    pub(super) truncated_banner: Option<AnyElement>,
}

pub(super) fn render_json_graph_controls(
    host: &DocumentHost,
    context: &JsonGraphRenderContext,
    graph_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    cx: &mut Context<DocumentHost>,
) -> JsonGraphControls {
    let theme = cx.global::<ThemeManager>().current_arc();
    let strings = cx.global::<I18nManager>().strings_arc();
    let colors = &theme.colors;
    let visual_preferences = cx
        .try_global::<VisualPreferencesManager>()
        .map(VisualPreferencesManager::current)
        .unwrap_or_default();
    let control_material = colors
        .workbench
        .material(SurfaceKind::Glass, visual_preferences);
    let floating_material = colors
        .workbench
        .material(SurfaceKind::GlassStrong, visual_preferences);
    let control_button = |id: &'static str,
                          icon: &'static str,
                          glyph_size: f32,
                          glyph_offset_x: f32,
                          glyph_offset_y: f32,
                          tooltip: SharedString| {
        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .size(px(28.0))
            .tab_index(0)
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(control_material.border)
            .bg(control_material.background)
            .hover(|button| button.bg(colors.workbench.control_hover))
            .focus(|button| button.border_color(colors.workbench.focus_ring))
            .cursor_pointer()
            .occlude()
            .tooltip(move |_window, cx| crate::ui::ui_tooltip(tooltip.clone(), cx))
            .child(
                svg()
                    .path(icon)
                    .size(px(glyph_size))
                    .relative()
                    .left(px(glyph_offset_x))
                    .top(px(glyph_offset_y))
                    .text_color(colors.workbench.text_secondary),
            )
    };
    let zoom_out = control_button(
        "json-graph-zoom-out",
        "icon/ui/minus.svg",
        14.0,
        0.0,
        0.0,
        strings.json_graph_zoom_out.clone().into(),
    )
    .on_click(cx.listener(|this, _, _, cx| {
        let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
            .derived
            .entry(DocumentViewId::json_graph())
            .or_default();
        state.zoom = (state.zoom - 0.1).clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM);
        cx.notify();
    }));
    let zoom_in = control_button(
        "json-graph-zoom-in",
        "icon/ui/plus.svg",
        14.0,
        0.0,
        0.0,
        strings.json_graph_zoom_in.clone().into(),
    )
    .on_click(cx.listener(|this, _, _, cx| {
        let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
            .derived
            .entry(DocumentViewId::json_graph())
            .or_default();
        state.zoom = (state.zoom + 0.1).clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM);
        cx.notify();
    }));
    let viewport_width = context.viewport_width;
    let viewport_height = context.viewport_height;
    let zoom = context.zoom;
    let actual_size = div()
        .id("json-graph-actual-size")
        .debug_selector(|| "json-graph-actual-size".to_owned())
        .h(px(28.0))
        .tab_index(0)
        .min_w(px(48.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .cursor_pointer()
        .text_size(px(11.0))
        .text_color(colors.workbench.text_secondary)
        .hover(|button| button.bg(colors.workbench.control_hover))
        .focus(|button| button.text_color(colors.workbench.text_primary))
        .tooltip(|_window, cx| crate::ui::ui_tooltip("实际大小（100%）", cx))
        .child(format!("{}%", (zoom * 100.0).round() as i32))
        .on_click(cx.listener(move |this, _, _, cx| {
            let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
                .derived
                .entry(DocumentViewId::json_graph())
                .or_default();
            let world_x = (viewport_width * 0.5 - state.camera_x) / state.zoom.max(f32::EPSILON);
            let world_y = (viewport_height * 0.5 - state.camera_y) / state.zoom.max(f32::EPSILON);
            state.zoom = 1.0;
            state.camera_x = viewport_width * 0.5 - world_x;
            state.camera_y = viewport_height * 0.5 - world_y;
            cx.notify();
        }));
    let fit_layout = context.layout.clone();
    let fit_bounds = graph_bounds.clone();
    // refresh.svg 的右侧弧线靠近 viewBox 边缘；缩小后居中绘制，避免高 DPI 下被裁剪。
    let fit = control_button(
        "json-graph-fit",
        "icon/ui/refresh.svg",
        12.0,
        0.0,
        0.0,
        strings.json_graph_fit.clone().into(),
    )
    .on_click(cx.listener(move |this, _, _, cx| {
        let (actual_width, actual_height) = fit_bounds
            .lock()
            .ok()
            .and_then(|bounds| *bounds)
            .map(|bounds| (f32::from(bounds.size.width), f32::from(bounds.size.height)))
            .unwrap_or((viewport_width, viewport_height));
        let (x, y, zoom) = fit_camera(&fit_layout, actual_width, actual_height, GRAPH_MIN_ZOOM);
        let state = document_view_state_mut(&mut this.document, &mut this.tab_view_state)
            .derived
            .entry(DocumentViewId::json_graph())
            .or_default();
        state.camera_x = x;
        state.camera_y = y;
        state.zoom = zoom;
        cx.notify();
    }));
    let search = div()
        .id("json-graph-search")
        .debug_selector(|| "json-graph-search".to_owned())
        .flex_1()
        .min_w(px(112.0))
        .max_w(px(210.0))
        .h(px(28.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .rounded(px(6.0))
        .border(px(1.0))
        .border_color(control_material.border)
        .bg(control_material.background)
        .child(
            svg()
                .path("icon/ui/search.svg")
                .size(px(13.0))
                .text_color(colors.workbench.text_tertiary),
        )
        .child(host.structured_filter_input.clone());
    let search_count = (!context.query.is_empty()).then(|| {
        div()
            .id("json-graph-search-count")
            .debug_selector(|| "json-graph-search-count".to_owned())
            .min_w(px(42.0))
            .text_size(px(11.0))
            .text_color(colors.workbench.text_tertiary)
            .child(if host.graph_search_matches.is_empty() {
                "0 / 0".to_owned()
            } else {
                format!(
                    "{} / {}",
                    host.graph_search_selected + 1,
                    host.graph_search_matches.len()
                )
            })
    });
    let search_previous = (!context.query.is_empty()).then(|| {
        control_button(
            "json-graph-search-previous",
            "icon/ui/chevron-up.svg",
            14.0,
            0.0,
            0.0,
            strings.json_graph_search_previous.clone().into(),
        )
        .on_click(cx.listener(|this, _, _, cx| this.navigate_json_graph_search(-1, cx)))
    });
    let search_next = (!context.query.is_empty()).then(|| {
        control_button(
            "json-graph-search-next",
            "icon/ui/chevron-down.svg",
            14.0,
            0.0,
            0.0,
            strings.json_graph_search_next.clone().into(),
        )
        .on_click(cx.listener(|this, _, _, cx| this.navigate_json_graph_search(1, cx)))
    });
    let selected_root = context.selected_id.as_ref().and_then(|selected| {
        context
            .graph
            .nodes
            .iter()
            .find(|node| {
                node.id == *selected
                    && matches!(node.kind, JsonValueKind::Object | JsonValueKind::Array)
            })
            .map(|node| {
                JsonGraphRoot::new(
                    node.source.clone(),
                    node.json_path.clone(),
                    node.label.clone(),
                )
            })
    });
    let focus_subtree = selected_root.map(|root| {
        div()
            .id("json-graph-focus-subtree")
            .debug_selector(|| "json-graph-focus-subtree".to_owned())
            .h(px(28.0))
            .tab_index(0)
            .px(px(9.0))
            .flex()
            .items_center()
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(control_material.border)
            .bg(control_material.background)
            .hover(|button| button.bg(colors.workbench.control_hover))
            .focus(|button| button.border_color(colors.workbench.focus_ring))
            .cursor_pointer()
            .text_size(px(11.0))
            .text_color(colors.workbench.text_secondary)
            .child(strings.json_graph_focus_subtree.clone())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.derived_projection_root = Some(root.clone());
                this.graph_selected_item = None;
                this.graph_state_initialized = false;
                this.graph_needs_fit = true;
                this.derived_projection_stale = this.derived_projection_snapshot.is_some();
                this.request_registered_projection(cx);
            }))
    });
    let reset_root = host.derived_projection_root.is_some().then(|| {
        div()
            .id("json-graph-reset-root")
            .debug_selector(|| "json-graph-reset-root".to_owned())
            .h(px(28.0))
            .tab_index(0)
            .px(px(9.0))
            .flex()
            .items_center()
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(control_material.border)
            .bg(control_material.background)
            .hover(|button| button.bg(colors.workbench.control_hover))
            .focus(|button| button.border_color(colors.workbench.focus_ring))
            .cursor_pointer()
            .text_size(px(11.0))
            .text_color(colors.workbench.text_secondary)
            .child(strings.json_graph_reset_root.clone())
            .on_click(cx.listener(|this, _, _, cx| {
                this.derived_projection_root = None;
                this.graph_selected_item = None;
                this.graph_state_initialized = false;
                this.graph_needs_fit = true;
                this.derived_projection_stale = this.derived_projection_snapshot.is_some();
                this.request_registered_projection(cx);
            }))
    });
    let toolbar = div()
        .absolute()
        .top(px(10.0))
        .left(px(10.0))
        .h(px(32.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .occlude()
        .child(search)
        .children(search_count)
        .children(search_previous)
        .children(search_next)
        .children(reset_root)
        .children(focus_subtree)
        .into_any_element();
    let zoom_toolbar = div()
        .id("json-graph-zoom-toolbar")
        .debug_selector(|| "json-graph-zoom-toolbar".to_owned())
        .absolute()
        .bottom(px(12.0))
        .left(relative(0.5))
        .ml(px(-77.0))
        .h(px(36.0))
        .px(px(4.0))
        .flex()
        .items_center()
        .gap(px(3.0))
        .rounded(px(9.0))
        .border(px(1.0))
        .border_color(floating_material.border)
        .bg(floating_material.background)
        .shadow_md()
        .occlude()
        .child(zoom_out)
        .child(actual_size)
        .child(zoom_in)
        .child(fit)
        .into_any_element();
    let stale_banner = host.derived_projection_stale.then(|| {
        let detail = host
            .derived_projection_error
            .clone()
            .unwrap_or_else(|| strings.json_graph_source_changed.clone().into());
        div()
            .id("json-graph-stale-banner")
            .debug_selector(|| "json-graph-stale-banner".to_owned())
            .absolute()
            .top(px(50.0))
            .left(px(10.0))
            .right(px(10.0))
            .h(px(34.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(colors.callout_warning_border)
            .bg(colors.callout_warning_bg)
            .text_size(px(11.0))
            .text_color(colors.workbench.text_primary)
            .child(strings.json_graph_stale.clone())
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_color(colors.workbench.text_tertiary)
                    .child(detail),
            )
            .into_any_element()
    });
    let truncated_banner = context.graph.truncated.then(|| {
        div()
            .absolute()
            .bottom(px(56.0))
            .left(px(10.0))
            .px(px(10.0))
            .h(px(30.0))
            .flex()
            .items_center()
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(control_material.border)
            .bg(control_material.background)
            .text_size(px(11.0))
            .text_color(colors.workbench.text_tertiary)
            .child(strings.json_graph_truncated.clone())
            .into_any_element()
    });

    JsonGraphControls {
        toolbar,
        zoom_toolbar,
        stale_banner,
        truncated_banner,
    }
}
