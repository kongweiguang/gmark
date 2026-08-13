// @author kongweiguang

use super::*;

impl Editor {
    pub(in crate::editor) fn render_split_pane_menu_overlay(
        &self,
        theme: &crate::theme::Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.tabs.split_pane_menu.as_ref()?;
        let position = menu.position;
        let menu_focus = menu.focus_handle.clone();
        let dimensions = &theme.dimensions;
        let workbench = &theme.colors.workbench;
        let visual_preferences = cx
            .try_global::<crate::ui::visual_preferences::VisualPreferencesManager>()
            .map(crate::ui::visual_preferences::VisualPreferencesManager::current)
            .unwrap_or_default();
        let material = workbench.material(
            crate::ui::theme::workbench::SurfaceKind::GlassStrong,
            visual_preferences,
        );
        let chinese = cx
            .global::<crate::i18n::I18nManager>()
            .current_language_id()
            .starts_with("zh");
        let labels = if chinese {
            [
                "向右拆分",
                "向左拆分",
                "向上拆分",
                "向下拆分",
                "关闭当前窗格",
            ]
        } else {
            [
                "Split Right",
                "Split Left",
                "Split Up",
                "Split Down",
                "Close Pane",
            ]
        };
        let panel_width = dimensions.context_menu_submenu_width.max(220.0);
        let panel_origin = clamped_floating_panel_origin(
            position,
            panel_width,
            compact_menu_panel_height(5, 1, dimensions),
            window.viewport_size(),
        );
        let can_close = self
            .pane_workspace
            .as_ref()
            .is_some_and(|workspace| workspace.read(cx).workspace().pane_count() > 1);
        let editor = cx.entity().downgrade();
        let dismiss_editor = editor.clone();
        let key_dismiss_editor = editor.clone();
        let action_dismiss_editor = editor.clone();
        let right_editor = editor.clone();
        let left_editor = editor.clone();
        let up_editor = editor.clone();
        let down_editor = editor.clone();
        let close_editor = editor;
        let item =
            |id: &'static str, label: &'static str, direction: &'static str, enabled: bool| {
                div()
                    .id(id)
                    .debug_selector(move || id.to_owned())
                    .h(px(dimensions.menu_item_height))
                    .px(px(dimensions.menu_item_padding_x))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .rounded(px(dimensions.menu_item_radius))
                    .text_size(px(dimensions.menu_text_size))
                    .text_color(if enabled {
                        workbench.text_primary
                    } else {
                        workbench.text_tertiary
                    })
                    .when(enabled, |item| {
                        item.hover(|item| item.bg(workbench.control_hover))
                            .cursor_pointer()
                    })
                    .child(label)
                    .child(
                        div()
                            .text_color(workbench.text_secondary)
                            .font_family(crate::document_host::source_monospace_font_family())
                            .child(direction),
                    )
            };
        Some(
            div()
                .id("split-pane-menu-overlay")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .tab_index(0)
                .track_focus(&menu_focus)
                .occlude()
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    let _ = dismiss_editor.update(cx, |editor, cx| {
                        editor.tabs.split_pane_menu = None;
                        cx.notify();
                    });
                })
                .capture_key_down(move |event, _window, cx| {
                    if event.keystroke.key == "escape" {
                        let _ = key_dismiss_editor.update(cx, |editor, cx| {
                            editor.tabs.split_pane_menu = None;
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }
                })
                .on_action(
                    move |_: &crate::components::DismissTransientUi, _window, cx| {
                        let _ = action_dismiss_editor.update(cx, |editor, cx| {
                            editor.tabs.split_pane_menu = None;
                            cx.notify();
                        });
                        cx.stop_propagation();
                    },
                )
                .child(
                    div()
                        .id("split-pane-menu")
                        .debug_selector(|| "split-pane-menu".to_owned())
                        .absolute()
                        .left(panel_origin.x)
                        .top(panel_origin.y)
                        .w(px(panel_width))
                        .p(px(dimensions.menu_panel_padding))
                        .flex()
                        .flex_col()
                        .gap(px(dimensions.menu_panel_gap))
                        .bg(material.background)
                        .border(px(dimensions.dialog_border_width))
                        .border_color(material.border)
                        .rounded(px(dimensions.menu_panel_radius))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation()
                        })
                        .child(item("pane-split-right", labels[0], "→", true).on_click(
                            move |_event, _window, cx| {
                                let _ = right_editor.update(cx, |editor, cx| {
                                    editor.tabs.split_pane_menu = None;
                                    editor.split_pane_toward(
                                        crate::editor::panes::PaneSplitDirection::Right,
                                        cx,
                                    );
                                });
                            },
                        ))
                        .child(item("pane-split-left", labels[1], "←", true).on_click(
                            move |_event, _window, cx| {
                                let _ = left_editor.update(cx, |editor, cx| {
                                    editor.tabs.split_pane_menu = None;
                                    editor.split_pane_toward(
                                        crate::editor::panes::PaneSplitDirection::Left,
                                        cx,
                                    );
                                });
                            },
                        ))
                        .child(item("pane-split-up", labels[2], "↑", true).on_click(
                            move |_event, _window, cx| {
                                let _ = up_editor.update(cx, |editor, cx| {
                                    editor.tabs.split_pane_menu = None;
                                    editor.split_pane_toward(
                                        crate::editor::panes::PaneSplitDirection::Up,
                                        cx,
                                    );
                                });
                            },
                        ))
                        .child(item("pane-split-down", labels[3], "↓", true).on_click(
                            move |_event, _window, cx| {
                                let _ = down_editor.update(cx, |editor, cx| {
                                    editor.tabs.split_pane_menu = None;
                                    editor.split_pane_toward(
                                        crate::editor::panes::PaneSplitDirection::Down,
                                        cx,
                                    );
                                });
                            },
                        ))
                        .child(
                            div()
                                .h(px(1.0))
                                .mx(px(dimensions.menu_item_padding_x))
                                .bg(material.border),
                        )
                        .child(item("pane-close-current", labels[4], "×", can_close).when(
                            can_close,
                            |item| {
                                item.on_click(move |_event, window, cx| {
                                    let _ = close_editor.update(cx, |editor, cx| {
                                        editor.tabs.split_pane_menu = None;
                                        editor.on_close_pane_action(
                                            &crate::components::ClosePane,
                                            window,
                                            cx,
                                        );
                                    });
                                })
                            },
                        )),
                )
                .into_any_element(),
        )
    }
}
