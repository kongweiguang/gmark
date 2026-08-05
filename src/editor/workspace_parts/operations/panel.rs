// @author kongweiguang

use super::*;
use crate::theme::workbench::SurfaceKind;
use crate::ui::visual_preferences::VisualPreferencesManager;

impl Editor {
    pub(in crate::editor) fn render_workspace_panel(
        &mut self,
        theme: &Theme,
        strings: &I18nStrings,
        panel_width: f32,
        resizable: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.focus_mode || !self.workspace.is_open {
            return None;
        }

        self.sync_workspace_models(cx);
        if self.workspace.active_tab == WorkspaceTab::Search {
            self.ensure_workspace_search_input(cx);
        }
        let focus_handle = self.ensure_workspace_focus_handle(cx);
        let editor = cx.entity().downgrade();
        let resize_editor = editor.clone();
        let resize_key_editor = editor.clone();
        let c = &theme.colors;
        let d = &theme.dimensions;
        let visual_preferences = cx
            .try_global::<VisualPreferencesManager>()
            .map(VisualPreferencesManager::current)
            .unwrap_or_default();
        // Docked navigation is a quiet material; compact navigation is the
        // only workspace surface allowed to float over the editor.
        let panel_material = c.workbench.material(
            if resizable {
                SurfaceKind::Navigation
            } else {
                SurfaceKind::Glass
            },
            visual_preferences,
        );
        let content_material = c.workbench.material(SurfaceKind::Solid, visual_preferences);
        let transparent = c.workbench.control_surface.opacity(0.0);
        let resize_focus_handle = resizable.then(|| self.ensure_workspace_resize_focus_handle(cx));
        let header_focus_handles = self.ensure_workspace_header_focus_handles(cx);
        let resize_active = self.workspace.resize_session.is_some();

        let tab = |label: String, icon: &'static str, tab: WorkspaceTab, active: bool| {
            let tab_editor = editor.clone();
            let tab_key_editor = editor.clone();
            let hover_editor = editor.clone();
            let tab_id = match tab {
                WorkspaceTab::Files => "workspace-tab-files",
                WorkspaceTab::Outline => "workspace-tab-outline",
                WorkspaceTab::Search => "workspace-tab-search",
            };
            let tab_focus_handle = header_focus_handles[match tab {
                WorkspaceTab::Files => 0,
                WorkspaceTab::Outline => 1,
                WorkspaceTab::Search => 2,
            }]
            .clone();
            let pointer_focus_handle = tab_focus_handle.clone();
            div()
                .id(tab_id)
                .debug_selector(move || tab_id.to_owned())
                .relative()
                .size(px(32.0))
                .tab_index(0)
                .track_focus(&tab_focus_handle)
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.0))
                .border(px(1.0))
                .border_color(if active {
                    c.workbench.border_subtle
                } else {
                    transparent
                })
                .bg(if active {
                    c.workbench.control_hover
                } else {
                    transparent
                })
                .hover(|this| this.bg(c.workbench.control_hover))
                .focus(|this| this.border_color(c.workbench.focus_ring))
                .cursor_pointer()
                .text_color(if active {
                    c.workbench.text_primary
                } else {
                    c.workbench.text_secondary
                })
                // GPUI 的 SVG 不稳定继承父级 currentColor，chrome 图标必须显式着色。
                .child(
                    svg()
                        .path(icon)
                        .size(px(16.0))
                        .text_color(if active {
                            c.workbench.text_primary
                        } else {
                            c.workbench.text_secondary
                        })
                        .debug_selector(move || format!("{tab_id}-icon")),
                )
                .children(
                    (self.workspace.tooltip_visible == Some(tab_id))
                        .then(|| render_workspace_tooltip(label, 36.0, theme)),
                )
                .on_hover(move |hovered, _window, cx| {
                    let _ = hover_editor.update(cx, |editor, cx| {
                        editor.set_workspace_tooltip_hover(tab_id, *hovered, cx);
                    });
                })
                .on_click(move |_event, window, cx| {
                    pointer_focus_handle.focus(window);
                    let _ = tab_editor.update(cx, |editor, cx| {
                        editor.set_workspace_tab(tab, cx);
                    });
                })
                .on_key_down(move |event, _window, cx| {
                    let _ = tab_key_editor.update(cx, |editor, cx| {
                        editor.on_workspace_tab_key_down(tab, event, cx);
                    });
                })
        };

        let body = match self.workspace.active_tab {
            WorkspaceTab::Files => self.render_workspace_files_tree(theme, strings, &editor),
            WorkspaceTab::Outline => self.render_workspace_outline_tree(theme, strings, &editor),
            WorkspaceTab::Search => self.render_workspace_search(theme, strings, &editor, cx),
        };

        Some(
            div()
                .id("workspace-panel")
                .debug_selector(|| "workspace-panel".to_owned())
                .track_focus(&focus_handle)
                .relative()
                .h_full()
                .w(px(panel_width))
                .flex()
                .flex_col()
                .flex_shrink_0()
                .bg(panel_material.background)
                .border_r(px(d.dialog_border_width))
                .border_color(panel_material.border)
                .child(
                    div()
                        .id("workspace-panel-header")
                        .debug_selector(|| "workspace-panel-header".to_owned())
                        .h(px(44.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .border_b(px(1.0))
                        .border_color(panel_material.border)
                        .child(tab(
                            strings.workspace_tab_files.clone(),
                            FILES_TAB_ICON,
                            WorkspaceTab::Files,
                            self.workspace.active_tab == WorkspaceTab::Files,
                        ))
                        .child(tab(
                            strings.workspace_tab_search.clone(),
                            SEARCH_TAB_ICON,
                            WorkspaceTab::Search,
                            self.workspace.active_tab == WorkspaceTab::Search,
                        ))
                        .child(div().flex_1().min_w(px(0.0))),
                )
                .child(
                    div()
                        .id("workspace-panel-scroll")
                        .track_scroll(&self.workspace.panel_scroll)
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_y_scroll()
                        .bg(content_material.background)
                        .px(px(8.0))
                        .py(px(10.0))
                        .child(body),
                )
                .children(resizable.then(|| {
                    let focus_handle = resize_focus_handle
                        .clone()
                        .expect("resizable workspace must own a focus handle");
                    let handle = div()
                        .id("workspace-resize-handle")
                        .debug_selector(|| "workspace-resize-handle".to_owned())
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .w(px(WORKSPACE_RESIZE_HIT_WIDTH))
                        .tab_index(0)
                        .track_focus(&focus_handle)
                        .cursor_col_resize()
                        .hover(|this| this.bg(c.workbench.control_hover.opacity(0.7)))
                        .focus(|this| this.bg(c.workbench.control_hover.opacity(0.7)))
                        .child(
                            div()
                                .id("workspace-resize-line")
                                .debug_selector(|| "workspace-resize-line".to_owned())
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(px((WORKSPACE_RESIZE_HIT_WIDTH - 1.0) * 0.5))
                                .w(px(1.0))
                                .bg(if resize_active {
                                    c.workbench.focus_ring.opacity(0.72)
                                } else {
                                    panel_material.border
                                }),
                        );
                    let handle = handle.right(px(-WORKSPACE_RESIZE_HIT_WIDTH * 0.5));
                    handle
                        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                            focus_handle.focus(window);
                            let _ = resize_editor.update(cx, |editor, cx| {
                                editor.start_workspace_resize(event.position.x, panel_width, cx);
                            });
                            cx.stop_propagation();
                        })
                        .on_key_down(move |event, _window, cx| {
                            let _ = resize_key_editor.update(cx, |editor, cx| {
                                editor.on_workspace_resize_key_down(event, panel_width, cx);
                            });
                        })
                }))
                .into_any_element(),
        )
    }
}
