// @author kongweiguang

//! Rendering for [`Block`] via GPUI's high-level [`Render`] trait.
//!
//! Each block kind produces a distinct visual style: H1 has a bottom border,
//! list items render a marker column (bullet / ordinal), and raw Markdown
//! fallback renders as plain text.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use gpui::prelude::FluentBuilder;
use gpui::*;

const BLOCK_EDITOR_CONTEXT: &str = "BlockEditor";

use super::element::{BlockTextElement, CodeLanguageInputElement};
use super::{
    Block, BlockDragPayload, BlockDropPlacement, BlockEvent, BlockKind, ImageResolvedSource,
    ImageRuntime, MermaidViewMode,
};
use crate::components::{
    HtmlCssColor, HtmlDocument, HtmlNode, HtmlNodeKind, InlineScript, MermaidThemeMode,
    TableAxisHighlight, TableAxisKind, TableAxisMarker, TableCellInlineImageSegment,
    TableColumnLayout, attr_value, display_math_font_size, inline_math_font_size,
    parse_display_math_source, parse_html_image_block, parse_mermaid_fence_source,
    parse_table_cell_inline_images, render_display_math_svg, render_inline_math_svg,
    render_mermaid_svg_for_display, resolve_image_source, style_for_node,
};
use crate::i18n::{I18nManager, I18nStrings};
use crate::theme::{Theme, ThemeDimensions, ThemeManager};

const CHEVRON_DOWN_ICON: &str = "icon/ui/chevron-down.svg";
const COPY_ICON: &str = "icon/ui/copy.svg";
const CHECK_ICON: &str = "icon/ui/check.svg";
const EXPAND_ICON: &str = "icon/ui/expand.svg";
const PLUS_ICON: &str = "icon/ui/plus.svg";
const CALLOUT_NOTE_ICON: &str = "icon/ui/info.svg";
const CALLOUT_TIP_ICON: &str = "icon/ui/lightbulb.svg";
const CALLOUT_IMPORTANT_ICON: &str = "icon/ui/shield.svg";
const CALLOUT_WARNING_ICON: &str = "icon/ui/triangle-alert.svg";
const CALLOUT_CAUTION_ICON: &str = "icon/ui/shield-alert.svg";
const FOOTNOTE_BACKREF_ICON: &str = "icon/ui/corner-up-left.svg";

fn yaml_frontmatter_body(source: &str) -> Option<&str> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let opening_end = source.find('\n')?;
    if source[..opening_end].trim_end() != "---" {
        return None;
    }

    let remainder = &source[opening_end + 1..];
    if matches!(remainder.trim_end(), "---" | "...") {
        return Some("");
    }
    let closing_start = remainder.rfind('\n')?;
    let closing = remainder[closing_start + 1..].trim_end();
    matches!(closing, "---" | "...").then_some(&remainder[..closing_start])
}

fn callout_icon(variant: super::CalloutVariant) -> &'static str {
    match variant {
        super::CalloutVariant::Note => CALLOUT_NOTE_ICON,
        super::CalloutVariant::Tip => CALLOUT_TIP_ICON,
        super::CalloutVariant::Important => CALLOUT_IMPORTANT_ICON,
        super::CalloutVariant::Warning => CALLOUT_WARNING_ICON,
        super::CalloutVariant::Caution => CALLOUT_CAUTION_ICON,
    }
}

fn bulleted_list_marker(depth: usize, color: Hsla) -> Div {
    let selector = bulleted_list_marker_selector(depth);
    let marker = div()
        .debug_selector(move || selector.to_owned())
        .flex_shrink_0();
    match depth % 3 {
        0 => marker.size(px(5.0)).rounded(px(2.5)).bg(color),
        1 => marker
            .size(px(6.0))
            .rounded(px(3.0))
            .border(px(1.0))
            .border_color(color),
        _ => marker
            .size(px(6.0))
            .rounded(px(1.0))
            .border(px(1.0))
            .border_color(color),
    }
}

fn bulleted_list_marker_selector(depth: usize) -> &'static str {
    match depth % 3 {
        0 => "bulleted-list-marker-filled",
        1 => "bulleted-list-marker-hollow",
        _ => "bulleted-list-marker-square",
    }
}

fn bulleted_list_marker_slot_selector(depth: usize) -> &'static str {
    match depth % 3 {
        0 => "bulleted-list-marker-slot-filled",
        1 => "bulleted-list-marker-slot-hollow",
        _ => "bulleted-list-marker-slot-square",
    }
}

fn column_axis_gutter_visible(
    preview_marker: Option<TableAxisMarker>,
    selected_marker: Option<TableAxisMarker>,
) -> bool {
    matches!(
        preview_marker,
        Some(TableAxisMarker {
            kind: TableAxisKind::Column,
            ..
        })
    ) || matches!(
        selected_marker,
        Some(TableAxisMarker {
            kind: TableAxisKind::Column,
            ..
        })
    )
}

/// Makes a row-axis highlight color more opaque (more solid, still translucent)
/// for the header row, keeping the theme's hue so the header handle reads as a
/// stronger version of the body-row handles in whatever colors the theme uses.
fn header_axis_emphasis(color: Hsla) -> Hsla {
    Hsla {
        a: color.a + (1.0 - color.a) * 0.5,
        ..color
    }
}

fn fallback_image_label(alt: &str, strings: &I18nStrings) -> SharedString {
    if alt.trim().is_empty() {
        SharedString::from(strings.image_placeholder.clone())
    } else {
        SharedString::from(alt.to_string())
    }
}

fn render_complex_warning(message: String, theme: &Theme, id: &'static str) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .h(px(22.0))
        .w_full()
        .min_w(px(0.0))
        .max_w(Length::Definite(relative(1.0)))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .overflow_hidden()
        .rounded(px(4.0))
        .bg(theme.colors.callout_warning_bg)
        .border_l(px(2.0))
        .border_color(theme.colors.callout_warning_border)
        .text_size(px(theme.typography.code_size))
        .text_color(theme.colors.text_default)
        .child(
            div()
                .size(px(14.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.colors.callout_warning_border)
                .debug_selector(move || format!("{id}-icon"))
                .child(svg().path(CALLOUT_WARNING_ICON).size(px(13.0))),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_grow()
                .overflow_hidden()
                .text_ellipsis()
                .child(SharedString::from(message)),
        )
        .into_any_element()
}

fn render_math_svg_content(
    rendered: &crate::components::LatexSvgRender,
    theme: &Theme,
) -> AnyElement {
    div()
        .id("math-rendered-content")
        .debug_selector(|| "math-rendered-content".to_owned())
        .w_full()
        .min_w(px(0.0))
        .max_w(Length::Definite(relative(1.0)))
        .flex()
        .justify_center()
        .py(px(theme.dimensions.block_padding_y.max(6.0)))
        .child(
            img(rendered.path.clone())
                .max_w(Length::Definite(relative(1.0)))
                .max_h(px(theme.dimensions.image_root_max_height))
                .object_fit(ObjectFit::Contain),
        )
        .into_any_element()
}

fn render_mermaid_svg_content(
    rendered: &crate::components::MermaidSvgRender,
    block_padding_y: f32,
    content_height: f32,
) -> AnyElement {
    let display_width = rendered.display_width.max(1.0);
    let display_height = rendered.display_height.max(1.0);
    let content_height = content_height.max(1.0);
    let vertical_padding = block_padding_y.max(6.0);
    let canvas_height =
        mermaid_preview_canvas_height(display_height, vertical_padding, content_height);
    let image = img(rendered.path.clone())
        // 失败态会保留上一次成功的 SVG；使用父级宽度并以缓存宽度为上限，
        // 让旧视口生成的图片收缩而不反向撑宽当前工作台。
        .w_full()
        .max_w(px(display_width))
        .h(px(display_height))
        .object_fit(ObjectFit::Contain);

    div()
        .id("mermaid-rendered-content")
        .debug_selector(|| "mermaid-rendered-content".to_owned())
        .w_full()
        .h(px(canvas_height))
        .min_w(px(0.0))
        .max_w(Length::Definite(relative(1.0)))
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .py(px(vertical_padding))
        .child(image)
        .into_any_element()
}

fn mermaid_preview_canvas_height(
    display_height: f32,
    vertical_padding: f32,
    viewport_height: f32,
) -> f32 {
    // 竖向长图必须保留可阅读尺寸，由外层预览窗滚动；不能为了塞进一屏再次缩成缩略图。
    (display_height.max(1.0) + 2.0 * vertical_padding.max(0.0)).max(viewport_height.max(1.0))
}

const MERMAID_NARROW_BREAKPOINT: f32 = 640.0;
const MERMAID_CONTENT_MIN_HEIGHT: f32 = 360.0;
const MERMAID_SPLIT_WIDE_MIN_HEIGHT: f32 = 420.0;
const MERMAID_SPLIT_NARROW_MIN_HEIGHT: f32 = 280.0;
const MERMAID_SPLIT_NARROW_MAX_HEIGHT: f32 = 360.0;
const MERMAID_WORKBENCH_MIN_MAX_HEIGHT: f32 = 420.0;
const MERMAID_WORKBENCH_MAX_HEIGHT: f32 = 760.0;
const MERMAID_PREVIEW_MAX_VIEWPORT_RATIO: f32 = 1.5;
const MERMAID_PREVIEW_MIN_MAX_HEIGHT: f32 = 720.0;
const MERMAID_PREVIEW_MAX_HEIGHT: f32 = 1_440.0;
const MERMAID_PREVIEW_VERTICAL_INSET: f32 = 24.0;
const MERMAID_SOURCE_VERTICAL_INSET: f32 = 24.0;
const MERMAID_SPLIT_DIVIDER_HEIGHT: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct MermaidWorkbenchHeights {
    body_height: f32,
    source_height: f32,
    preview_height: f32,
}

fn mermaid_workbench_body_height(
    mode: MermaidViewMode,
    viewport_width: f32,
    viewport_height: f32,
    successful_svg_display_height: Option<f32>,
    source_line_count: usize,
    source_line_height: f32,
) -> MermaidWorkbenchHeights {
    // 工作台不再凭模式固定高度：短内容维持紧凑，长图/长源码仅扩展到窗口可读上限，
    // 再交给各自面板滚动。成功 SVG 即使正在重渲染或本次失败也会继续参与计算，避免闪烁。
    let max_height = (viewport_height.max(1.0) * 0.70).clamp(
        MERMAID_WORKBENCH_MIN_MAX_HEIGHT,
        MERMAID_WORKBENCH_MAX_HEIGHT,
    );
    let source_desired = source_line_count.max(1) as f32 * source_line_height.max(1.0)
        + MERMAID_SOURCE_VERTICAL_INSET;
    let preview_desired = successful_svg_display_height
        .map(|height| height.max(1.0) + MERMAID_PREVIEW_VERTICAL_INSET)
        .unwrap_or(MERMAID_CONTENT_MIN_HEIGHT);
    let source_height = source_desired.clamp(MERMAID_CONTENT_MIN_HEIGHT, max_height);
    let preview_max_height = (viewport_height.max(1.0) * MERMAID_PREVIEW_MAX_VIEWPORT_RATIO)
        .clamp(MERMAID_PREVIEW_MIN_MAX_HEIGHT, MERMAID_PREVIEW_MAX_HEIGHT);
    let preview_height = preview_desired.clamp(MERMAID_CONTENT_MIN_HEIGHT, preview_max_height);

    match mode {
        MermaidViewMode::Preview => MermaidWorkbenchHeights {
            body_height: preview_height,
            source_height,
            preview_height,
        },
        MermaidViewMode::Source => MermaidWorkbenchHeights {
            body_height: source_height,
            source_height,
            preview_height,
        },
        MermaidViewMode::Split if viewport_width < MERMAID_NARROW_BREAKPOINT => {
            let source_height = source_desired.clamp(
                MERMAID_SPLIT_NARROW_MIN_HEIGHT,
                MERMAID_SPLIT_NARROW_MAX_HEIGHT,
            );
            let preview_height = preview_desired.clamp(
                MERMAID_SPLIT_NARROW_MIN_HEIGHT,
                MERMAID_SPLIT_NARROW_MAX_HEIGHT,
            );
            MermaidWorkbenchHeights {
                body_height: source_height + MERMAID_SPLIT_DIVIDER_HEIGHT + preview_height,
                source_height,
                preview_height,
            }
        }
        MermaidViewMode::Split => {
            let body_height = source_desired
                .max(preview_desired)
                .clamp(MERMAID_SPLIT_WIDE_MIN_HEIGHT, max_height);
            MermaidWorkbenchHeights {
                body_height,
                source_height: body_height,
                preview_height: body_height,
            }
        }
    }
}

fn render_image_placeholder(
    runtime: &ImageRuntime,
    width: Length,
    height: Pixels,
    theme: &Theme,
    strings: &I18nStrings,
) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;
    let t = &theme.typography;
    div()
        .w(width)
        .h(height)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(d.image_radius))
        .border(px(1.0))
        .border_color(c.image_placeholder_border)
        .bg(c.image_placeholder_bg)
        .px(px(d.block_padding_x))
        .text_center()
        .text_size(px(t.text_size))
        .text_color(c.image_placeholder_text)
        .child(fallback_image_label(&runtime.alt, strings))
        .into_any_element()
}

fn render_loading_placeholder(
    runtime: &ImageRuntime,
    width: Length,
    height: Pixels,
    theme: &Theme,
    strings: &I18nStrings,
) -> AnyElement {
    let c = &theme.colors;
    let d = &theme.dimensions;
    let t = &theme.typography;
    div()
        .w(width)
        .h(height)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(d.image_radius))
        .border(px(1.0))
        .border_color(c.image_placeholder_border)
        .bg(c.image_placeholder_bg)
        .px(px(d.block_padding_x))
        .text_center()
        .text_size(px(t.code_size))
        .text_color(c.image_placeholder_text)
        .child(if runtime.alt.trim().is_empty() {
            SharedString::from(strings.image_loading_without_alt.clone())
        } else {
            SharedString::from(
                strings
                    .image_loading_with_alt_template
                    .replace("{alt}", &runtime.alt),
            )
        })
        .into_any_element()
}

fn wrap_with_quote_guides(content: AnyElement, quote_depth: usize, theme: &Theme) -> AnyElement {
    if quote_depth == 0 {
        return content;
    }

    let c = &theme.colors;
    let d = &theme.dimensions;
    let guide_offset = d.quote_padding_left;
    let total_padding = guide_offset * quote_depth as f32;
    let gutter_inset = rendered_content_inset(d);

    div()
        .w_full()
        .relative()
        .pl(px(total_padding))
        .child(content)
        .children((0..quote_depth).map(|level| {
            div()
                .debug_selector(move || format!("quote-guide-{level}"))
                .absolute()
                .top_0()
                .bottom_0()
                // 引用线从统一块入口之后开始，单一“…”不会遮挡引用语义，也无需改变 X 轴。
                .left(px(gutter_inset + guide_offset * level as f32))
                .w(px(d.quote_border_width))
                .bg(c.border_quote)
        }))
        .into_any_element()
}

fn callout_accent_and_background(variant: super::CalloutVariant, theme: &Theme) -> (Hsla, Hsla) {
    let c = &theme.colors;
    match variant {
        super::CalloutVariant::Note => (c.callout_note_border, c.callout_note_bg),
        super::CalloutVariant::Tip => (c.callout_tip_border, c.callout_tip_bg),
        super::CalloutVariant::Important => (c.callout_important_border, c.callout_important_bg),
        super::CalloutVariant::Warning => (c.callout_warning_border, c.callout_warning_bg),
        super::CalloutVariant::Caution => (c.callout_caution_border, c.callout_caution_bg),
    }
}

fn visible_quote_guides(block: &Block) -> usize {
    block.visible_quote_depth
}

/// 根级可见 surface 的统一左右 inset：交互 gutter 属于 shell，不得改变表格、
/// 代码、引用线等内容实体的边界。
pub(crate) fn rendered_content_inset(d: &ThemeDimensions) -> f32 {
    d.block_padding_x + super::slash_command::BLOCK_GUTTER_TEXT_RESERVE
}

fn block_content_insets(
    rendered_inset: f32,
    nested_indent: f32,
    render_depth: usize,
    grouped_surface: bool,
) -> (f32, f32) {
    // Callout/脚注外壳已经提供水平留白，内部块只保留语义层级缩进，避免双重 padding。
    let surface_inset = if grouped_surface { 0.0 } else { rendered_inset };
    (
        surface_inset + nested_indent * render_depth as f32,
        surface_inset,
    )
}

fn effective_table_width(block: &Block, viewport_width: f32, d: &ThemeDimensions) -> f32 {
    container_image_width_budget(block, viewport_width, d)
        .max((d.table_cell_padding_x * 2.0 + 80.0).max(120.0))
}

fn container_image_width_budget(block: &Block, viewport_width: f32, d: &ThemeDimensions) -> f32 {
    let centered_width = crate::ui::centered_column_width(viewport_width, d);
    let visible_quote_guides = visible_quote_guides(block);
    let quote_inset = d.quote_padding_left * visible_quote_guides as f32;
    let callout_inset = if block.callout_depth > 0 {
        d.callout_padding_x * 2.0 + d.callout_border_width
    } else {
        0.0
    };

    centered_width
        - rendered_content_inset(d) * 2.0
        - d.nested_block_indent * block.render_depth as f32
        - quote_inset
        - callout_inset
}

fn effective_image_width(block: &Block, viewport_width: f32, d: &ThemeDimensions) -> f32 {
    container_image_width_budget(block, viewport_width, d).max(160.0)
}

fn effective_list_item_image_width(block: &Block, viewport_width: f32, d: &ThemeDimensions) -> f32 {
    let marker_width = match block.kind() {
        BlockKind::BulletedListItem => d.list_marker_width,
        BlockKind::TaskListItem { .. } => d.list_marker_width.max(d.task_checkbox_size),
        BlockKind::NumberedListItem => d.ordered_list_marker_width,
        _ => 0.0,
    };
    (container_image_width_budget(block, viewport_width, d) - marker_width - d.list_marker_gap)
        .max(160.0)
}

/// Returns a human-readable list ordinal: numbers at depth 0, lowercase
/// letters at depth 1, and unicode roman numerals at depth 2+.
fn numbered_list_marker(depth: usize, ordinal: usize) -> String {
    match depth {
        0 => format!("{ordinal}."),
        1 => format!("{}.", alphabetic_list_marker(ordinal)),
        _ => format!("{}.", roman_list_marker(ordinal)),
    }
}

/// Expands beyond 26 by wrapping: a...z, a1...z1, a2...z2, ...
fn alphabetic_list_marker(ordinal: usize) -> String {
    const ALPHABET: &[u8; 26] = b"abcdefghijklmnopqrstuvwxyz";

    let ordinal = ordinal.max(1);
    if ordinal <= ALPHABET.len() {
        return char::from(ALPHABET[ordinal - 1]).to_string();
    }

    let wrapped = ordinal - (ALPHABET.len() + 1);
    let letter = char::from(ALPHABET[wrapped % ALPHABET.len()]);
    let suffix = wrapped + 1;
    format!("{letter}{suffix}")
}

/// Converts an ASCII roman numeral string to its unicode ligature equivalents
/// where possible (for example, "III" to a single roman numeral glyph).
fn roman_list_marker(ordinal: usize) -> String {
    let ascii = ascii_roman_numeral(ordinal.max(1));
    let mut index = 0;
    let mut marker = String::new();

    while index < ascii.len() {
        let remaining = &ascii[index..];
        if let Some((token_len, token)) = roman_unicode_token(remaining) {
            marker.push_str(token);
            index += token_len;
        } else {
            break;
        }
    }

    marker
}

fn ascii_roman_numeral(mut ordinal: usize) -> String {
    const MAP: &[(usize, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];

    let mut result = String::new();
    for (value, symbol) in MAP {
        while ordinal >= *value {
            result.push_str(symbol);
            ordinal -= *value;
        }
    }
    result
}

fn roman_unicode_token(remaining: &str) -> Option<(usize, &'static str)> {
    const TOKENS: &[(&str, &str)] = &[
        ("XII", "\u{216B}"),
        ("XI", "\u{216A}"),
        ("IX", "\u{2168}"),
        ("VIII", "\u{2167}"),
        ("VII", "\u{2166}"),
        ("VI", "\u{2165}"),
        ("IV", "\u{2163}"),
        ("III", "\u{2162}"),
        ("II", "\u{2161}"),
        ("I", "\u{2160}"),
        ("V", "\u{2164}"),
        ("X", "\u{2169}"),
        ("L", "\u{216C}"),
        ("C", "\u{216D}"),
        ("D", "\u{216E}"),
        ("M", "\u{216F}"),
    ];

    TOKENS.iter().find_map(|(ascii, unicode)| {
        remaining
            .starts_with(ascii)
            .then_some((ascii.len(), *unicode))
    })
}

fn html_children_text(node: &HtmlNode) -> String {
    if node.children.is_empty() {
        return node.raw_source.clone();
    }

    let mut text = String::new();
    for child in &node.children {
        if child.tag_name == "br" {
            text.push('\n');
        } else {
            text.push_str(&html_children_text(child));
        }
    }
    text
}

#[derive(Clone, Copy, Debug)]
struct HtmlComputedStyle {
    color: Hsla,
    font_size: f32,
    root_font_size: f32,
}

#[derive(Clone, Copy, Debug)]
struct HtmlNodeVisualStyle {
    computed: HtmlComputedStyle,
    background: Option<Hsla>,
}

impl HtmlComputedStyle {
    fn root(theme: &Theme) -> Self {
        Self {
            color: theme.colors.text_default,
            font_size: theme.typography.text_size,
            root_font_size: theme.typography.text_size,
        }
    }
}

fn html_css_color_to_hsla(color: HtmlCssColor, current_color: Hsla) -> Hsla {
    match color {
        HtmlCssColor::CurrentColor => current_color,
        HtmlCssColor::Rgba(color) => Hsla::from(Rgba {
            r: color.red as f32 / 255.0,
            g: color.green as f32 / 255.0,
            b: color.blue as f32 / 255.0,
            a: color.alpha.clamp(0.0, 1.0),
        }),
    }
}

fn html_node_visual_style(
    node: &HtmlNode,
    parent: HtmlComputedStyle,
    theme: &Theme,
) -> HtmlNodeVisualStyle {
    let c = &theme.colors;
    let t = &theme.typography;
    let mut computed = parent;
    let mut background = None;

    match node.tag_name.as_str() {
        "a" => computed.color = c.text_link,
        "blockquote" => computed.color = c.text_quote,
        "code" | "kbd" | "pre" => {
            computed.color = c.code_text;
            computed.font_size = t.code_size;
            background = Some(c.code_bg);
        }
        "mark" => background = Some(c.comment_bg),
        "figcaption" => {
            computed.color = c.image_caption_text;
            computed.font_size = t.code_size;
        }
        "small" | "sup" | "sub" => computed.font_size = (computed.font_size * 0.8).max(6.0),
        "th" => background = Some(c.table_header_bg),
        "td" => background = Some(c.table_cell_bg),
        _ => {}
    }

    let inline_style = style_for_node(node);
    if let Some(color) = inline_style.color {
        computed.color = html_css_color_to_hsla(color, computed.color);
    }
    if let Some(font_size) = inline_style.font_size {
        computed.font_size = font_size.resolve(computed.font_size, computed.root_font_size);
    }
    if let Some(color) = inline_style.background_color {
        background = Some(html_css_color_to_hsla(color, computed.color));
    }

    HtmlNodeVisualStyle {
        computed,
        background,
    }
}

impl Block {}

#[path = "render_parts/html.rs"]
mod html;
#[path = "render_parts/inline_media.rs"]
mod inline_media;
#[path = "render_parts/mermaid.rs"]
mod mermaid;

impl Focusable for Block {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[path = "render_parts/callout.rs"]
mod callout;
#[path = "render_parts/code.rs"]
mod code;
#[path = "render_parts/footnote.rs"]
mod footnote;
#[path = "render_parts/heading.rs"]
mod heading;
#[path = "render_parts/resource.rs"]
mod resource;
#[path = "render_parts/table.rs"]
mod table;
#[path = "render_parts/task.rs"]
mod task;
#[path = "render_parts/view.rs"]
mod view;

/// Break a styled inline text run into wrap-friendly chunks for the mixed
/// inline-visual layout. Runs that carry their own box (inline code, background
/// highlight) stay a single chunk so their padding/background is continuous;
/// everything else is split on whitespace with each word keeping its trailing
/// space, so the `flex_wrap` row can break between words instead of pushing the
/// next inline visual onto its own line.
/// Wraps a rendered inline link run so the hand cursor only appears while the
/// Cmd/Ctrl follow modifier is held. Links in mixed inline-visual blocks (math,
/// scripts, inline images) render as plain divs, so this sets `PointingHand`
/// when its hitbox is hovered and the modifier is down, like `BlockTextElement`
/// does for normal text. The editor root repaints on follow-modifier toggles,
/// so the cursor re-evaluates without the pointer moving. Layout and painting
/// are delegated to the child.
struct LinkFollowCursor {
    child: AnyElement,
}

impl IntoElement for LinkFollowCursor {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LinkFollowCursor {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.child.prepaint(window, cx);
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if hitbox.is_hovered(window) && window.modifiers().secondary() {
            // The editor root repaints on follow-modifier toggles, so the hand
            // cursor re-evaluates here even while the pointer stays still.
            window.set_cursor_style(CursorStyle::PointingHand, hitbox);
        }
        self.child.paint(window, cx);
    }
}

fn inline_word_chunks(text: &str, code: bool, has_background: bool) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    if code || has_background {
        return vec![text];
    }
    text.split_inclusive(char::is_whitespace).collect()
}

#[cfg(test)]
#[path = "../../../tests/unit/components/block/render.rs"]
mod tests;
