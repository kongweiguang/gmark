// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn finish_workspace_move(
        &mut self,
        plan: &super::workspace_file_ops::WorkspaceMovePlan,
        active_path: Option<&Path>,
        selection: &UndoSelectionSnapshot,
        view_mode: super::ViewMode,
        cx: &mut Context<Self>,
    ) {
        if let Some(active_path) = active_path {
            let next_path = super::workspace_file_ops::map_moved_path(
                active_path,
                &plan.source,
                &plan.destination,
            );
            if let Some(rewrite) = plan
                .rewrites
                .iter()
                .find(|rewrite| rewrite.before_path == active_path)
            {
                if let Ok(opened) = crate::document_io::decode_markdown_bytes(&rewrite.after) {
                    self.replace_document_from_markdown(opened.text, Some(next_path.clone()), cx);
                    self.source_encoding = opened.encoding;
                    self.set_view_mode(view_mode, cx);
                    self.apply_selection_snapshot_in_current_mode(selection, cx);
                }
            } else if next_path != active_path {
                self.document_kind = DocumentKind::from_path(&next_path);
                self.file_path = Some(next_path.clone());
                self.saved_file_fingerprint = crate::recovery::fingerprint_file(&next_path).ok();
                self.pending_window_title_refresh = true;
                self.restart_file_watcher(cx);
                self.checkpoint_recovery_journal();
                self.sync_workspace_after_document_path_change(cx);
            }
            if self.file_path.as_ref() == Some(&next_path) {
                crate::app_menu::record_recent_file_from_editor(&next_path, cx);
            }
        }
        self.workspace.operation_dialog = None;
        self.workspace.pinned_empty_directories = self
            .workspace
            .pinned_empty_directories
            .iter()
            .map(|path| {
                super::workspace_file_ops::map_moved_path(path, &plan.source, &plan.destination)
            })
            .collect();
        self.workspace.undo_file_operation = Some(WorkspaceUndoOperation::Move(plan.reversed()));
        self.workspace.operation_error = None;
        self.invalidate_workspace_file_tree();
        self.sync_workspace_after_document_path_change(cx);
    }

    pub(in crate::editor) fn render_workspace_panel(
        &mut self,
        theme: &Theme,
        strings: &I18nStrings,
        panel_width: f32,
        resizable: bool,
        position: WorkspaceSidebarPosition,
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
                    c.dialog_border
                } else {
                    hsla(0.0, 0.0, 0.0, 0.0)
                })
                .bg(if active {
                    c.chrome_hover
                } else {
                    hsla(0.0, 0.0, 0.0, 0.0)
                })
                .hover(|this| this.bg(c.chrome_hover))
                .focus(|this| this.border_color(c.text_link))
                .cursor_pointer()
                .text_color(if active {
                    c.text_default
                } else {
                    c.dialog_muted
                })
                // GPUI 的 SVG 不稳定继承父级 currentColor，chrome 图标必须显式着色。
                .child(
                    svg()
                        .path(icon)
                        .size(px(16.0))
                        .text_color(if active {
                            c.text_default
                        } else {
                            c.dialog_muted
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
                .bg(c.sidebar_background)
                .border_l(px(if position == WorkspaceSidebarPosition::Right {
                    d.dialog_border_width
                } else {
                    0.0
                }))
                .border_r(px(if position == WorkspaceSidebarPosition::Left {
                    d.dialog_border_width
                } else {
                    0.0
                }))
                .border_color(c.dialog_border)
                .child(
                    div()
                        .id("workspace-panel-header")
                        .debug_selector(|| "workspace-panel-header".to_owned())
                        .h(px(44.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(tab(
                            strings.workspace_tab_files.clone(),
                            FILES_TAB_ICON,
                            WorkspaceTab::Files,
                            self.workspace.active_tab == WorkspaceTab::Files,
                        ))
                        .child(tab(
                            strings.workspace_tab_outline.clone(),
                            OUTLINE_TAB_ICON,
                            WorkspaceTab::Outline,
                            self.workspace.active_tab == WorkspaceTab::Outline,
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
                        .hover(|this| this.bg(c.text_link.opacity(0.08)))
                        .focus(|this| this.bg(c.text_link.opacity(0.08)))
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
                                    c.text_link.opacity(0.72)
                                } else {
                                    c.dialog_border
                                }),
                        );
                    let handle = if position == WorkspaceSidebarPosition::Left {
                        handle.right(px(-WORKSPACE_RESIZE_HIT_WIDTH * 0.5))
                    } else {
                        handle.left(px(-WORKSPACE_RESIZE_HIT_WIDTH * 0.5))
                    };
                    handle
                        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                            focus_handle.focus(window);
                            let _ = resize_editor.update(cx, |editor, cx| {
                                editor.start_workspace_resize(
                                    event.position.x,
                                    panel_width,
                                    position,
                                    cx,
                                );
                            });
                            cx.stop_propagation();
                        })
                        .on_key_down(move |event, _window, cx| {
                            let _ = resize_key_editor.update(cx, |editor, cx| {
                                editor.on_workspace_resize_key_down(
                                    event,
                                    panel_width,
                                    position,
                                    cx,
                                );
                            });
                        })
                }))
                .into_any_element(),
        )
    }

    pub(in crate::editor) fn render_workspace_operation_dialog_overlay(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.workspace.operation_dialog.as_ref()?;
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let title = match dialog.kind {
            WorkspaceOperationKind::Rename => strings.workspace_rename_title.clone(),
            WorkspaceOperationKind::Move => strings.workspace_move_title.clone(),
            WorkspaceOperationKind::NewFile => strings.workspace_new_file_title.clone(),
            WorkspaceOperationKind::NewFolder => strings.workspace_new_folder_title.clone(),
        };
        let status = if dialog.running {
            Some((
                strings.workspace_operation_busy.clone(),
                REFRESH_ICON,
                "workspace-operation-status-progress-icon",
                c.text_link,
            ))
        } else if let Some(error) = dialog.error.as_ref() {
            Some((
                error.clone(),
                WARNING_ICON,
                "workspace-operation-status-error-icon",
                c.dialog_danger_button_bg,
            ))
        } else {
            dialog.plan.as_ref().map(|plan| match plan {
                WorkspacePendingPlan::Move(plan) => (
                    strings
                        .workspace_operation_affected_template
                        .replace("{count}", &plan.rewrites.len().to_string()),
                    CHECK_ICON,
                    "workspace-operation-status-ready-icon",
                    c.dialog_muted,
                ),
                WorkspacePendingPlan::Create(plan) => (
                    plan.path.display().to_string(),
                    CHECK_ICON,
                    "workspace-operation-status-ready-icon",
                    c.dialog_muted,
                ),
            })
        };
        let primary_label = if dialog.plan.is_some() {
            strings.workspace_apply_operation.clone()
        } else {
            strings.workspace_review_operation.clone()
        };
        let primary_handler = if dialog.plan.is_some() {
            Self::on_apply_workspace_operation
                as fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>)
        } else {
            Self::on_review_workspace_operation
                as fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>)
        };
        let enabled = !dialog.running;
        let primary = dialog_button(
            "confirm-workspace-operation",
            primary_label,
            if enabled {
                DialogButtonKind::Primary
            } else {
                DialogButtonKind::Secondary
            },
            theme,
        );
        let primary = if enabled {
            primary.on_click(cx.listener(primary_handler))
        } else {
            primary.opacity(0.62)
        };

        Some(
            modal_overlay("workspace-operation-dialog-overlay", theme)
                .child(
                    dialog_panel(
                        "workspace-operation-dialog",
                        d.dialog_width.min(520.0),
                        theme,
                    )
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation()
                    })
                    .child(
                        dialog_content("workspace-operation-dialog-content", theme)
                            .child(dialog_title_with_icon(
                                "workspace-operation-title",
                                title,
                                DialogTitleIcon::Files,
                                theme,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .text_size(px(t.dialog_body_size))
                                            .text_color(c.dialog_body)
                                            .child(strings.workspace_destination_label.clone()),
                                    )
                                    .child(
                                        div()
                                            .id("workspace-operation-destination-input")
                                            .debug_selector(|| {
                                                "workspace-operation-destination-input".to_owned()
                                            })
                                            .min_h(px(38.0))
                                            .w_full()
                                            .px(px(8.0))
                                            .flex()
                                            .items_center()
                                            .rounded(px(6.0))
                                            .border(px(d.dialog_border_width))
                                            .border_color(c.dialog_border)
                                            .child(dialog.input.clone()),
                                    ),
                            )
                            .children(status.map(|(message, icon, selector, color)| {
                                workspace_status_row(
                                    "workspace-operation-status",
                                    selector,
                                    icon,
                                    message,
                                    color,
                                    t.dialog_body_size,
                                )
                                .into_any_element()
                            })),
                    )
                    .child(
                        dialog_actions(theme)
                            .child(
                                dialog_button(
                                    "cancel-workspace-operation",
                                    strings.open_link_cancel.clone(),
                                    DialogButtonKind::Secondary,
                                    theme,
                                )
                                .on_click(cx.listener(Self::on_cancel_workspace_operation)),
                            )
                            .child(primary),
                    ),
                )
                .into_any_element(),
        )
    }

    pub(in crate::editor) fn render_quick_open_overlay(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.workspace.quick_open.as_ref()?;
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let editor = cx.entity().downgrade();
        let dismiss_editor = editor.clone();
        let close_editor = editor.clone();
        let close_tooltip: SharedString = strings.ui_close.clone().into();
        let query_empty = state.input.read(cx).display_text().trim().is_empty();
        let status = if state.running && state.results.is_empty() {
            Some(strings.quick_open_scanning.clone())
        } else if state.results.is_empty() {
            Some(if query_empty {
                strings.quick_open_prompt.clone()
            } else {
                strings.quick_open_no_results.clone()
            })
        } else {
            None
        };

        Some(
            div()
                .id("quick-open-overlay")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .occlude()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(82.0))
                .bg(c.dialog_backdrop)
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    let _ = dismiss_editor.update(cx, |editor, cx| {
                        editor.workspace.quick_open = None;
                        cx.notify();
                    });
                })
                .child(
                    div()
                        .id("quick-open-dialog")
                        .debug_selector(|| "quick-open-dialog".to_owned())
                        .w(px(560.0))
                        .max_w(relative(0.92))
                        .max_h(relative(0.74))
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .bg(c.dialog_surface)
                        .border(px(d.dialog_border_width))
                        .border_color(c.dialog_border)
                        .rounded(px(d.dialog_radius.min(8.0)))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .child(
                            div()
                                .h(px(38.0))
                                .px(px(14.0))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .truncate()
                                        .text_size(px(t.dialog_title_size))
                                        .font_weight(t.dialog_title_weight.to_font_weight())
                                        .text_color(c.dialog_title)
                                        .child(strings.quick_open_title.clone()),
                                )
                                .child(
                                    div()
                                        .id("quick-open-close")
                                        .debug_selector(|| "quick-open-close".to_owned())
                                        .size(px(28.0))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(5.0))
                                        .cursor_pointer()
                                        .hover(|this| this.bg(c.dialog_secondary_button_hover))
                                        .tooltip(move |_window, cx| {
                                            crate::ui::ui_tooltip(close_tooltip.clone(), cx)
                                        })
                                        .child(
                                            svg()
                                                .path(CLOSE_ICON)
                                                .size(px(15.0))
                                                .text_color(c.dialog_muted)
                                                .debug_selector(|| {
                                                    "quick-open-close-icon".to_owned()
                                                }),
                                        )
                                        .on_click(move |_event, _window, cx| {
                                            let _ = close_editor.update(cx, |editor, cx| {
                                                editor.workspace.quick_open = None;
                                                cx.notify();
                                            });
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .id("quick-open-input")
                                .debug_selector(|| "quick-open-input".to_owned())
                                .mx(px(12.0))
                                .mb(px(10.0))
                                .min_h(px(40.0))
                                .px(px(10.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .rounded(px(6.0))
                                .border(px(d.dialog_border_width))
                                .border_color(c.dialog_border)
                                .child(
                                    div()
                                        .id("quick-open-search-icon")
                                        .debug_selector(|| "quick-open-search-icon".to_owned())
                                        .size(px(16.0))
                                        .flex_shrink_0()
                                        .text_color(c.dialog_muted)
                                        .child(
                                            svg()
                                                .path(SEARCH_TAB_ICON)
                                                .size(px(16.0))
                                                .text_color(c.dialog_muted)
                                                .debug_selector(|| {
                                                    "quick-open-search-icon-svg".to_owned()
                                                }),
                                        ),
                                )
                                .child(div().flex_1().min_w(px(0.0)).child(state.input.clone())),
                        )
                        .child(
                            div()
                                .id("quick-open-results")
                                .debug_selector(|| "quick-open-results".to_owned())
                                .flex_1()
                                .min_h(px(52.0))
                                .overflow_y_scroll()
                                .px(px(8.0))
                                .pb(px(8.0))
                                .children(status.map(|message| {
                                    div()
                                        .px(px(10.0))
                                        .py(px(14.0))
                                        .text_size(px(t.dialog_body_size))
                                        .text_color(c.dialog_muted)
                                        .child(message)
                                }))
                                .children(state.results.iter().enumerate().map(
                                    |(index, result)| {
                                        let editor = editor.clone();
                                        let path = result.path.clone();
                                        div()
                                            .id(("quick-open-result", index))
                                            .debug_selector(move || {
                                                format!("quick-open-result-{index}")
                                            })
                                            .h(px(34.0))
                                            .w_full()
                                            .px(px(10.0))
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .overflow_hidden()
                                            .rounded(px(5.0))
                                            .bg(if index == state.selected {
                                                c.selection
                                            } else {
                                                hsla(0.0, 0.0, 0.0, 0.0)
                                            })
                                            .hover(|this| this.bg(c.dialog_secondary_button_hover))
                                            .cursor_pointer()
                                            .text_size(px(t.dialog_body_size))
                                            .text_color(c.text_default)
                                            .child(
                                                svg()
                                                    .path(MARKDOWN_ICON)
                                                    .size(px(16.0))
                                                    .flex_shrink_0()
                                                    .text_color(c.dialog_muted)
                                                    .debug_selector(move || {
                                                        format!("quick-open-result-icon-{index}")
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .min_w(px(0.0))
                                                    .overflow_hidden()
                                                    .truncate()
                                                    .child(middle_ellipsis(
                                                        &result.relative_path,
                                                        56,
                                                    )),
                                            )
                                            .on_click(move |_event, window, cx| {
                                                let path = path.clone();
                                                let _ = editor.update(cx, |editor, cx| {
                                                    editor.workspace.quick_open = None;
                                                    editor.open_workspace_file(path, window, cx);
                                                });
                                            })
                                    },
                                )),
                        ),
                )
                .into_any_element(),
        )
    }
}
