// @author kongweiguang

use super::html_text_flow::{
    HtmlFlowStyle, HtmlTextFlowElement, append_html_flow_node, html_node_is_flow_inline,
};
use super::*;
use crate::components::InlineLinkHit;

impl Block {
    pub(super) fn render_html_block(
        &mut self,
        focused_base: Stateful<Div>,
        text_size: f32,
        _is_placeholder: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        #[cfg(feature = "native-html-render")]
        {
            let Some(html) = self.record.html.as_ref().cloned() else {
                // Parsing and sanitizing belong to the projection/update path.
                // A render pass must never turn a missing cache into a
                // synchronous HTML parse or expose a stale derived tree.
                let raw_text = div()
                    .debug_selector(|| "html-raw-text".to_owned())
                    .w_full()
                    .text_size(px(theme.typography.code_size))
                    .text_color(c.text_default)
                    .line_height(rems(theme.typography.text_line_height))
                    .child(BlockTextElement::new(cx.entity(), _is_placeholder));
                return focused_base.child(raw_text).into_any_element();
            };
            self.sync_html_details_state(&html.raw_source);
            let html_surface = div()
                .debug_selector(|| "rendered-html-surface".to_owned())
                .w_full()
                .min_w(px(0.0))
                .text_size(px(text_size))
                .text_color(c.text_default)
                .line_height(rems(theme.typography.text_line_height))
                .child(self.render_html_document(&html, theme, cx));
            focused_base.child(html_surface).into_any_element()
        }
        #[cfg(not(feature = "native-html-render"))]
        {
            let html_raw_text = div()
                .debug_selector(|| "html-raw-text".to_owned())
                .w_full()
                .text_size(px(theme.typography.code_size))
                .text_color(c.text_default)
                .line_height(rems(theme.typography.text_line_height))
                .child(BlockTextElement::new(cx.entity(), _is_placeholder));
            focused_base.child(html_raw_text).into_any_element()
        }
    }

    pub(super) fn render_html_document(
        &self,
        document: &HtmlDocument,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let wb = &c.workbench;
        let d = &theme.dimensions;
        let t = &theme.typography;
        if !document.is_semantic() {
            let mut fallback = div()
                .w_full()
                .rounded_sm()
                .bg(wb.solid_surface)
                .px(px(d.block_padding_x))
                .py(px(d.block_padding_y))
                .text_size(px(t.code_size))
                .text_color(c.text_default)
                .child(SharedString::from(document.raw_source.clone()));
            if !document.diagnostics.is_empty() {
                fallback = fallback.child(render_complex_warning(
                    "HTML 内容已被阻止或降级为源码".to_owned(),
                    theme,
                    "html-render-warning",
                ));
            }
            return fallback.into_any_element();
        }

        let mut children = Vec::with_capacity(document.nodes.len() + 1);
        if !document.diagnostics.is_empty() {
            children.push(render_complex_warning(
                "部分 HTML 内容已被阻止或忽略".to_owned(),
                theme,
                "html-render-warning",
            ));
        }
        children.extend(self.render_html_children(
            &document.nodes,
            theme,
            HtmlComputedStyle::root(theme),
            cx,
        ));
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(d.block_gap * 0.4))
            .children(children)
            .into_any_element()
    }

    /// Groups adjacent inline HTML nodes into one shaped text element.  A
    /// block may still contain images or nested block nodes, so the helper
    /// returns a small sequence of flow elements and structural siblings.
    fn render_html_children(
        &self,
        children: &[HtmlNode],
        theme: &Theme,
        inherited_style: HtmlComputedStyle,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rendered = Vec::new();
        let mut inline_group = Vec::new();
        for child in children {
            if html_node_is_flow_inline(child) {
                inline_group.push(child);
            } else {
                if !inline_group.is_empty() {
                    rendered.push(self.render_html_inline_flow(
                        &inline_group,
                        theme,
                        inherited_style,
                        cx,
                    ));
                    inline_group.clear();
                }
                rendered.push(self.render_html_node(child, theme, inherited_style, cx));
            }
        }
        if !inline_group.is_empty() {
            rendered.push(self.render_html_inline_flow(&inline_group, theme, inherited_style, cx));
        }
        rendered
    }

    fn render_html_inline_flow(
        &self,
        nodes: &[&HtmlNode],
        theme: &Theme,
        inherited_style: HtmlComputedStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let initial_style = HtmlFlowStyle::root(inherited_style);
        let mut text = String::new();
        let mut runs = Vec::new();
        let mut links = Vec::new();
        for node in nodes {
            append_html_flow_node(node, initial_style, theme, &mut text, &mut runs, &mut links);
        }
        let flow = HtmlTextFlowElement::new(
            SharedString::from(text),
            runs,
            inherited_style.font_size,
            theme.typography.text_line_height,
        );
        if links.is_empty() {
            return flow.into_any_element();
        }

        let ranges = links.iter().map(|(range, _)| range.clone()).collect();
        let targets = links
            .into_iter()
            .map(|(_, href)| InlineLinkHit {
                prompt_target: href.clone(),
                open_target: href,
            })
            .collect::<Vec<_>>();
        let block = cx.entity().downgrade();
        flow.with_link_listener(ranges, move |index, _window, cx| {
            let Some(link) = targets.get(index).cloned() else {
                return;
            };
            let _ = block.update(cx, |block, cx| block.open_rendered_link(&link, cx));
        })
        .into_any_element()
    }

    pub(super) fn render_html_node(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        inherited_style: HtmlComputedStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &theme.colors;
        let wb = &c.workbench;
        let d = &theme.dimensions;
        let t = &theme.typography;

        if node.kind == HtmlNodeKind::RawTextBlock {
            return div()
                .w_full()
                .rounded_sm()
                .bg(wb.solid_surface)
                .px(px(d.block_padding_x * 0.6))
                .py(px(d.block_padding_y * 0.6))
                .text_size(px(t.code_size))
                .text_color(c.text_default)
                .child(SharedString::from(node.raw_source.clone()))
                .into_any_element();
        }

        if node.tag_name == "#text" {
            return self.render_html_inline_flow(
                std::slice::from_ref(&node),
                theme,
                inherited_style,
                cx,
            );
        }

        let node_style = html_node_visual_style(node, inherited_style, theme);
        match node.tag_name.as_str() {
            "strong" | "b" => {
                self.render_html_inline_container(node, theme, node_style, FontWeight::BOLD, cx)
            }
            "em" | "i" | "span" | "abbr" | "dfn" | "time" | "u" | "ins" | "del" | "small"
            | "sup" | "sub" => {
                self.render_html_inline_container(node, theme, node_style, FontWeight::NORMAL, cx)
            }
            "a" => self.render_html_link(node, theme, node_style, cx),
            "mark" => {
                self.render_html_inline_container(node, theme, node_style, FontWeight::NORMAL, cx)
            }
            "code" | "kbd" => self.render_html_inline_flow(
                std::slice::from_ref(&node),
                theme,
                node_style.computed,
                cx,
            ),
            "q" => self.render_html_inline_flow(
                std::slice::from_ref(&node),
                theme,
                node_style.computed,
                cx,
            ),
            "br" => div().child("\n").into_any_element(),
            "hr" => div()
                .w_full()
                .h(px(d.separator_thickness))
                .my(px(d.separator_margin_y))
                .bg(c.separator_color)
                .rounded(px(999.0))
                .into_any_element(),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = node.tag_name.as_bytes().get(1).copied().unwrap_or(b'6') - b'0';
                let scale = match level {
                    1 => 1.8,
                    2 => 1.55,
                    3 => 1.35,
                    4 => 1.2,
                    5 => 1.08,
                    _ => 1.0,
                };
                let mut element = div()
                    .w_full()
                    .font_weight(FontWeight::BOLD)
                    .text_size(px((t.text_size * scale).clamp(8.0, 48.0)))
                    .text_color(node_style.computed.color)
                    .children(self.render_html_children(
                        &node.children,
                        theme,
                        node_style.computed,
                        cx,
                    ));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "ul" | "ol" | "dl" => {
                let mut element = div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(d.block_gap * 0.25))
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .children(self.render_html_children(
                        &node.children,
                        theme,
                        node_style.computed,
                        cx,
                    ));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "li" => {
                let marker =
                    if node.children.iter().any(|child| {
                        child.tag_name == "#text" && !child.raw_source.trim().is_empty()
                    }) {
                        "•"
                    } else {
                        ""
                    };
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .gap(px(d.list_marker_gap))
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .child(div().min_w(px(d.list_marker_width)).child(marker))
                    .children(self.render_html_children(
                        &node.children,
                        theme,
                        node_style.computed,
                        cx,
                    ))
                    .into_any_element()
            }
            "dt" => div()
                .w_full()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(px(node_style.computed.font_size))
                .text_color(node_style.computed.color)
                .children(self.render_html_children(&node.children, theme, node_style.computed, cx))
                .into_any_element(),
            "dd" => div()
                .w_full()
                .pl(px(d.list_marker_width + d.list_marker_gap))
                .text_size(px(node_style.computed.font_size))
                .text_color(node_style.computed.color)
                .children(self.render_html_children(&node.children, theme, node_style.computed, cx))
                .into_any_element(),
            "caption" => div()
                .w_full()
                .text_center()
                .text_size(px(node_style.computed.font_size))
                .text_color(node_style.computed.color)
                .children(self.render_html_children(&node.children, theme, node_style.computed, cx))
                .into_any_element(),
            "section" | "article" | "aside" | "main" | "header" | "footer" | "nav" => {
                let mut element = div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(d.block_gap * 0.4))
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .children(self.render_html_children(
                        &node.children,
                        theme,
                        node_style.computed,
                        cx,
                    ));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "blockquote" => {
                let mut element = div()
                    .w_full()
                    .pl(px(d.quote_padding_left))
                    .border_l(px(d.quote_border_width))
                    .border_color(c.border_quote)
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .children(self.render_html_children(
                        &node.children,
                        theme,
                        node_style.computed,
                        cx,
                    ));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "pre" => {
                let mut element = div()
                    .w_full()
                    .rounded_sm()
                    .px(px(d.code_block_padding_x))
                    .py(px(d.code_block_padding_y))
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .child(SharedString::from(html_children_text(node)));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "img" => self.render_html_image(node, theme, node_style, cx),
            "table" => self.render_html_table(node, theme, node_style, cx),
            "thead" | "tbody" | "tfoot" => {
                let mut element = div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .children(self.render_html_children(
                        &node.children,
                        theme,
                        node_style.computed,
                        cx,
                    ));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "tr" => self.render_html_table_row(node, theme, node_style, cx),
            "th" | "td" => {
                let span = attr_value(node, "colspan")
                    .and_then(|value| value.parse::<usize>().ok())
                    .filter(|span| *span > 0)
                    .unwrap_or(1);
                let mut element = div()
                    .min_w(px(0.0))
                    .flex_grow()
                    .flex_shrink_0()
                    .min_w(px(96.0 * span as f32))
                    .border(px(1.0))
                    .border_color(wb.border_subtle)
                    .px(px(d.table_cell_padding_x))
                    .py(px(d.table_cell_padding_y))
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .font_weight(if node.tag_name == "th" {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .children(self.render_html_children(
                        &node.children,
                        theme,
                        node_style.computed,
                        cx,
                    ));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "details" => self.render_html_details(node, theme, node_style, cx),
            "summary" => {
                let mut element = div()
                    .w_full()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .children(self.render_html_children(
                        &node.children,
                        theme,
                        node_style.computed,
                        cx,
                    ));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "figure" => {
                let mut element = div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(d.image_caption_gap))
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .children(self.render_html_children(
                        &node.children,
                        theme,
                        node_style.computed,
                        cx,
                    ));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            "figcaption" => {
                let mut element = div()
                    .w_full()
                    .text_center()
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .children(self.render_html_children(
                        &node.children,
                        theme,
                        node_style.computed,
                        cx,
                    ));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
            _ => {
                let mut element = div()
                    .w_full()
                    .text_size(px(node_style.computed.font_size))
                    .text_color(node_style.computed.color)
                    .children(self.render_html_children(
                        &node.children,
                        theme,
                        node_style.computed,
                        cx,
                    ));
                if let Some(bg) = node_style.background {
                    element = element.bg(bg);
                }
                element.into_any_element()
            }
        }
    }

    pub(super) fn render_html_inline_container(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        node_style: HtmlNodeVisualStyle,
        weight: FontWeight,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let _ = weight;
        let mut element = div().min_w(px(0.0)).child(self.render_html_inline_flow(
            std::slice::from_ref(&node),
            theme,
            node_style.computed,
            _cx,
        ));
        match node.tag_name.as_str() {
            "sup" => {
                element = element
                    .relative()
                    .top(px(-node_style.computed.font_size * 0.28))
            }
            "sub" => {
                element = element
                    .relative()
                    .top(px(node_style.computed.font_size * 0.22))
            }
            _ => {}
        }
        element.into_any_element()
    }

    fn render_html_link(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        node_style: HtmlNodeVisualStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_html_inline_container(node, theme, node_style, FontWeight::NORMAL, cx)
    }

    pub(super) fn render_html_image(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        node_style: HtmlNodeVisualStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let image_markup = {
            let attrs = node
                .attrs
                .iter()
                .map(|attr| {
                    format!(
                        "{}=\"{}\"",
                        attr.name,
                        attr.value.clone().unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("<img {attrs} />")
        };
        let parsed_image = parse_html_image_block(&image_markup);
        let src = parsed_image
            .as_ref()
            .map(|image| image.src.as_str())
            .or_else(|| attr_value(node, "src"))
            .filter(|src| !src.trim().is_empty());
        let Some(src) = src else {
            let mut element = div()
                .text_size(px(node_style.computed.font_size))
                .text_color(node_style.computed.color)
                .child(SharedString::from(node.raw_source.clone()));
            if let Some(bg) = node_style.background {
                element = element.bg(bg);
            }
            return element.into_any_element();
        };
        let alt = parsed_image
            .as_ref()
            .map(|image| image.alt.clone())
            .unwrap_or_else(|| attr_value(node, "alt").unwrap_or_default().to_string());
        let zoom = parsed_image
            .as_ref()
            .map(|image| image.zoom_factor())
            .unwrap_or(1.0);
        let width_factor = parsed_image
            .as_ref()
            .and_then(|image| image.width_percent)
            .map(|width| f32::from(width) / 100.0)
            .unwrap_or(1.0);
        let height_factor = attr_value(node, "height")
            .and_then(|value| value.trim().parse::<f32>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|value| value.clamp(24.0, 8_192.0))
            .unwrap_or(theme.dimensions.image_root_max_height * zoom * width_factor);
        let mut runtime = ImageRuntime {
            alt,
            src: src.to_string(),
            title: parsed_image.as_ref().and_then(|image| image.title.clone()),
            width_percent: 100,
            resolved_source: resolve_image_source(src, self.image_base_dir()),
            asset_key: None,
            asset_state: crate::editor::render_asset_manager::AssetState::Idle,
        };
        if let Some(node_id) = node.id
            && let Some((key, state)) = self.html_image_asset_state(node_id)
        {
            runtime.asset_key = Some(key.clone());
            runtime.asset_state = state.clone();
        }
        let strings = cx.global::<I18nManager>().strings_arc();
        let content = self.render_image_content(
            runtime,
            Length::Definite(relative((zoom * width_factor).min(3.0))),
            px(height_factor),
            px(theme.dimensions.image_root_placeholder_height * zoom * width_factor),
            theme.dimensions.image_root_max_height.max(1.0),
            false,
            theme,
            &strings,
            cx,
        );
        if let Some(bg) = node_style.background {
            div().w_full().bg(bg).child(content).into_any_element()
        } else {
            content
        }
    }

    pub(super) fn render_html_table(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        node_style: HtmlNodeVisualStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let column_count = node
            .children
            .iter()
            .flat_map(|section| section.children.iter())
            .filter(|row| row.tag_name == "tr")
            .map(|row| {
                row.children
                    .iter()
                    .filter(|cell| matches!(cell.tag_name.as_str(), "td" | "th"))
                    .map(|cell| {
                        attr_value(cell, "colspan")
                            .and_then(|value| value.parse::<usize>().ok())
                            .filter(|span| *span > 0)
                            .unwrap_or(1)
                    })
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(1)
            .min(16);
        let min_width = (column_count as f32 * 112.0).clamp(240.0, 1_792.0);
        let mut table = div()
            .min_w(px(min_width))
            .border(px(1.0))
            .border_color(theme.colors.workbench.border_subtle)
            .text_size(px(node_style.computed.font_size))
            .text_color(node_style.computed.color)
            .children(self.render_html_children(&node.children, theme, node_style.computed, cx));
        if let Some(bg) = node_style.background {
            table = table.bg(bg);
        }
        let mut scroll = div()
            .id(SharedString::from(format!(
                "html-table-scroll-{}",
                self.record.id
            )))
            .w_full()
            .min_w(px(0.0))
            .overflow_x_scroll()
            .child(table);
        scroll.style().restrict_scroll_to_axis = Some(true);
        scroll.into_any_element()
    }

    pub(super) fn render_html_table_row(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        node_style: HtmlNodeVisualStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut element = div()
            .w_full()
            .flex()
            .text_size(px(node_style.computed.font_size))
            .text_color(node_style.computed.color)
            .children(self.render_html_children(&node.children, theme, node_style.computed, cx));
        if let Some(bg) = node_style.background {
            element = element.bg(bg);
        }
        element.into_any_element()
    }

    pub(super) fn render_html_details(
        &self,
        node: &HtmlNode,
        theme: &Theme,
        node_style: HtmlNodeVisualStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let initial_open = attr_value(node, "open").is_some();
        let node_id = node.id;
        let is_open = node_id
            .and_then(|id| self.html_details_state.get(&id).copied())
            .unwrap_or(initial_open);
        let summary = node
            .children
            .iter()
            .find(|child| child.tag_name == "summary");
        let body = node
            .children
            .iter()
            .filter(|child| child.tag_name != "summary");

        let mut container = div()
            .w_full()
            .rounded_sm()
            .border(px(1.0))
            .border_color(theme.colors.workbench.border_subtle)
            .px(px(theme.dimensions.block_padding_x))
            .py(px(theme.dimensions.block_padding_y))
            .text_size(px(node_style.computed.font_size))
            .text_color(node_style.computed.color)
            .child(
                div()
                    .w_full()
                    .flex()
                    .gap(px(theme.dimensions.list_marker_gap))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |block, event, window, cx| {
                            if let Some(node_id) = node_id {
                                block.on_html_details_toggle_mouse_down(
                                    node_id,
                                    initial_open,
                                    event,
                                    window,
                                    cx,
                                );
                            }
                        }),
                    )
                    .child(if is_open { "\u{25BE}" } else { "\u{25B8}" })
                    .children(summary.into_iter().map(|summary| {
                        self.render_html_node(summary, theme, node_style.computed, cx)
                    })),
            );
        if let Some(bg) = node_style.background {
            container = container.bg(bg);
        }
        if is_open {
            container =
                container.child(
                    div()
                        .w_full()
                        .pt(px(theme.dimensions.block_padding_y))
                        .children(body.map(|child| {
                            self.render_html_node(child, theme, node_style.computed, cx)
                        })),
                );
        }
        container.into_any_element()
    }
}
