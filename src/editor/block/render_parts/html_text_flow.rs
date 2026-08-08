// @author kongweiguang

//! A single GPUI text element for safe HTML inline content.
//!
//! HTML inline nodes are flattened into one shaped text layout before they
//! enter GPUI.  Keeping the inline content in one `shape_text` call is
//! important: a flex child per HTML tag makes GPUI treat every run as a
//! separate block and prevents normal wrapping, CJK shaping, and selection
//! geometry from working together.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HtmlTextRunStyle {
    pub(super) color: Hsla,
    pub(super) background: Option<Hsla>,
    pub(super) weight: FontWeight,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strikethrough: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HtmlTextRun {
    len: usize,
    style: HtmlTextRunStyle,
}

type LinkListener = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(Clone)]
pub(super) struct HtmlTextFlowElement {
    text: SharedString,
    runs: Vec<HtmlTextRun>,
    font_size: f32,
    line_height: f32,
    link_ranges: Vec<Range<usize>>,
    link_listener: Option<LinkListener>,
}

impl HtmlTextFlowElement {
    pub(super) fn new(
        text: SharedString,
        runs: Vec<(usize, HtmlTextRunStyle)>,
        font_size: f32,
        line_height: f32,
    ) -> Self {
        Self {
            text,
            runs: runs
                .into_iter()
                .filter(|(len, _)| *len > 0)
                .map(|(len, style)| HtmlTextRun { len, style })
                .collect(),
            font_size: font_size.clamp(8.0, 48.0),
            line_height: line_height.clamp(1.0, 2.0),
            link_ranges: Vec::new(),
            link_listener: None,
        }
    }

    pub(super) fn with_link_listener(
        mut self,
        ranges: Vec<Range<usize>>,
        listener: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.link_ranges = ranges;
        self.link_listener = Some(Rc::new(listener));
        self
    }
}

pub(super) struct RequestLayoutState {
    lines: Rc<RefCell<Option<Vec<WrappedLine>>>>,
}

pub(super) struct PrepaintState {
    lines: Vec<WrappedLine>,
    line_height: Pixels,
    bounds: Bounds<Pixels>,
    hitbox: Hitbox,
}

impl IntoElement for HtmlTextFlowElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for HtmlTextFlowElement {
    type RequestLayoutState = RequestLayoutState;
    type PrepaintState = PrepaintState;

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
        let text = self.text.clone();
        let text_runs = self.runs.clone();
        let font_size = px(self.font_size);
        let line_height = px(self.font_size * self.line_height);
        let underline_thickness = px(cx
            .global::<ThemeManager>()
            .current_arc()
            .dimensions
            .underline_thickness);
        let base_font = window.text_style().font();

        let lines = Rc::new(RefCell::new(None));
        let lines_for_layout = lines.clone();
        let mut layout_style = Style::default();
        layout_style.size.width = relative(1.).into();
        layout_style.min_size.width = px(0.0).into();
        layout_style.max_size.width = relative(1.).into();

        let layout_id = window.request_measured_layout(
            layout_style,
            move |known_dimensions, available_space, window, _cx| {
                let wrap_width = known_dimensions.width.or(match available_space.width {
                    AvailableSpace::Definite(width) => Some(width),
                    AvailableSpace::MinContent => Some(px(1.0)),
                    AvailableSpace::MaxContent => Some(window.viewport_size().width.max(px(1.0))),
                });
                let runs = text_runs
                    .iter()
                    .map(|run| {
                        let mut font = base_font.clone();
                        font.weight = run.style.weight;
                        if run.style.italic {
                            font.style = FontStyle::Italic;
                        }
                        let underline = run.style.underline.then_some(UnderlineStyle {
                            color: Some(run.style.color),
                            thickness: underline_thickness,
                            wavy: false,
                        });
                        let strikethrough = run.style.strikethrough.then_some(StrikethroughStyle {
                            color: Some(run.style.color),
                            thickness: underline_thickness,
                        });
                        TextRun {
                            len: run.len,
                            font,
                            color: run.style.color,
                            background_color: run.style.background,
                            underline,
                            strikethrough,
                        }
                    })
                    .collect::<Vec<_>>();

                let shaped = if text.is_empty() {
                    Vec::new()
                } else {
                    window
                        .text_system()
                        .shape_text(text.clone(), font_size, &runs, wrap_width, None)
                        .map(|lines| lines.into_vec())
                        .unwrap_or_default()
                };
                let mut measured: Size<Pixels> = Size::default();
                for line in &shaped {
                    let line_size = line.size(line_height);
                    measured.width = measured.width.max(line_size.width);
                    measured.height += line_size.height;
                }
                *lines_for_layout.borrow_mut() = Some(shaped);
                measured
            },
        );

        (layout_id, RequestLayoutState { lines })
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let line_height = px(self.font_size * self.line_height);
        let lines = request_layout.lines.borrow_mut().take().unwrap_or_default();
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        PrepaintState {
            lines,
            line_height,
            bounds,
            hitbox,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let mut y_offset = Pixels::default();
        for line in &prepaint.lines {
            line.paint(
                point(bounds.origin.x, bounds.origin.y + y_offset),
                prepaint.line_height,
                TextAlign::Left,
                None,
                window,
                _cx,
            )
            .ok();
            y_offset += line.size(prepaint.line_height).height;
        }

        let Some(listener) = self.link_listener.take() else {
            return;
        };
        if self.link_ranges.is_empty() {
            return;
        }
        let link_ranges = std::mem::take(&mut self.link_ranges);
        let lines = prepaint.lines.clone();
        let bounds = prepaint.bounds;
        let line_height = prepaint.line_height;
        let hitbox = prepaint.hitbox.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || !event.modifiers.secondary()
                || !hitbox.is_hovered(window)
            {
                return;
            }
            let Some(index) =
                html_text_index_for_position(&lines, bounds, line_height, event.position)
            else {
                return;
            };
            if let Some((range_index, _)) = link_ranges
                .iter()
                .enumerate()
                .find(|(_, range)| range.contains(&index))
            {
                listener(range_index, window, cx);
            }
        });
    }
}

fn html_text_index_for_position(
    lines: &[WrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    position: Point<Pixels>,
) -> Option<usize> {
    if position.y < bounds.top() || position.y >= bounds.bottom() {
        return None;
    }
    let mut line_origin = bounds.origin;
    let mut line_start = 0usize;
    for line in lines {
        let line_bottom = line_origin.y + line.size(line_height).height;
        if position.y <= line_bottom {
            let within = position - line_origin;
            return line
                .index_for_position(within, line_height)
                .ok()
                .map(|index| line_start + index);
        }
        line_origin.y = line_bottom;
        line_start += line.len() + 1;
    }
    None
}

#[derive(Clone, Copy)]
pub(super) struct HtmlFlowStyle {
    computed: HtmlComputedStyle,
    background: Option<Hsla>,
    weight: FontWeight,
    italic: bool,
    underline: bool,
    strikethrough: bool,
}

impl HtmlFlowStyle {
    pub(super) fn root(computed: HtmlComputedStyle) -> Self {
        Self {
            computed,
            background: None,
            weight: FontWeight::NORMAL,
            italic: false,
            underline: false,
            strikethrough: false,
        }
    }

    fn run(self) -> HtmlTextRunStyle {
        HtmlTextRunStyle {
            color: self.computed.color,
            background: self.background,
            weight: self.weight,
            italic: self.italic,
            underline: self.underline,
            strikethrough: self.strikethrough,
        }
    }
}

pub(super) fn html_node_is_flow_inline(node: &HtmlNode) -> bool {
    node.tag_name == "#text"
        || node.tag_name == "br"
        || matches!(
            node.tag_name.as_str(),
            "a" | "strong"
                | "em"
                | "b"
                | "i"
                | "u"
                | "mark"
                | "del"
                | "ins"
                | "code"
                | "kbd"
                | "sup"
                | "sub"
                | "small"
                | "abbr"
                | "dfn"
                | "time"
                | "q"
                | "span"
        )
}

pub(super) fn append_html_flow_node(
    node: &HtmlNode,
    inherited: HtmlFlowStyle,
    theme: &Theme,
    text: &mut String,
    runs: &mut Vec<(usize, HtmlTextRunStyle)>,
    links: &mut Vec<(Range<usize>, String)>,
) {
    if node.tag_name == "#text" {
        append_html_flow_text(&node.raw_source, inherited.run(), text, runs);
        return;
    }
    if node.tag_name == "br" {
        append_html_flow_segment("\n", inherited.run(), text, runs);
        return;
    }

    let visual = html_node_visual_style(node, inherited.computed, theme);
    let mut style = inherited;
    style.computed = visual.computed;
    style.background = visual.background.or(inherited.background);
    match node.tag_name.as_str() {
        "strong" | "b" => style.weight = FontWeight::BOLD,
        "em" | "i" => style.italic = true,
        "u" | "ins" => style.underline = true,
        "s" | "del" => style.strikethrough = true,
        "a" => style.underline = true,
        "q" => append_html_flow_segment("\u{201c}", style.run(), text, runs),
        _ => {}
    }
    let link_start = (node.tag_name == "a").then_some(text.len());
    for child in &node.children {
        append_html_flow_node(child, style, theme, text, runs, links);
    }
    if let Some(start) = link_start
        && let Some(href) = attr_value(node, "href")
        && start < text.len()
    {
        links.push((start..text.len(), href.to_owned()));
    }
    if node.tag_name == "q" {
        append_html_flow_segment("\u{201d}", style.run(), text, runs);
    }
}

fn append_html_flow_text(
    source: &str,
    style: HtmlTextRunStyle,
    text: &mut String,
    runs: &mut Vec<(usize, HtmlTextRunStyle)>,
) {
    let mut pending_space = false;
    for character in source.chars() {
        if character.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !text.is_empty() && !matches!(text.chars().last(), Some(' ' | '\n')) {
            append_html_flow_segment(" ", style, text, runs);
        }
        pending_space = false;
        let mut encoded = [0u8; 4];
        append_html_flow_segment(character.encode_utf8(&mut encoded), style, text, runs);
    }
    if pending_space && !text.is_empty() && !matches!(text.chars().last(), Some(' ' | '\n')) {
        append_html_flow_segment(" ", style, text, runs);
    }
}

fn append_html_flow_segment(
    segment: &str,
    style: HtmlTextRunStyle,
    text: &mut String,
    runs: &mut Vec<(usize, HtmlTextRunStyle)>,
) {
    if segment.is_empty() {
        return;
    }
    text.push_str(segment);
    let len = segment.len();
    if let Some((last_len, last_style)) = runs.last_mut()
        && *last_style == style
    {
        *last_len += len;
    } else {
        runs.push((len, style));
    }
}
