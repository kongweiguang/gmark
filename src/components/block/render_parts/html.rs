// @author kongweiguang

use super::*;

fn preview_key(source: &str, parameters: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    parameters.hash(&mut hasher);
    hasher.finish()
}

impl Block {
    pub(super) fn on_html_details_toggle_mouse_down(
        &mut self,
        _: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.html_details_open = !self.html_details_open;
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn render_image_content(
        &self,
        runtime: ImageRuntime,
        max_width: Length,
        max_height: Pixels,
        placeholder_height: Pixels,
        resize_basis_width: f32,
        theme: &Theme,
        strings: &I18nStrings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let source = runtime.resolved_source.clone();
        let placeholder_theme = theme.clone();
        let loading_theme = theme.clone();
        let placeholder_strings = strings.clone();
        let loading_strings = strings.clone();
        let runtime_for_fallback = runtime.clone();
        let runtime_for_loading = runtime.clone();

        let image = match source {
            ImageResolvedSource::Local(path) => img(path),
            ImageResolvedSource::Remote(uri) => img(uri),
        }
        .w_full()
        .max_w(Length::Definite(relative(1.0)))
        .max_h(max_height)
        .object_fit(ObjectFit::Contain)
        .with_fallback(move || {
            render_image_placeholder(
                &runtime_for_fallback,
                Length::Definite(relative(1.0)),
                placeholder_height,
                &placeholder_theme,
                &placeholder_strings,
            )
        })
        .with_loading(move || {
            render_loading_placeholder(
                &runtime_for_loading,
                Length::Definite(relative(1.0)),
                placeholder_height,
                &loading_theme,
                &loading_strings,
            )
        });

        let selected = self.image_selected && !self.is_read_only();
        let resize_tooltip: SharedString = strings.image_resize.clone().into();
        let mut image_frame = div()
            .id("rendered-image-frame")
            .debug_selector(|| "rendered-image-frame".to_owned())
            .relative()
            .w(max_width)
            .max_w(Length::Definite(relative(1.0)))
            .child(image);
        if selected {
            image_frame = image_frame
                .border(px(1.0))
                .border_color(c.table_cell_active_outline)
                .rounded(px(3.0))
                .child(
                    div()
                        .id("image-resize-handle")
                        .debug_selector(|| "image-resize-handle".to_owned())
                        .absolute()
                        .right_0()
                        .bottom_0()
                        .w(px(14.0))
                        .h(px(14.0))
                        .rounded(px(3.0))
                        .border(px(2.0))
                        .border_color(c.editor_background)
                        .bg(c.table_cell_active_outline)
                        .cursor(CursorStyle::PointingHand)
                        .tooltip(move |_window, cx| {
                            crate::ui::ui_tooltip(resize_tooltip.clone(), cx)
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |block, event: &MouseDownEvent, _window, cx| {
                                block.start_image_resize(event.position.x, resize_basis_width, cx);
                                cx.stop_propagation();
                            }),
                        ),
                );
        }

        let mut container = div()
            .debug_selector(|| "rendered-image-content".to_owned())
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(d.image_caption_gap))
            .child(image_frame);

        if let Some(title) = runtime
            .title
            .as_ref()
            .filter(|title| !title.trim().is_empty())
        {
            container = container.child(
                div()
                    .w_full()
                    .text_center()
                    .text_size(px(t.code_size))
                    .text_color(c.image_caption_text)
                    .child(SharedString::from(title.clone())),
            );
        }

        container.into_any_element()
    }

    pub(super) fn render_math_content(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let raw = self
            .record
            .raw_fallback
            .as_deref()
            .unwrap_or_else(|| self.display_text())
            .to_string();
        let text_color = c.text_default;
        let font_size = display_math_font_size(t.text_size);
        let key = preview_key(&raw, (format!("{text_color:?}"), font_size.to_bits()));
        if self.math_preview_key != Some(key) {
            self.math_preview_key = Some(key);
            self.math_render_error = None;
            let source = raw.clone();
            self.math_preview_task = Some(cx.spawn(
                async move |this: WeakEntity<Block>, cx: &mut AsyncApp| {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    let result = cx
                        .background_spawn(async move {
                            parse_display_math_source(&source)
                                .ok_or_else(|| "invalid display math source".to_owned())
                                .and_then(|parsed| {
                                    render_display_math_svg(&parsed, text_color, font_size)
                                        .map_err(|error| error.to_string())
                                })
                        })
                        .await;
                    let _ = this.update(cx, |block, cx| {
                        if block.math_preview_key != Some(key) {
                            return;
                        }
                        block.math_preview_task = None;
                        match result {
                            Ok(rendered) => {
                                block.last_successful_math_render = Some(rendered);
                                block.math_render_error = None;
                            }
                            Err(error) => block.math_render_error = Some(error),
                        }
                        cx.notify();
                    });
                },
            ));
        }

        match (
            self.last_successful_math_render.as_ref(),
            self.math_render_error.as_ref(),
        ) {
            (Some(rendered), None) => render_math_svg_content(rendered, theme),
            (Some(rendered), Some(error)) => div()
                .id("math-render-fallback")
                .debug_selector(|| "math-render-fallback".to_owned())
                .w_full()
                .min_w(px(0.0))
                .max_w(Length::Definite(relative(1.0)))
                .overflow_hidden()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(render_math_svg_content(rendered, theme))
                .child(render_complex_warning(
                    format!("LaTeX render error: {error}"),
                    theme,
                    "math-render-warning",
                ))
                .into_any_element(),
            (None, Some(error)) => div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .rounded_sm()
                .bg(c.source_mode_block_bg)
                .px(px(d.block_padding_x))
                .py(px(d.block_padding_y))
                .text_size(px(t.text_size))
                .line_height(rems(t.text_line_height))
                .text_color(c.text_default)
                .child(SharedString::from(raw))
                .child(render_complex_warning(
                    format!("LaTeX render error: {error}"),
                    theme,
                    "math-render-warning",
                ))
                .into_any_element(),
            (None, None) => div()
                .id("math-render-pending")
                .debug_selector(|| "math-render-pending".to_owned())
                .w_full()
                .min_h(px(64.0))
                .rounded_sm()
                .bg(c.source_mode_block_bg)
                .into_any_element(),
        }
    }

    pub(super) fn render_mermaid_content(
        &mut self,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let d = &theme.dimensions;
        let t = &theme.typography;
        let raw = self
            .record
            .raw_fallback
            .as_deref()
            .unwrap_or_else(|| self.display_text())
            .to_string();

        let viewport_width = f32::from(window.viewport_size().width.max(px(1.0)));
        let available_width = effective_image_width(self, viewport_width, d);
        let theme_mode = MermaidThemeMode::from_theme(theme);
        let key = preview_key(
            &raw,
            (
                available_width.to_bits(),
                viewport_width.to_bits(),
                theme_mode,
            ),
        );
        if self.mermaid_preview_key != Some(key) {
            self.mermaid_preview_key = Some(key);
            let source = raw.clone();
            self.mermaid_preview_task = Some(cx.spawn(
                async move |this: WeakEntity<Block>, cx: &mut AsyncApp| {
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                    let result = cx
                        .background_spawn(async move {
                            parse_mermaid_fence_source(&source)
                                .ok_or_else(|| "invalid Mermaid fence source".to_owned())
                                .and_then(|parsed| {
                                    render_mermaid_svg_for_display(
                                        &parsed,
                                        available_width,
                                        viewport_width,
                                        theme_mode,
                                    )
                                    .map_err(|error| error.to_string())
                                })
                        })
                        .await;
                    let _ = this.update(cx, |block, cx| {
                        if block.mermaid_preview_key != Some(key) {
                            return;
                        }
                        block.mermaid_preview_task = None;
                        match result {
                            Ok(rendered) => {
                                block.last_successful_mermaid_render = Some(rendered);
                                block.mermaid_render_error = None;
                            }
                            Err(error) => block.mermaid_render_error = Some(error),
                        }
                        cx.notify();
                    });
                },
            ));
        }
        let content = match (
            self.last_successful_mermaid_render.as_ref(),
            self.mermaid_render_error.as_ref(),
        ) {
            (Some(rendered), None) => render_mermaid_svg_content(rendered, d.block_padding_y),
            (Some(rendered), Some(error)) => div()
                .id("mermaid-render-fallback")
                .debug_selector(|| "mermaid-render-fallback".to_owned())
                .w_full()
                .min_w(px(0.0))
                .max_w(Length::Definite(relative(1.0)))
                .overflow_hidden()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(render_mermaid_svg_content(rendered, d.block_padding_y))
                .child(render_complex_warning(
                    format!("Mermaid render error: {error}"),
                    theme,
                    "mermaid-render-warning",
                ))
                .into_any_element(),
            (None, Some(error)) => div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .rounded_sm()
                .bg(c.source_mode_block_bg)
                .px(px(d.block_padding_x))
                .py(px(d.block_padding_y))
                .text_size(px(t.text_size))
                .line_height(rems(t.text_line_height))
                .text_color(c.text_default)
                .child(SharedString::from(raw))
                .child(render_complex_warning(
                    format!("Mermaid render error: {error}"),
                    theme,
                    "mermaid-render-warning",
                ))
                .into_any_element(),
            (None, None) => div()
                .id("mermaid-render-pending")
                .debug_selector(|| "mermaid-render-pending".to_owned())
                .w_full()
                .min_h(px(96.0))
                .rounded_sm()
                .bg(c.source_mode_block_bg)
                .into_any_element(),
        };
        let Some(rendered) = self.last_successful_mermaid_render.clone() else {
            return content;
        };
        let preview_key = key;
        div()
            .relative()
            .w_full()
            .min_w(px(0.0))
            .max_w(Length::Definite(relative(1.0)))
            .overflow_hidden()
            .child(content)
            .child(
                div()
                    .id("mermaid-open-overlay")
                    .debug_selector(|| "mermaid-open-overlay".to_owned())
                    .absolute()
                    .top(px(6.0))
                    .right(px(6.0))
                    .size(px(30.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .bg(c.dialog_surface)
                    .border(px(1.0))
                    .border_color(c.dialog_border)
                    .text_color(c.text_link)
                    .cursor_pointer()
                    .hover(|this| this.bg(c.chrome_hover))
                    .child("↗")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_block, _event, _window, cx| {
                            cx.emit(BlockEvent::RequestOpenMermaidOverlay {
                                preview_key,
                                rendered: rendered.clone(),
                            });
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_text_or_mixed_inline_visuals(
        &self,
        theme: &Theme,
        focused: bool,
        is_placeholder: bool,
        placeholder_text: Option<SharedString>,
        placeholder_color: Option<Hsla>,
        text_color: Hsla,
        font_size: f32,
        font_weight: FontWeight,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Mixed inline visuals are display-only. Once focused, the text element
        // takes over so caret movement, projection markers, and IME ranges stay
        // anchored to editable text rather than rendered SVG/script offsets.
        if focused || is_placeholder || !self.has_mixed_inline_visuals() {
            return match placeholder_text {
                Some(placeholder) => BlockTextElement::with_placeholder(
                    cx.entity(),
                    is_placeholder,
                    placeholder,
                    placeholder_color,
                )
                .into_any_element(),
                None => BlockTextElement::new(cx.entity(), is_placeholder).into_any_element(),
            };
        }

        self.render_mixed_inline_visual_runs(theme, text_color, font_size, font_weight, cx)
    }

    pub(super) fn render_mixed_inline_visual_runs(
        &self,
        theme: &Theme,
        base_color: Hsla,
        font_size: f32,
        font_weight: FontWeight,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_inline_tree_runs(
            &self.record.title,
            theme,
            base_color,
            font_size,
            font_weight,
            cx,
        )
    }

    pub(super) fn render_inline_tree_runs(
        &self,
        tree: &crate::components::InlineTextTree,
        theme: &Theme,
        base_color: Hsla,
        font_size: f32,
        font_weight: FontWeight,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(0.0))
            .text_size(px(font_size))
            .line_height(rems(theme.typography.text_line_height))
            .children(self.render_inline_tree_children(
                tree,
                theme,
                base_color,
                font_size,
                font_weight,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_inline_tree_children(
        &self,
        tree: &crate::components::InlineTextTree,
        theme: &Theme,
        base_color: Hsla,
        font_size: f32,
        font_weight: FontWeight,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let cache = tree.render_cache();
        let text = cache.visible_text();
        let mut children = Vec::new();
        let mut cursor = 0usize;

        for span in cache.spans() {
            if cursor < span.range.start {
                let fallback_span = crate::components::InlineSpan {
                    range: cursor..span.range.start,
                    style: crate::components::InlineStyle::default(),
                    html_style: None,
                    link: None,
                    footnote: None,
                    math: None,
                };
                children.extend(self.render_inline_text_word_segments(
                    &text[cursor..span.range.start],
                    &fallback_span,
                    theme,
                    base_color,
                    font_size,
                    font_weight,
                    cx,
                ));
            }

            let span_text = &text[span.range.clone()];
            if let Some(math) = span.math.as_ref() {
                children.push(
                    self.render_inline_math_segment(math, span, theme, base_color, font_size, cx),
                );
            } else {
                children.extend(self.render_inline_text_word_segments(
                    span_text,
                    span,
                    theme,
                    base_color,
                    font_size,
                    font_weight,
                    cx,
                ));
            }
            cursor = span.range.end;
        }

        if cursor < text.len() {
            let fallback_span = crate::components::InlineSpan {
                range: cursor..text.len(),
                style: crate::components::InlineStyle::default(),
                html_style: None,
                link: None,
                footnote: None,
                math: None,
            };
            children.extend(self.render_inline_text_word_segments(
                &text[cursor..],
                &fallback_span,
                theme,
                base_color,
                font_size,
                font_weight,
                cx,
            ));
        }

        children
    }

    /// Split a styled text run into wrap-friendly word segments. The mixed
    /// inline-visual layout is a `flex_wrap` row, so a long run rendered as one
    /// element wraps internally and claims the full row width, pushing the next
    /// item (inline math, a script, ...) onto its own line. Emitting one element
    /// per whitespace-delimited word lets the row break between words and keeps
    /// adjacent visuals on the same visual line. Inline code and background
    /// highlights stay a single element so their pill/background is continuous.
    pub(super) fn render_inline_text_word_segments(
        &self,
        text: &str,
        span: &crate::components::InlineSpan,
        theme: &Theme,
        base_color: Hsla,
        font_size: f32,
        font_weight: FontWeight,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let has_background = span
            .html_style
            .is_some_and(|style| style.background_color.is_some());
        let mut segments = Vec::new();
        for word in inline_word_chunks(text, span.style.code, has_background) {
            segments.push(self.render_inline_text_segment(
                word,
                span,
                theme,
                base_color,
                font_size,
                font_weight,
                cx,
            ));
        }
        segments
    }

    pub(super) fn render_inline_text_segment(
        &self,
        text: &str,
        span: &crate::components::InlineSpan,
        theme: &Theme,
        base_color: Hsla,
        font_size: f32,
        font_weight: FontWeight,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if text.is_empty() {
            return div().into_any_element();
        }

        let mut color = if span.link.is_some() || span.footnote.is_some() {
            theme.colors.text_link
        } else {
            base_color
        };
        if let Some(style) = span.html_style
            && let Some(html_color) = style.color
        {
            color = html_css_color_to_hsla(html_color, color);
        }

        let script_offset = match span.style.script {
            InlineScript::Normal => 0.0,
            InlineScript::Superscript => -font_size * 0.28,
            InlineScript::Subscript => font_size * 0.22,
        };
        let display_font_size = if span.style.has_script() {
            (font_size * 0.72).max(6.0)
        } else {
            font_size
        };

        let mut element = div()
            .min_w(px(0.0))
            .text_size(px(display_font_size))
            .line_height(rems(theme.typography.text_line_height))
            .text_color(color)
            .font_weight(if span.style.bold {
                FontWeight::BOLD
            } else {
                font_weight
            })
            .child(SharedString::from(text.to_string()));

        if script_offset != 0.0 {
            element = element.relative().top(px(script_offset));
        }

        if span.style.underline || span.link.is_some() || span.footnote.is_some() {
            element = element.underline();
        }
        if span.style.code {
            element = element
                .rounded(px(theme.dimensions.code_bg_radius))
                .px(px(theme.dimensions.code_bg_pad_x))
                .py(px(theme.dimensions.code_bg_pad_y))
                .bg(theme.colors.code_bg);
        }
        if let Some(style) = span.html_style
            && let Some(background) = style.background_color
        {
            element = element
                .rounded(px(3.0))
                .px(px(2.0))
                .bg(html_css_color_to_hsla(background, color));
        }

        // This run renders as plain (non-interactive) text, so a link inside a
        // mixed inline-visual block (alongside math or a script) would otherwise
        // have no way to be followed. Attach the open-link handlers directly to
        // the segment; they act only on Cmd/Ctrl+click so a plain click still
        // falls through and focuses the block for editing. The wrapper element
        // gates the hand cursor on that same modifier, matching the normal-text
        // path where links render through `BlockTextElement`.
        if let Some(link) = span.link.clone() {
            let element = element
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(Self::on_rendered_link_mouse_down),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |block, event: &MouseUpEvent, _window, cx| {
                        if event.modifiers.secondary() {
                            block.open_rendered_link(&link, cx);
                        }
                    }),
                );
            return LinkFollowCursor {
                child: element.into_any_element(),
            }
            .into_any_element();
        }

        element.into_any_element()
    }

    pub(super) fn render_inline_math_segment(
        &self,
        math: &crate::components::InlineMath,
        span: &crate::components::InlineSpan,
        theme: &Theme,
        base_color: Hsla,
        font_size: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut color = base_color;
        if let Some(style) = span.html_style
            && let Some(html_color) = style.color
        {
            color = html_css_color_to_hsla(html_color, color);
        }
        let math_size = inline_math_font_size(font_size);
        match render_inline_math_svg(&math.body, color, math_size) {
            Ok(rendered) => div()
                .flex()
                .items_center()
                .h(px(math_size * 1.65))
                .child(
                    img(rendered.path)
                        .max_h(px(math_size * 1.65))
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element(),
            Err(_) => self.render_inline_text_segment(
                &math.source,
                span,
                theme,
                base_color,
                font_size,
                FontWeight::NORMAL,
                cx,
            ),
        }
    }
}
