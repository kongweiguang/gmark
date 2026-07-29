// @author kongweiguang

//! Mermaid fenced-block parsing and SVG rendering helpers.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow};
use directories::ProjectDirs;

use crate::theme::Theme;

const SIMPLE_MERMAID_LINE_LIMIT: usize = 8;
const MERMAID_SIMPLE_TARGET_WIDTH_RATIO: f32 = 0.65;
const MERMAID_SIMPLE_MAX_SCALE: f32 = 1.5;
const MERMAID_COMPLEX_TARGET_WIDTH_RATIO: f32 = 0.9;
const MERMAID_SCALE_PER_EXTRA_LINE: f32 = 0.035;
const MERMAID_MAX_SCALE: f32 = 1.75;
// 任何会改变基础 SVG 几何或样式的渲染策略都必须升级此版本，避免沿用旧缓存。
const MERMAID_RENDER_CACHE_VERSION: u8 = 6;
const MERMAID_FLOWCHART_SPACING: f32 = 64.0;
const MERMAID_ARROW_TIP_EXTENSION: f32 = 5.0;

/// Mermaid 的颜色模式必须进入缓存身份，避免切换应用主题后复用旧主题 SVG。
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum MermaidThemeMode {
    Light,
    Dark,
}

impl MermaidThemeMode {
    /// 编辑器与导出必须共享同一明暗判定，避免同一主题生成两套不同配色的 SVG。
    pub(crate) fn from_theme(theme: &Theme) -> Self {
        if theme.colors.editor_background.l < 0.5 {
            Self::Dark
        } else {
            Self::Light
        }
    }
}

/// Opening fence metadata for a Mermaid fenced code block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MermaidFence {
    /// Fence marker, either backtick or tilde.
    pub(crate) marker: char,
    /// Opening fence run length.
    pub(crate) len: usize,
}

/// Parsed Mermaid source preserved from Markdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MermaidSource {
    /// Full Markdown source, including the opening and closing fences.
    pub(crate) raw: String,
    /// Mermaid diagram source between the fences.
    pub(crate) body: String,
    /// The full info string after the opening fence.
    pub(crate) info: String,
}

/// Result of rendering a Mermaid diagram into an SVG cache file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MermaidSvgRender {
    /// Path to the SVG file consumed by GPUI's image element.
    pub(crate) path: PathBuf,
    /// SVG document content, used by export paths.
    pub(crate) svg: String,
    /// Concrete display width encoded into the cached SVG.
    pub(crate) display_width: f32,
    /// Concrete display height encoded into the cached SVG.
    pub(crate) display_height: f32,
    /// Scale applied to the renderer's intrinsic SVG size for editor display.
    pub(crate) display_scale: f32,
}

/// Concrete dimensions encoded into a display SVG.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MermaidSvgSize {
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// Returns true when a fenced code info string declares Mermaid content.
pub(crate) fn is_mermaid_info_string(info: Option<&str>) -> bool {
    info.and_then(|info| info.split_whitespace().next())
        .is_some_and(|first| {
            first.eq_ignore_ascii_case("mermaid") || first.eq_ignore_ascii_case("mmd")
        })
}

/// Parse a line as a Mermaid opening fence.
pub(crate) fn parse_mermaid_fence_start(line: &str) -> Option<MermaidFence> {
    let trimmed = strip_fence_indent(line)?.trim_end();
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }

    let len = trimmed.chars().take_while(|ch| *ch == marker).count();
    if len < 3 {
        return None;
    }

    let info = trimmed[marker.len_utf8() * len..].trim();
    if marker == '`' && info.contains('`') {
        return None;
    }

    is_mermaid_info_string((!info.is_empty()).then_some(info))
        .then_some(MermaidFence { marker, len })
}

/// Returns true when `line` closes the given Mermaid fence.
pub(crate) fn is_mermaid_closing_fence(line: &str, fence: MermaidFence) -> bool {
    let Some(trimmed) = strip_fence_indent(line).map(str::trim_end) else {
        return false;
    };
    if !trimmed.starts_with(fence.marker) {
        return false;
    }

    let len = trimmed.chars().take_while(|ch| *ch == fence.marker).count();
    len >= fence.len && trimmed[fence.marker.len_utf8() * len..].trim().is_empty()
}

/// Parse raw fenced Markdown into the Mermaid diagram source it contains.
pub(crate) fn parse_mermaid_fence_source(raw: &str) -> Option<MermaidSource> {
    let raw = raw.trim_matches('\n').to_string();
    let lines = raw.split('\n').collect::<Vec<_>>();
    if lines.len() < 2 {
        return None;
    }

    let opening = strip_fence_indent(lines[0])?.trim_end();
    let fence = parse_mermaid_fence_start(opening)?;
    let info = opening[fence.marker.len_utf8() * fence.len..]
        .trim()
        .to_string();
    if !is_mermaid_closing_fence(lines.last()?, fence) {
        return None;
    }

    let body = lines[1..lines.len() - 1].join("\n");
    Some(MermaidSource { raw, body, info })
}

/// Render Mermaid source into a cached SVG sized for editor display.
pub(crate) fn render_mermaid_svg_for_display(
    source: &MermaidSource,
    available_width: f32,
    viewport_width: f32,
    theme_mode: MermaidThemeMode,
) -> anyhow::Result<MermaidSvgRender> {
    let renderer = match theme_mode {
        MermaidThemeMode::Light => render_mermaid_raw_light,
        MermaidThemeMode::Dark => render_mermaid_raw_dark,
    };
    render_mermaid_svg_for_display_with(
        source,
        available_width,
        viewport_width,
        theme_mode,
        renderer,
    )
}

fn render_mermaid_svg_for_display_with(
    source: &MermaidSource,
    available_width: f32,
    viewport_width: f32,
    theme_mode: MermaidThemeMode,
    renderer: MermaidRenderer,
) -> anyhow::Result<MermaidSvgRender> {
    let cache_dir = mermaid_cache_dir()?;
    render_mermaid_svg_for_display_in_cache(
        source,
        available_width,
        viewport_width,
        theme_mode,
        renderer,
        &cache_dir,
    )
}

fn render_mermaid_svg_for_display_in_cache(
    source: &MermaidSource,
    available_width: f32,
    viewport_width: f32,
    theme_mode: MermaidThemeMode,
    renderer: MermaidRenderer,
    cache_dir: &Path,
) -> anyhow::Result<MermaidSvgRender> {
    let base_key = mermaid_themed_cache_key(&source.body, theme_mode);
    let base_path = mermaid_cache_file_path_in(cache_dir, "base", &base_key)?;
    let base_svg = render_mermaid_to_svg_cached_with(&source.body, &base_path, renderer)?;
    let intrinsic = mermaid_svg_intrinsic_size(&base_svg)?;
    let scale = mermaid_display_scale(
        &source.body,
        intrinsic.width,
        intrinsic.height,
        available_width,
        viewport_width,
    );

    let display_key = mermaid_display_cache_key(&source.body, scale, theme_mode);
    let display_path = mermaid_cache_file_path_in(cache_dir, "display", &display_key)?;
    if display_path.exists() {
        let svg = fs::read_to_string(&display_path).with_context(|| {
            format!(
                "failed to read Mermaid display SVG cache '{}'",
                display_path.display()
            )
        })?;
        // display 缓存保留原始 viewBox，但根 width/height 才是 GPUI 实际布局尺寸。
        let size = mermaid_svg_display_size(&svg)?;
        return Ok(MermaidSvgRender {
            path: display_path,
            svg,
            display_width: size.width,
            display_height: size.height,
            display_scale: scale,
        });
    }

    let (svg, size) = scale_mermaid_svg_for_display(&base_svg, scale)?;
    fs::write(&display_path, &svg).with_context(|| {
        format!(
            "failed to write Mermaid display SVG cache '{}'",
            display_path.display()
        )
    })?;
    Ok(MermaidSvgRender {
        path: display_path,
        svg,
        display_width: size.width,
        display_height: size.height,
        display_scale: scale,
    })
}

/// Render a Mermaid diagram body into cached SVG text for the active theme.
pub(crate) fn render_mermaid_to_svg(
    source: &str,
    theme_mode: MermaidThemeMode,
) -> anyhow::Result<String> {
    let key = mermaid_themed_cache_key(source, theme_mode);
    let path = mermaid_cache_file_path("base", &key)?;
    let renderer = match theme_mode {
        MermaidThemeMode::Light => render_mermaid_raw_light,
        MermaidThemeMode::Dark => render_mermaid_raw_dark,
    };
    render_mermaid_to_svg_cached_with(source, &path, renderer)
}

type MermaidRenderer = fn(&str) -> anyhow::Result<String>;

fn render_mermaid_to_svg_cached_with(
    source: &str,
    path: &Path,
    renderer: MermaidRenderer,
) -> anyhow::Result<String> {
    if path.exists() {
        return fs::read_to_string(path).with_context(|| {
            format!("failed to read Mermaid base SVG cache '{}'", path.display())
        });
    }

    let svg = renderer(source)?;
    fs::write(path, &svg).with_context(|| {
        format!(
            "failed to write Mermaid base SVG cache '{}'",
            path.display()
        )
    })?;
    Ok(svg)
}

#[cfg(test)]
fn render_mermaid_raw(source: &str) -> anyhow::Result<String> {
    render_mermaid_raw_with_theme(source, MermaidThemeMode::Light)
}

fn render_mermaid_raw_light(source: &str) -> anyhow::Result<String> {
    render_mermaid_raw_with_theme(source, MermaidThemeMode::Light)
}

fn render_mermaid_raw_dark(source: &str) -> anyhow::Result<String> {
    render_mermaid_raw_with_theme(source, MermaidThemeMode::Dark)
}

fn render_mermaid_raw_with_theme(
    source: &str,
    theme_mode: MermaidThemeMode,
) -> anyhow::Result<String> {
    if !looks_like_supported_mermaid_source(source) {
        return Err(anyhow::anyhow!("unsupported Mermaid diagram"));
    }
    let mut options = mermaid_render_options(source);
    if theme_mode == MermaidThemeMode::Dark {
        options.theme = mermaid_rs_renderer::Theme::dark();
    }
    let parsed = mermaid_rs_renderer::parse_mermaid_strict(source)
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let mut layout =
        mermaid_rs_renderer::compute_layout(&parsed.graph, &options.theme, &options.layout);
    if let Some(direction) = flowchart_direction(source) {
        normalize_flowchart_decision_branches(&mut layout, direction);
    }
    let svg = mermaid_rs_renderer::render_svg(&layout, &options.theme, &options.layout);
    if svg.contains("class=\"error-text\"") || svg.contains("Syntax error in text") {
        return Err(anyhow::anyhow!("Mermaid syntax error"));
    }
    Ok(smooth_forward_flowchart_edges(source, &svg))
}

fn normalize_flowchart_decision_branches(
    layout: &mut mermaid_rs_renderer::Layout,
    direction: FlowchartDirection,
) {
    if layout.kind != mermaid_rs_renderer::DiagramKind::Flowchart {
        return;
    }
    let nodes = layout.nodes.clone();
    for (index, edge) in layout.edges.iter_mut().enumerate() {
        let (Some(source), Some(target)) = (nodes.get(&edge.from), nodes.get(&edge.to)) else {
            continue;
        };
        if source.shape != mermaid_rs_renderer::NodeShape::Diamond {
            continue;
        }

        let source_center = (
            source.x + source.width / 2.0,
            source.y + source.height / 2.0,
        );
        let target_center = (
            target.x + target.width / 2.0,
            target.y + target.height / 2.0,
        );
        let side = match direction {
            FlowchartDirection::TopDown | FlowchartDirection::BottomUp => {
                branch_side(target_center.0 - source_center.0, index)
            }
            FlowchartDirection::LeftRight | FlowchartDirection::RightLeft => {
                branch_side(target_center.1 - source_center.1, index)
            }
        };
        let tip_gap = if edge.arrow_end {
            MERMAID_ARROW_TIP_EXTENSION
        } else {
            0.0
        };
        let (start, end) = match direction {
            FlowchartDirection::TopDown => (
                (
                    source_center.0 + side * source.width * 0.25,
                    source.y + source.height * 0.75,
                ),
                (target_center.0, target.y - tip_gap),
            ),
            FlowchartDirection::BottomUp => (
                (
                    source_center.0 + side * source.width * 0.25,
                    source.y + source.height * 0.25,
                ),
                (target_center.0, target.y + target.height + tip_gap),
            ),
            FlowchartDirection::LeftRight => (
                (
                    source.x + source.width * 0.75,
                    source_center.1 + side * source.height * 0.25,
                ),
                (target.x - tip_gap, target_center.1),
            ),
            FlowchartDirection::RightLeft => (
                (
                    source.x + source.width * 0.25,
                    source_center.1 + side * source.height * 0.25,
                ),
                (target.x + target.width + tip_gap, target_center.1),
            ),
        };
        let primary_gap = match direction {
            FlowchartDirection::TopDown | FlowchartDirection::BottomUp => (end.1 - start.1).abs(),
            FlowchartDirection::LeftRight | FlowchartDirection::RightLeft => {
                (end.0 - start.0).abs()
            }
        };
        let label_gap = (primary_gap * 0.33).clamp(28.0, 64.0);
        let tail = (primary_gap * 0.12).clamp(10.0, 18.0);
        let (label_anchor, last_port) = match direction {
            FlowchartDirection::TopDown => ((end.0, end.1 - label_gap), (end.0, end.1 - tail)),
            FlowchartDirection::BottomUp => ((end.0, end.1 + label_gap), (end.0, end.1 + tail)),
            FlowchartDirection::LeftRight => ((end.0 - label_gap, end.1), (end.0 - tail, end.1)),
            FlowchartDirection::RightLeft => ((end.0 + label_gap, end.1), (end.0 + tail, end.1)),
        };

        edge.points = vec![start, label_anchor, last_port, end];
        if edge.label.is_some() {
            edge.label_anchor = Some(label_anchor);
        }
    }
}

fn branch_side(delta: f32, edge_index: usize) -> f32 {
    if delta.abs() > f32::EPSILON {
        delta.signum()
    } else if edge_index.is_multiple_of(2) {
        -1.0
    } else {
        1.0
    }
}

fn mermaid_render_options(source: &str) -> mermaid_rs_renderer::RenderOptions {
    let mut options = mermaid_rs_renderer::RenderOptions::modern();
    if flowchart_direction(source).is_some() {
        options = options
            .with_node_spacing(MERMAID_FLOWCHART_SPACING)
            .with_rank_spacing(MERMAID_FLOWCHART_SPACING);
    }
    // 网格路由会把短分支挤进同一条狭窄通道，圆角化后形成明显的 S 形回折。
    // 普通分层路由更接近 Mermaid JS：分支从判断节点自然展开，且仍避让节点。
    options.layout.flowchart.routing.enable_grid_router = false;
    options
}

#[derive(Clone, Copy)]
enum FlowchartDirection {
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

fn flowchart_direction(source: &str) -> Option<FlowchartDirection> {
    let header = source.lines().find_map(|line| {
        let line = line.trim();
        (!line.is_empty() && !line.starts_with("%%")).then_some(line)
    })?;
    let mut parts = header.split_whitespace();
    let kind = parts.next()?;
    if !kind.eq_ignore_ascii_case("flowchart") && !kind.eq_ignore_ascii_case("graph") {
        return None;
    }
    Some(
        match parts.next().unwrap_or("TD").to_ascii_uppercase().as_str() {
            "BT" => FlowchartDirection::BottomUp,
            "LR" => FlowchartDirection::LeftRight,
            "RL" => FlowchartDirection::RightLeft,
            _ => FlowchartDirection::TopDown,
        },
    )
}

fn smooth_forward_flowchart_edges(source: &str, svg: &str) -> String {
    let Some(direction) = flowchart_direction(source) else {
        return svg.to_string();
    };
    let mut output = String::with_capacity(svg.len());
    let mut cursor = 0;
    while let Some(relative_start) = svg[cursor..].find("<path") {
        let start = cursor + relative_start;
        output.push_str(&svg[cursor..start]);
        let Some(relative_end) = svg[start..].find('>') else {
            output.push_str(&svg[start..]);
            return output;
        };
        let end = start + relative_end + 1;
        let path = &svg[start..end];
        output.push_str(&smooth_flowchart_edge_path(path, direction));
        cursor = end;
    }
    output.push_str(&svg[cursor..]);
    output
}

fn smooth_flowchart_edge_path(path: &str, direction: FlowchartDirection) -> String {
    if !path.contains("class=\"edgePath\"") || !path.contains(" Q ") {
        return path.to_string();
    }
    let Some(d_start) = path.find(" d=\"").map(|index| index + 4) else {
        return path.to_string();
    };
    let Some(d_len) = path[d_start..].find('"') else {
        return path.to_string();
    };
    let d_end = d_start + d_len;
    let points = flowchart_path_endpoints(&path[d_start..d_end]);
    if points.len() < 4 {
        return path.to_string();
    }
    let (Some(start), Some(first_port), Some(last_port), Some(end)) = (
        points.first(),
        points.get(1),
        points.get(points.len().saturating_sub(2)),
        points.last(),
    ) else {
        return path.to_string();
    };
    let forward = match direction {
        FlowchartDirection::TopDown => end[1] > start[1],
        FlowchartDirection::BottomUp => end[1] < start[1],
        FlowchartDirection::LeftRight => end[0] > start[0],
        FlowchartDirection::RightLeft => end[0] < start[0],
    };
    if !forward {
        return path.to_string();
    }

    let overall = [end[0] - start[0], end[1] - start[1]];
    let end_tangent = [end[0] - last_port[0], end[1] - last_port[1]];
    let Some(overall_unit) = normalized_vector(overall) else {
        return path.to_string();
    };
    let Some(end_tangent_unit) = normalized_vector(end_tangent) else {
        return path.to_string();
    };
    let start_tangent = if path.contains("marker-start=") {
        normalized_vector([first_port[0] - start[0], first_port[1] - start[1]])
            .unwrap_or(overall_unit)
    } else {
        overall_unit
    };
    let handle = vector_length(overall).mul_add(0.32, 0.0).clamp(12.0, 80.0);
    let first_control = [
        start[0] + start_tangent[0] * handle,
        start[1] + start_tangent[1] * handle,
    ];
    let last_control = [
        end[0] - end_tangent_unit[0] * handle,
        end[1] - end_tangent_unit[1] * handle,
    ];

    // 中段使用连续曲线；末端控制点保持原入射方向，箭头会沿正确切线进入节点。
    let curve = format!(
        "M {:.3},{:.3} C {:.3},{:.3} {:.3},{:.3} {:.3},{:.3}",
        start[0],
        start[1],
        first_control[0],
        first_control[1],
        last_control[0],
        last_control[1],
        end[0],
        end[1]
    );
    format!("{}{}{}", &path[..d_start], curve, &path[d_end..])
}

fn vector_length(vector: [f32; 2]) -> f32 {
    vector[0].hypot(vector[1])
}

fn normalized_vector(vector: [f32; 2]) -> Option<[f32; 2]> {
    let length = vector_length(vector);
    (length.is_finite() && length > f32::EPSILON)
        .then_some([vector[0] / length, vector[1] / length])
}

fn flowchart_path_endpoints(path: &str) -> Vec<[f32; 2]> {
    let normalized = path.replace(',', " ");
    let mut tokens = normalized.split_whitespace();
    let mut points = Vec::new();
    while let Some(command) = tokens.next() {
        let number = |token: Option<&str>| token.and_then(|value| value.parse::<f32>().ok());
        match command {
            "M" | "L" => {
                let (Some(x), Some(y)) = (number(tokens.next()), number(tokens.next())) else {
                    return Vec::new();
                };
                points.push([x, y]);
            }
            "Q" => {
                let (Some(_control_x), Some(_control_y), Some(x), Some(y)) = (
                    number(tokens.next()),
                    number(tokens.next()),
                    number(tokens.next()),
                    number(tokens.next()),
                ) else {
                    return Vec::new();
                };
                points.push([x, y]);
            }
            _ => return Vec::new(),
        }
    }
    points
}

/// Stable cache key for Mermaid content.
pub(crate) fn mermaid_cache_key(source: &str) -> String {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn mermaid_themed_cache_key(source: &str, theme_mode: MermaidThemeMode) -> String {
    let mut hasher = DefaultHasher::new();
    MERMAID_RENDER_CACHE_VERSION.hash(&mut hasher);
    mermaid_cache_key(source).hash(&mut hasher);
    theme_mode.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Stable cache key for editor display SVG content and scale.
pub(crate) fn mermaid_display_cache_key(
    source: &str,
    scale: f32,
    theme_mode: MermaidThemeMode,
) -> String {
    let mut hasher = DefaultHasher::new();
    mermaid_themed_cache_key(source, theme_mode).hash(&mut hasher);
    scale.max(0.01).to_bits().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Counts diagram lines that materially contribute to rendered complexity.
pub(crate) fn semantic_mermaid_line_count(source: &str) -> usize {
    let mut in_frontmatter = false;
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return false;
            }
            if trimmed == "---" {
                in_frontmatter = !in_frontmatter;
                return false;
            }
            !(in_frontmatter || trimmed.starts_with("%%"))
        })
        .count()
}

/// Display scale used by the editor for rendered Mermaid diagrams.
pub(crate) fn mermaid_display_scale(
    source: &str,
    intrinsic_width: f32,
    intrinsic_height: f32,
    available_width: f32,
    _viewport_width: f32,
) -> f32 {
    let line_count = semantic_mermaid_line_count(source);
    let intrinsic_width = intrinsic_width.max(1.0);
    let available_width = available_width.max(1.0);
    let fit_scale = available_width / intrinsic_width;
    if line_count <= SIMPLE_MERMAID_LINE_LIMIT {
        let target_scale = (available_width * MERMAID_SIMPLE_TARGET_WIDTH_RATIO / intrinsic_width)
            .max(1.0)
            .min(MERMAID_SIMPLE_MAX_SCALE);
        return target_scale.min(fit_scale).max(0.01);
    }

    let _intrinsic_height = intrinsic_height.max(1.0);
    let extra_lines = line_count.saturating_sub(SIMPLE_MERMAID_LINE_LIMIT) as f32;

    let complexity_scale = (1.0 + extra_lines * MERMAID_SCALE_PER_EXTRA_LINE)
        .max(1.0)
        .min(MERMAID_MAX_SCALE);
    let target_column_width = available_width * MERMAID_COMPLEX_TARGET_WIDTH_RATIO;
    let column_fit_scale = if intrinsic_width < target_column_width {
        target_column_width / intrinsic_width
    } else {
        1.0
    };
    // 编辑器优先完整呈现图表；复杂度只能决定列内放大程度，不能把 SVG 推出内容列。
    complexity_scale
        .max(column_fit_scale)
        .min(fit_scale)
        .min(MERMAID_MAX_SCALE)
        .max(0.01)
}

fn strip_fence_indent(line: &str) -> Option<&str> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    (indent <= 3).then_some(&line[indent..])
}

/// Rewrites the root SVG element so GPUI sees the intended intrinsic size.
pub(crate) fn scale_mermaid_svg_for_display(
    svg: &str,
    scale: f32,
) -> anyhow::Result<(String, MermaidSvgSize)> {
    let scale = scale.max(0.01);
    let (start, end) = svg_root_tag_range(svg)?;
    let root_tag = &svg[start..end];
    let base_size = svg_root_size(root_tag)?;
    let intended_size = MermaidSvgSize {
        width: (base_size.width * scale).max(1.0),
        height: (base_size.height * scale).max(1.0),
    };
    let rewritten_root = rewrite_svg_root_tag(root_tag, intended_size)?;
    let mut rewritten = String::with_capacity(svg.len() + 48);
    rewritten.push_str(&svg[..start]);
    rewritten.push_str(&rewritten_root);
    rewritten.push_str(&svg[end..]);
    let rewritten = make_mermaid_display_background_transparent(&rewritten)?;
    let size = mermaid_svg_display_size(&rewritten)?;
    Ok((rewritten, size))
}

fn make_mermaid_display_background_transparent(svg: &str) -> anyhow::Result<String> {
    let (_, root_end) = svg_root_tag_range(svg)?;
    let Some(rect_start) = svg[root_end..].find("<rect").map(|index| root_end + index) else {
        return Ok(svg.to_string());
    };
    let Some(rect_end) = svg[rect_start..]
        .find('>')
        .map(|index| rect_start + index + 1)
    else {
        return Ok(svg.to_string());
    };
    let rect = &svg[rect_start..rect_end];
    let Some(fill_start) = rect.find("fill=\"").map(|index| index + "fill=\"".len()) else {
        return Ok(svg.to_string());
    };
    let Some(fill_len) = rect[fill_start..].find('"') else {
        return Ok(svg.to_string());
    };
    let fill_start = rect_start + fill_start;
    let fill_end = fill_start + fill_len;
    Ok(format!("{}none{}", &svg[..fill_start], &svg[fill_end..]))
}

fn mermaid_svg_intrinsic_size(svg: &str) -> anyhow::Result<MermaidSvgSize> {
    let (start, end) = svg_root_tag_range(svg)?;
    svg_root_size(&svg[start..end])
}

fn mermaid_svg_display_size(svg: &str) -> anyhow::Result<MermaidSvgSize> {
    let (start, end) = svg_root_tag_range(svg)?;
    let root_tag = &svg[start..end];
    let width = svg_root_attr(root_tag, "width")
        .and_then(|value| parse_svg_length(&value))
        .ok_or_else(|| anyhow!("Mermaid display SVG root did not expose a usable width"))?;
    let height = svg_root_attr(root_tag, "height")
        .and_then(|value| parse_svg_length(&value))
        .ok_or_else(|| anyhow!("Mermaid display SVG root did not expose a usable height"))?;
    Ok(MermaidSvgSize { width, height })
}

fn svg_root_tag_range(svg: &str) -> anyhow::Result<(usize, usize)> {
    let start = svg
        .find("<svg")
        .ok_or_else(|| anyhow!("Mermaid renderer output did not contain an SVG root"))?;
    let bytes = svg.as_bytes();
    let mut quote = None;
    let mut index = start;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active_quote) = quote {
            if byte == active_quote {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if byte == b'>' {
            return Ok((start, index + 1));
        }
        index += 1;
    }
    Err(anyhow!(
        "Mermaid renderer output had an unterminated SVG root tag"
    ))
}

fn svg_root_size(root_tag: &str) -> anyhow::Result<MermaidSvgSize> {
    if let Some(view_box) = svg_root_attr(root_tag, "viewBox")
        && let Some(size) = parse_view_box_size(&view_box)
    {
        return Ok(size);
    }

    let width = svg_root_attr(root_tag, "width")
        .and_then(|value| parse_svg_length(&value))
        .ok_or_else(|| anyhow!("Mermaid SVG root did not expose a usable width"))?;
    let height = svg_root_attr(root_tag, "height")
        .and_then(|value| parse_svg_length(&value))
        .ok_or_else(|| anyhow!("Mermaid SVG root did not expose a usable height"))?;
    Ok(MermaidSvgSize { width, height })
}

fn parse_view_box_size(view_box: &str) -> Option<MermaidSvgSize> {
    let values = view_box
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (values.len() == 4 && values[2].is_finite() && values[3].is_finite()).then_some(
        MermaidSvgSize {
            width: values[2].max(1.0),
            height: values[3].max(1.0),
        },
    )
}

fn parse_svg_length(value: &str) -> Option<f32> {
    let value = value.trim();
    let end = value
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+' | 'e' | 'E'))
        .map(|(index, ch)| index + ch.len_utf8())
        .last()?;
    let parsed = value[..end].parse::<f32>().ok()?;
    (parsed.is_finite() && parsed > 0.0).then_some(parsed)
}

fn svg_root_attr(root_tag: &str, attr_name: &str) -> Option<String> {
    svg_root_attrs(root_tag)
        .into_iter()
        .find(|attr| attr.name.eq_ignore_ascii_case(attr_name))
        .and_then(|attr| attr.value)
}

fn rewrite_svg_root_tag(root_tag: &str, size: MermaidSvgSize) -> anyhow::Result<String> {
    let attrs = svg_root_attrs(root_tag)
        .into_iter()
        .filter(|attr| {
            !["width", "height", "style"]
                .iter()
                .any(|name| attr.name.eq_ignore_ascii_case(name))
        })
        .map(|attr| attr.raw)
        .collect::<Vec<_>>();

    let mut rewritten = String::from("<svg");
    for attr in attrs {
        rewritten.push(' ');
        rewritten.push_str(attr.trim());
    }
    rewritten.push_str(&format!(
        " width=\"{:.3}\" height=\"{:.3}\">",
        size.width, size.height
    ));
    Ok(rewritten)
}

#[derive(Debug)]
struct SvgRootAttr {
    name: String,
    value: Option<String>,
    raw: String,
}

fn svg_root_attrs(root_tag: &str) -> Vec<SvgRootAttr> {
    let Some(mut index) = root_tag.find("<svg").map(|index| index + "<svg".len()) else {
        return Vec::new();
    };
    let end = root_tag.rfind('>').unwrap_or(root_tag.len());
    let bytes = root_tag.as_bytes();
    let mut attrs = Vec::new();

    while index < end {
        while index < end && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= end || bytes[index] == b'/' {
            break;
        }

        let attr_start = index;
        while index < end
            && !bytes[index].is_ascii_whitespace()
            && bytes[index] != b'='
            && bytes[index] != b'/'
        {
            index += 1;
        }
        let name = root_tag[attr_start..index].to_string();
        if name.is_empty() {
            break;
        }

        while index < end && bytes[index].is_ascii_whitespace() {
            index += 1;
        }

        let mut value = None;
        if index < end && bytes[index] == b'=' {
            index += 1;
            while index < end && bytes[index].is_ascii_whitespace() {
                index += 1;
            }

            if index < end && (bytes[index] == b'"' || bytes[index] == b'\'') {
                let quote = bytes[index];
                index += 1;
                let value_start = index;
                while index < end && bytes[index] != quote {
                    index += 1;
                }
                value = Some(root_tag[value_start..index].to_string());
                if index < end {
                    index += 1;
                }
            } else {
                let value_start = index;
                while index < end && !bytes[index].is_ascii_whitespace() && bytes[index] != b'/' {
                    index += 1;
                }
                value = Some(root_tag[value_start..index].to_string());
            }
        }

        let raw = root_tag[attr_start..index].trim().to_string();
        attrs.push(SvgRootAttr { name, value, raw });
    }

    attrs
}

fn mermaid_cache_dir() -> anyhow::Result<PathBuf> {
    let root = ProjectDirs::from("com", "kongweiguang", "gmark")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("gmark"));
    let dir = root.join("mermaid-svg");
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create Mermaid SVG cache '{}'", dir.display()))?;
    Ok(dir)
}

fn mermaid_cache_file_path(kind: &str, key: &str) -> anyhow::Result<PathBuf> {
    mermaid_cache_file_path_in(&mermaid_cache_dir()?, kind, key)
}

fn mermaid_cache_file_path_in(cache_dir: &Path, kind: &str, key: &str) -> anyhow::Result<PathBuf> {
    let dir = cache_dir.join(kind);
    fs::create_dir_all(&dir).with_context(|| {
        format!(
            "failed to create Mermaid {kind} SVG cache '{}'",
            dir.display()
        )
    })?;
    Ok(dir.join(format!("{key}.svg")))
}

fn looks_like_supported_mermaid_source(source: &str) -> bool {
    let mut in_frontmatter = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "---" {
            in_frontmatter = !in_frontmatter;
            continue;
        }
        if in_frontmatter || trimmed.starts_with("%%") {
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        return [
            "sequencediagram",
            "classdiagram",
            "statediagram",
            "erdiagram",
            "pie",
            "mindmap",
            "journey",
            "timeline",
            "gantt",
            "requirementdiagram",
            "gitgraph",
            "c4",
            "sankey",
            "quadrantchart",
            "zenuml",
            "block",
            "packet",
            "kanban",
            "architecture",
            "radar",
            "treemap",
            "xychart",
            "flowchart",
            "graph",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix));
    }
    false
}

#[cfg(test)]
#[path = "../../../tests/unit/components/mermaid/tests.rs"]
mod tests;
