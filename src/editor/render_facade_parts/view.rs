// @author kongweiguang

use super::layout::*;
use super::*;

pub(super) fn submenu_panel_top(
    items: &[OwnedMenuItem],
    item_index: usize,
    dimensions: &ThemeDimensions,
) -> f32 {
    let prior_items_height: f32 = items
        .iter()
        .take(item_index)
        .map(|item| menu_item_visual_height(item, dimensions))
        .sum();
    let prior_gaps = dimensions.menu_panel_gap * item_index as f32;
    dimensions.menu_panel_top + dimensions.menu_panel_padding + prior_items_height + prior_gaps
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MenuSubmenuBridgeGeometry {
    pub(super) left: f32,
    pub(super) top: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

#[cfg(test)]
pub(super) fn submenu_bridge_geometry<S: AsRef<str>, T: AsRef<str>>(
    open_index: usize,
    menu_labels: &[S],
    items: &[OwnedMenuItem],
    item_index: usize,
    submenu_labels: &[T],
    dimensions: &ThemeDimensions,
) -> Option<MenuSubmenuBridgeGeometry> {
    submenu_bridge_geometry_from_origin(
        0.0,
        open_index,
        menu_labels,
        items,
        item_index,
        submenu_labels,
        dimensions,
    )
}

pub(super) fn submenu_bridge_geometry_from_origin<S: AsRef<str>, T: AsRef<str>>(
    origin_x: f32,
    open_index: usize,
    menu_labels: &[S],
    items: &[OwnedMenuItem],
    item_index: usize,
    submenu_labels: &[T],
    dimensions: &ThemeDimensions,
) -> Option<MenuSubmenuBridgeGeometry> {
    let item = items.get(item_index)?;
    let main_panel_left =
        menu_panel_left_from_origin(origin_x, open_index, menu_labels, dimensions);
    let main_panel_width = menu_panel_width_for_labels(&owned_menu_item_labels(items), dimensions);
    let submenu_width = menu_panel_width_for_labels(submenu_labels, dimensions);
    let vertical_tolerance = dimensions.menu_panel_padding + dimensions.menu_panel_gap;
    let item_top = submenu_panel_top(items, item_index, dimensions);
    let top = (item_top - vertical_tolerance).max(dimensions.menu_panel_top);
    Some(MenuSubmenuBridgeGeometry {
        left: main_panel_left + main_panel_width,
        top,
        width: dimensions.menu_panel_gap + submenu_width,
        height: menu_item_visual_height(item, dimensions) + vertical_tolerance * 2.0,
    })
}

pub(super) fn footnote_group_shell(
    children: Vec<AnyElement>,
    theme: &Theme,
    dimensions: &ThemeDimensions,
) -> AnyElement {
    div()
        .w_full()
        .flex_shrink_0()
        .px(px(crate::components::rendered_content_inset(dimensions)))
        .child(
            div()
                .debug_selector(|| "footnote-surface".to_owned())
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(0.0))
                .px(px(dimensions.footnote_padding_x))
                .py(px(dimensions.footnote_padding_y))
                .rounded(px(dimensions.footnote_radius))
                .border(px(1.0))
                .border_color(theme.colors.footnote_border)
                .bg(theme.colors.footnote_bg)
                .children(children),
        )
        .into_any_element()
}

impl Editor {}

#[path = "../render_parts/dialogs.rs"]
mod dialogs;
#[path = "../render_parts/info_dialog.rs"]
mod info_dialog;
#[path = "../render_parts/window_actions.rs"]
mod window_actions;
#[path = "../render_parts/window_state.rs"]
mod window_state;

impl Editor {
    pub(crate) fn on_go_to_line_action(
        &mut self,
        action: &crate::components::GoToLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(large_file) = self.source_surface.disk_view_cloned() {
            large_file.update(cx, |large_file, cx| {
                large_file.on_go_to_line(action, window, cx);
            });
        }
    }

    pub(in crate::editor) fn activate_document_toolbar_action(
        &mut self,
        action: DocumentToolbarAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            DocumentToolbarAction::QuickOpen => {
                self.on_quick_open_action(&crate::components::QuickOpen, window, cx)
            }
            DocumentToolbarAction::Find => {
                self.on_find_in_document_action(&crate::components::FindInDocument, window, cx)
            }
            DocumentToolbarAction::CommandPalette => {
                self.on_command_palette_action(&crate::components::CommandPalette, window, cx)
            }
        }
    }
}

impl Editor {
    pub(super) fn sync_accessibility_bridge(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let actions = self
            .accessibility_bridge
            .as_ref()
            .map(crate::accessibility::AccessibilityBridge::take_actions)
            .unwrap_or_default();
        for request in actions {
            if request.action != accesskit::Action::Click {
                continue;
            }
            match request.target_node {
                crate::accessibility::SAVE_ID => {
                    self.on_save_document(&crate::components::SaveDocument, window, cx)
                }
                crate::accessibility::FIND_ID => {
                    self.on_find_in_document_action(&crate::components::FindInDocument, window, cx)
                }
                crate::accessibility::GO_TO_LINE_ID => {
                    self.on_go_to_line_action(&crate::components::GoToLine, window, cx)
                }
                crate::accessibility::ERROR_ID => {
                    if let Some(large_file) = self.source_surface.disk_view_cloned() {
                        large_file.update(cx, |view, cx| view.activate_accessibility_error(cx));
                    }
                }
                _ => {}
            }
        }
        if let Some(bridge) = self.accessibility_bridge.as_mut() {
            bridge.update_focus(window.is_window_active());
        }
        let revision = self.current_accessibility_revision(cx);
        if self.accessibility_revision != Some(revision) {
            let snapshot = self.accessibility_snapshot(cx);
            if let Some(bridge) = self.accessibility_bridge.as_mut() {
                bridge.update(snapshot);
            }
            self.accessibility_revision = Some(revision);
        }
    }
}

#[path = "../render_parts/document_view.rs"]
mod document_view;

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(large_file) = self.source_surface.as_ref() {
            let dirty = large_file.read(cx).is_dirty();
            self.document_dirty = dirty;
            self.pending_window_edited = dirty;
        }
        self.sync_accessibility_bridge(window, cx);
        let source_bytes = self.source_document.len();
        if let Some(started) = self.first_render_started.take() {
            super::perf::emit(
                "editor_first_render",
                started,
                Some(source_bytes),
                Some(true),
                Some("GPUI render boundary; not draw or platform present"),
            );
        }
        if let Some(input_trace) = self.pending_input_trace.take() {
            input_trace.record_next_render(source_bytes);
        }
        self.install_close_guard(cx, window);
        self.install_menu_window_activation_observer(window, cx);
        self.sync_split_scroll_handles(cx);
        self.apply_pending_focus(window, cx);
        self.apply_pending_scroll_into_view(window, cx);
        self.last_selection_snapshot = self.capture_source_selection_snapshot(cx);
        self.refresh_find_if_stale(cx);
        self.sync_workspace_session_view_state(cx);
        self.sync_pending_save(window, cx);
        self.sync_pending_save_as(window, cx);
        self.sync_pending_open_link(window, cx);
        self.sync_window_edited_state(window);

        let content_area = self.render_document_content(window, cx);
        let theme = cx.global::<ThemeManager>().current_arc();
        let strings = cx.global::<I18nManager>().strings_arc();
        let d = &theme.dimensions;
        let status_bar_height =
            if !self.focus_mode && EditorSettings::status_bar_preferences(cx).enabled {
                d.status_bar_height
            } else {
                0.0
            };
        let editor = cx.entity().downgrade();
        let split_divider_editor = editor.clone();
        let has_menus = cx
            .get_menus()
            .map(|menus| !menus.is_empty())
            .unwrap_or(false);
        let titlebar_height = custom_titlebar_height(window, d);
        let menu_chrome = in_window_menu_chrome_layout(
            std::env::consts::OS,
            has_menus,
            self.focus_mode,
            titlebar_height,
            d,
        );
        let menu_bar_height = menu_chrome.content_height;

        // Repaint when the Cmd/Ctrl follow modifier toggles so a hovered link's
        // hand cursor updates without moving the pointer. `ModifiersChanged` is
        // dispatched along the focused element's path to the root, and this root
        // is an ancestor of every block, so one listener here covers a link in any
        // block while editing. Gated to the secondary modifier so Shift during
        // selection does not repaint.
        let follow_modifier_active = window.modifiers().secondary();

        let base = div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .relative()
            .bg(theme.colors.editor_background)
            .on_modifiers_changed(move |event, window, _| {
                if event.modifiers.secondary() != follow_modifier_active {
                    window.refresh();
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(Self::on_editor_surface_mouse_down),
            )
            .capture_action(cx.listener(Self::on_copy_capture))
            .capture_action(cx.listener(Self::on_copy_as_markdown_capture))
            .capture_action(cx.listener(Self::on_cut_capture))
            .capture_action(cx.listener(Self::on_delete_capture))
            .capture_action(cx.listener(Self::on_delete_back_capture))
            .capture_key_down(cx.listener(Self::on_editor_key_down_capture))
            .can_drop(|dragged, _window, _cx| dragged.is::<ExternalPaths>())
            .on_drop::<ExternalPaths>(cx.listener(Self::on_external_paths_drop))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_save_document))
            .on_action(cx.listener(Self::on_save_document_as))
            .on_action(cx.listener(Self::on_export_html))
            .on_action(cx.listener(Self::on_export_image))
            .on_action(cx.listener(Self::on_export_pdf))
            .on_action(cx.listener(Self::on_normalize_line_endings_lf))
            .on_action(cx.listener(Self::on_normalize_line_endings_crlf))
            .on_action(cx.listener(Self::on_normalize_line_endings_cr))
            .on_action(cx.listener(Self::on_quit_application))
            .on_action(cx.listener(Self::on_close_window))
            .on_action(cx.listener(Self::on_new_tab_action))
            .on_action(cx.listener(Self::on_close_tab_action))
            .on_action(cx.listener(Self::on_reopen_closed_tab_action))
            .on_action(cx.listener(Self::on_previous_tab_action))
            .on_action(cx.listener(Self::on_next_tab_action))
            .on_action(cx.listener(Self::on_toggle_view_mode_action))
            .on_action(cx.listener(Self::on_toggle_workspace_action))
            .on_action(cx.listener(Self::on_quick_open_action))
            .on_action(cx.listener(Self::on_command_palette_action))
            .on_action(cx.listener(Self::on_go_to_line_action))
            .on_action(cx.listener(Self::on_find_in_document_action))
            .on_action(cx.listener(Self::on_replace_in_document_action))
            .on_action(cx.listener(Self::on_find_next_action))
            .on_action(cx.listener(Self::on_find_previous_action))
            .on_action(cx.listener(Self::on_open_folder_action))
            .on_action(cx.listener(Self::on_toggle_focus_mode_action))
            .on_action(cx.listener(Self::on_toggle_typewriter_mode_action))
            .on_action(cx.listener(Self::on_page_up))
            .on_action(cx.listener(Self::on_page_down))
            .on_action(cx.listener(Self::on_jump_to_top))
            .on_action(cx.listener(Self::on_jump_to_bottom))
            .on_action(cx.listener(Self::on_dismiss_transient_ui))
            .on_action(cx.listener(Self::on_install_cli_tool))
            .on_action(cx.listener(Self::on_uninstall_cli_tool));
        // Fetch menus + collect labels once for both renderers; previously each
        // of render_in_window_menu_bar / render_in_window_menu_panel called
        // cx.get_menus() and walked menus.iter().map(|m| m.name.to_string())
        // independently — two redundant Vec<OwnedMenu> + two redundant
        // Vec<String>-of-N-allocations per frame.
        let menus = supports_in_window_menu()
            .then(|| {
                let snapshot = cx
                    .try_global::<crate::app_menu::AppMenuState>()
                    .map(|state| state.in_window_menus.clone())
                    .unwrap_or_default();
                let platform_menus = cx.get_menus().unwrap_or_default();
                // Windows 的原生菜单桥在窄标题栏可能只回传前几个菜单。
                // 快照来自 install_menus，是完整的应用命令模型，优先保证可达性。
                if snapshot.len() > platform_menus.len() {
                    snapshot
                } else {
                    platform_menus
                }
            })
            .filter(|menus| !menus.is_empty());
        let menu_labels: Vec<SharedString> = menus
            .as_ref()
            .map(|m| m.iter().map(|menu| menu.name.clone()).collect())
            .unwrap_or_default();
        let base = if let Some(titlebar) = render_custom_titlebar(
            "editor-titlebar",
            None,
            None,
            &theme,
            window,
            cx,
            Self::on_titlebar_close,
        ) {
            base.child(titlebar)
        } else {
            base
        };
        let base = if self.focus_mode {
            base
        } else if let Some(menu_bar) = self.render_in_window_menu_bar(
            &theme,
            cx,
            menus.as_deref(),
            &menu_labels,
            menu_chrome.bar_top,
            menu_chrome.bar_height,
            menu_chrome.origin_x,
            menu_chrome.integrated,
        ) {
            base.child(menu_bar)
        } else {
            base
        };
        let tab_strip_height = self.tab_strip_height();
        let viewport_width = f32::from(window.viewport_size().width);
        let compact_workspace = workspace_uses_overlay(viewport_width);
        let workspace_position = EditorSettings::workspace_sidebar_position(cx);
        let workspace_width =
            workspace_panel_width_for_viewport(viewport_width, self.workspace_panel_width());
        let workspace_panel = self.render_workspace_panel(
            &theme,
            &strings,
            workspace_width,
            !compact_workspace,
            workspace_position,
            cx,
        );
        let effective_workspace_width = if workspace_panel.is_some() && !compact_workspace {
            workspace_width
        } else {
            0.0
        };
        let (docked_workspace_panel, overlay_workspace_panel) = if compact_workspace {
            (None, workspace_panel)
        } else {
            (workspace_panel, None)
        };
        let (left_workspace_panel, right_workspace_panel) = match workspace_position {
            WorkspaceSidebarPosition::Left => (docked_workspace_panel, None),
            WorkspaceSidebarPosition::Right => (None, docked_workspace_panel),
        };
        let (tab_strip_left, tab_strip_right) =
            editor_tab_strip_insets(workspace_position, effective_workspace_width);
        let base = if let Some(tab_strip) = self.render_tab_strip(
            &theme,
            titlebar_height + menu_bar_height,
            tab_strip_left,
            tab_strip_right,
            cx,
        ) {
            base.child(tab_strip)
        } else {
            base
        };
        let main_content = div()
            .id("editor-main-content")
            .debug_selector(|| "editor-main-content".to_owned())
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .pt(px(titlebar_height + menu_bar_height))
            .flex()
            .min_w(px(0.0))
            .on_mouse_move(cx.listener(Self::on_editor_layout_resize_mouse_move))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(Self::on_editor_layout_resize_mouse_up),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(Self::on_editor_layout_resize_mouse_up),
            );
        let main_content = if let Some(workspace_panel) = left_workspace_panel {
            main_content.child(workspace_panel)
        } else {
            main_content
        };
        let resident_content = if self.view_mode == super::ViewMode::Split {
            let available_width = (f32::from(window.viewport_size().width)
                - effective_workspace_width
                - SPLIT_DIVIDER_HIT_WIDTH)
                .max(1.0);
            let ratio = clamped_split_pane_ratio(self.split_pane_ratio, available_width);
            let source_width = available_width * ratio;
            let preview_width = available_width - source_width;
            let editor_viewport_height = (f32::from(window.viewport_size().height)
                - titlebar_height
                - menu_bar_height
                - tab_strip_height
                - status_bar_height)
                .max(1.0);
            let preview = self
                .render_split_preview_pane(&theme, preview_width, editor_viewport_height, cx)
                .unwrap_or_else(|| div().flex_1().into_any_element());
            let divider_editor = split_divider_editor.clone();
            let divider_focused = self.split_divider_focus_handle.is_focused(window);
            let divider_active = self.split_resize_session.is_some() || divider_focused;
            let divider_focus_handle = self.split_divider_focus_handle.clone();
            let divider_key_editor = split_divider_editor.clone();
            div()
                .w_full()
                .h_full()
                .flex()
                .min_w(px(0.0))
                .child(
                    div()
                        .id("split-source-pane-shell")
                        .debug_selector(|| "split-source-pane-shell".to_owned())
                        .h_full()
                        .w(px(source_width))
                        .flex_none()
                        .min_w(px(0.0))
                        .child(content_area),
                )
                .child(
                    div()
                        .id("split-divider")
                        .debug_selector(|| "split-divider".to_owned())
                        .relative()
                        .h_full()
                        .w(px(SPLIT_DIVIDER_HIT_WIDTH))
                        .flex_none()
                        .tab_index(0)
                        .track_focus(&divider_focus_handle)
                        .cursor_col_resize()
                        .hover(|this| this.bg(theme.colors.text_link.opacity(0.08)))
                        .focus(|this| this.bg(theme.colors.text_link.opacity(0.08)))
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(px((SPLIT_DIVIDER_HIT_WIDTH - 1.0) * 0.5))
                                .w(px(1.0))
                                .bg(if divider_active {
                                    theme.colors.text_link.opacity(0.72)
                                } else {
                                    theme.colors.dialog_border
                                })
                                .debug_selector(|| "split-divider-line".to_owned()),
                        )
                        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                            divider_focus_handle.focus(window);
                            let _ = divider_editor.update(cx, |editor, cx| {
                                if event.click_count >= 2 {
                                    editor.split_pane_ratio = 0.5;
                                    editor.split_resize_session = None;
                                    editor.schedule_workspace_session_save(cx);
                                    cx.notify();
                                } else {
                                    editor.start_split_resize(
                                        event.position.x,
                                        available_width,
                                        ratio,
                                        cx,
                                    );
                                }
                            });
                            cx.stop_propagation();
                        })
                        .on_key_down(move |event, window, cx| {
                            let _ = divider_key_editor.update(cx, |editor, cx| {
                                editor.on_split_divider_key_down(
                                    event,
                                    available_width,
                                    window,
                                    cx,
                                );
                            });
                        }),
                )
                .child(preview)
                .into_any_element()
        } else {
            content_area.into_any_element()
        };
        let editor_content = self.source_surface.render_content(resident_content);
        let editor_content = div()
            .id("editor-content")
            .debug_selector(|| "editor-content".to_owned())
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .font(editor_text_font(cx))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(Self::on_editor_content_mouse_down),
            )
            .child(editor_content);
        // Tab 属于编辑器列而非整个窗口；工作区从导航下方直接开始，右侧内容则在 Tab 下方开始。
        let editor_pane = div()
            .id("editor-pane")
            .debug_selector(|| "editor-pane".to_owned())
            .h_full()
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .pt(px(tab_strip_height))
            .flex()
            .flex_col()
            .child(editor_content);
        let main_content = main_content.child(editor_pane);
        let main_content = if let Some(workspace_panel) = right_workspace_panel {
            main_content.child(workspace_panel)
        } else {
            main_content
        };
        let base = base.child(main_content);
        let base = if let Some(find_panel) = self.render_find_panel(
            &theme,
            &strings,
            titlebar_height + menu_bar_height + tab_strip_height,
            cx,
        ) {
            base.child(find_panel)
        } else {
            base
        };
        let base = if let Some(workspace_panel) = overlay_workspace_panel {
            let overlay = div()
                .id("compact-workspace-overlay")
                .debug_selector(|| "compact-workspace-overlay".to_owned())
                .absolute()
                .top(px(titlebar_height + menu_bar_height))
                .bottom(px(status_bar_height))
                .w(px(workspace_width))
                .shadow_lg()
                .child(workspace_panel);
            let overlay = match workspace_position {
                WorkspaceSidebarPosition::Left => overlay.left(px(0.0)),
                WorkspaceSidebarPosition::Right => overlay.right(px(0.0)),
            };
            base.child(overlay)
        } else {
            base
        };
        let base = if let Some(status_bar) = self.render_status_bar(&theme, &strings, window, cx) {
            base.child(status_bar)
        } else {
            base
        };
        let base = if self.focus_mode {
            base
        } else if let Some(menu_panel) = self.render_in_window_menu_panel(
            &theme,
            window,
            cx,
            menus.as_deref(),
            &menu_labels,
            menu_chrome.panel_top_offset,
            menu_chrome.origin_x,
            f32::from(window.viewport_size().height.max(px(1.0))),
        ) {
            base.child(menu_panel)
        } else {
            base
        };
        let base = if let Some(context_menu) = self.render_context_menu_overlay(&theme, window, cx)
        {
            base.child(context_menu)
        } else {
            base
        };
        let base = if let Some(table_dialog) = self.render_table_insert_dialog_overlay(&theme, cx) {
            base.child(table_dialog)
        } else {
            base
        };
        let base = if let Some(workspace_dialog) =
            self.render_workspace_operation_dialog_overlay(&theme, &strings, cx)
        {
            base.child(workspace_dialog)
        } else {
            base
        };
        let base = if let Some(quick_open) = self.render_quick_open_overlay(&theme, &strings, cx) {
            base.child(quick_open)
        } else {
            base
        };
        let base = if let Some(command_palette) =
            self.render_command_palette_overlay(&theme, &strings, cx)
        {
            base.child(command_palette)
        } else {
            base
        };
        let base = if let Some(tab_context_menu) =
            self.render_tab_context_menu_overlay(&theme, &strings, window, cx)
        {
            base.child(tab_context_menu)
        } else {
            base
        };
        let base = if self.export_in_progress {
            base.child(self.render_export_progress(&theme, status_bar_height, cx))
        } else {
            base
        };
        let base = if let Some(tab_close_dialog) =
            self.render_tab_close_dialog_overlay(&theme, &strings, cx)
        {
            base.child(tab_close_dialog)
        } else {
            base
        };
        if self.show_external_conflict_dialog {
            base.child(self.render_external_conflict_overlay(&theme, window, cx))
        } else if self.show_encoding_conversion_dialog {
            base.child(self.render_encoding_conversion_overlay(&theme, cx))
        } else if let Some(kind) = self.info_dialog {
            base.child(self.render_info_dialog_overlay(&theme, kind, cx))
        } else if self.show_drop_replace_dialog {
            base.child(self.render_drop_replace_overlay(&theme, cx))
        } else if self.show_unsaved_changes_dialog {
            base.child(self.render_unsaved_changes_overlay(&theme, cx))
        } else {
            base
        }
    }
}
