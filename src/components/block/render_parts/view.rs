// @author kongweiguang

use super::*;

/// The render method builds the full element tree for a block:
/// - Common wrapper: key_context, track_focus, action handlers, mouse events.
/// - Kind-specific styling: headings get size/weight/border, list items get
///   a flex row with marker + content, everything else renders as plain text.
/// - The [`BlockTextElement`] handles text layout, selection, and cursor.
impl Render for Block {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = !self.is_read_only() && self.focus_handle.is_focused(window);
        let code_language_focused =
            !self.is_read_only() && self.code_language_focus_handle.is_focused(window);
        let selection_link_focused = self
            .selection_toolbar_link_input
            .as_ref()
            .is_some_and(|input| input.read(cx).focus_handle.is_focused(window));
        let contextual_editing_focused = focused || selection_link_focused;
        if !code_language_focused {
            self.code_language_menu_open = false;
        }
        let input_active = focused || code_language_focused;
        if contextual_editing_focused {
            self.refresh_slash_menu(cx);
            self.refresh_selection_toolbar();
        } else {
            self.slash_menu = None;
            self.slash_menu_dismissed_query = None;
            self.selection_toolbar_dismissed_range = None;
            self.selection_toolbar_overflow_open = false;
            self.selection_toolbar_type_menu_open = false;
            self.selection_toolbar_link_input = None;
            self.selection_toolbar_link_range = None;
            self.selection_toolbar_link_had_target = false;
        }
        if self.sync_image_focus_state(focused) {
            cx.notify();
        }

        let showing_rendered_image = self.showing_rendered_image();
        // Inline math stays in the projected view while focused (its `$...$`
        // source shows as editable text), so links and other styling in the same
        // block keep their attributes instead of collapsing to raw Markdown, the
        // same way script spans already behave.
        self.sync_inline_projection_for_focus(
            contextual_editing_focused && !showing_rendered_image,
        );

        if input_active && self.cursor_blink_task.is_none() {
            self.start_cursor_blink(cx);
        } else if !input_active && self.cursor_blink_task.is_some() {
            self.cursor_blink_task = None;
        }
        if !input_active {
            self.reset_code_language_input_layout();
        }

        let block_id = ElementId::Name(format!("block-{}", self.record.id).into());
        // 显式输入框占位文案在失焦时也必须可见；普通文档空段落仍只在聚焦时显示提示。
        let input_placeholder = self.input_placeholder();
        let is_placeholder = self.display_text().is_empty()
            && self.marked_range.is_none()
            && (focused || input_placeholder.is_some());

        let theme = cx.global::<ThemeManager>().current_arc();
        let strings = cx.global::<I18nManager>().strings_arc();
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let content_inset = rendered_content_inset(d);
        let depth_padding = content_inset + d.nested_block_indent * self.render_depth as f32;

        if self.is_table_cell() {
            let is_header = self
                .table_cell_position()
                .map(|position| position.is_header())
                .unwrap_or(false);
            // The header row is only styled distinctly (shaded background, medium
            // weight) when the show-table-headers preference is enabled.
            let style_as_header =
                is_header && crate::config::EditorSettings::show_table_headers(cx);
            let highlight = self.table_axis_highlight;
            let base_bg = if style_as_header {
                c.table_header_bg
            } else {
                c.table_cell_bg
            };
            let bg = match highlight {
                TableAxisHighlight::None => base_bg,
                TableAxisHighlight::Preview => c.table_axis_preview_bg,
                TableAxisHighlight::Selected => c.table_axis_selected_bg,
            };
            let border_color = if focused {
                c.table_cell_active_outline
            } else {
                match highlight {
                    TableAxisHighlight::None => c.table_border,
                    TableAxisHighlight::Preview => c.table_axis_preview_bg,
                    TableAxisHighlight::Selected => c.table_axis_selected_bg,
                }
            };
            let cell_base = self
                .render_shell(
                    block_id,
                    false,
                    if showing_rendered_image {
                        CursorStyle::PointingHand
                    } else {
                        CursorStyle::IBeam
                    },
                    0.0,
                    0.0,
                    d,
                    cx,
                )
                .w_full()
                .h_full()
                .min_h(px(d.table_cell_min_height))
                .px(px(d.table_cell_padding_x))
                .py(px(d.table_cell_padding_y))
                .rounded(px(2.0))
                .border(px(1.0))
                .border_color(border_color)
                .bg(bg)
                .text_size(px(t.text_size))
                .text_color(c.text_default)
                .line_height(rems(t.text_line_height));

            let cell_base = if style_as_header {
                cell_base.font_weight(FontWeight::MEDIUM)
            } else {
                cell_base
            };

            if showing_rendered_image && let Some(mut runtime) = self.image_runtime().cloned() {
                let width_percent = self.current_image_width_percent();
                runtime.width_percent = width_percent;
                return cell_base
                    .child(self.render_image_content(
                        runtime,
                        Length::Definite(relative(f32::from(width_percent) / 100.0)),
                        px(d.image_cell_max_height),
                        px(d.image_cell_placeholder_height),
                        f32::from(window.viewport_size().width.max(px(1.0))),
                        &theme,
                        &strings,
                        cx,
                    ))
                    .into_any_element();
            }

            if !focused
                && let Some(inline_images) = self.render_table_cell_inline_images(
                    &theme,
                    &strings,
                    if style_as_header {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    },
                    cx,
                )
            {
                return cell_base.child(inline_images).into_any_element();
            }

            return cell_base
                .child(self.render_text_or_mixed_inline_visuals(
                    &theme,
                    focused,
                    is_placeholder,
                    self.input_placeholder(),
                    None,
                    c.text_default,
                    t.text_size,
                    if style_as_header {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    },
                    cx,
                ))
                .into_any_element();
        }

        // Frontmatter 在 Live/Preview 投影里虽沿用 RawMarkdown 的保真编辑能力，
        // 但失焦时仍需走专用元数据视觉；Source/Split 左栏是单一 Paragraph，不受影响。
        let rendered_frontmatter = self.record.is_yaml_frontmatter();
        let rendered_comment = self.kind() == BlockKind::Comment;

        // Source-mode rendering: raw text with no formatting.
        if self.is_source_raw_mode()
            && !rendered_frontmatter
            && !rendered_comment
            && (focused
                || !matches!(
                    self.kind(),
                    BlockKind::HtmlBlock | BlockKind::MathBlock | BlockKind::MermaidBlock
                ))
        {
            if focused && self.cursor_blink_task.is_none() {
                self.start_cursor_blink(cx);
            } else if !focused && self.cursor_blink_task.is_some() {
                self.cursor_blink_task = None;
            }
            let compact_source_host = self.compact_source_host();
            let source_text_size = self.host_text_size().unwrap_or(t.text_size);
            let source_padding = if compact_source_host {
                0.0
            } else {
                d.block_padding_x
            };
            let source_base = self
                .render_shell(
                    block_id.clone(),
                    true,
                    CursorStyle::IBeam,
                    source_padding,
                    source_padding,
                    d,
                    cx,
                )
                .when(compact_source_host, |base| {
                    base.min_h(px(0.0)).py(px(0.0)).rounded(px(0.0))
                })
                .font_family(crate::document_host::source_monospace_font_family())
                .text_size(px(source_text_size))
                .text_color(c.text_default)
                .line_height(rems(t.text_line_height));

            let source_base = if self.kind() == BlockKind::Comment {
                source_base.bg(c.comment_bg).rounded_sm()
            } else {
                source_base
            };

            let text = match input_placeholder {
                Some(placeholder) => BlockTextElement::with_placeholder(
                    cx.entity(),
                    is_placeholder,
                    placeholder,
                    Some(c.text_placeholder),
                ),
                None => BlockTextElement::new(cx.entity(), is_placeholder),
            };
            let source_editor = source_base.child(text);
            if focused && matches!(self.kind(), BlockKind::MathBlock | BlockKind::MermaidBlock) {
                let preview = match self.kind() {
                    BlockKind::MathBlock => self.render_math_content(&theme, cx),
                    BlockKind::MermaidBlock => self.render_mermaid_content(&theme, window, cx),
                    _ => unreachable!("guarded complex preview kind"),
                };
                return div()
                    .debug_selector(|| "complex-source-live-preview".to_owned())
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(source_editor)
                    .child(
                        div()
                            .debug_selector(|| "complex-source-live-preview-result".to_owned())
                            .w_full()
                            .border_t_1()
                            .border_color(c.dialog_border)
                            .pt(px(8.0))
                            .child(preview),
                    )
                    .into_any_element();
            }
            return source_editor.into_any_element();
        }

        let focused_base = self.render_shell(
            block_id.clone(),
            false,
            if showing_rendered_image {
                CursorStyle::PointingHand
            } else {
                CursorStyle::IBeam
            },
            depth_padding,
            content_inset,
            d,
            cx,
        );

        if self.kind() == BlockKind::Paragraph
            && crate::components::is_toc_marker(self.display_text())
            && !focused
        {
            let entries = self.toc_entries.clone();
            let rows = entries.iter().enumerate().map(|(index, entry)| {
                let target = entry.target;
                div()
                    .id(("toc-entry", index))
                    .w_full()
                    .pl(px(12.0 * f32::from(entry.level.saturating_sub(1))))
                    .py(px(2.0))
                    .cursor_pointer()
                    .text_color(c.text_link)
                    .hover(|this| this.bg(c.source_mode_block_bg))
                    .child(entry.title.clone())
                    .on_click(cx.listener(move |_block, _event, _window, cx| {
                        cx.emit(BlockEvent::RequestJumpToTocHeading { target });
                        cx.notify();
                    }))
            });
            let toc_surface = div()
                .debug_selector(|| "document-toc".to_owned())
                .w_full()
                .min_w(px(0.0))
                .py(px(8.0))
                .border_l(px(2.0))
                .border_color(c.text_link)
                .flex()
                .flex_col()
                .children(rows);
            return focused_base.child(toc_surface).into_any_element();
        }

        if showing_rendered_image && self.kind() == BlockKind::Paragraph {
            let viewport_width = f32::from(window.viewport_size().width.max(px(1.0)));
            let resize_basis_width = effective_image_width(self, viewport_width, d);
            if let Some(mut runtime) = self.image_runtime().cloned() {
                let width_percent = self.current_image_width_percent();
                runtime.width_percent = width_percent;
                // 相对父内容列定宽，工作区侧栏打开时也不会按整个窗口宽度溢出。
                let max_width = Length::Definite(relative(f32::from(width_percent) / 100.0));
                return focused_base
                    .child(self.render_image_content(
                        runtime,
                        max_width,
                        px(d.image_root_max_height),
                        px(d.image_root_placeholder_height),
                        resize_basis_width,
                        &theme,
                        &strings,
                        cx,
                    ))
                    .into_any_element();
            }
        }

        let content = match self.kind() {
            BlockKind::Separator => focused_base
                .debug_selector(|| "separator-shell".to_owned())
                .py(px(d.separator_margin_y))
                .flex()
                .items_center()
                .child(
                    div()
                        .debug_selector(|| "separator-surface".to_owned())
                        .flex_1()
                        .min_w(px(0.0))
                        .h(px(d.separator_thickness))
                        .bg(c.separator_color)
                        // GPUI 对远大于元素高度的圆角会生成越界几何；分隔线只需半高圆角。
                        .rounded(px(d.separator_thickness / 2.0)),
                )
                .into_any_element(),
            BlockKind::Heading {
                level: level @ 1..=6,
            } => self.render_heading_content(
                focused_base,
                focused,
                is_placeholder,
                level,
                &theme,
                cx,
            ),
            BlockKind::BulletedListItem => {
                let render_depth = self.render_depth;
                focused_base
                    .text_size(px(t.text_size))
                    .text_color(c.text_default)
                    .line_height(rems(t.text_line_height))
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(d.list_marker_gap))
                    .children([
                        div()
                            .debug_selector(move || {
                                bulleted_list_marker_slot_selector(render_depth).to_owned()
                            })
                            .w(px(d.list_marker_width))
                            .h(px(t.text_size * t.text_line_height))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(bulleted_list_marker(render_depth, c.text_default)),
                        if showing_rendered_image {
                            let viewport_width =
                                f32::from(window.viewport_size().width.max(px(1.0)));
                            let resize_basis_width =
                                effective_list_item_image_width(self, viewport_width, d);
                            if let Some(mut runtime) = self.image_runtime().cloned() {
                                let width_percent = self.current_image_width_percent();
                                runtime.width_percent = width_percent;
                                let max_width =
                                    Length::Definite(relative(f32::from(width_percent) / 100.0));
                                div()
                                    .min_w(px(0.0))
                                    .flex_grow()
                                    .child(self.render_image_content(
                                        runtime,
                                        max_width,
                                        px(d.image_root_max_height),
                                        px(d.image_root_placeholder_height),
                                        resize_basis_width,
                                        &theme,
                                        &strings,
                                        cx,
                                    ))
                            } else {
                                div().min_w(px(0.0)).flex_grow().child(
                                    self.render_text_or_mixed_inline_visuals(
                                        &theme,
                                        focused,
                                        is_placeholder,
                                        None,
                                        None,
                                        c.text_default,
                                        t.text_size,
                                        FontWeight::NORMAL,
                                        cx,
                                    ),
                                )
                            }
                        } else {
                            div().min_w(px(0.0)).flex_grow().child(
                                self.render_text_or_mixed_inline_visuals(
                                    &theme,
                                    focused,
                                    is_placeholder,
                                    None,
                                    None,
                                    c.text_default,
                                    t.text_size,
                                    FontWeight::NORMAL,
                                    cx,
                                ),
                            )
                        },
                    ])
                    .into_any_element()
            }
            BlockKind::TaskListItem { checked } => self.render_task_list_content(
                focused_base,
                focused,
                is_placeholder,
                checked,
                showing_rendered_image,
                &theme,
                &strings,
                window,
                cx,
            ),
            BlockKind::NumberedListItem => focused_base
                .text_size(px(t.text_size))
                .text_color(c.text_default)
                .line_height(rems(t.text_line_height))
                .w_full()
                .flex()
                .flex_row()
                .items_start()
                .gap(px(d.list_marker_gap))
                .children([
                    div()
                        .debug_selector(|| "numbered-list-marker-slot".to_owned())
                        .min_w(px(d.ordered_list_marker_width))
                        .child(SharedString::from(numbered_list_marker(
                            self.render_depth,
                            self.list_ordinal.unwrap_or(1),
                        ))),
                    if showing_rendered_image {
                        let viewport_width = f32::from(window.viewport_size().width.max(px(1.0)));
                        let resize_basis_width =
                            effective_list_item_image_width(self, viewport_width, d);
                        if let Some(mut runtime) = self.image_runtime().cloned() {
                            let width_percent = self.current_image_width_percent();
                            runtime.width_percent = width_percent;
                            let max_width =
                                Length::Definite(relative(f32::from(width_percent) / 100.0));
                            div()
                                .min_w(px(0.0))
                                .flex_grow()
                                .child(self.render_image_content(
                                    runtime,
                                    max_width,
                                    px(d.image_root_max_height),
                                    px(d.image_root_placeholder_height),
                                    resize_basis_width,
                                    &theme,
                                    &strings,
                                    cx,
                                ))
                        } else {
                            div().min_w(px(0.0)).flex_grow().child(
                                self.render_text_or_mixed_inline_visuals(
                                    &theme,
                                    focused,
                                    is_placeholder,
                                    None,
                                    None,
                                    c.text_default,
                                    t.text_size,
                                    FontWeight::NORMAL,
                                    cx,
                                ),
                            )
                        }
                    } else {
                        div().min_w(px(0.0)).flex_grow().child(
                            self.render_text_or_mixed_inline_visuals(
                                &theme,
                                focused,
                                is_placeholder,
                                None,
                                None,
                                c.text_default,
                                t.text_size,
                                FontWeight::NORMAL,
                                cx,
                            ),
                        )
                    },
                ])
                .into_any_element(),
            BlockKind::Quote => focused_base
                .text_size(px(t.text_size))
                .text_color(c.text_quote)
                .line_height(rems(t.text_line_height))
                .child(self.render_text_or_mixed_inline_visuals(
                    &theme,
                    focused,
                    is_placeholder,
                    None,
                    None,
                    c.text_quote,
                    t.text_size,
                    FontWeight::NORMAL,
                    cx,
                ))
                .into_any_element(),
            BlockKind::Callout(variant) => self.render_callout_content(
                focused_base,
                focused,
                is_placeholder,
                variant,
                &theme,
                cx,
            ),
            BlockKind::FootnoteDefinition => self.render_footnote_definition_content(
                focused_base,
                focused,
                is_placeholder,
                &theme,
                &strings,
                cx,
            ),
            BlockKind::CodeBlock { .. } => {
                self.render_code_block_content(focused_base, is_placeholder, &theme, &strings, cx)
            }
            BlockKind::Table => self.render_table_content(
                focused_base,
                focused,
                is_placeholder,
                &theme,
                &strings,
                window,
                cx,
            ),
            BlockKind::HtmlBlock => {
                let html = self.record.html.as_ref().cloned().unwrap_or_else(|| {
                    crate::components::parse_html_document(
                        self.record
                            .raw_fallback
                            .as_deref()
                            .unwrap_or_else(|| self.display_text()),
                    )
                });
                let html_surface = div()
                    .debug_selector(|| "rendered-html-surface".to_owned())
                    .w_full()
                    .min_w(px(0.0))
                    .text_size(px(t.text_size))
                    .text_color(c.text_default)
                    .line_height(rems(t.text_line_height))
                    .child(self.render_html_document(&html, &theme, cx));
                focused_base.child(html_surface).into_any_element()
            }
            BlockKind::MathBlock => {
                if !focused {
                    self.last_layout = None;
                    self.last_bounds = None;
                }
                let child = if focused {
                    BlockTextElement::new(cx.entity(), is_placeholder).into_any_element()
                } else {
                    self.render_math_content(&theme, cx)
                };
                focused_base.w_full().child(child).into_any_element()
            }
            BlockKind::MermaidBlock => {
                if !focused {
                    self.last_layout = None;
                    self.last_bounds = None;
                }
                let child = if focused {
                    BlockTextElement::new(cx.entity(), is_placeholder).into_any_element()
                } else {
                    self.render_mermaid_content(&theme, window, cx)
                };
                focused_base.w_full().child(child).into_any_element()
            }
            BlockKind::RawMarkdown if rendered_frontmatter => {
                let body = yaml_frontmatter_body(self.display_text()).unwrap_or_default();
                let lines = body
                    .split('\n')
                    .map(|line| {
                        div().w_full().child(SharedString::from(if line.is_empty() {
                            " ".to_owned()
                        } else {
                            line.to_owned()
                        }))
                    })
                    .collect::<Vec<_>>();
                let content = div()
                    .debug_selector(|| "yaml-frontmatter-rendered-body".to_owned())
                    .w_full()
                    .flex()
                    .flex_col()
                    .children(lines);
                let separator = div()
                    .debug_selector(|| "yaml-frontmatter-separator".to_owned())
                    .w_full()
                    .mt(px(8.0))
                    .border_b(px(1.0))
                    .border_dashed()
                    .border_color(c.text_placeholder.opacity(0.48));

                let frontmatter_surface = div()
                    .debug_selector(|| "yaml-frontmatter".to_owned())
                    .w_full()
                    .min_w(px(0.0))
                    .px(px(d.code_block_padding_x))
                    .py(px(d.code_block_padding_y))
                    .flex()
                    .flex_col()
                    .text_size(px(t.code_size))
                    .text_color(c.text_placeholder)
                    .line_height(rems(t.text_line_height))
                    .child(content)
                    .child(separator);
                focused_base.child(frontmatter_surface).into_any_element()
            }
            BlockKind::Comment => focused_base
                .debug_selector(|| "html-comment".to_owned())
                .text_size(px(t.code_size))
                .text_color(c.text_link.opacity(0.82))
                .line_height(rems(t.text_line_height))
                .child(BlockTextElement::new(cx.entity(), is_placeholder))
                .into_any_element(),
            BlockKind::Paragraph | BlockKind::RawMarkdown | BlockKind::Heading { .. } => {
                focused_base
                    .text_size(px(t.text_size))
                    .text_color(c.text_default)
                    .line_height(rems(t.text_line_height))
                    .child(self.render_text_or_mixed_inline_visuals(
                        &theme,
                        focused,
                        is_placeholder,
                        None,
                        None,
                        c.text_default,
                        t.text_size,
                        FontWeight::NORMAL,
                        cx,
                    ))
                    .into_any_element()
            }
        };

        let content = wrap_with_quote_guides(content, visible_quote_guides(self), &theme);
        let viewport = window.viewport_size();
        let selection_toolbar = contextual_editing_focused
            .then(|| self.render_selection_toolbar(&theme, viewport, cx))
            .flatten();
        let slash_menu = focused
            .then(|| self.render_slash_menu(&theme, &strings, viewport, cx))
            .flatten();
        let block_group: SharedString = format!("block-context-{}", self.record.id).into();
        let block_gutter =
            self.render_block_gutter(focused, block_group.clone(), &theme, &strings, cx);
        let drop_target = cx.entity().downgrade();
        let drop_border = theme.colors.text_link;
        let drop_placement = self.block_drop_placement;
        if selection_toolbar.is_some() || slash_menu.is_some() || block_gutter.is_some() {
            div()
                .group(block_group)
                .relative()
                .w_full()
                .drag_over::<BlockDragPayload>(move |style, _payload, _window, _cx| {
                    match drop_placement {
                        BlockDropPlacement::Before => style.border_t(px(2.0)),
                        BlockDropPlacement::After => style.border_b(px(2.0)),
                    }
                    .border_color(drop_border)
                })
                .on_mouse_move(cx.listener(|block, event: &MouseMoveEvent, _window, cx| {
                    if !event.dragging() {
                        return;
                    }
                    let Some(bounds) = block.last_bounds else {
                        return;
                    };
                    let placement = if event.position.y < bounds.center().y {
                        BlockDropPlacement::Before
                    } else {
                        BlockDropPlacement::After
                    };
                    if block.block_drop_placement != placement {
                        block.block_drop_placement = placement;
                        cx.notify();
                    }
                }))
                .on_drop(move |payload: &BlockDragPayload, _window, cx| {
                    let source = payload.source;
                    let _ = drop_target.update(cx, |block, cx| {
                        cx.emit(BlockEvent::RequestMoveBlock {
                            source,
                            placement: block.block_drop_placement,
                        });
                    });
                })
                .child(content)
                .children(block_gutter)
                .children(selection_toolbar)
                .children(slash_menu)
                .into_any_element()
        } else {
            content
        }
    }
}
