// @author kongweiguang

use super::*;

impl Editor {
    pub(super) fn sync_window_edited_state(&mut self, window: &mut Window) {
        if self.pending_window_edited {
            self.pending_window_edited = false;
            window.set_window_edited(true);
        }
    }

    pub(super) fn sync_scroll_viewport(
        &mut self,
        viewport_size: Size<Pixels>,
        cx: &mut Context<Self>,
    ) {
        match self.last_scroll_viewport_size {
            Some(previous) if Self::viewport_size_changed(previous, viewport_size) => {
                self.last_scroll_viewport_size = Some(viewport_size);
                self.request_active_block_scroll_into_view(cx);
            }
            Some(_) => {}
            None => {
                self.last_scroll_viewport_size = Some(viewport_size);
            }
        }
    }

    pub(super) fn sync_window_title(&mut self, window: &mut Window, strings: &I18nStrings) {
        if self.pending_window_title_refresh {
            self.pending_window_title_refresh = false;
            let title = Self::window_title(self.file_path.as_deref(), self.document_dirty, strings);
            window.set_window_title(&title);
        }
    }

    /// Renders the in-window fallback menu bar backed by the app menus
    /// registered through `App::set_menus`. `menus` and `menu_labels` are
    /// fetched and computed once at the caller and shared with
    /// [`Self::render_in_window_menu_panel`].
    pub(super) fn render_in_window_menu_bar(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
        menus: Option<&[gpui::OwnedMenu]>,
        menu_labels: &[SharedString],
        top_offset: f32,
        height: f32,
        origin_x: f32,
        integrated: bool,
    ) -> Option<AnyElement> {
        let menus = menus?;
        if menus.is_empty() {
            return None;
        }

        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let editor = cx.entity().downgrade();
        let button_widths = menu_labels
            .iter()
            .enumerate()
            .map(|(index, label)| top_level_menu_button_width(index, label, d))
            .collect::<Vec<_>>();
        // 应用图标独立控制一级导航；下拉面板开关不再影响导航展开状态。
        let visible_button_count =
            visible_menu_button_count(self.menu_bar_expanded, menu_labels.len());
        let menu_width = d.menu_bar_padding_x * 2.0
            + button_widths.iter().take(visible_button_count).sum::<f32>()
            + d.menu_bar_gap * visible_button_count.saturating_sub(1) as f32;

        let bar = div()
            .id("app-menu-bar")
            .absolute()
            .top(px(top_offset))
            .left(px(origin_x))
            .h(px(height))
            .occlude()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap(px(d.menu_bar_gap))
            .px(px(d.menu_bar_padding_x))
            .py(px(d.menu_bar_padding_y))
            .bg(if integrated {
                hsla(0.0, 0.0, 0.0, 0.0)
            } else {
                c.chrome_background
            })
            .border_b(px(if integrated {
                0.0
            } else {
                theme.dimensions.dialog_border_width
            }))
            .border_color(c.dialog_border)
            .on_hover(cx.listener(Self::on_menu_bar_hover));
        let bar = if integrated {
            bar.w(px(menu_width))
        } else {
            bar.right_0()
        };

        Some(
            bar.children(
                menu_labels
                    .iter()
                    .enumerate()
                    .take(visible_button_count)
                    .map(|(index, label)| {
                        let label = label.clone();
                        let is_open = self.menu_bar_open == Some(index);
                        let button_editor = editor.clone();
                        let click_editor = editor.clone();
                        let button_width = button_widths[index];
                        let button_height = if index == 0 {
                            d.status_bar_height
                        } else {
                            d.menu_bar_button_height
                        };
                        // 图标按钮不能沿用文字菜单的 8px 横向内边距，否则 20px SVG 会在 24px 槽内被压缩到 8px。
                        let button_padding_x = if index == 0 {
                            0.0
                        } else {
                            d.menu_bar_button_padding_x
                        };

                        let button = div()
                            .id(("app-menu-button", index))
                            .debug_selector(move || {
                                if index == 0 {
                                    "app-menu-launcher".to_owned()
                                } else {
                                    format!("app-menu-button-{index}")
                                }
                            })
                            .h(px(button_height))
                            .w(px(button_width))
                            .px(px(button_padding_x))
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .justify_center()
                            .rounded(px(d.menu_bar_button_radius))
                            .bg(if is_open {
                                c.chrome_hover
                            } else {
                                c.chrome_background
                            })
                            .hover(|this| this.bg(c.chrome_hover))
                            .active(|this| this.opacity(0.92))
                            .cursor_pointer()
                            .text_size(px(d.menu_text_size))
                            .font_weight(t.dialog_button_weight.to_font_weight())
                            .text_color(c.dialog_secondary_button_text)
                            .whitespace_nowrap();

                        if index == 0 {
                            button
                                .child(
                                    // GPUI 的 `svg()` 会把所有不透明像素压成单色蒙版；品牌图标
                                    // 必须走 `img()`，才能保留深色底与白色字形的 RGBA 层次。
                                    img(MENU_LAUNCHER_ICON).size(px(MENU_LAUNCHER_ICON_SIZE)),
                                )
                                .on_hover(move |hovered, _window, cx| {
                                    if *hovered {
                                        let _ = button_editor.update(cx, |editor, cx| {
                                            if editor.menu_bar_open.is_some() {
                                                editor.clear_menu_keyboard_cursor(cx);
                                                editor.open_menu_bar(0, cx);
                                            }
                                        });
                                    }
                                })
                                .on_click(move |event, _window, cx| {
                                    if event.standard_click() {
                                        let _ = click_editor.update(cx, |editor, cx| {
                                            editor.clear_menu_keyboard_cursor(cx);
                                            editor.toggle_menu_bar_expanded(cx);
                                        });
                                    }
                                })
                        } else {
                            button.child(label).on_hover(move |hovered, _window, cx| {
                                if *hovered {
                                    let _ = button_editor.update(cx, |editor, cx| {
                                        editor.clear_menu_keyboard_cursor(cx);
                                        editor.open_menu_bar(index, cx);
                                    });
                                }
                            })
                        }
                    }),
            )
            .into_any_element(),
        )
    }

    pub(super) fn render_in_window_menu_item(
        &self,
        item: OwnedMenuItem,
        item_index: usize,
        theme: &Theme,
        editor: WeakEntity<Self>,
        window: &Window,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;

        match item {
            OwnedMenuItem::Separator => div()
                .id(("app-menu-separator", item_index))
                .flex_shrink_0()
                .mx(px(d.menu_separator_margin_x))
                .my(px(d.menu_separator_margin_y))
                .h(px(d.menu_separator_height))
                .bg(c.dialog_border)
                .into_any_element(),
            OwnedMenuItem::Action { name, action, .. } => {
                let is_disabled = action.as_ref().as_any().is::<NoRecentFiles>();
                let is_keyboard_focused = self.menu_keyboard_item == Some(item_index);
                let shortcut = menu_shortcut_text(window, action.as_ref());
                let click_editor = editor.clone();
                let hover_editor = editor.clone();
                let base = div()
                    .id(("app-menu-item", item_index))
                    .debug_selector(move || format!("app-menu-item-{item_index}"))
                    .w_full()
                    .h(px(d.menu_item_height))
                    .flex_shrink_0()
                    .px(px(d.menu_item_padding_x))
                    .flex()
                    .items_center()
                    .rounded(px(d.menu_item_radius))
                    .bg(if is_keyboard_focused {
                        c.dialog_secondary_button_hover
                    } else {
                        c.dialog_surface
                    })
                    .text_size(px(d.menu_text_size))
                    .font_weight(t.dialog_body_weight.to_font_weight())
                    .text_color(if is_disabled {
                        c.dialog_muted
                    } else {
                        c.dialog_secondary_button_text
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .truncate()
                            .child(name),
                    )
                    .children(shortcut.map(|text| {
                        menu_shortcut_slot(text, theme)
                            .debug_selector(move || format!("app-menu-shortcut-{item_index}"))
                    }))
                    .on_hover(move |hovered, _window, cx| {
                        if *hovered {
                            let _ = hover_editor.update(cx, |editor, cx| {
                                editor.clear_menu_keyboard_cursor(cx);
                                editor.close_menu_submenu(cx);
                            });
                        }
                    });

                if is_disabled {
                    base.into_any_element()
                } else {
                    base.hover(|this| this.bg(c.dialog_secondary_button_hover))
                        .active(|this| this.opacity(0.92))
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            let _ = click_editor.update(cx, |editor, cx| editor.close_menu_bar(cx));
                            dispatch_menu_action_for_editor(
                                action.as_ref(),
                                &click_editor,
                                window,
                                cx,
                            );
                        })
                        .into_any_element()
                }
            }
            OwnedMenuItem::Submenu(submenu) => {
                let is_open = self.menu_submenu_open == Some(item_index);
                let is_keyboard_focused = self.menu_keyboard_item == Some(item_index);
                let hover_editor = editor.clone();
                div()
                    .id(("app-menu-submenu", item_index))
                    .w_full()
                    .h(px(d.menu_item_height))
                    .flex_shrink_0()
                    .px(px(d.menu_item_padding_x))
                    .flex()
                    .items_center()
                    .rounded(px(d.menu_item_radius))
                    .bg(if is_open || is_keyboard_focused {
                        c.dialog_secondary_button_hover
                    } else {
                        c.dialog_surface
                    })
                    .hover(|this| this.bg(c.dialog_secondary_button_hover))
                    .cursor_pointer()
                    .text_size(px(d.menu_text_size))
                    .font_weight(t.dialog_body_weight.to_font_weight())
                    .text_color(c.dialog_secondary_button_text)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .truncate()
                            .child(submenu.name.to_string()),
                    )
                    .child(svg().path(CHEVRON_RIGHT_ICON).size(px(14.0)))
                    .on_hover(move |hovered, _window, cx| {
                        if *hovered {
                            let _ = hover_editor.update(cx, |editor, cx| {
                                editor.clear_menu_keyboard_cursor(cx);
                                editor.open_menu_submenu(item_index, cx);
                            });
                        }
                    })
                    .into_any_element()
            }
            OwnedMenuItem::SystemMenu(os_menu) => div()
                .id(("app-menu-system", item_index))
                .w_full()
                .h(px(d.menu_item_height))
                .flex_shrink_0()
                .px(px(d.menu_item_padding_x))
                .flex()
                .items_center()
                .rounded(px(d.menu_item_radius))
                .bg(c.dialog_surface)
                .text_size(px(d.menu_text_size))
                .text_color(c.dialog_muted)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .truncate()
                        .child(os_menu.name.to_string()),
                )
                .into_any_element(),
        }
    }

    /// Renders the currently open in-window fallback menu as a floating
    /// panel. `menus` and `menu_labels` are fetched and computed once at
    /// the caller and shared with [`Self::render_in_window_menu_bar`].
    pub(super) fn render_in_window_menu_panel(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
        menus: Option<&[gpui::OwnedMenu]>,
        menu_labels: &[SharedString],
        top_offset: f32,
        origin_x: f32,
        viewport_height: f32,
    ) -> Option<AnyElement> {
        let open_index = self.menu_bar_open?;
        let menus = menus?;
        let menu = menus.get(open_index)?.clone();
        let menu_items = menu.items.clone();
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let editor = cx.entity().downgrade();
        let menu_item_labels = owned_menu_item_labels(&menu_items);
        let menu_panel_width = menu_panel_width_for_labels(&menu_item_labels, d);
        let submenu_bridge = self.menu_submenu_open.and_then(|submenu_index| {
            match menu_items.get(submenu_index)? {
                OwnedMenuItem::Submenu(submenu) => {
                    let submenu_labels = owned_menu_item_labels(&submenu.items);
                    let geometry = submenu_bridge_geometry_from_origin(
                        origin_x,
                        open_index,
                        menu_labels,
                        &menu_items,
                        submenu_index,
                        &submenu_labels,
                        d,
                    )?;
                    Some(
                        div()
                            .id(("app-submenu-bridge", open_index * 1000 + submenu_index))
                            .absolute()
                            .occlude()
                            .top(px(top_offset + geometry.top))
                            .left(px(geometry.left))
                            .w(px(geometry.width))
                            .h(px(geometry.height))
                            .bg(hsla(0.0, 0.0, 0.0, 0.0))
                            .on_hover(cx.listener(Self::on_menu_submenu_bridge_hover))
                            .into_any_element(),
                    )
                }
                _ => None,
            }
        });
        let submenu_panel =
            self.menu_submenu_open.and_then(|submenu_index| {
                match menu_items.get(submenu_index)? {
                    OwnedMenuItem::Submenu(submenu) => {
                        let submenu_labels = owned_menu_item_labels(&submenu.items);
                        let left =
                            menu_panel_left_from_origin(origin_x, open_index, menu_labels, d)
                                + menu_panel_width
                                + d.menu_panel_gap;
                        let top = submenu_panel_top(&menu_items, submenu_index, d);
                        let submenu_width = menu_panel_width_for_labels(&submenu_labels, d);
                        let submenu_items = submenu.items.clone().into_iter().enumerate().map(
                            |(item_index, item)| match item {
                                OwnedMenuItem::Separator => div()
                                    .id((
                                        "app-submenu-separator",
                                        submenu_index * 1000 + item_index,
                                    ))
                                    .mx(px(d.menu_separator_margin_x))
                                    .my(px(d.menu_separator_margin_y))
                                    .h(px(d.menu_separator_height))
                                    .bg(c.dialog_border)
                                    .into_any_element(),
                                OwnedMenuItem::Action { name, action, .. } => {
                                    let is_disabled =
                                        action.as_ref().as_any().is::<NoRecentFiles>();
                                    let is_keyboard_focused =
                                        self.menu_keyboard_submenu_item == Some(item_index);
                                    let shortcut = menu_shortcut_text(window, action.as_ref());
                                    let editor = editor.clone();
                                    let hover_editor = editor.clone();
                                    let base = div()
                                        .id(("app-submenu-item", submenu_index * 1000 + item_index))
                                        .w_full()
                                        .h(px(d.menu_item_height))
                                        .px(px(d.menu_item_padding_x))
                                        .flex()
                                        .items_center()
                                        .rounded(px(d.menu_item_radius))
                                        .bg(if is_keyboard_focused {
                                            c.dialog_secondary_button_hover
                                        } else {
                                            c.dialog_surface
                                        })
                                        .text_size(px(d.menu_text_size))
                                        .font_weight(t.dialog_body_weight.to_font_weight())
                                        .text_color(if is_disabled {
                                            c.dialog_muted
                                        } else {
                                            c.dialog_secondary_button_text
                                        })
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w(px(0.0))
                                                .overflow_hidden()
                                                .truncate()
                                                .child(name),
                                        )
                                        .children(shortcut.map(|text| {
                                            menu_shortcut_slot(text, theme).debug_selector(move || {
                                            format!(
                                                "app-submenu-shortcut-{submenu_index}-{item_index}"
                                            )
                                        })
                                        }))
                                        .on_hover(move |hovered, _window, cx| {
                                            if *hovered {
                                                let _ = hover_editor.update(cx, |editor, cx| {
                                                    editor.clear_menu_keyboard_cursor(cx);
                                                });
                                            }
                                        });

                                    if is_disabled {
                                        base.into_any_element()
                                    } else {
                                        base.hover(|this| this.bg(c.dialog_secondary_button_hover))
                                            .active(|this| this.opacity(0.92))
                                            .cursor_pointer()
                                            .on_click(move |_, window, cx| {
                                                let _ = editor.update(cx, |editor, cx| {
                                                    editor.close_menu_bar(cx)
                                                });
                                                dispatch_menu_action_for_editor(
                                                    action.as_ref(),
                                                    &editor,
                                                    window,
                                                    cx,
                                                );
                                            })
                                            .into_any_element()
                                    }
                                }
                                OwnedMenuItem::Submenu(submenu) => div()
                                    .id(("app-submenu-nested", submenu_index * 1000 + item_index))
                                    .w_full()
                                    .h(px(d.menu_item_height))
                                    .px(px(d.menu_item_padding_x))
                                    .flex()
                                    .items_center()
                                    .rounded(px(d.menu_item_radius))
                                    .bg(c.dialog_surface)
                                    .text_size(px(d.menu_text_size))
                                    .text_color(c.dialog_muted)
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .truncate()
                                            .child(submenu.name.to_string()),
                                    )
                                    .into_any_element(),
                                OwnedMenuItem::SystemMenu(os_menu) => div()
                                    .id(("app-submenu-system", submenu_index * 1000 + item_index))
                                    .w_full()
                                    .h(px(d.menu_item_height))
                                    .px(px(d.menu_item_padding_x))
                                    .flex()
                                    .items_center()
                                    .rounded(px(d.menu_item_radius))
                                    .bg(c.dialog_surface)
                                    .text_size(px(d.menu_text_size))
                                    .text_color(c.dialog_muted)
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .overflow_hidden()
                                            .truncate()
                                            .child(os_menu.name.to_string()),
                                    )
                                    .into_any_element(),
                            },
                        );

                        Some(
                            div()
                                .id(("app-submenu-panel", open_index * 1000 + submenu_index))
                                .absolute()
                                .occlude()
                                .top(px(top_offset + top))
                                .left(px(left))
                                .w(px(submenu_width))
                                .p(px(d.menu_panel_padding))
                                .flex()
                                .flex_col()
                                .gap(px(d.menu_panel_gap))
                                .bg(c.dialog_surface)
                                .border(px(d.dialog_border_width))
                                .border_color(c.dialog_border)
                                .rounded(px(d.menu_panel_radius))
                                .shadow_lg()
                                .on_hover(cx.listener(Self::on_menu_submenu_panel_hover))
                                .children(submenu_items)
                                .into_any_element(),
                        )
                    }
                    _ => None,
                }
            });

        let main_panel = div()
            .id(("app-menu-panel", open_index))
            .debug_selector(move || format!("app-menu-panel-{open_index}"))
            .absolute()
            .occlude()
            .top(px(top_offset + d.menu_panel_top))
            .left(px(menu_panel_left_from_origin(
                origin_x,
                open_index,
                menu_labels,
                d,
            )))
            .w(px(menu_panel_width))
            .p(px(d.menu_panel_padding))
            .flex()
            .flex_col()
            .gap(px(d.menu_panel_gap))
            .bg(c.dialog_surface)
            .border(px(d.dialog_border_width))
            .border_color(c.dialog_border)
            .rounded(px(d.menu_panel_radius))
            .shadow_lg()
            .on_hover(cx.listener(Self::on_menu_panel_hover));
        let main_panel = if let Some(split_index) = import_menu_split_index(&menu_items) {
            let scroll_items = &menu_items[..split_index];
            let footer_items = &menu_items[split_index..];
            let scroll_height = scrollable_import_menu_scroll_height(
                scroll_items,
                footer_items,
                viewport_height,
                top_offset,
                d,
            );
            let scroll_area = (!scroll_items.is_empty()).then(|| {
                div()
                    .id(("app-menu-scroll-area", open_index))
                    .w_full()
                    .h(px(scroll_height))
                    .flex_shrink_0()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap(px(d.menu_panel_gap))
                            .children(scroll_items.iter().cloned().enumerate().map(
                                |(item_index, item)| {
                                    self.render_in_window_menu_item(
                                        item,
                                        item_index,
                                        theme,
                                        editor.clone(),
                                        window,
                                    )
                                },
                            )),
                    )
                    .into_any_element()
            });
            let footer_elements =
                footer_items
                    .iter()
                    .cloned()
                    .enumerate()
                    .map(|(footer_index, item)| {
                        self.render_in_window_menu_item(
                            item,
                            split_index + footer_index,
                            theme,
                            editor.clone(),
                            window,
                        )
                    });

            main_panel
                .children(scroll_area)
                .children(footer_elements)
                .into_any_element()
        } else {
            let items = menu_items
                .iter()
                .cloned()
                .enumerate()
                .map(|(item_index, item)| {
                    self.render_in_window_menu_item(item, item_index, theme, editor.clone(), window)
                });

            main_panel.children(items).into_any_element()
        };

        let layer = div()
            .id(("app-menu-panel-layer", open_index))
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .child(main_panel);
        let layer = if let Some(submenu_bridge) = submenu_bridge {
            layer.child(submenu_bridge)
        } else {
            layer
        };
        let layer = if let Some(submenu_panel) = submenu_panel {
            layer.child(submenu_panel)
        } else {
            layer
        };

        Some(layer.into_any_element())
    }
}
