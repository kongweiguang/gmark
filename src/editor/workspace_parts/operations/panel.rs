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
        let resize_focus_handle = resizable.then(|| self.ensure_workspace_resize_focus_handle(cx));
        let resize_active = self.workspace.resize_session.is_some();

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
                // Files and Search no longer live in a permanent top chrome;
                // the panel body starts at the top edge and the status bar owns
                // the two navigation actions, matching Zed's workbench.
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
