// @author kongweiguang

//! GPUI rendering and interactions for the recursive pane workspace.

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AppContext, Context, CursorStyle, ElementId, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px, svg,
};

use super::layout::{
    MIN_PANE_HEIGHT, MIN_PANE_WIDTH, PANE_TAB_BAR_HEIGHT, PaneDivider, PaneRect,
    compute_pane_layout,
};
use super::pane::{PaneTabDragPreview, PaneTabDragVisual};
use super::workspace::PaneWorkspaceView;
use crate::editor::panes::{PaneEvent, PaneId, PaneTabDragPayload, SplitAxis};

const PANE_TAB_MIN_WIDTH: f32 = 88.0;
const PANE_TAB_MAX_WIDTH: f32 = 220.0;
const PANE_TAB_TOOL_SIZE: f32 = 28.0;
const PANE_TAB_NEW_ICON: &str = "icon/ui/plus.svg";
const PANE_TAB_SPLIT_ICON: &str = "icon/ui/split.svg";
const PANE_TAB_CLOSE_ICON: &str = "icon/ui/close.svg";

impl PaneWorkspaceView {
    fn render_divider(
        &mut self,
        divider: PaneDivider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = divider.path().to_vec();
        let focus_handle = self
            .divider_focus_handles
            .entry(path.clone())
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let focused = focus_handle.is_focused(window);
        let dragging = self.drag.as_ref().is_some_and(|drag| drag.path == path);
        let theme = cx.global::<crate::theme::ThemeManager>().current_arc();
        let workbench = &theme.colors.workbench;
        let rect = divider.rect();
        let axis = divider.axis();
        let resize_label: SharedString = match axis {
            SplitAxis::Horizontal => "Resize left/right panes".into(),
            SplitAxis::Vertical => "Resize top/bottom panes".into(),
        };
        let divider_for_drag = divider.clone();
        let divider_for_key = divider.clone();
        let mut element = div()
            .id(ElementId::Name(
                format!("pane-divider-{}", path_key(&path)).into(),
            ))
            .debug_selector(|| "pane-divider".to_owned())
            .absolute()
            .left(px(rect.x))
            .top(px(rect.y))
            .w(px(rect.width))
            .h(px(rect.height))
            .tab_index(0)
            .track_focus(&focus_handle)
            .cursor(match axis {
                SplitAxis::Horizontal => CursorStyle::ResizeLeftRight,
                SplitAxis::Vertical => CursorStyle::ResizeUpDown,
            })
            .tooltip(move |_window, cx| crate::ui::ui_tooltip(resize_label.clone(), cx))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.start_ratio_drag(&divider_for_drag, event, window, cx);
                }),
            )
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _window, cx| {
                if this.adjust_ratio_from_key(
                    divider_for_key.path(),
                    divider_for_key.axis(),
                    event,
                    cx,
                ) {
                    cx.stop_propagation();
                }
            }));
        let line_color = if focused || dragging {
            workbench.focus_ring
        } else {
            workbench.border_subtle
        };
        element = match axis {
            SplitAxis::Horizontal => element
                // 横向拆分会预留 6px 的拖动命中区。顶部必须延续两侧
                // Tab 栏表面，否则命中区会在左侧 Tab 栏末端露出缺口。
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(PANE_TAB_BAR_HEIGHT))
                        .bg(theme.colors.tab_strip_background)
                        .debug_selector(|| "pane-divider-tab-bar-fill".to_owned()),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px((rect.width - 1.0) * 0.5))
                        .w(px(1.0))
                        .bg(line_color),
                ),
            SplitAxis::Vertical => element.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px((rect.height - 1.0) * 0.5))
                    .h(px(1.0))
                    .bg(line_color),
            ),
        };
        element.into_any_element()
    }

    /// 窗格 Tab 复用全局轮廓契约，避免递归分屏出现不同半径，同时保持既有控件和布局边界稳定。
    fn render_pane_tab_bar(
        &mut self,
        pane: PaneId,
        focused: bool,
        theme: &crate::theme::Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workbench = &theme.colors.workbench;
        let active = self
            .workspace
            .pane(pane)
            .and_then(|state| state.active_tab_id());
        let tabs = self
            .workspace
            .pane(pane)
            .map(|state| {
                state
                    .tabs()
                    .iter()
                    .map(|tab| {
                        (
                            tab.id(),
                            *tab.document(),
                            tab.view().display_title(),
                            tab.view().icon(),
                            tab.is_pinned(),
                            tab.view().is_dirty(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let controller = self.controller.clone();
        let header_controller = controller.clone();
        let new_tooltip: SharedString = cx
            .global::<crate::i18n::I18nManager>()
            .strings()
            .menu_new_tab
            .clone()
            .into();
        let split_tooltip: SharedString = if cx
            .global::<crate::i18n::I18nManager>()
            .current_language_id()
            .starts_with("zh")
        {
            "拆分窗格".into()
        } else {
            "Split Pane".into()
        };
        let close_tooltip: SharedString = cx
            .global::<crate::i18n::I18nManager>()
            .strings()
            .menu_close_tab
            .clone()
            .into();
        let pane_uuid = pane.as_uuid();
        let new_tab_focus = self
            .new_tab_focus_handles
            .entry(pane)
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let split_focus = self
            .split_focus_handles
            .entry(pane)
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let tab_strip_background = theme.colors.tab_strip_background;
        let drag_background = workbench.elevated_surface;
        let drag_text = workbench.text_primary;
        let active_tab_index = tabs.iter().position(|(tab, ..)| active == Some(*tab));

        div()
            .id(ElementId::Name(format!("pane-tab-bar-{pane_uuid}").into()))
            .debug_selector(|| "pane-tab-bar".to_owned())
            .h(px(PANE_TAB_BAR_HEIGHT))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .overflow_hidden()
            .bg(tab_strip_background)
            .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                header_controller.focus(pane, window, cx);
            })
            .child(
                div()
                    .h_full()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .children(tabs.into_iter().enumerate().map(
                        |(index, (tab, document_id, title, icon, pinned, dirty))| {
                            let selected = active == Some(tab);
                            let separates_inactive_tabs = index > 0
                                && active_tab_index != Some(index)
                                && active_tab_index != Some(index.saturating_sub(1));
                            let tab_controller = controller.clone();
                            let close_controller = controller.clone();
                            let title_text: SharedString = title.clone().into();
                            let close_tooltip = close_tooltip.clone();
                            let drag_visual = PaneTabDragVisual {
                                title: title.clone().into(),
                                background: drag_background,
                                text: drag_text,
                            };
                            let drag_payload = PaneTabDragPayload {
                                source: pane,
                                tab,
                                document_id,
                            };
                            let tab_uuid = tab.as_uuid();
                            div()
                                .id(ElementId::Name(
                                    format!("pane-tab-{pane_uuid}-{tab_uuid}").into(),
                                ))
                                .debug_selector(move || {
                                    if selected {
                                        "pane-tab-active".to_owned()
                                    } else {
                                        "pane-tab-inactive".to_owned()
                                    }
                                })
                                .group(SharedString::from(format!(
                                    "pane-tab-group-{pane_uuid}-{tab_uuid}"
                                )))
                                .h_full()
                                .min_w(px(PANE_TAB_MIN_WIDTH))
                                .max_w(px(PANE_TAB_MAX_WIDTH))
                                .px(px(9.0))
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .relative()
                                .bg(if selected {
                                    workbench.editor_surface
                                } else {
                                    tab_strip_background
                                })
                                .when(separates_inactive_tabs, |this| {
                                    this.child(
                                        crate::editor::tabs::terminal_inactive_tab_separator(
                                            workbench.border_subtle,
                                            0.0,
                                            PANE_TAB_BAR_HEIGHT,
                                            format!("pane-tab-inactive-separator-{index}"),
                                        ),
                                    )
                                })
                                .when(selected, |this| {
                                    this.rounded_t(px(
                                        crate::editor::tabs::TERMINAL_TAB_SHOULDER_RADIUS,
                                    ))
                                    // 外扩包裹层只改变绘制轮廓，因此不会把圆肩区域加入 Tab 的命中盒。
                                    .child(crate::editor::tabs::terminal_tab_shoulder_cutout(
                                        workbench.editor_surface,
                                        tab_strip_background,
                                        true,
                                        "pane-tab-active-bottom-curve-left".to_owned(),
                                    ))
                                    .child(
                                        crate::editor::tabs::terminal_tab_shoulder_cutout(
                                            workbench.editor_surface,
                                            tab_strip_background,
                                            false,
                                            "pane-tab-active-bottom-curve-right".to_owned(),
                                        ),
                                    )
                                })
                                .text_color(if selected {
                                    workbench.text_primary
                                } else {
                                    workbench.text_secondary
                                })
                                .hover(|this| {
                                    this.bg(if selected {
                                        workbench.editor_surface
                                    } else {
                                        workbench.control_hover
                                    })
                                })
                                // 选中 Tab 用编辑器表面铺满自身并向两侧外扩
                                // 8px 肩部，圆形栏色切口负责把活动面连续接回正文。
                                .cursor_pointer()
                                .tooltip(move |_window, cx| {
                                    crate::ui::ui_tooltip(title_text.clone(), cx)
                                })
                                // Zed keeps the close target in the leading icon
                                // slot.  The normal document/pin icon (or dirty
                                // dot) occupies that slot; hovering the tab swaps
                                // it for the close glyph without changing bounds.
                                .child(
                                    div()
                                        .id(ElementId::Name(
                                            format!("pane-tab-close-{pane_uuid}-{tab_uuid}").into(),
                                        ))
                                        .debug_selector(|| "pane-tab-close".to_owned())
                                        .relative()
                                        .size(px(24.0))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(4.0))
                                        .hover(|this| this.bg(workbench.control_hover))
                                        .cursor_pointer()
                                        .tooltip(move |_window, cx| {
                                            crate::ui::ui_tooltip(close_tooltip.clone(), cx)
                                        })
                                        .child(
                                            svg()
                                                .absolute()
                                                .path(if pinned {
                                                    "icon/editor/tab-pin.svg"
                                                } else {
                                                    icon
                                                })
                                                .size(px(13.0))
                                                .text_color(if pinned {
                                                    workbench.accent
                                                } else {
                                                    workbench.icon
                                                })
                                                .opacity(if dirty { 0.0 } else { 1.0 })
                                                .group_hover(
                                                    SharedString::from(format!(
                                                        "pane-tab-group-{pane_uuid}-{tab_uuid}"
                                                    )),
                                                    |this| this.opacity(0.0),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .size(px(6.0))
                                                .rounded_full()
                                                .bg(workbench.accent)
                                                .debug_selector(|| "pane-tab-dirty".to_owned())
                                                .opacity(if dirty { 1.0 } else { 0.0 })
                                                .group_hover(
                                                    SharedString::from(format!(
                                                        "pane-tab-group-{pane_uuid}-{tab_uuid}"
                                                    )),
                                                    |this| this.opacity(0.0),
                                                ),
                                        )
                                        .child(
                                            svg()
                                                .absolute()
                                                .path(PANE_TAB_CLOSE_ICON)
                                                .size(px(13.0))
                                                .text_color(workbench.icon)
                                                .opacity(0.0)
                                                .group_hover(
                                                    SharedString::from(format!(
                                                        "pane-tab-group-{pane_uuid}-{tab_uuid}"
                                                    )),
                                                    |this| this.opacity(1.0),
                                                ),
                                        )
                                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(move |_event, window, cx| {
                                            close_controller.emit(
                                                PaneEvent::CloseTab { pane, tab },
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .truncate()
                                        .text_size(px(theme.typography.text_size * 0.88))
                                        .child(title),
                                )
                                .on_click(move |_event, window, cx| {
                                    tab_controller.emit(
                                        PaneEvent::ActivateTab { pane, tab },
                                        window,
                                        cx,
                                    );
                                })
                                .on_drag(drag_payload, move |_payload, position, _, cx| {
                                    let visual = drag_visual.clone();
                                    cx.new(|_| PaneTabDragPreview { visual, position })
                                })
                        },
                    )),
            )
            .children(focused.then(|| {
                let new_controller = controller.clone();
                let new_key_controller = controller.clone();
                let new_pointer_focus = new_tab_focus.clone();
                let split_controller = controller.clone();
                let split_key_controller = controller.clone();
                let split_pointer_focus = split_focus.clone();
                div()
                    .id(ElementId::Name(
                        format!("pane-tab-tools-{pane_uuid}").into(),
                    ))
                    .debug_selector(|| "pane-tab-tools".to_owned())
                    .h_full()
                    .px(px(4.0))
                    .gap(px(2.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .bg(tab_strip_background)
                    .border_l(px(1.0))
                    .border_color(workbench.border_subtle)
                    .child(
                        div()
                            .id(ElementId::Name(format!("pane-tab-new-{pane_uuid}").into()))
                            .debug_selector(|| "pane-tab-new".to_owned())
                            .size(px(PANE_TAB_TOOL_SIZE))
                            .tab_index(0)
                            .track_focus(&new_tab_focus)
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.0))
                            .border(px(1.0))
                            .border_color(workbench.border_subtle.opacity(0.0))
                            .hover(|this| this.bg(workbench.control_hover))
                            .focus(|this| this.border_color(workbench.focus_ring))
                            .cursor_pointer()
                            .tooltip(move |_window, cx| {
                                crate::ui::ui_tooltip(new_tooltip.clone(), cx)
                            })
                            .child(
                                svg()
                                    .path(PANE_TAB_NEW_ICON)
                                    .size(px(14.0))
                                    .text_color(workbench.icon),
                            )
                            .on_click(move |event, window, cx| {
                                new_pointer_focus.focus(window);
                                new_controller.emit(
                                    PaneEvent::OpenNewTabMenu {
                                        pane,
                                        x: f32::from(event.position().x),
                                        y: f32::from(event.position().y),
                                    },
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            })
                            .on_key_down(move |event, window, cx| {
                                match event.keystroke.key.as_str() {
                                    "enter" | "space" => {
                                        let position = window.mouse_position();
                                        new_key_controller.emit(
                                            PaneEvent::OpenNewTabMenu {
                                                pane,
                                                x: f32::from(position.x),
                                                y: f32::from(position.y),
                                            },
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }
                                    "escape" => {
                                        new_key_controller.emit(
                                            PaneEvent::DismissMenus,
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }
                                    _ => {}
                                }
                            }),
                    )
                    .child(
                        div()
                            .id(ElementId::Name(
                                format!("pane-tab-split-{pane_uuid}").into(),
                            ))
                            .debug_selector(|| "pane-tab-split".to_owned())
                            .size(px(PANE_TAB_TOOL_SIZE))
                            .tab_index(0)
                            .track_focus(&split_focus)
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.0))
                            .border(px(1.0))
                            .border_color(workbench.border_subtle.opacity(0.0))
                            .hover(|this| this.bg(workbench.control_hover))
                            .focus(|this| this.border_color(workbench.focus_ring))
                            .cursor_pointer()
                            .tooltip(move |_window, cx| {
                                crate::ui::ui_tooltip(split_tooltip.clone(), cx)
                            })
                            .child(
                                svg()
                                    .path(PANE_TAB_SPLIT_ICON)
                                    .size(px(15.0))
                                    .text_color(workbench.icon),
                            )
                            .on_click(move |event, window, cx| {
                                split_pointer_focus.focus(window);
                                split_controller.emit(
                                    PaneEvent::OpenSplitMenu {
                                        pane,
                                        x: f32::from(event.position().x),
                                        y: f32::from(event.position().y),
                                    },
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            })
                            .on_key_down(move |event, window, cx| {
                                match event.keystroke.key.as_str() {
                                    "enter" | "space" => {
                                        let position = window.mouse_position();
                                        split_key_controller.emit(
                                            PaneEvent::OpenSplitMenu {
                                                pane,
                                                x: f32::from(position.x),
                                                y: f32::from(position.y),
                                            },
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }
                                    "escape" => {
                                        split_key_controller.emit(
                                            PaneEvent::DismissMenus,
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }
                                    _ => {}
                                }
                            }),
                    )
            }))
            .into_any_element()
    }

    fn render_pane(
        &mut self,
        pane: PaneId,
        rect: PaneRect,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.global::<crate::theme::ThemeManager>().current_arc();
        let focused = self.workspace.focused_pane() == pane;
        let header = self.render_pane_tab_bar(pane, focused, &theme, cx);
        let content = self
            .active_views
            .get(&pane)
            .cloned()
            .map(|entity| entity.into_any_element())
            .unwrap_or_else(|| {
                div()
                    .id(ElementId::Name(
                        format!("pane-empty-{}", pane.as_uuid()).into(),
                    ))
                    .size_full()
                    .bg(theme.colors.workbench.editor_surface)
                    .into_any_element()
            });
        let drop_controller = self.controller.clone();
        div()
            .id(ElementId::Name(
                format!("pane-shell-{}", pane.as_uuid()).into(),
            ))
            .debug_selector(|| "pane-shell".to_owned())
            .absolute()
            .left(px(rect.x))
            .top(px(rect.y))
            .w(px(rect.width))
            .h(px(rect.height))
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(theme.colors.workbench.editor_surface)
            .when(!compact, |this| {
                this.min_w(px(MIN_PANE_WIDTH)).min_h(px(MIN_PANE_HEIGHT))
            })
            .drag_over::<PaneTabDragPayload>(move |style, _, _, _| {
                style.bg(theme.colors.workbench.accent_soft)
            })
            .on_drop(move |payload: &PaneTabDragPayload, window, cx| {
                if payload.source == pane {
                    return;
                }
                if window.modifiers().secondary() {
                    drop_controller.copy_tab(payload.source, pane, payload.tab, window, cx);
                } else {
                    drop_controller.move_tab(payload.source, pane, payload.tab, window, cx);
                }
            })
            .child(header)
            .child(
                div()
                    .debug_selector(|| "pane-content".to_owned())
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .child(content),
            )
            .into_any_element()
    }
}

fn path_key(path: &[bool]) -> String {
    path.iter()
        .map(|part| if *part { '1' } else { '0' })
        .collect()
}

impl Render for PaneWorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_active_views(cx);
        self.layout = compute_pane_layout(
            self.workspace.root(),
            self.viewport,
            self.workspace.focused_pane(),
        );

        let theme = cx.global::<crate::theme::ThemeManager>().current_arc();
        let mut children = Vec::new();
        let compact = self.layout.is_degraded();
        let pane_order = self.layout.pane_order().to_vec();
        for pane in pane_order {
            if let Some(rect) = self.layout.rect(pane) {
                children.push(self.render_pane(pane, rect, compact, cx));
            }
        }
        let dividers = self.layout.dividers().to_vec();
        for divider in dividers {
            children.push(self.render_divider(divider, window, cx));
        }
        div()
            .id("pane-workspace")
            .debug_selector(|| "pane-workspace".to_owned())
            .relative()
            .size_full()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .bg(theme.colors.workbench.editor_surface)
            .on_mouse_move(cx.listener(Self::update_ratio_drag))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.end_ratio_drag(cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.end_ratio_drag(cx);
                }),
            )
            .children(children)
    }
}
