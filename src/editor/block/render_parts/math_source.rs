// @author kongweiguang

use super::super::runtime::math_source::MathSourceInputElement;
use super::*;

impl Block {
    pub(super) fn render_math_source_editor(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = &theme.colors;
        let wb = &colors.workbench;
        let dimensions = &theme.dimensions;
        let focus = self.math_source_focus_handle.clone();
        let block = cx.entity();
        div()
            .id("math-source-editor")
            .debug_selector(|| "math-source-editor".to_owned())
            .w_full()
            .min_w(px(0.0))
            .h(px(30.0))
            .overflow_hidden()
            .px(px(dimensions.code_language_input_padding_x))
            .flex()
            .items_center()
            .rounded(px(dimensions.code_language_input_radius.max(6.0).min(10.0)))
            .border(px(dimensions.code_language_input_border_width))
            .border_color(colors.code_language_input_border)
            .bg(colors.code_language_input_bg)
            .text_size(px((theme.typography.code_size - 1.0).max(10.0)))
            .text_color(colors.code_language_input_text)
            .font_family(crate::document_host::source_monospace_font_family())
            .cursor(CursorStyle::IBeam)
            .key_context(BLOCK_EDITOR_CONTEXT)
            .track_focus(&focus)
            .focus(|this| this.border_color(wb.focus_ring))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(Self::on_math_source_mouse_down),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(Self::on_math_source_mouse_up),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(Self::on_math_source_mouse_up_out),
            )
            .on_mouse_move(cx.listener(Self::on_math_source_mouse_move))
            .on_key_down(cx.listener(Self::on_math_source_key_down))
            .on_action(cx.listener(Self::on_math_source_delete_back))
            .on_action(cx.listener(Self::on_math_source_delete))
            .on_action(cx.listener(Self::on_math_source_move_left))
            .on_action(cx.listener(Self::on_math_source_move_right))
            .on_action(cx.listener(Self::on_math_source_select_left))
            .on_action(cx.listener(Self::on_math_source_select_right))
            .on_action(cx.listener(Self::on_math_source_home))
            .on_action(cx.listener(Self::on_math_source_end))
            .on_action(cx.listener(Self::on_math_source_select_all))
            .on_action(cx.listener(Self::on_math_source_copy))
            .on_action(cx.listener(Self::on_math_source_cut))
            .on_action(cx.listener(Self::on_math_source_paste))
            .on_action(cx.listener(Self::on_exit_code_block))
            .child(MathSourceInputElement::new(block))
            .into_any_element()
    }

    pub(super) fn render_active_inline_math_editor(
        &mut self,
        _content: AnyElement,
        theme: &Theme,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let visual = self.render_mixed_inline_visual_runs(
            theme,
            theme.colors.text_default,
            theme.typography.text_size,
            FontWeight::NORMAL,
            cx,
        );
        let structure_focus = self.math_structure_focus_handle.clone();
        let source_focus = self.math_source_focus_handle.clone();
        let structured = self.math_edit_session.is_some();
        let root = div()
            .id("active-inline-math-editor")
            .w_full()
            .relative()
            .flex()
            .flex_col()
            .items_center()
            .key_context(BLOCK_EDITOR_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_exit_code_block))
            .child(
                div()
                    .id("inline-math-visual-editor")
                    .debug_selector(|| "inline-math-visual-editor".to_owned())
                    .w_full()
                    .min_w(px(0.0))
                    .tab_index(0)
                    .key_context(BLOCK_EDITOR_CONTEXT)
                    .track_focus(&structure_focus)
                    .on_click(move |_event, window, cx| {
                        if structured {
                            structure_focus.focus(window);
                        } else {
                            source_focus.focus(window);
                        }
                        cx.stop_propagation();
                    })
                    .on_action(cx.listener(Self::on_math_structure_delete_back))
                    .on_action(cx.listener(Self::on_math_structure_delete))
                    .on_action(cx.listener(Self::on_exit_code_block))
                    .on_key_down(cx.listener(Self::on_math_structure_key_down))
                    .child(visual),
            );
        root.child(self.render_math_palette_overlay(theme, viewport, cx))
            .into_any_element()
    }
}
