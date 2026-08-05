// @author kongweiguang

//! JSON graph value-edit transactions and their modal surface.

use super::support::{field_edit_target, node_edit_target};
use super::*;
use crate::theme::workbench::SurfaceKind;
use crate::ui::visual_preferences::VisualPreferencesManager;

impl DocumentHost {
    pub(in crate::document_host::implementation) fn begin_json_graph_edit(
        &mut self,
        target: JsonGraphEditTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        const MAX_GRAPH_EDIT_BYTES: u64 = 256 * 1024;
        let byte_len = target.range.end.saturating_sub(target.range.start);
        let content = if byte_len <= MAX_GRAPH_EDIT_BYTES {
            self.document
                .as_ref()
                .and_then(|document| document.read_range(target.range.clone()).ok())
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        } else {
            None
        };
        let Some(content) = content else {
            let strings = cx.global::<I18nManager>().strings();
            self.graph_edit_error = Some(
                strings
                    .json_graph_edit_too_large_template
                    .replace("{bytes}", &byte_len.to_string())
                    .into(),
            );
            self.graph_edit_issue = Some(JsonGraphEditIssue::TooLarge);
            self.graph_edit_original = None;
            self.graph_edit_target = Some(target);
            self.graph_edit_input.update(cx, |block, cx| {
                let len = block.display_text().len();
                block.replace_text_in_visible_range(0..len, "", None, false, cx);
                block.focus_handle.focus(window);
            });
            cx.notify();
            return;
        };
        self.graph_context_menu = None;
        self.graph_edit_error = None;
        self.graph_edit_issue = None;
        self.graph_edit_original = Some(Arc::from(content.as_str()));
        self.graph_edit_target = Some(target);
        self.graph_edit_input.update(cx, |block, cx| {
            let len = block.display_text().len();
            block.replace_text_in_visible_range(0..len, &content, None, false, cx);
            block.focus_handle.focus(window);
        });
        cx.notify();
    }

    fn commit_json_graph_edit(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.graph_edit_target.clone() else {
            return;
        };
        let replacement = self.graph_edit_input.read(cx).display_text().to_owned();
        let current_revision = self.document.as_ref().map(DocumentSession::revision);
        if target.document_epoch != self.document_epoch
            || current_revision != Some(target.base_revision)
        {
            let strings = cx.global::<I18nManager>().strings();
            self.graph_edit_error = Some(strings.json_graph_source_changed.clone().into());
            self.graph_edit_issue = Some(JsonGraphEditIssue::Stale);
            cx.notify();
            return;
        }
        let parsed = serde_json::from_str::<serde_json::Value>(replacement.trim());
        let valid = parsed.as_ref().is_ok_and(|value| match target.kind {
            JsonValueKind::Object => value.is_object(),
            JsonValueKind::Array => value.is_array(),
            JsonValueKind::String
            | JsonValueKind::Number
            | JsonValueKind::Boolean
            | JsonValueKind::Null => !value.is_object() && !value.is_array(),
        });
        if !valid {
            let strings = cx.global::<I18nManager>().strings();
            self.graph_edit_error = Some(strings.json_graph_edit_invalid.clone().into());
            self.graph_edit_issue = Some(JsonGraphEditIssue::Invalid);
            cx.notify();
            return;
        }
        if self.replace_source_range_from_graph(
            target.base_revision,
            target.range,
            replacement.trim(),
            cx,
        ) {
            self.graph_edit_target = None;
            self.graph_edit_error = None;
            self.graph_edit_issue = None;
            self.graph_edit_original = None;
        }
    }

    fn cancel_json_graph_edit(&mut self, cx: &mut Context<Self>) {
        self.graph_edit_target = None;
        self.graph_edit_error = None;
        self.graph_edit_issue = None;
        self.graph_edit_original = None;
        cx.notify();
    }

    fn cancel_json_graph_edit_if_pristine(&mut self, cx: &mut Context<Self>) {
        let draft = self.graph_edit_input.read(cx).display_text();
        if self
            .graph_edit_original
            .as_deref()
            .is_some_and(|original| original != draft)
        {
            return;
        }
        self.cancel_json_graph_edit(cx);
    }

    fn reload_json_graph_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item_id) = self
            .graph_edit_target
            .as_ref()
            .map(|target| target.item_id.clone())
        else {
            return;
        };
        let Some(target) = self.resolve_json_graph_edit_target(&item_id) else {
            self.cancel_json_graph_edit(cx);
            return;
        };
        self.begin_json_graph_edit(target, window, cx);
    }

    pub(in crate::document_host::implementation) fn resolve_json_graph_edit_target(
        &self,
        item_id: &JsonGraphItemId,
    ) -> Option<JsonGraphEditTarget> {
        let snapshot = self
            .derived_projection_snapshot
            .as_ref()?
            .as_any()
            .downcast_ref::<JsonGraphSnapshot>()?;
        if let Some(node) = snapshot
            .projection()
            .nodes
            .iter()
            .find(|node| node.id == *item_id)
        {
            return Some(node_edit_target(snapshot, node));
        }
        snapshot
            .projection()
            .nodes
            .iter()
            .flat_map(|node| node.fields.iter())
            .find(|field| field.id == *item_id)
            .map(|field| field_edit_target(snapshot, field))
    }

    pub(crate) fn begin_selected_json_graph_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self
            .graph_selected_item
            .as_ref()
            .and_then(|item| self.resolve_json_graph_edit_target(item))
        else {
            self.graph_focus_handle.focus(window);
            return;
        };
        self.begin_json_graph_edit(target, window, cx);
    }

    /// 编辑遮罩必须挂在 SourceBacked 内容根层，不能成为可缩放、可裁剪画布的子元素。
    /// 这样 Preview 与 Split 共享同一套焦点和尺寸语义，窗口变化时也不会丢失草稿。
    pub(in crate::document_host::implementation) fn render_json_graph_edit_overlay(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let target = self.graph_edit_target.clone()?;
        let theme = cx.global::<ThemeManager>().current();
        let colors = &theme.colors;
        let strings = cx.global::<I18nManager>().strings();
        let visual_preferences = cx
            .try_global::<VisualPreferencesManager>()
            .map(VisualPreferencesManager::current)
            .unwrap_or_default();
        let overlay_material = colors
            .workbench
            .material(SurfaceKind::GlassStrong, visual_preferences);
        let control_material = colors
            .workbench
            .material(SurfaceKind::Glass, visual_preferences);
        let container = matches!(target.kind, JsonValueKind::Object | JsonValueKind::Array);
        let error = self.graph_edit_error.clone();
        let issue = self.graph_edit_issue;
        let title = format!("{} · {}", strings.json_graph_edit_value, target.label);
        Some(
            div()
                .id("json-graph-edit-overlay")
                .debug_selector(|| "json-graph-edit-overlay".to_owned())
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .items_center()
                .justify_center()
                .occlude()
                .bg(colors.workbench.overlay_scrim)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.cancel_json_graph_edit_if_pristine(cx)),
                )
                .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        cx.stop_propagation();
                        this.cancel_json_graph_edit(cx);
                    } else if event.keystroke.key == "enter"
                        && (!container
                            || event.keystroke.modifiers.control
                            || event.keystroke.modifiers.platform)
                    {
                        cx.stop_propagation();
                        this.commit_json_graph_edit(cx);
                    }
                }))
                .child(
                    div()
                        .id("json-graph-edit-panel")
                        .debug_selector(|| "json-graph-edit-panel".to_owned())
                        .w(px(viewport_width.min(560.0).max(300.0)))
                        .max_h(px((viewport_height - 64.0).max(280.0)))
                        .p(px(14.0))
                        .flex()
                        .flex_col()
                        .gap(px(9.0))
                        .rounded(px(9.0))
                        .border(px(1.0))
                        .border_color(overlay_material.border)
                        .bg(overlay_material.background)
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(colors.workbench.text_primary)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(colors.workbench.text_tertiary)
                                .child(strings.json_graph_edit_help.clone()),
                        )
                        .child(
                            div()
                                .id("json-graph-edit-input")
                                .debug_selector(|| "json-graph-edit-input".to_owned())
                                .min_h(px(if container { 180.0 } else { 44.0 }))
                                .max_h(px(320.0))
                                .p(px(9.0))
                                .overflow_y_scroll()
                                .rounded(px(6.0))
                                .border(px(1.0))
                                .border_color(if error.is_some() {
                                    colors.callout_warning_border
                                } else {
                                    overlay_material.border
                                })
                                .bg(colors.workbench.editor_surface)
                                .child(self.graph_edit_input.clone()),
                        )
                        .children(error.map(|error| {
                            div()
                                .id("json-graph-edit-error")
                                .debug_selector(|| "json-graph-edit-error".to_owned())
                                .text_size(px(11.0))
                                .text_color(colors.workbench.text_primary)
                                .child(error)
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_end()
                                .gap(px(7.0))
                                .children((issue == Some(JsonGraphEditIssue::Stale)).then(|| {
                                    div()
                                        .id("json-graph-edit-reload")
                                        .debug_selector(|| "json-graph-edit-reload".to_owned())
                                        .h(px(30.0))
                                        .tab_index(0)
                                        .px(px(11.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.0))
                                        .cursor_pointer()
                                        .bg(control_material.background)
                                        .hover(|button| button.bg(colors.workbench.control_hover))
                                        .focus(|button| {
                                            button.border_color(colors.workbench.focus_ring)
                                        })
                                        .text_size(px(11.0))
                                        .text_color(colors.workbench.text_secondary)
                                        .child(strings.json_graph_reload_value.clone())
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.reload_json_graph_edit(window, cx)
                                        }))
                                }))
                                .children((issue == Some(JsonGraphEditIssue::TooLarge)).then(
                                    || {
                                        let range = target.range.clone();
                                        div()
                                            .id("json-graph-edit-source")
                                            .debug_selector(|| "json-graph-edit-source".to_owned())
                                            .h(px(30.0))
                                            .tab_index(0)
                                            .px(px(11.0))
                                            .flex()
                                            .items_center()
                                            .rounded(px(6.0))
                                            .cursor_pointer()
                                            .bg(control_material.background)
                                            .hover(|button| {
                                                button.bg(colors.workbench.control_hover)
                                            })
                                            .focus(|button| {
                                                button.border_color(colors.workbench.focus_ring)
                                            })
                                            .text_size(px(11.0))
                                            .text_color(colors.workbench.text_secondary)
                                            .child(strings.json_graph_edit_source.clone())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.cancel_json_graph_edit(cx);
                                                this.select_json_source_range(
                                                    range.clone(),
                                                    false,
                                                    cx,
                                                );
                                                cx.emit(DocumentHostEvent::ViewModeChanged(
                                                    DocumentHostMode::Source,
                                                ));
                                            }))
                                    },
                                ))
                                .child(
                                    div()
                                        .id("json-graph-edit-cancel")
                                        .debug_selector(|| "json-graph-edit-cancel".to_owned())
                                        .h(px(30.0))
                                        .tab_index(0)
                                        .px(px(11.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.0))
                                        .cursor_pointer()
                                        .bg(control_material.background)
                                        .hover(|button| button.bg(colors.workbench.control_hover))
                                        .focus(|button| {
                                            button.border_color(colors.workbench.focus_ring)
                                        })
                                        .text_size(px(11.0))
                                        .text_color(colors.workbench.text_secondary)
                                        .child(strings.unsaved_changes_cancel.clone())
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.cancel_json_graph_edit(cx)
                                        })),
                                )
                                .children((issue != Some(JsonGraphEditIssue::TooLarge)).then(
                                    || {
                                        div()
                                            .id("json-graph-edit-save")
                                            .debug_selector(|| "json-graph-edit-save".to_owned())
                                            .h(px(30.0))
                                            .tab_index(0)
                                            .px(px(11.0))
                                            .flex()
                                            .items_center()
                                            .rounded(px(6.0))
                                            .cursor_pointer()
                                            .bg(colors.workbench.accent)
                                            .hover(|button| {
                                                button.bg(colors.workbench.accent_hover)
                                            })
                                            .focus(|button| {
                                                button.border_color(colors.workbench.focus_ring)
                                            })
                                            .text_size(px(11.0))
                                            .text_color(colors.workbench.text_inverse)
                                            .child(strings.menu_save.clone())
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.commit_json_graph_edit(cx)
                                            }))
                                    },
                                )),
                        ),
                ),
        )
    }
}
