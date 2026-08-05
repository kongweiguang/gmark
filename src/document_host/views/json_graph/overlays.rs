// @author kongweiguang

//! JSON graph inspector and context-menu overlays.

use super::panel_state::JsonGraphRenderContext;
use super::support::{bounded_node_content, jsonpath_for_display, node_edit_target_for_identity};
use super::*;
use crate::theme::workbench::SurfaceKind;
use crate::ui::visual_preferences::VisualPreferencesManager;
use gpui::AnyElement;

pub(super) struct JsonGraphOverlays {
    pub(super) detail_panel: Option<AnyElement>,
    pub(super) context_menu: Option<AnyElement>,
}

pub(super) fn render_json_graph_overlays(
    host: &DocumentHost,
    context: &JsonGraphRenderContext,
    cx: &mut Context<DocumentHost>,
) -> JsonGraphOverlays {
    let theme = cx.global::<ThemeManager>().current_arc();
    let strings = cx.global::<I18nManager>().strings_arc();
    let colors = &theme.colors;
    let visual_preferences = cx
        .try_global::<VisualPreferencesManager>()
        .map(VisualPreferencesManager::current)
        .unwrap_or_default();
    let floating_material = colors
        .workbench
        .material(SurfaceKind::GlassStrong, visual_preferences);
    let control_material = colors
        .workbench
        .material(SurfaceKind::Glass, visual_preferences);
    let viewport_width = context.viewport_width;
    let viewport_height = context.viewport_height;
    let detail_panel = context.selected_detail.clone().map(|detail| {
        let json_path = jsonpath_for_display(&detail.json_path);
        let copy_path = json_path.clone();
        let wide = viewport_width >= 820.0;
        let panel_width = 360.0_f32.min((viewport_width - 24.0).max(280.0));
        let panel_top = if wide {
            54.0
        } else {
            (viewport_height - viewport_height.min(320.0) - 12.0).max(54.0)
        };
        div()
            .id("json-graph-node-details")
            .debug_selector(|| "json-graph-node-details".to_owned())
            .absolute()
            .top(px(panel_top))
            .when(wide, |panel| panel.right(px(12.0)).w(px(panel_width)))
            .when(!wide, |panel| panel.left(px(12.0)).right(px(12.0)))
            .max_h(px(if wide {
                (viewport_height - 66.0).max(240.0)
            } else {
                viewport_height.min(320.0)
            }))
            .p(px(14.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .occlude()
            .rounded(px(9.0))
            .border(px(1.0))
            .border_color(floating_material.border)
            .bg(floating_material.background)
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation()
            })
            .child(
                div()
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(13.0))
                    .text_color(colors.workbench.text_primary)
                    .child(strings.json_graph_details_title.clone())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .id("json-graph-node-details-copy-path")
                                    .h(px(26.0))
                                    .tab_index(0)
                                    .px(px(8.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.0))
                                    .cursor_pointer()
                                    .text_size(px(11.0))
                                    .text_color(colors.workbench.text_secondary)
                                    .hover(|button| button.bg(colors.workbench.control_hover))
                                    .focus(|button| {
                                        button.text_color(colors.workbench.text_primary)
                                    })
                                    .child(strings.json_graph_copy_path.clone())
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy_path.clone(),
                                        ));
                                    })),
                            )
                            .child(
                                div()
                                    .id("json-graph-node-details-edit")
                                    .h(px(26.0))
                                    .tab_index(0)
                                    .px(px(8.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.0))
                                    .cursor_pointer()
                                    .text_size(px(11.0))
                                    .text_color(colors.workbench.text_secondary)
                                    .hover(|button| button.bg(colors.workbench.control_hover))
                                    .focus(|button| {
                                        button.text_color(colors.workbench.text_primary)
                                    })
                                    .child(strings.json_graph_edit_value.clone())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.begin_json_graph_edit(
                                            detail.edit_target.clone(),
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                div()
                                    .id("json-graph-node-details-close")
                                    .size(px(26.0))
                                    .tab_index(0)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(5.0))
                                    .cursor_pointer()
                                    .hover(|button| button.bg(colors.workbench.control_hover))
                                    .focus(|button| {
                                        button.border_color(colors.workbench.focus_ring)
                                    })
                                    .tooltip({
                                        let label: SharedString = strings.ui_close.clone().into();
                                        move |_window, cx| crate::ui::ui_tooltip(label.clone(), cx)
                                    })
                                    .child(
                                        svg()
                                            .path(CLOSE_ICON)
                                            .size(px(14.0))
                                            .text_color(colors.workbench.text_tertiary),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.dismiss_json_graph_details();
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(colors.workbench.text_tertiary)
                            .child(strings.json_graph_content.clone()),
                    )
                    .child(
                        div()
                            .id("json-graph-node-details-content")
                            .max_h(px(210.0))
                            .p(px(10.0))
                            .overflow_y_scroll()
                            .rounded(px(6.0))
                            .bg(colors.workbench.editor_surface)
                            .font_family(source_monospace_font_family())
                            .text_size(px(11.0))
                            .text_color(colors.workbench.text_primary)
                            .child(detail.content),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(colors.workbench.text_tertiary)
                            .child(strings.json_graph_path.clone()),
                    )
                    .child(
                        div()
                            .id("json-graph-node-details-path")
                            .debug_selector(|| "json-graph-node-details-path".to_owned())
                            .p(px(9.0))
                            .overflow_hidden()
                            .truncate()
                            .rounded(px(6.0))
                            .bg(colors.workbench.editor_surface)
                            .font_family(source_monospace_font_family())
                            .text_size(px(11.0))
                            .text_color(colors.workbench.accent)
                            .child(json_path),
                    ),
            )
            .into_any_element()
    });
    let context_menu = host.graph_context_menu.as_ref().and_then(|menu| {
        let node = context
            .graph
            .nodes
            .iter()
            .find(|node| node.id == menu.node)?;
        let source = node.source.range.clone();
        let json_path = jsonpath_for_display(&node.json_path);
        let content = bounded_node_content(host.document.as_ref(), node);
        let node_id = node.id.clone();
        let edit_target = node_edit_target_for_identity(
            context.projection_epoch,
            context.projection_revision,
            node,
        );
        let collapsible = node.child_count > 0;
        let is_collapsed = context.collapsed.contains(node.id.as_str());
        let focus_root =
            matches!(node.kind, JsonValueKind::Object | JsonValueKind::Array).then(|| {
                JsonGraphRoot::new(
                    node.source.clone(),
                    node.json_path.clone(),
                    node.label.clone(),
                )
            });
        let panel_width = 210.0;
        let panel_height = 30.0 * (4.0 + f32::from(collapsible)) + 16.0;
        let left =
            f32::from(menu.position.x).clamp(8.0, (viewport_width - panel_width - 8.0).max(8.0));
        let top =
            f32::from(menu.position.y).clamp(8.0, (viewport_height - panel_height - 8.0).max(8.0));
        let item = |id: &'static str, label: String| {
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .h(px(30.0))
                .tab_index(0)
                .px(px(10.0))
                .flex()
                .items_center()
                .rounded(px(5.0))
                .text_size(px(11.0))
                .text_color(colors.workbench.text_secondary)
                .hover(|item| item.bg(colors.workbench.control_hover))
                .focus(|item| item.text_color(colors.workbench.text_primary))
                .cursor_pointer()
                .child(label)
        };
        Some(
            div()
                .id("json-graph-context-menu-overlay")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.graph_context_menu = None;
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .id("json-graph-context-menu")
                        .debug_selector(|| "json-graph-context-menu".to_owned())
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .w(px(panel_width))
                        .p(px(7.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .rounded(px(8.0))
                        .border(px(1.0))
                        .border_color(control_material.border)
                        .bg(control_material.background)
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                        .child(
                            item(
                                "json-graph-context-edit",
                                strings.json_graph_edit_value.clone(),
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.begin_json_graph_edit(edit_target.clone(), window, cx);
                                },
                            )),
                        )
                        .child(
                            item(
                                "json-graph-context-locate",
                                strings.json_graph_locate_source.clone(),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    let preserve_split =
                                        this.view_mode == DocumentHostViewMode::Split;
                                    this.graph_context_menu = None;
                                    this.select_json_source_range(
                                        source.clone(),
                                        preserve_split,
                                        cx,
                                    );
                                    if !preserve_split {
                                        cx.emit(DocumentHostEvent::ViewModeChanged(
                                            DocumentHostMode::Source,
                                        ));
                                    }
                                },
                            )),
                        )
                        .children(focus_root.map(|root| {
                            item(
                                "json-graph-context-focus",
                                strings.json_graph_focus_subtree.clone(),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.graph_context_menu = None;
                                    this.derived_projection_root = Some(root.clone());
                                    this.graph_selected_item = None;
                                    this.graph_state_initialized = false;
                                    this.graph_needs_fit = true;
                                    this.derived_projection_stale =
                                        this.derived_projection_snapshot.is_some();
                                    this.request_registered_projection(cx);
                                },
                            ))
                        }))
                        .children(collapsible.then(|| {
                            item(
                                "json-graph-context-toggle",
                                if is_collapsed {
                                    strings.json_graph_expand.clone()
                                } else {
                                    strings.json_graph_collapse.clone()
                                },
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.graph_context_menu = None;
                                    let state = document_view_state_mut(
                                        &mut this.document,
                                        &mut this.tab_view_state,
                                    )
                                    .derived
                                    .entry(DocumentViewId::json_graph())
                                    .or_default();
                                    if is_collapsed {
                                        state
                                            .collapsed_items
                                            .retain(|item| item.as_ref() != node_id.as_str());
                                    } else {
                                        state.collapsed_items.push(Arc::from(node_id.as_str()));
                                    }
                                    cx.notify();
                                },
                            ))
                        }))
                        .child(
                            item(
                                "json-graph-context-copy-path",
                                strings.json_graph_copy_path.clone(),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        json_path.clone(),
                                    ));
                                    this.graph_context_menu = None;
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            item(
                                "json-graph-context-copy-content",
                                strings.json_graph_copy_content.clone(),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        content.to_string(),
                                    ));
                                    this.graph_context_menu = None;
                                    cx.notify();
                                },
                            )),
                        ),
                )
                .into_any_element(),
        )
    });

    JsonGraphOverlays {
        detail_panel,
        context_menu,
    }
}
