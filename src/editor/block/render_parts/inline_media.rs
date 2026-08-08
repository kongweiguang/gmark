// @author kongweiguang

use super::*;

impl Block {
    pub(super) fn render_inline_image_content(
        &self,
        runtime: &ImageRuntime,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let d = &theme.dimensions;
        let source = runtime.resolved_source.clone();
        let max_height = px(d.image_cell_placeholder_height);
        let max_width =
            Length::Definite(px((d.image_cell_placeholder_height * 1.6).max(48.0)).into());
        let placeholder_theme = theme.clone();
        let loading_theme = theme.clone();
        let placeholder_strings = strings.clone();
        let loading_strings = strings.clone();
        let runtime_for_fallback = runtime.clone();
        let runtime_for_loading = runtime.clone();
        let retry_block = cx.entity().downgrade();

        let managed_local = matches!(source, ImageResolvedSource::Local(_))
            && runtime.local_asset_state().is_some();
        let image = if managed_local {
            if let Some(render_image) = runtime
                .asset_state
                .last_good()
                .and_then(crate::editor::render_asset_manager::AssetValue::render_image)
            {
                img(render_image)
                    .max_w(max_width)
                    .max_h(max_height)
                    .object_fit(ObjectFit::Contain)
                    .into_any_element()
            } else if runtime.asset_state.error_message().is_some()
                && !runtime.asset_state.is_loading()
            {
                render_image_placeholder_with_retry(
                    runtime,
                    max_width,
                    max_height,
                    &placeholder_theme,
                    &placeholder_strings,
                    retry_block.clone(),
                )
            } else {
                render_loading_placeholder(
                    runtime,
                    max_width,
                    max_height,
                    &loading_theme,
                    &loading_strings,
                )
            }
        } else {
            match source {
                ImageResolvedSource::Local(path) => img(path),
                ImageResolvedSource::Remote(uri) => img(uri),
            }
            .max_w(max_width)
            .max_h(max_height)
            .object_fit(ObjectFit::Contain)
            .with_fallback(move || {
                render_image_placeholder_with_retry(
                    &runtime_for_fallback,
                    max_width,
                    max_height,
                    &placeholder_theme,
                    &placeholder_strings,
                    retry_block.clone(),
                )
            })
            .with_loading(move || {
                render_loading_placeholder(
                    &runtime_for_loading,
                    max_width,
                    max_height,
                    &loading_theme,
                    &loading_strings,
                )
            })
            .into_any_element()
        };

        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .child(image)
            .into_any_element()
    }

    pub(super) fn render_table_cell_inline_images(
        &self,
        theme: &Theme,
        strings: &I18nStrings,
        font_weight: FontWeight,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let segments = parse_table_cell_inline_images(&self.record.title.serialize_markdown());
        if !segments
            .iter()
            .any(|segment| matches!(segment, TableCellInlineImageSegment::Image { .. }))
        {
            return None;
        }

        let mut children = Vec::new();
        for segment in segments {
            match segment {
                TableCellInlineImageSegment::Text(text) => {
                    if text.is_empty() {
                        continue;
                    }
                    let tree = self.inline_tree_from_markdown_with_context(&text);
                    children.extend(self.render_inline_tree_children(
                        &tree,
                        theme,
                        theme.colors.text_default,
                        theme.typography.text_size,
                        font_weight,
                        cx,
                    ));
                }
                TableCellInlineImageSegment::Image { markdown, syntax } => {
                    if let Some(runtime) = self.image_runtime_for_syntax(syntax) {
                        children
                            .push(self.render_inline_image_content(&runtime, theme, strings, cx));
                    } else {
                        let tree = crate::components::InlineTextTree::plain(markdown);
                        children.extend(self.render_inline_tree_children(
                            &tree,
                            theme,
                            theme.colors.text_default,
                            theme.typography.text_size,
                            font_weight,
                            cx,
                        ));
                    }
                }
            }
        }

        Some(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.0))
                .text_size(px(theme.typography.text_size))
                .line_height(rems(theme.typography.text_line_height))
                .children(children)
                .into_any_element(),
        )
    }

    pub(super) fn render_shell(
        &self,
        block_id: ElementId,
        source_mode: bool,
        cursor_style: CursorStyle,
        padding_left: f32,
        padding_right: f32,
        dimensions: &ThemeDimensions,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let base = div()
            .id(block_id)
            .w_full()
            .min_w(px(0.0))
            .flex_shrink_0()
            .min_h(px(dimensions.block_min_height))
            .py(px(dimensions.block_padding_y))
            .pl(px(padding_left))
            .pr(px(padding_right))
            .cursor(cursor_style);

        if self.is_read_only() {
            return base
                // Preview 只禁止正文变更，不应同时失去文本焦点、选择和复制能力。
                .key_context(BLOCK_EDITOR_CONTEXT)
                .track_focus(&self.focus_handle)
                .on_action(cx.listener(Self::on_select_all))
                .on_action(cx.listener(Self::on_copy))
                .on_action(cx.listener(Self::on_copy_as_markdown))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(Self::on_read_only_mouse_down),
                )
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_read_only_mouse_up))
                .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_read_only_mouse_up))
                .on_mouse_move(cx.listener(Self::on_mouse_move));
        }

        let base = base
            .key_context(BLOCK_EDITOR_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_newline))
            .on_action(cx.listener(Self::on_delete_back))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_word_delete_back))
            .on_action(cx.listener(Self::on_word_delete_forward))
            .on_action(cx.listener(Self::on_focus_prev))
            .on_action(cx.listener(Self::on_focus_next))
            .on_action(cx.listener(Self::on_move_left))
            .on_action(cx.listener(Self::on_move_right))
            .on_action(cx.listener(Self::on_word_move_left))
            .on_action(cx.listener(Self::on_word_move_right))
            .on_action(cx.listener(Self::on_home))
            .on_action(cx.listener(Self::on_end))
            .on_action(cx.listener(Self::on_block_up))
            .on_action(cx.listener(Self::on_block_down))
            .on_action(cx.listener(Self::on_select_left))
            .on_action(cx.listener(Self::on_select_right))
            .on_action(cx.listener(Self::on_word_select_left))
            .on_action(cx.listener(Self::on_word_select_right))
            .on_action(cx.listener(Self::on_select_home))
            .on_action(cx.listener(Self::on_select_end))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_copy_as_markdown))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_action(cx.listener(Self::on_paste_as_plain_text))
            .on_action(cx.listener(Self::on_exit_code_block))
            .on_key_down(cx.listener(Self::on_block_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move));

        let base = base.when(self.has_host_action_handler(), |base| {
            base.on_action(cx.listener(Self::on_host_save))
                .on_action(cx.listener(Self::on_host_undo))
                .on_action(cx.listener(Self::on_host_redo))
                .on_action(cx.listener(Self::on_host_find))
                .on_action(cx.listener(Self::on_host_find_next))
                .on_action(cx.listener(Self::on_host_find_previous))
                .on_action(cx.listener(Self::on_host_go_to_line))
                .on_action(cx.listener(Self::on_host_page_up))
                .on_action(cx.listener(Self::on_host_page_down))
                .on_action(cx.listener(Self::on_host_jump_to_top))
                .on_action(cx.listener(Self::on_host_jump_to_bottom))
                .on_action(cx.listener(Self::on_host_dismiss))
        });
        let base = base.when(self.host_submit_enabled(), |base| {
            base.on_action(cx.listener(Self::on_host_submit))
        });

        if source_mode {
            base
        } else {
            base.on_action(cx.listener(Self::on_indent_block))
                .on_action(cx.listener(Self::on_outdent_block))
                .on_action(cx.listener(Self::on_bold_selection))
                .on_action(cx.listener(Self::on_italic_selection))
                .on_action(cx.listener(Self::on_strikethrough_selection))
                .on_action(cx.listener(Self::on_underline_selection))
                .on_action(cx.listener(Self::on_highlight_selection))
                .on_action(cx.listener(Self::on_superscript_selection))
                .on_action(cx.listener(Self::on_subscript_selection))
                .on_action(cx.listener(Self::on_inline_math_selection))
                .on_action(cx.listener(Self::on_code_selection))
                .on_action(cx.listener(Self::on_link_selection))
        }
    }
}
