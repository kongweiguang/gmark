// @author kongweiguang

use super::*;

impl Block {
    /// 渲染代码语言选择、复制反馈与代码正文。
    pub(super) fn render_code_block_content(
        &mut self,
        focused_base: Stateful<Div>,
        is_placeholder: bool,
        theme: &Theme,
        strings: &I18nStrings,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let wb = &c.workbench;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let language_placeholder = SharedString::from(strings.code_language_placeholder.clone());
        let language_menu_tooltip: SharedString = strings.code_language_menu.clone().into();
        let copy_tooltip: SharedString = if self.code_copy_feedback {
            strings.code_copied.clone()
        } else {
            strings.code_copy.clone()
        }
        .into();
        let language_items = crate::components::CODE_LANGUAGE_MENU_ITEMS
            .iter()
            .enumerate()
            .map(|(index, language)| {
                let selected = index == self.code_language_menu_selected;
                div()
                    .id(SharedString::from(format!("code-language-{language}")))
                    .debug_selector(move || format!("code-language-{language}"))
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(6.0))
                    .bg(if selected {
                        wb.control_hover
                    } else {
                        wb.control_surface
                    })
                    .hover(|this| this.bg(wb.control_hover))
                    .cursor_pointer()
                    .text_size(px((t.code_size - 1.0).max(10.0)))
                    .text_color(wb.text_primary)
                    .child(*language)
                    .on_click(cx.listener(move |block, _event, _window, cx| {
                        block.select_code_language_menu_item(index, cx);
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let language_menu = self.code_language_menu_open.then(|| {
            deferred(
                div()
                    .id("code-language-menu")
                    .debug_selector(|| "code-language-menu".to_owned())
                    .absolute()
                    .left_0()
                    .top(px(28.0))
                    .w(px(d.code_language_input_width))
                    .max_h(px(240.0))
                    .overflow_y_scroll()
                    .p(px(3.0))
                    .occlude()
                    .bg(wb.elevated_surface)
                    .border(px(d.dialog_border_width))
                    .border_color(wb.border_subtle)
                    .rounded(px(10.0))
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .children(language_items),
            )
            .with_priority(30)
        });
        let language_control = div()
            .id("code-language-control")
            .debug_selector(|| "code-language-control".to_owned())
            .relative()
            .h(px(28.0))
            .w(px(d.code_language_input_width))
            .min_w(px(96.0))
            .flex_shrink()
            .pr(px(22.0))
            .pl(px(d.code_language_input_padding_x))
            .flex()
            .items_center()
            .rounded(px(d.code_language_input_radius.max(6.0).min(10.0)))
            .border(px(d.code_language_input_border_width))
            .border_color(c.code_language_input_border)
            .bg(c.code_language_input_bg)
            .focus(|this| this.border_color(wb.focus_ring))
            .key_context(BLOCK_EDITOR_CONTEXT)
            .track_focus(&self.code_language_focus_handle)
            .on_action(cx.listener(Self::on_code_language_newline))
            .on_action(cx.listener(Self::on_code_language_dismiss))
            .on_action(cx.listener(Self::on_code_language_delete_back))
            .on_action(cx.listener(Self::on_code_language_delete))
            .on_action(cx.listener(Self::on_code_language_focus_content))
            .on_action(cx.listener(Self::on_code_language_focus_next))
            .on_action(cx.listener(Self::on_code_language_move_left))
            .on_action(cx.listener(Self::on_code_language_move_right))
            .on_action(cx.listener(Self::on_code_language_home))
            .on_action(cx.listener(Self::on_code_language_end))
            .on_action(cx.listener(Self::on_code_language_select_left))
            .on_action(cx.listener(Self::on_code_language_select_right))
            .on_action(cx.listener(Self::on_code_language_select_all))
            .on_action(cx.listener(Self::on_code_language_copy))
            .on_action(cx.listener(Self::on_code_language_cut))
            .on_action(cx.listener(Self::on_code_language_paste))
            .on_action(cx.listener(Self::on_code_language_indent))
            .on_action(cx.listener(Self::on_code_language_outdent))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(Self::on_code_language_mouse_down),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(Self::on_code_language_mouse_up),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(Self::on_code_language_mouse_up_out),
            )
            .on_mouse_move(cx.listener(Self::on_code_language_mouse_move))
            .text_size(px((t.code_size - 1.0).max(10.0)))
            .text_color(c.code_language_input_text)
            .cursor(CursorStyle::IBeam)
            .child(CodeLanguageInputElement::new(
                cx.entity(),
                language_placeholder,
            ))
            .child(
                div()
                    .id("code-language-menu-button")
                    .debug_selector(|| "code-language-menu-button".to_owned())
                    .absolute()
                    .right_0()
                    .top_0()
                    .size(px(22.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .hover(|this| this.bg(wb.control_hover))
                    .cursor_pointer()
                    .tooltip(move |_window, cx| {
                        crate::ui::ui_tooltip(language_menu_tooltip.clone(), cx)
                    })
                    .text_color(c.code_language_input_text)
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .on_click(cx.listener(|block, _event, window, cx| {
                        block.toggle_code_language_menu(window, cx);
                    }))
                    .child(
                        svg()
                            .path(CHEVRON_DOWN_ICON)
                            .size(px(14.0))
                            .text_color(wb.text_primary),
                    ),
            )
            .children(language_menu);
        // GPUI SVGs do not inherit the parent `text_color`; keep the icon
        // color explicit so the copy/check glyph remains visible in every
        // built-in light and dark palette.
        let copy_color = if self.code_copy_feedback {
            wb.success
        } else {
            wb.icon
        };
        let copy_button = div()
            .id("code-block-copy")
            .debug_selector(|| "code-block-copy".to_owned())
            .tab_index(0)
            .track_focus(&self.code_copy_focus_handle)
            .size(px(28.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(wb.control_surface.opacity(0.0))
            .hover(|this| this.bg(wb.control_hover))
            .focus(|this| this.border_color(wb.focus_ring))
            .active(|this| this.opacity(0.86))
            .cursor_pointer()
            .tooltip(move |_window, cx| crate::ui::ui_tooltip(copy_tooltip.clone(), cx))
            .text_color(copy_color)
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .on_click(cx.listener(|block, _event, window, cx| {
                block.code_copy_focus_handle.focus(window);
                block.copy_code_block(cx);
            }))
            .on_key_down(cx.listener(|block, event: &KeyDownEvent, _window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    block.copy_code_block(cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                svg()
                    .path(if self.code_copy_feedback {
                        CHECK_ICON
                    } else {
                        COPY_ICON
                    })
                    .size(px(15.0))
                    .text_color(copy_color),
            );
        let toolbar = div()
            .id("code-block-toolbar")
            .debug_selector(|| "code-block-toolbar".to_owned())
            .w_full()
            .h(px(32.0))
            .pr(px(1.0))
            .flex_shrink_0()
            .flex()
            .items_start()
            .gap(px(d.code_language_input_gap.max(8.0)))
            .child(language_control)
            .child(copy_button);
        // The content column starts narrowing before the window itself feels
        // narrow (sidebars and the centered reading width consume the same
        // viewport). Keep the compact control group at the content edge until
        // that transition is complete; otherwise both controls can be pushed
        // past a clipped editor pane while the code remains readable.
        let toolbar = if window.viewport_size().width <= px(d.centered_shrink_end) {
            toolbar.justify_start()
        } else {
            toolbar.justify_end()
        };
        // 交互 shell 负责左侧“+”预留；可见 surface 必须从正文内容边界开始。
        let code_surface = div()
            .debug_selector(|| "code-block-surface".to_owned())
            .w_full()
            .min_w(px(0.0))
            .bg(c.code_bg)
            .border(px(1.0))
            .border_color(wb.border_subtle)
            .rounded(px(8.0))
            .pl(px(d.code_block_padding_x))
            .pr(px(d.code_block_padding_x))
            .pt(px(4.0))
            .pb(px(d.code_block_padding_y))
            .flex()
            .flex_col()
            .text_size(px(t.code_size))
            .text_color(c.code_text)
            .line_height(rems(t.text_line_height))
            .child(toolbar)
            .child(
                div()
                    .min_w(px(0.0))
                    .w_full()
                    .pt(px(4.0))
                    .font_family(crate::document_host::source_monospace_font_family())
                    .child(BlockTextElement::new(cx.entity(), is_placeholder)),
            );
        focused_base.child(code_surface).into_any_element()
    }
}
