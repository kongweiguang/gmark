// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn render_context_menu_overlay(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.context_menu.as_ref()?;
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let s = cx.global::<I18nManager>().strings().clone();

        match menu {
            ContextMenuState::Insert {
                position,
                submenu_open,
                ..
            } => {
                let viewport = window.viewport_size();
                let panel_width = d.context_menu_panel_width;
                let panel_height = compact_menu_panel_height(1, 0, d);
                let panel_origin =
                    clamped_floating_panel_origin(*position, panel_width, panel_height, viewport);
                let panel_x = panel_origin.x;
                let panel_y = panel_origin.y;
                let submenu_width = d.context_menu_submenu_width;
                let submenu_height = compact_menu_panel_height(INSERT_COMMANDS.len(), 0, d);
                let submenu_origin = clamped_floating_panel_origin(
                    panel_origin,
                    submenu_width,
                    submenu_height,
                    viewport,
                );
                let submenu_x = floating_submenu_x(
                    panel_x,
                    panel_width,
                    submenu_width,
                    d.context_menu_submenu_gap,
                    viewport.width,
                );

                let submenu = submenu_open.then(|| {
                    let mut panel = div()
                        .id("editor-context-menu-submenu")
                        .debug_selector(|| "editor-context-menu-submenu".to_owned())
                        .absolute()
                        .left(submenu_x)
                        .top(submenu_origin.y)
                        .w(px(submenu_width))
                        .p(px(d.menu_panel_padding))
                        .flex()
                        .flex_col()
                        .gap(px(d.menu_panel_gap))
                        .occlude()
                        .bg(c.dialog_surface)
                        .border(px(d.dialog_border_width))
                        .border_color(c.dialog_border)
                        .rounded(px(d.menu_panel_radius))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation()
                        })
                        .on_hover(cx.listener(Self::on_context_menu_submenu_hover));
                    for (index, command) in INSERT_COMMANDS.into_iter().enumerate() {
                        let descriptor = command.descriptor();
                        let label = s
                            .slash_commands
                            .get(descriptor.localization_key)
                            .cloned()
                            .unwrap_or_else(|| descriptor.localization_key.to_owned());
                        panel = panel.child(
                            div()
                                .id(("editor-context-menu-insert-command", index))
                                .debug_selector(move || {
                                    format!("editor-context-menu-insert-{}", command.stable_id())
                                })
                                .h(px(d.menu_item_height))
                                .px(px(d.menu_item_padding_x))
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .rounded(px(d.menu_item_radius))
                                .bg(if self.context_menu_keyboard_submenu_item == Some(index) {
                                    c.dialog_secondary_button_hover
                                } else {
                                    c.dialog_surface
                                })
                                .hover(|this| this.bg(c.dialog_secondary_button_hover))
                                .active(|this| this.opacity(0.92))
                                .cursor_pointer()
                                .text_size(px(d.menu_text_size))
                                .font_weight(t.dialog_body_weight.to_font_weight())
                                .text_color(c.dialog_secondary_button_text)
                                .child(
                                    menu_icon_slot(Some(descriptor.icon_path), c.dialog_muted)
                                        .debug_selector(move || {
                                            format!(
                                                "editor-context-menu-insert-{}-icon",
                                                command.stable_id()
                                            )
                                        }),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .truncate()
                                        .child(label),
                                )
                                .on_click(cx.listener(move |editor, event, window, cx| {
                                    editor
                                        .on_context_menu_insert_command(command, event, window, cx)
                                })),
                        );
                    }
                    panel
                });

                let overlay = div()
                    .id("editor-context-menu-overlay")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(Self::on_dismiss_context_menu_overlay),
                    )
                    .child(
                        div()
                            .id("editor-context-menu-panel")
                            .debug_selector(|| "editor-context-menu-panel".to_owned())
                            .absolute()
                            .left(panel_x)
                            .top(panel_y)
                            .w(px(panel_width))
                            .p(px(d.menu_panel_padding))
                            .flex()
                            .flex_col()
                            .gap(px(d.menu_panel_gap))
                            .bg(c.dialog_surface)
                            .border(px(d.dialog_border_width))
                            .border_color(c.dialog_border)
                            .rounded(px(d.menu_panel_radius))
                            .shadow_lg()
                            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                                cx.stop_propagation()
                            })
                            .child(
                                div()
                                    .id("editor-context-menu-insert")
                                    .debug_selector(|| "editor-context-menu-insert".to_owned())
                                    .h(px(d.menu_item_height))
                                    .px(px(d.menu_item_padding_x))
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .rounded(px(d.menu_item_radius))
                                    .bg(
                                        if *submenu_open
                                            || self.context_menu_keyboard_item == Some(0)
                                        {
                                            c.dialog_secondary_button_hover
                                        } else {
                                            c.dialog_surface
                                        },
                                    )
                                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                                    .text_size(px(d.menu_text_size))
                                    .font_weight(t.dialog_body_weight.to_font_weight())
                                    .text_color(c.dialog_secondary_button_text)
                                    .child(
                                        menu_icon_slot(Some(PLUS_ICON), c.dialog_muted)
                                            .debug_selector(|| {
                                                "editor-context-menu-insert-icon".to_owned()
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .truncate()
                                            .child(s.context_menu_insert.clone()),
                                    )
                                    .child(
                                        svg()
                                            .path("icon/ui/chevron-right.svg")
                                            .size(px(14.0))
                                            .flex_shrink_0(),
                                    )
                                    .on_hover(cx.listener(Self::on_context_menu_insert_hover)),
                            ),
                    );

                Some(if let Some(submenu) = submenu {
                    overlay.child(submenu).into_any_element()
                } else {
                    overlay.into_any_element()
                })
            }
            ContextMenuState::Spelling {
                position,
                diagnostic,
                ..
            } => {
                let panel_width = d.context_menu_submenu_width.max(220.0);
                let viewport = window.viewport_size();
                let panel_max_height = (f32::from(viewport.height) - 16.0).max(80.0);
                let panel_height =
                    compact_menu_panel_height(diagnostic.replacements.len() + 1, 0, d)
                        .min(panel_max_height);
                let panel_origin =
                    clamped_floating_panel_origin(*position, panel_width, panel_height, viewport);
                let mut panel = div()
                    .id("editor-spelling-menu-panel")
                    .debug_selector(|| "editor-spelling-menu-panel".to_owned())
                    .absolute()
                    .left(panel_origin.x)
                    .top(panel_origin.y)
                    .w(px(panel_width))
                    .max_h(px(panel_max_height))
                    .p(px(d.menu_panel_padding))
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .track_scroll(&self.context_menu_scroll_handle)
                    .scrollbar_width(px(0.0))
                    .gap(px(d.menu_panel_gap))
                    .bg(c.dialog_surface)
                    .border(px(d.dialog_border_width))
                    .border_color(c.dialog_border)
                    .rounded(px(d.menu_panel_radius))
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation()
                    })
                    .child(
                        div()
                            .px(px(d.menu_item_padding_x))
                            .py(px(4.0))
                            .text_size(px((d.menu_text_size - 1.0).max(10.0)))
                            .text_color(c.dialog_muted)
                            .child(diagnostic.message.clone()),
                    );
                for (index, replacement) in diagnostic.replacements.iter().enumerate() {
                    panel = panel.child(
                        div()
                            .id(("editor-spelling-suggestion", index))
                            .debug_selector(move || format!("editor-spelling-suggestion-{index}"))
                            .h(px(d.menu_item_height))
                            .px(px(d.menu_item_padding_x))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .rounded(px(d.menu_item_radius))
                            .bg(if self.context_menu_keyboard_item == Some(index) {
                                c.dialog_secondary_button_hover
                            } else {
                                c.dialog_surface
                            })
                            .hover(|this| this.bg(c.dialog_secondary_button_hover))
                            .on_hover(cx.listener(Self::on_context_menu_pointer_hover))
                            .cursor_pointer()
                            .text_size(px(d.menu_text_size))
                            .text_color(c.dialog_secondary_button_text)
                            .child(menu_icon_slot(Some(CHECK_ICON), c.dialog_muted))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .truncate()
                                    .child(replacement.clone()),
                            )
                            .on_click(cx.listener(move |this, event, window, cx| {
                                this.apply_spelling_suggestion(index, event, window, cx)
                            })),
                    );
                }
                Some(
                    div()
                        .id("editor-spelling-menu-overlay")
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(Self::on_dismiss_context_menu_overlay),
                        )
                        .child(panel)
                        .into_any_element(),
                )
            }
            ContextMenuState::TableAxis {
                position,
                selection,
            } => {
                let (row_count, separator_count) = match selection.kind {
                    TableAxisKind::Column => (6, 2),
                    TableAxisKind::Row if selection.index == 0 => (4, 2),
                    TableAxisKind::Row => (3, 1),
                };
                let panel_width = d.context_menu_axis_panel_width;
                let panel_origin = clamped_floating_panel_origin(
                    *position,
                    panel_width,
                    compact_menu_panel_height(row_count, separator_count, d),
                    window.viewport_size(),
                );
                let Some(table_block) = self.table_block_by_id(selection.table_block_id, cx) else {
                    return None;
                };
                let table = table_block.read(cx).record.table.clone()?;
                let items = match selection.kind {
                    TableAxisKind::Column => vec![
                        Self::render_axis_menu_item(
                            theme,
                            "table-axis-align-column-left",
                            s.table_axis_align_column_left.clone(),
                            ALIGN_LEFT_ICON,
                            true,
                            self.context_menu_keyboard_item == Some(0),
                            false,
                            Self::on_align_table_column_left,
                            cx,
                        ),
                        Self::render_axis_menu_item(
                            theme,
                            "table-axis-align-column-center",
                            s.table_axis_align_column_center.clone(),
                            ALIGN_CENTER_ICON,
                            true,
                            self.context_menu_keyboard_item == Some(1),
                            false,
                            Self::on_align_table_column_center,
                            cx,
                        ),
                        Self::render_axis_menu_item(
                            theme,
                            "table-axis-align-column-right",
                            s.table_axis_align_column_right.clone(),
                            ALIGN_RIGHT_ICON,
                            true,
                            self.context_menu_keyboard_item == Some(2),
                            false,
                            Self::on_align_table_column_right,
                            cx,
                        ),
                        div()
                            .mx(px(d.menu_separator_margin_x))
                            .my(px(d.menu_separator_margin_y))
                            .h(px(d.menu_separator_height))
                            .bg(c.dialog_border)
                            .into_any_element(),
                        Self::render_axis_menu_item(
                            theme,
                            "table-axis-move-column-left",
                            s.table_axis_move_column_left.clone(),
                            ARROW_LEFT_ICON,
                            selection.index > 0,
                            self.context_menu_keyboard_item == Some(3),
                            false,
                            Self::on_move_table_column_left,
                            cx,
                        ),
                        Self::render_axis_menu_item(
                            theme,
                            "table-axis-move-column-right",
                            s.table_axis_move_column_right.clone(),
                            ARROW_RIGHT_ICON,
                            selection.index + 1 < table.column_count(),
                            self.context_menu_keyboard_item == Some(4),
                            false,
                            Self::on_move_table_column_right,
                            cx,
                        ),
                        div()
                            .mx(px(d.menu_separator_margin_x))
                            .my(px(d.menu_separator_margin_y))
                            .h(px(d.menu_separator_height))
                            .bg(c.dialog_border)
                            .into_any_element(),
                        Self::render_axis_menu_item(
                            theme,
                            "table-axis-delete-column",
                            s.table_axis_delete_column.clone(),
                            TRASH_ICON,
                            // Always enabled: deleting the last column removes the
                            // whole table.
                            true,
                            self.context_menu_keyboard_item == Some(5),
                            true,
                            Self::on_delete_table_column,
                            cx,
                        ),
                    ],
                    TableAxisKind::Row => {
                        let mut items: Vec<AnyElement> = Vec::new();
                        // The header row (visual index 0) shares the normal row
                        // menu, with its Header Row styling toggle added on top.
                        if selection.index == 0 {
                            let headers_shown =
                                crate::config::EditorSettings::show_table_headers(cx);
                            items.push(
                                div()
                                    .id("table-header-toggle")
                                    .h(px(d.menu_item_height))
                                    .px(px(d.menu_item_padding_x))
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .rounded(px(d.menu_item_radius))
                                    .bg(if self.context_menu_keyboard_item == Some(0) {
                                        c.dialog_secondary_button_hover
                                    } else {
                                        c.dialog_surface
                                    })
                                    .text_size(px(d.menu_text_size))
                                    .font_weight(t.dialog_body_weight.to_font_weight())
                                    .text_color(c.dialog_secondary_button_text)
                                    .child(menu_icon_slot(Some(TABLE_ICON), c.dialog_muted))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .truncate()
                                            .child(s.table_header_row.clone()),
                                    )
                                    .child(
                                        div()
                                            .size(px(18.0))
                                            .flex_shrink_0()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .children(
                                                headers_shown
                                                    .then(|| svg().path(CHECK_ICON).size(px(14.0))),
                                            ),
                                    )
                                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                                    .on_hover(cx.listener(Self::on_context_menu_pointer_hover))
                                    .cursor_pointer()
                                    .on_click(cx.listener(Self::on_toggle_table_headers))
                                    .into_any_element(),
                            );
                            items.push(
                                div()
                                    .mx(px(d.menu_separator_margin_x))
                                    .my(px(d.menu_separator_margin_y))
                                    .h(px(d.menu_separator_height))
                                    .bg(c.dialog_border)
                                    .into_any_element(),
                            );
                        }
                        items.push(Self::render_axis_menu_item(
                            theme,
                            "table-axis-move-row-up",
                            s.table_axis_move_row_up.clone(),
                            ARROW_UP_ICON,
                            selection.index > 0,
                            self.context_menu_keyboard_item
                                == Some(if selection.index == 0 { 1 } else { 0 }),
                            false,
                            Self::on_move_table_row_up,
                            cx,
                        ));
                        items.push(Self::render_axis_menu_item(
                            theme,
                            "table-axis-move-row-down",
                            s.table_axis_move_row_down.clone(),
                            ARROW_DOWN_ICON,
                            selection.index < table.rows.len(),
                            self.context_menu_keyboard_item
                                == Some(if selection.index == 0 { 2 } else { 1 }),
                            false,
                            Self::on_move_table_row_down,
                            cx,
                        ));
                        items.push(
                            div()
                                .mx(px(d.menu_separator_margin_x))
                                .my(px(d.menu_separator_margin_y))
                                .h(px(d.menu_separator_height))
                                .bg(c.dialog_border)
                                .into_any_element(),
                        );
                        // Always enabled: deleting the header promotes the first
                        // body row, and deleting the last remaining row removes
                        // the whole table.
                        items.push(Self::render_axis_menu_item(
                            theme,
                            "table-axis-delete-row",
                            s.table_axis_delete_row.clone(),
                            TRASH_ICON,
                            true,
                            self.context_menu_keyboard_item
                                == Some(if selection.index == 0 { 3 } else { 2 }),
                            true,
                            Self::on_delete_table_row,
                            cx,
                        ));
                        items
                    }
                };

                Some(
                    div()
                        .id("table-axis-context-menu-overlay")
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(Self::on_dismiss_context_menu_overlay),
                        )
                        .child(
                            div()
                                .id("table-axis-context-menu-panel")
                                .debug_selector(|| "table-axis-context-menu-panel".to_owned())
                                .absolute()
                                .left(panel_origin.x)
                                .top(panel_origin.y)
                                .w(px(panel_width))
                                .p(px(d.menu_panel_padding))
                                .flex()
                                .flex_col()
                                .gap(px(d.menu_panel_gap))
                                .bg(c.dialog_surface)
                                .border(px(d.dialog_border_width))
                                .border_color(c.dialog_border)
                                .rounded(px(d.menu_panel_radius))
                                .shadow_lg()
                                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                                    cx.stop_propagation()
                                })
                                .children(items),
                        )
                        .into_any_element(),
                )
            }
            ContextMenuState::Workspace { position, .. } => {
                let panel_width = d.context_menu_submenu_width.max(220.0);
                let panel_origin = clamped_floating_panel_origin(
                    *position,
                    panel_width,
                    compact_menu_panel_height(5, 2, d),
                    window.viewport_size(),
                );
                let item = |id: &'static str,
                            keyboard_index: usize,
                            label: String,
                            icon: &'static str,
                            enabled: bool,
                            handler: fn(
                    &mut Editor,
                    &ClickEvent,
                    &mut Window,
                    &mut Context<Editor>,
                )| {
                    let row = div()
                        .id(id)
                        .debug_selector(move || id.to_owned())
                        .h(px(d.menu_item_height))
                        .px(px(d.menu_item_padding_x))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .rounded(px(d.menu_item_radius))
                        .bg(if self.context_menu_keyboard_item == Some(keyboard_index) {
                            c.dialog_secondary_button_hover
                        } else {
                            c.dialog_surface
                        })
                        .text_size(px(d.menu_text_size))
                        .text_color(if enabled {
                            c.dialog_secondary_button_text
                        } else {
                            c.dialog_muted
                        })
                        .child(
                            menu_icon_slot(Some(icon), c.dialog_muted)
                                .debug_selector(move || format!("{id}-icon")),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .overflow_hidden()
                                .truncate()
                                .child(label),
                        )
                        .on_hover(cx.listener(Self::on_context_menu_pointer_hover));
                    if enabled {
                        row.hover(|this| this.bg(c.dialog_secondary_button_hover))
                            .cursor_pointer()
                            .on_click(cx.listener(handler))
                            .into_any_element()
                    } else {
                        row.into_any_element()
                    }
                };
                let panel = div()
                    .id("workspace-context-menu-panel")
                    .debug_selector(|| "workspace-context-menu-panel".to_owned())
                    .absolute()
                    .left(panel_origin.x)
                    .top(panel_origin.y)
                    .w(px(panel_width))
                    .p(px(d.menu_panel_padding))
                    .flex()
                    .flex_col()
                    .gap(px(d.menu_panel_gap))
                    .bg(c.dialog_surface)
                    .border(px(d.dialog_border_width))
                    .border_color(c.dialog_border)
                    .rounded(px(d.menu_panel_radius))
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation()
                    })
                    .child(item(
                        "workspace-context-new-file",
                        0,
                        s.workspace_new_file.clone(),
                        PLUS_ICON,
                        true,
                        Self::on_workspace_new_file_menu,
                    ))
                    .child(item(
                        "workspace-context-new-folder",
                        1,
                        s.workspace_new_folder.clone(),
                        "icon/workspace/folder.svg",
                        true,
                        Self::on_workspace_new_folder_menu,
                    ))
                    .child(
                        div()
                            .mx(px(d.menu_separator_margin_x))
                            .my(px(d.menu_separator_margin_y))
                            .h(px(d.menu_separator_height))
                            .bg(c.dialog_border),
                    )
                    .child(item(
                        "workspace-context-rename",
                        2,
                        s.workspace_rename.clone(),
                        "icon/ui/type.svg",
                        !self.workspace_context_target_is_root(),
                        Self::on_workspace_rename_menu,
                    ))
                    .child(item(
                        "workspace-context-move",
                        3,
                        s.workspace_move.clone(),
                        ARROW_RIGHT_ICON,
                        !self.workspace_context_target_is_root(),
                        Self::on_workspace_move_menu,
                    ))
                    .child(
                        div()
                            .mx(px(d.menu_separator_margin_x))
                            .my(px(d.menu_separator_margin_y))
                            .h(px(d.menu_separator_height))
                            .bg(c.dialog_border),
                    )
                    .child(item(
                        "workspace-context-undo",
                        4,
                        s.workspace_undo_file_operation.clone(),
                        "icon/ui/undo.svg",
                        self.workspace_can_undo_file_operation(),
                        Self::on_workspace_undo_file_operation,
                    ));
                Some(
                    div()
                        .id("workspace-context-menu-overlay")
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(Self::on_dismiss_context_menu_overlay),
                        )
                        .child(panel)
                        .into_any_element(),
                )
            }
        }
    }
}
