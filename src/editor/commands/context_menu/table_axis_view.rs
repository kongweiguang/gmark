// @author kongweiguang

use super::*;
use crate::theme::workbench::SurfaceKind;
use crate::ui::visual_preferences::VisualPreferencesManager;

impl Editor {
    pub(super) fn render_table_axis_context_menu(
        &self,
        position: &Point<Pixels>,
        selection: &TableAxisSelection,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let c = &theme.colors;
        let visual_preferences = cx
            .try_global::<VisualPreferencesManager>()
            .map(VisualPreferencesManager::current)
            .unwrap_or_default();
        let palette = &c.workbench;
        let material = palette.material(SurfaceKind::Glass, visual_preferences);
        let d = &theme.dimensions;
        let t = &theme.typography;
        let s = cx.global::<I18nManager>().strings().clone();
        let (row_count, separator_count) = match selection.kind {
            TableAxisKind::Column => (10, 2),
            TableAxisKind::Row if selection.index == 0 => (8, 2),
            TableAxisKind::Row => (7, 1),
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
                    "table-axis-insert-column-before",
                    s.slash_commands
                        .get("insert_column_before")
                        .cloned()
                        .unwrap_or_else(|| "Insert Column Before".to_owned()),
                    PLUS_ICON,
                    true,
                    self.context_menu_keyboard_item == Some(0),
                    false,
                    Self::on_insert_table_column_before,
                    cx,
                ),
                Self::render_axis_menu_item(
                    theme,
                    "table-axis-insert-column-after",
                    s.slash_commands
                        .get("insert_column_after")
                        .cloned()
                        .unwrap_or_else(|| "Insert Column After".to_owned()),
                    PLUS_ICON,
                    true,
                    self.context_menu_keyboard_item == Some(1),
                    false,
                    Self::on_insert_table_column_after,
                    cx,
                ),
                Self::render_axis_menu_item(
                    theme,
                    "table-axis-duplicate-column",
                    s.slash_commands
                        .get("duplicate_column")
                        .cloned()
                        .unwrap_or_else(|| "Duplicate Column".to_owned()),
                    COPY_ICON,
                    true,
                    self.context_menu_keyboard_item == Some(2),
                    false,
                    Self::on_duplicate_table_column,
                    cx,
                ),
                Self::render_axis_menu_item(
                    theme,
                    "table-axis-align-column-left",
                    s.table_axis_align_column_left.clone(),
                    ALIGN_LEFT_ICON,
                    true,
                    self.context_menu_keyboard_item == Some(3),
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
                    self.context_menu_keyboard_item == Some(4),
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
                    self.context_menu_keyboard_item == Some(5),
                    false,
                    Self::on_align_table_column_right,
                    cx,
                ),
                div()
                    .mx(px(d.menu_separator_margin_x))
                    .my(px(d.menu_separator_margin_y))
                    .h(px(d.menu_separator_height))
                    .bg(material.border)
                    .into_any_element(),
                Self::render_axis_menu_item(
                    theme,
                    "table-axis-move-column-left",
                    s.table_axis_move_column_left.clone(),
                    ARROW_LEFT_ICON,
                    selection.index > 0,
                    self.context_menu_keyboard_item == Some(6),
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
                    self.context_menu_keyboard_item == Some(7),
                    false,
                    Self::on_move_table_column_right,
                    cx,
                ),
                div()
                    .mx(px(d.menu_separator_margin_x))
                    .my(px(d.menu_separator_margin_y))
                    .h(px(d.menu_separator_height))
                    .bg(material.border)
                    .into_any_element(),
                Self::render_axis_menu_item(
                    theme,
                    "table-axis-delete-column",
                    s.table_axis_delete_column.clone(),
                    TRASH_ICON,
                    table.column_count() > 1,
                    self.context_menu_keyboard_item == Some(8),
                    selection.index != 0 || !table.rows.is_empty(),
                    Self::on_delete_table_column,
                    cx,
                ),
                Self::render_axis_menu_item(
                    theme,
                    "table-axis-delete-table",
                    s.slash_commands
                        .get("delete_table")
                        .cloned()
                        .unwrap_or_else(|| "Delete Table".to_owned()),
                    TRASH_ICON,
                    true,
                    self.context_menu_keyboard_item == Some(9),
                    true,
                    Self::on_delete_selected_table,
                    cx,
                ),
            ],
            TableAxisKind::Row => {
                let mut items: Vec<AnyElement> = Vec::new();
                items.push(Self::render_axis_menu_item(
                    theme,
                    "table-axis-insert-row-before",
                    s.slash_commands
                        .get("insert_row_before")
                        .cloned()
                        .unwrap_or_else(|| "Insert Row Before".to_owned()),
                    PLUS_ICON,
                    true,
                    self.context_menu_keyboard_item == Some(0),
                    false,
                    Self::on_insert_table_row_before,
                    cx,
                ));
                items.push(Self::render_axis_menu_item(
                    theme,
                    "table-axis-insert-row-after",
                    s.slash_commands
                        .get("insert_row_after")
                        .cloned()
                        .unwrap_or_else(|| "Insert Row After".to_owned()),
                    PLUS_ICON,
                    true,
                    self.context_menu_keyboard_item == Some(1),
                    false,
                    Self::on_insert_table_row_after,
                    cx,
                ));
                items.push(Self::render_axis_menu_item(
                    theme,
                    "table-axis-duplicate-row",
                    s.slash_commands
                        .get("duplicate_row")
                        .cloned()
                        .unwrap_or_else(|| "Duplicate Row".to_owned()),
                    COPY_ICON,
                    true,
                    self.context_menu_keyboard_item == Some(2),
                    false,
                    Self::on_duplicate_table_row,
                    cx,
                ));
                // The header row (visual index 0) shares the normal row menu,
                // with its Header Row styling toggle added on top.
                if selection.index == 0 {
                    let headers_shown = crate::config::EditorSettings::show_table_headers(cx);
                    items.push(
                        div()
                            .id("table-header-toggle")
                            .h(px(d.menu_item_height))
                            .px(px(d.menu_item_padding_x))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .rounded(px(d.menu_item_radius))
                            .bg(if self.context_menu_keyboard_item == Some(3) {
                                palette.control_hover
                            } else {
                                material.background
                            })
                            .text_size(px(d.menu_text_size))
                            .font_weight(t.dialog_body_weight.to_font_weight())
                            .text_color(palette.text_primary)
                            .child(menu_icon_slot(Some(TABLE_ICON), palette.icon))
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
                            .hover(|this| this.bg(palette.control_hover))
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
                            .bg(material.border)
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
                        == Some(if selection.index == 0 { 4 } else { 3 }),
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
                        == Some(if selection.index == 0 { 5 } else { 4 }),
                    false,
                    Self::on_move_table_row_down,
                    cx,
                ));
                items.push(
                    div()
                        .mx(px(d.menu_separator_margin_x))
                        .my(px(d.menu_separator_margin_y))
                        .h(px(d.menu_separator_height))
                        .bg(material.border)
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
                        == Some(if selection.index == 0 { 6 } else { 5 }),
                    true,
                    Self::on_delete_table_row,
                    cx,
                ));
                items.push(Self::render_axis_menu_item(
                    theme,
                    "table-axis-delete-table",
                    s.slash_commands
                        .get("delete_table")
                        .cloned()
                        .unwrap_or_else(|| "Delete Table".to_owned()),
                    TRASH_ICON,
                    true,
                    self.context_menu_keyboard_item
                        == Some(if selection.index == 0 { 7 } else { 6 }),
                    true,
                    Self::on_delete_selected_table,
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
                        .max_h(relative(0.82))
                        .overflow_y_scroll()
                        .bg(material.background)
                        .border(px(d.dialog_border_width))
                        .border_color(material.border)
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
}
